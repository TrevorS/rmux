//! Per-client state management.
//!
//! Each connected client has a `ServerClient` that tracks its identification
//! state, attached session, terminal size, and provides message I/O.

use bitflags::bitflags;
use rmux_protocol::codec::MessageWriter;
use rmux_protocol::identify::IdentifyState;
use rmux_protocol::message::Message;

bitflags! {
    /// Client state flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct ClientFlags: u32 {
        /// Client has completed identification.
        const IDENTIFIED   = 0x0001;
        /// Client is attached to a session.
        const ATTACHED     = 0x0002;
        /// Client needs a redraw.
        const REDRAW       = 0x0004;
        /// Client is exiting.
        const EXITING      = 0x0008;
    }
}

/// What happens when the prompt is submitted.
#[derive(Debug, Clone, Default)]
pub enum PromptType {
    /// Execute the buffer as a command.
    #[default]
    Command,
    /// Search forward in copy mode.
    SearchForward,
    /// Search backward in copy mode.
    SearchBackward,
    /// Go to line number in copy mode.
    GotoLine,
}

/// State for the interactive command prompt (:).
#[derive(Debug, Clone, Default)]
pub struct PromptState {
    /// Current input buffer.
    pub buffer: String,
    /// Cursor position in the buffer.
    pub cursor_pos: usize,
    /// What to do when submitted.
    pub prompt_type: PromptType,
    /// Custom prompt string (e.g., "(rename-session) ").
    pub prompt_str: Option<String>,
    /// Command template — input replaces `%%` (e.g., "rename-session '%%'").
    pub template: Option<String>,
}

/// A connected client on the server side.
pub struct ServerClient {
    /// Unique client ID.
    pub id: u64,
    /// Message writer for sending data to this client.
    pub writer: MessageWriter,
    /// Identification state machine.
    pub identify: IdentifyState,
    /// Client flags.
    pub flags: ClientFlags,
    /// Attached session ID (if any).
    pub session_id: Option<u32>,
    /// Previously attached session ID (for switch-client -l).
    pub last_session_id: Option<u32>,
    /// Terminal width.
    pub sx: u32,
    /// Terminal height.
    pub sy: u32,
    /// Command prompt state (Some = prompt mode active).
    pub prompt: Option<PromptState>,
    /// Active overlay (choose-tree, display-menu, etc.).
    pub overlay: Option<crate::overlay::OverlayState>,
    /// Mouse click tracking for double/triple-click detection.
    pub click_state: ClickState,
    /// Unix timestamp of last activity (client input).
    pub activity: u64,
    /// Timed status message and its expiry instant.
    pub timed_message: Option<(String, std::time::Instant)>,
    /// Control mode: send text notifications instead of raw terminal output.
    pub control_mode: bool,
}

/// State for detecting double/triple-click sequences.
#[derive(Debug, Clone)]
pub struct ClickState {
    /// Timestamp of the last click.
    pub last_click: std::time::Instant,
    /// Position of the last click (x, y).
    pub last_x: u32,
    pub last_y: u32,
    /// Number of rapid consecutive clicks at the same position (1, 2, 3).
    pub count: u32,
}

impl Default for ClickState {
    fn default() -> Self {
        Self { last_click: std::time::Instant::now(), last_x: 0, last_y: 0, count: 0 }
    }
}

impl ClickState {
    /// Register a click and return the click count (1=single, 2=double, 3=triple).
    /// Double-click threshold is 500ms and must be at the same position.
    pub fn register_click(&mut self, x: u32, y: u32) -> u32 {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_click);
        let same_pos = self.last_x == x && self.last_y == y;

        if same_pos && elapsed.as_millis() < 500 && self.count < 3 {
            self.count += 1;
        } else {
            self.count = 1;
        }

        self.last_click = now;
        self.last_x = x;
        self.last_y = y;
        self.count
    }
}

/// Result of processing a single prompt input byte/sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    /// Submit the prompt (Enter was pressed).
    Submit,
    /// Cancel the prompt (Escape was pressed).
    Cancel,
    /// The prompt buffer was modified (needs redraw).
    Changed,
    /// The byte was consumed but no action needed.
    Ignored,
    /// Need more bytes (incomplete UTF-8 or escape sequence).
    NeedMore,
}

/// Process prompt input bytes and return what action to take.
///
/// This is a pure function operating on `PromptState`, extracted from the server
/// event loop for testability. Returns `(action, bytes_consumed)`.
pub fn process_prompt_input(prompt: &mut PromptState, data: &[u8]) -> (PromptAction, usize) {
    if data.is_empty() {
        return (PromptAction::NeedMore, 0);
    }

    match data[0] {
        // Enter
        0x0D | 0x0A => (PromptAction::Submit, 1),
        // Escape (bare or double)
        0x1B if data.len() == 1 || data[1] == 0x1B => (PromptAction::Cancel, 1),
        // Backspace / DEL
        0x7F | 0x08 => {
            prompt.buffer.pop();
            (PromptAction::Changed, 1)
        }
        // Ctrl-U — clear line
        0x15 => {
            prompt.buffer.clear();
            (PromptAction::Changed, 1)
        }
        // Printable ASCII
        0x20..=0x7E => {
            prompt.buffer.push(data[0] as char);
            (PromptAction::Changed, 1)
        }
        // UTF-8 multi-byte
        0xC2..=0xF4 => {
            let utf8_len = match data[0] {
                0xC2..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF4 => 4,
                _ => 1,
            };
            if data.len() < utf8_len {
                return (PromptAction::NeedMore, 0);
            }
            if let Ok(s) = std::str::from_utf8(&data[..utf8_len]) {
                if let Some(ch) = s.chars().next() {
                    if !ch.is_control() {
                        prompt.buffer.push(ch);
                        return (PromptAction::Changed, utf8_len);
                    }
                }
                (PromptAction::Ignored, utf8_len)
            } else {
                (PromptAction::Ignored, 1)
            }
        }
        // ESC sequence (not bare)
        0x1B => {
            let (_, consumed) =
                rmux_terminal::keys::parse_key(data).unwrap_or((rmux_core::key::KEYC_UNKNOWN, 1));
            (PromptAction::Ignored, consumed)
        }
        // Other control chars
        _ => (PromptAction::Ignored, 1),
    }
}

impl ServerClient {
    /// Create a new client from a message writer.
    pub fn new(id: u64, writer: MessageWriter) -> Self {
        Self {
            id,
            writer,
            identify: IdentifyState::default(),
            flags: ClientFlags::empty(),
            session_id: None,
            last_session_id: None,
            sx: 80,
            sy: 24,
            prompt: None,
            overlay: None,
            click_state: ClickState::default(),
            activity: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            timed_message: None,
            control_mode: false,
        }
    }

    /// Whether this client has completed identification.
    pub fn is_identified(&self) -> bool {
        self.flags.contains(ClientFlags::IDENTIFIED)
    }

    /// Whether this client is attached to a session.
    pub fn is_attached(&self) -> bool {
        self.flags.contains(ClientFlags::ATTACHED)
    }

    /// Mark client as needing a redraw.
    pub fn mark_redraw(&mut self) {
        self.flags.insert(ClientFlags::REDRAW);
    }

    /// Check and clear the redraw flag.
    pub fn needs_redraw(&mut self) -> bool {
        let needs = self.flags.contains(ClientFlags::REDRAW);
        self.flags.remove(ClientFlags::REDRAW);
        needs
    }

    /// Set the terminal size.
    pub fn set_size(&mut self, sx: u32, sy: u32) {
        if self.sx != sx || self.sy != sy {
            self.sx = sx;
            self.sy = sy;
            self.mark_redraw();
        }
    }

    /// Attach to a session.
    pub fn attach(&mut self, session_id: u32) {
        self.session_id = Some(session_id);
        self.flags.insert(ClientFlags::ATTACHED);
        self.mark_redraw();
    }

    /// Detach from the current session.
    pub fn detach(&mut self) {
        self.session_id = None;
        self.flags.remove(ClientFlags::ATTACHED);
    }

    /// Send a message to this client.
    pub async fn send(&mut self, msg: &Message) -> Result<(), rmux_protocol::codec::CodecError> {
        self.writer.write_message(msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // Prompt input processing
    // ============================================================

    fn prompt_with(buf: &str) -> PromptState {
        PromptState { buffer: buf.to_string(), ..PromptState::default() }
    }

    #[test]
    fn prompt_enter_submits() {
        let mut prompt = prompt_with("list-sessions");
        let (action, consumed) = process_prompt_input(&mut prompt, b"\r");
        assert_eq!(action, PromptAction::Submit);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn prompt_newline_submits() {
        let mut prompt = PromptState::default();
        let (action, _) = process_prompt_input(&mut prompt, b"\n");
        assert_eq!(action, PromptAction::Submit);
    }

    #[test]
    fn prompt_escape_cancels() {
        let mut prompt = prompt_with("partial");
        let (action, _) = process_prompt_input(&mut prompt, b"\x1b");
        assert_eq!(action, PromptAction::Cancel);
    }

    #[test]
    fn prompt_double_escape_cancels() {
        let mut prompt = PromptState::default();
        let (action, _) = process_prompt_input(&mut prompt, b"\x1b\x1b");
        assert_eq!(action, PromptAction::Cancel);
    }

    #[test]
    fn prompt_printable_ascii() {
        let mut prompt = PromptState::default();
        let (action, consumed) = process_prompt_input(&mut prompt, b"a");
        assert_eq!(action, PromptAction::Changed);
        assert_eq!(consumed, 1);
        assert_eq!(prompt.buffer, "a");
    }

    #[test]
    fn prompt_builds_string() {
        let mut prompt = PromptState::default();
        for ch in b"hello" {
            process_prompt_input(&mut prompt, std::slice::from_ref(ch));
        }
        assert_eq!(prompt.buffer, "hello");
    }

    #[test]
    fn prompt_backspace_deletes() {
        let mut prompt = prompt_with("abc");
        let (action, _) = process_prompt_input(&mut prompt, b"\x7f");
        assert_eq!(action, PromptAction::Changed);
        assert_eq!(prompt.buffer, "ab");
    }

    #[test]
    fn prompt_ctrl_h_deletes() {
        let mut prompt = prompt_with("abc");
        let (action, _) = process_prompt_input(&mut prompt, b"\x08");
        assert_eq!(action, PromptAction::Changed);
        assert_eq!(prompt.buffer, "ab");
    }

    #[test]
    fn prompt_backspace_empty_noop() {
        let mut prompt = PromptState::default();
        let (action, _) = process_prompt_input(&mut prompt, b"\x7f");
        assert_eq!(action, PromptAction::Changed);
        assert_eq!(prompt.buffer, "");
    }

    #[test]
    fn prompt_ctrl_u_clears() {
        let mut prompt = prompt_with("some text");
        let (action, _) = process_prompt_input(&mut prompt, b"\x15");
        assert_eq!(action, PromptAction::Changed);
        assert_eq!(prompt.buffer, "");
    }

    #[test]
    fn prompt_control_char_ignored() {
        let mut prompt = PromptState::default();
        let (action, consumed) = process_prompt_input(&mut prompt, b"\x01"); // Ctrl-A
        assert_eq!(action, PromptAction::Ignored);
        assert_eq!(consumed, 1);
        assert_eq!(prompt.buffer, "");
    }

    #[test]
    fn prompt_utf8_input() {
        let mut prompt = PromptState::default();
        let (action, consumed) = process_prompt_input(&mut prompt, "é".as_bytes());
        assert_eq!(action, PromptAction::Changed);
        assert_eq!(consumed, 2);
        assert_eq!(prompt.buffer, "é");
    }

    #[test]
    fn prompt_utf8_cjk() {
        let mut prompt = PromptState::default();
        let (action, consumed) = process_prompt_input(&mut prompt, "世".as_bytes());
        assert_eq!(action, PromptAction::Changed);
        assert_eq!(consumed, 3);
        assert_eq!(prompt.buffer, "世");
    }

    #[test]
    fn prompt_incomplete_utf8_needs_more() {
        let mut prompt = PromptState::default();
        let (action, consumed) = process_prompt_input(&mut prompt, &[0xC3]); // Incomplete 2-byte
        assert_eq!(action, PromptAction::NeedMore);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn prompt_esc_sequence_ignored() {
        let mut prompt = PromptState::default();
        // CSI A (cursor up) — should be consumed and ignored in prompt
        let (action, consumed) = process_prompt_input(&mut prompt, b"\x1b[A");
        assert_eq!(action, PromptAction::Ignored);
        assert!(consumed >= 3);
        assert_eq!(prompt.buffer, "");
    }

    #[test]
    fn prompt_empty_data_needs_more() {
        let mut prompt = PromptState::default();
        let (action, consumed) = process_prompt_input(&mut prompt, b"");
        assert_eq!(action, PromptAction::NeedMore);
        assert_eq!(consumed, 0);
    }

    // ============================================================
    // Click state
    // ============================================================

    #[test]
    fn click_state_single_click() {
        let mut cs = ClickState::default();
        assert_eq!(cs.register_click(5, 10), 1);
    }

    #[test]
    fn click_state_double_click() {
        let mut cs = ClickState::default();
        assert_eq!(cs.register_click(5, 10), 1);
        assert_eq!(cs.register_click(5, 10), 2);
    }

    #[test]
    fn click_state_triple_click() {
        let mut cs = ClickState::default();
        assert_eq!(cs.register_click(5, 10), 1);
        assert_eq!(cs.register_click(5, 10), 2);
        assert_eq!(cs.register_click(5, 10), 3);
    }

    #[test]
    fn click_state_caps_at_three() {
        let mut cs = ClickState::default();
        cs.register_click(5, 10);
        cs.register_click(5, 10);
        cs.register_click(5, 10);
        // Fourth click resets to 1
        assert_eq!(cs.register_click(5, 10), 1);
    }

    #[test]
    fn click_state_different_position_resets() {
        let mut cs = ClickState::default();
        assert_eq!(cs.register_click(5, 10), 1);
        assert_eq!(cs.register_click(20, 10), 1); // Different x
    }

    #[test]
    fn click_state_timeout_resets() {
        let mut cs = ClickState::default();
        assert_eq!(cs.register_click(5, 10), 1);
        // Simulate timeout by backdating last_click
        cs.last_click =
            std::time::Instant::now().checked_sub(std::time::Duration::from_millis(600)).unwrap();
        assert_eq!(cs.register_click(5, 10), 1); // Should reset due to timeout
    }
}
