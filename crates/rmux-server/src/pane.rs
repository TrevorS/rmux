//! Pane management.

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
}

impl Pane {
    /// Create a new pane with the given dimensions.
    #[must_use]
    pub fn new(sx: u32, sy: u32, history_limit: u32) -> Self {
        Self {
            id: NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed),
            screen: Screen::new(sx, sy, history_limit),
            parser: InputParser::new(),
            pty_fd: -1,
            pid: 0,
            sx,
            sy,
            xoff: 0,
            yoff: 0,
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
}
