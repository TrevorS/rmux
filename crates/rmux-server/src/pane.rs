//! Pane management.

use crate::copymode::CopyModeState;
use rmux_core::screen::Screen;
use rmux_terminal::input::InputParser;
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
        Self {
            id,
            screen: Screen::new(sx, sy, history_limit),
            parser: InputParser::new(),
            pty_fd: -1,
            pid: 0,
            sx,
            sy,
            xoff: 0,
            yoff: 0,
            dead: false,
            copy_mode: None,
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
    fn copy_mode_enter_twice_noop() {
        let mut p = Pane::new(80, 24, 0);
        p.enter_copy_mode("vi");
        assert!(p.is_in_copy_mode());
        // Entering again should not reset state
        p.enter_copy_mode("vi");
        assert!(p.is_in_copy_mode());
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
