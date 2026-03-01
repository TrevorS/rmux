//! PTY (pseudo-terminal) operations.
//!
//! Creates and manages pseudo-terminals for pane processes.

use nix::libc;
use nix::pty::{openpty, OpenptyResult, Winsize};
use nix::unistd::{fork, ForkResult, Pid};
use std::ffi::CString;
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

/// A spawned process with its PTY master fd.
#[derive(Debug)]
pub struct SpawnedProcess {
    /// Master fd for communicating with the child process.
    pub master: OwnedFd,
    /// Child process PID.
    pub pid: Pid,
}

impl SpawnedProcess {
    /// Resize the PTY.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: master fd is valid and winsize is a valid struct.
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

    /// Spawn a shell process in this PTY.
    ///
    /// Consumes the PTY, forking a child process that runs the given shell.
    /// Returns a `SpawnedProcess` with the master fd and child PID.
    /// The slave fd is closed in the parent process after fork.
    pub fn spawn_shell(self, shell: &str, cwd: &str) -> Result<SpawnedProcess, PtyError> {
        // Prepare C strings before fork (allocation is not async-signal-safe).
        let shell_cstr = CString::new(shell).map_err(|e| {
            PtyError::Spawn(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;
        let cwd_cstr = CString::new(cwd).map_err(|e| {
            PtyError::Spawn(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;

        // Create login shell argv[0]: "-basename"
        let basename = shell.rsplit('/').next().unwrap_or(shell);
        let login_name = format!("-{basename}");
        let login_cstr = CString::new(login_name).map_err(|e| {
            PtyError::Spawn(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;

        let Pty { master, slave } = self;
        let master_raw = master.as_raw_fd();
        let slave_raw = slave.as_raw_fd();

        // SAFETY: fork() is required for PTY process spawning. The child process
        // only calls async-signal-safe libc functions (setsid, ioctl, dup2, close,
        // chdir, signal, execvp, _exit) before exec replaces the process image.
        // All strings were allocated before fork.
        let result = unsafe { fork().map_err(PtyError::Create)? };

        match result {
            ForkResult::Child => {
                // SAFETY: In child process after fork. Only calling async-signal-safe
                // libc functions with pre-allocated C strings.
                unsafe {
                    // Create new session (detach from parent's controlling terminal)
                    libc::setsid();

                    // Set the slave as the controlling terminal
                    libc::ioctl(slave_raw, libc::TIOCSCTTY, 0);

                    // Redirect stdin/stdout/stderr to the slave PTY
                    libc::dup2(slave_raw, libc::STDIN_FILENO);
                    libc::dup2(slave_raw, libc::STDOUT_FILENO);
                    libc::dup2(slave_raw, libc::STDERR_FILENO);

                    // Close the original fds if they're not 0/1/2
                    if slave_raw > 2 {
                        libc::close(slave_raw);
                    }
                    if master_raw > 2 {
                        libc::close(master_raw);
                    }

                    // Change to the working directory
                    libc::chdir(cwd_cstr.as_ptr());

                    // Reset signal handlers to defaults
                    libc::signal(libc::SIGPIPE, libc::SIG_DFL);
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    libc::signal(libc::SIGTERM, libc::SIG_DFL);
                    libc::signal(libc::SIGHUP, libc::SIG_DFL);

                    // Exec the shell as a login shell
                    let argv: [*const libc::c_char; 2] =
                        [login_cstr.as_ptr(), std::ptr::null()];
                    libc::execvp(shell_cstr.as_ptr(), argv.as_ptr());

                    // execvp only returns on failure
                    libc::_exit(127);
                }
            }
            ForkResult::Parent { child } => {
                // Close slave fd in parent (child has its own copy from fork)
                drop(slave);

                Ok(SpawnedProcess { master, pid: child })
            }
        }
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

/// Get the default shell from $SHELL or fall back to /bin/sh.
#[must_use]
pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
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

    #[test]
    fn spawn_echo() {
        let pty = Pty::open(80, 24).unwrap();
        let result = pty.spawn_shell("/bin/echo", "/tmp");
        assert!(result.is_ok(), "spawn failed: {result:?}");
        let spawned = result.unwrap();
        assert!(spawned.pid.as_raw() > 0);

        // Wait a bit for the process to produce output
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Read output from PTY master
        let mut buf = [0u8; 256];
        let n = nix::unistd::read(spawned.master.as_raw_fd(), &mut buf);
        assert!(n.is_ok(), "read failed: {n:?}");

        // Wait for child to finish
        use nix::sys::wait::{waitpid, WaitPidFlag};
        waitpid(spawned.pid, Some(WaitPidFlag::WNOHANG)).ok();
    }

    #[test]
    fn spawn_shell_and_exit() {
        let pty = Pty::open(80, 24).unwrap();
        let spawned = pty.spawn_shell("/bin/sh", "/tmp").unwrap();

        // Write exit command to the shell
        nix::unistd::write(&spawned.master, b"exit\n").ok();

        // Wait for child to exit
        std::thread::sleep(std::time::Duration::from_millis(200));

        use nix::sys::wait::{waitpid, WaitPidFlag};
        let status = waitpid(spawned.pid, Some(WaitPidFlag::WNOHANG));
        assert!(status.is_ok());
    }

    #[test]
    fn default_shell_not_empty() {
        let shell = default_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn spawned_process_resize() {
        let pty = Pty::open(80, 24).unwrap();
        let spawned = pty.spawn_shell("/bin/sh", "/tmp").unwrap();
        assert!(spawned.resize(120, 40).is_ok());

        // Clean up
        nix::unistd::write(&spawned.master, b"exit\n").ok();
        std::thread::sleep(std::time::Duration::from_millis(100));
        use nix::sys::wait::{waitpid, WaitPidFlag};
        waitpid(spawned.pid, Some(WaitPidFlag::WNOHANG)).ok();
    }
}
