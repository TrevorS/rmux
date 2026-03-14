//! Pane management.

use crate::copymode::CopyModeState;
use rmux_core::screen::Screen;
use rmux_terminal::input::InputParser;
use std::io::Write as IoWrite;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_PANE_ID: AtomicU32 = AtomicU32::new(0);

/// A tmux pane (a virtual terminal with a running process).
#[derive(Debug)]
pub struct Pane {
    /// Unique pane ID.
    pub id: u32,
    /// Screen state.
    pub screen: Screen,
    /// Input parser for processing PTY output.
    pub parser: InputParser,
    /// PTY master fd (-1 if not yet spawned).
    pub pty_fd: i32,
    /// Child process PID (0 if not yet spawned).
    pub pid: u32,
    /// Pane width.
    pub sx: u32,
    /// Pane height.
    pub sy: u32,
    /// X offset within window.
    pub xoff: u32,
    /// Y offset within window.
    pub yoff: u32,
    /// Whether the pane's process has exited.
    pub dead: bool,
    /// Copy mode state (Some = pane is in copy mode).
    pub copy_mode: Option<CopyModeState>,
    /// Child process for pipe-pane (piping PTY output to a shell command).
    pub pipe_child: Option<Child>,
    /// The command used to start this pane's process.
    pub start_command: String,
}

impl Pane {
    /// Create a new pane with the given dimensions and an auto-generated ID.
    #[must_use]
    pub fn new(sx: u32, sy: u32, history_limit: u32) -> Self {
        Self::with_id(NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed), sx, sy, history_limit)
    }

    /// Create a new pane with a specific ID.
    #[must_use]
    pub fn with_id(id: u32, sx: u32, sy: u32, history_limit: u32) -> Self {
        let mut screen = Screen::new(sx, sy, history_limit);
        // tmux defaults pane_title to the system hostname.
        if let Ok(hostname) = nix::unistd::gethostname() {
            screen.title = hostname.to_string_lossy().into_owned();
        }
        Self {
            id,
            screen,
            parser: InputParser::new(),
            pty_fd: -1,
            pid: 0,
            sx,
            sy,
            xoff: 0,
            yoff: 0,
            dead: false,
            copy_mode: None,
            pipe_child: None,
            start_command: String::new(),
        }
    }

    /// Feed data from the PTY into the parser.
    pub fn process_input(&mut self, data: &[u8]) {
        self.parser.parse(data, &mut self.screen);
    }

    /// Resize the pane.
    pub fn resize(&mut self, sx: u32, sy: u32) {
        self.sx = sx;
        self.sy = sy;
        self.screen.resize(sx, sy);
    }

    /// Enter copy mode on this pane.
    pub fn enter_copy_mode(&mut self, mode_keys: &str) {
        if self.copy_mode.is_none() {
            self.copy_mode = Some(CopyModeState::enter(&self.screen, mode_keys));
        }
    }

    /// Exit copy mode on this pane.
    pub fn exit_copy_mode(&mut self) {
        self.copy_mode = None;
        self.screen.selection = None;
    }

    /// Whether this pane is in copy mode.
    pub fn is_in_copy_mode(&self) -> bool {
        self.copy_mode.is_some()
    }

    /// Start piping PTY output to a shell command.
    pub fn start_pipe(&mut self, command: &str) -> Result<(), std::io::Error> {
        // Close any existing pipe
        self.stop_pipe();
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.pipe_child = Some(child);
        Ok(())
    }

    /// Stop piping (close the child's stdin and wait).
    pub fn stop_pipe(&mut self) {
        if let Some(mut child) = self.pipe_child.take() {
            // Drop stdin to signal EOF to the child
            drop(child.stdin.take());
            child.wait().ok();
        }
    }

    /// Feed data to the pipe child (if active).
    pub fn pipe_output(&mut self, data: &[u8]) {
        if let Some(ref mut child) = self.pipe_child {
            if let Some(ref mut stdin) = child.stdin {
                if stdin.write_all(data).is_err() {
                    // Pipe broken — clean up
                    self.pipe_child = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pane() {
        let p = Pane::new(80, 24, 2000);
        assert_eq!(p.sx, 80);
        assert_eq!(p.sy, 24);
        assert_eq!(p.pty_fd, -1);
    }

    #[test]
    fn process_input() {
        let mut p = Pane::new(80, 24, 0);
        p.process_input(b"Hello World");
        assert_eq!(p.screen.cursor.x, 11);
    }

    #[test]
    fn resize_pane() {
        let mut p = Pane::new(80, 24, 0);
        p.resize(120, 40);
        assert_eq!(p.screen.width(), 120);
        assert_eq!(p.screen.height(), 40);
    }

    #[test]
    fn copy_mode_lifecycle() {
        let mut p = Pane::new(80, 24, 0);
        assert!(!p.is_in_copy_mode());
        p.enter_copy_mode("vi");
        assert!(p.is_in_copy_mode());
        p.exit_copy_mode();
        assert!(!p.is_in_copy_mode());
    }

    #[test]
    fn dead_flag() {
        let mut p = Pane::new(80, 24, 0);
        assert!(!p.dead);
        p.dead = true;
        assert!(p.dead);
    }

    #[test]
    fn with_id_constructor() {
        let p = Pane::with_id(42, 80, 24, 1000);
        assert_eq!(p.id, 42);
        assert_eq!(p.sx, 80);
        assert_eq!(p.sy, 24);
        assert!(!p.dead);
        assert!(p.copy_mode.is_none());
        assert!(p.pipe_child.is_none());
    }

    #[test]
    fn pipe_lifecycle() {
        let mut p = Pane::new(80, 24, 0);
        // Start piping to a simple command
        assert!(p.start_pipe("cat > /dev/null").is_ok());
        assert!(p.pipe_child.is_some());
        // Feed some data
        p.pipe_output(b"hello");
        // Stop piping
        p.stop_pipe();
        assert!(p.pipe_child.is_none());
    }

    #[test]
    fn pipe_output_no_pipe_noop() {
        let mut p = Pane::new(80, 24, 0);
        // Should not panic when no pipe is active
        p.pipe_output(b"hello");
    }

    #[test]
    fn stop_pipe_no_pipe_noop() {
        let mut p = Pane::new(80, 24, 0);
        p.stop_pipe();
        assert!(p.pipe_child.is_none());
    }

    #[test]
    fn start_pipe_replaces_existing() {
        let mut p = Pane::new(80, 24, 0);
        assert!(p.start_pipe("cat > /dev/null").is_ok());
        // Starting a new pipe should stop the old one
        assert!(p.start_pipe("cat > /dev/null").is_ok());
        assert!(p.pipe_child.is_some());
        p.stop_pipe();
    }

    #[test]
    fn copy_mode_enter_twice_noop() {
        let mut p = Pane::new(80, 24, 0);
        p.enter_copy_mode("vi");
        assert!(p.is_in_copy_mode());
        // Entering again should not reset state
        p.enter_copy_mode("vi");
        assert!(p.is_in_copy_mode());
    }

    #[test]
    fn default_pane_title_is_hostname() {
        let p = Pane::new(80, 24, 0);
        // tmux defaults pane_title to the system hostname
        let expected = nix::unistd::gethostname()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert_eq!(p.screen.title, expected);
        assert!(!p.screen.title.is_empty(), "pane title should not be empty");
    }

    #[test]
    fn osc_title_overrides_default() {
        let mut p = Pane::new(80, 24, 0);
        // OSC 0 sets pane title
        p.process_input(b"\x1b]0;My Custom Title\x07");
        assert_eq!(p.screen.title, "My Custom Title");
    }

    #[test]
    fn exit_copy_mode_clears_selection() {
        use rmux_core::screen::selection::{Selection, SelectionType};
        let mut p = Pane::new(80, 24, 0);
        p.screen.selection = Some(Selection {
            sel_type: SelectionType::Normal,
            start_x: 0,
            start_y: 0,
            end_x: 5,
            end_y: 0,
            active: true,
        });
        assert!(p.screen.selection.is_some());
        p.exit_copy_mode();
        assert!(p.screen.selection.is_none());
    }
}
