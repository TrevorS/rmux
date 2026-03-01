//! Terminal setup and restore.
//!
//! Handles raw mode for attached sessions and terminal size queries.

use nix::sys::termios::{self, SetArg, Termios};

/// RAII guard for raw terminal mode.
///
/// Saves the terminal state on creation and restores it on drop.
pub struct RawTerminal {
    original: Termios,
}

impl RawTerminal {
    /// Enter raw mode on stdin.
    pub fn enter() -> Result<Self, nix::Error> {
        let stdin = std::io::stdin();
        let original = termios::tcgetattr(&stdin)?;

        let mut raw = original.clone();
        termios::cfmakeraw(&mut raw);

        // For tmux compatibility, we want full raw mode.
        // The prefix key (Ctrl-b) handles user interaction.
        termios::tcsetattr(&stdin, SetArg::TCSAFLUSH, &raw)?;

        Ok(Self { original })
    }

    /// Restore the terminal to its original state.
    pub fn restore(&self) -> Result<(), nix::Error> {
        let stdin = std::io::stdin();
        termios::tcsetattr(&stdin, SetArg::TCSAFLUSH, &self.original)
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        self.restore().ok();
    }
}

/// Get the current terminal size (columns, rows).
#[allow(unsafe_code)]
pub fn get_terminal_size() -> (u32, u32) {
    // SAFETY: ioctl with TIOCGWINSZ is safe with a valid fd and zeroed buffer.
    unsafe {
        let mut ws: nix::libc::winsize = std::mem::zeroed();
        if nix::libc::ioctl(nix::libc::STDOUT_FILENO, nix::libc::TIOCGWINSZ, &mut ws) == 0 {
            (u32::from(ws.ws_col), u32::from(ws.ws_row))
        } else {
            (80, 24) // Fallback
        }
    }
}
