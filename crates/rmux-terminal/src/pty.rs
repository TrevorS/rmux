//! PTY (pseudo-terminal) operations.
//!
//! Creates and manages pseudo-terminals for pane processes.

use nix::libc;
use nix::pty::{openpty, OpenptyResult, Winsize};
use std::os::fd::{AsRawFd, OwnedFd};

/// Error type for PTY operations.
#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("failed to create PTY: {0}")]
    Create(#[from] nix::Error),
    #[error("failed to spawn process: {0}")]
    Spawn(std::io::Error),
}

/// A PTY pair (master + slave).
pub struct Pty {
    /// Master fd (server reads/writes this).
    pub master: OwnedFd,
    /// Slave fd (child process's stdin/stdout/stderr).
    pub slave: OwnedFd,
}

impl Pty {
    /// Create a new PTY pair with the given initial size.
    pub fn open(cols: u16, rows: u16) -> Result<Self, PtyError> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let OpenptyResult { master, slave } = openpty(&winsize, None)?;

        // openpty already returns OwnedFd, so just use them directly
        Ok(Self { master, slave })
    }

    /// Resize the PTY.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: master fd is valid and we're passing a valid Winsize struct.
        unsafe {
            let ret = libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &winsize);
            if ret < 0 {
                return Err(PtyError::Create(nix::Error::last()));
            }
        }
        Ok(())
    }

    /// Get the master fd for reading/writing.
    #[must_use]
    pub fn master_fd(&self) -> i32 {
        self.master.as_raw_fd()
    }

    /// Get the slave fd for the child process.
    #[must_use]
    pub fn slave_fd(&self) -> i32 {
        self.slave.as_raw_fd()
    }
}

/// Set a file descriptor to non-blocking mode.
pub fn set_nonblocking(fd: i32) -> Result<(), PtyError> {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    let flags = fcntl(fd, FcntlArg::F_GETFL).map_err(PtyError::Create)?;
    let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
    fcntl(fd, FcntlArg::F_SETFL(flags)).map_err(PtyError::Create)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_pty() {
        let pty = Pty::open(80, 24);
        assert!(pty.is_ok());
        let pty = pty.unwrap();
        assert!(pty.master_fd() >= 0);
        assert!(pty.slave_fd() >= 0);
    }

    #[test]
    fn resize_pty() {
        let pty = Pty::open(80, 24).unwrap();
        assert!(pty.resize(120, 40).is_ok());
    }
}
