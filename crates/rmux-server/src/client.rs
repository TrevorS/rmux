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

/// State for the interactive command prompt (:).
#[derive(Debug, Clone, Default)]
pub struct PromptState {
    /// Current input buffer.
    pub buffer: String,
    /// Cursor position in the buffer.
    pub cursor_pos: usize,
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
    /// Terminal width.
    pub sx: u32,
    /// Terminal height.
    pub sy: u32,
    /// Command prompt state (Some = prompt mode active).
    pub prompt: Option<PromptState>,
    /// Mouse click tracking for double/triple-click detection.
    pub click_state: ClickState,
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
        Self {
            last_click: std::time::Instant::now(),
            last_x: 0,
            last_y: 0,
            count: 0,
        }
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

impl ServerClient {
    /// Create a new client from a message writer.
    pub fn new(id: u64, writer: MessageWriter) -> Self {
        Self {
            id,
            writer,
            identify: IdentifyState::default(),
            flags: ClientFlags::empty(),
            session_id: None,
            sx: 80,
            sy: 24,
            prompt: None,
            click_state: ClickState::default(),
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
        cs.last_click = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(600))
            .unwrap();
        assert_eq!(cs.register_click(5, 10), 1); // Should reset due to timeout
    }
}
