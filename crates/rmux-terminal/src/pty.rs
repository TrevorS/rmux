//! PTY (pseudo-terminal) operations.
//!
//! Creates and manages pseudo-terminals for pane processes.

use nix::libc;
use nix::pty::{OpenptyResult, Winsize, openpty};
use nix::unistd::{ForkResult, Pid, fork};
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
        let winsize = Winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
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
        let winsize = Winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };

        let OpenptyResult { master, slave } = openpty(&winsize, None)?;

        // openpty already returns OwnedFd, so just use them directly
        Ok(Self { master, slave })
    }

    /// Resize the PTY.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        let winsize = Winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
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

    /// Resize a PTY given its raw file descriptor.
    pub fn resize_fd(fd: i32, cols: u16, rows: u16) -> Result<(), PtyError> {
        let winsize = Winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
        // SAFETY: fd is a valid PTY master fd, winsize is a valid struct.
        unsafe {
            let ret = libc::ioctl(fd, libc::TIOCSWINSZ, &winsize);
            if ret < 0 {
                return Err(PtyError::Create(nix::Error::last()));
            }
        }
        Ok(())
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
                    libc::ioctl(slave_raw, libc::TIOCSCTTY as libc::c_ulong, 0);

                    // Redirect stdin/stdout/stderr to the slave PTY
                    libc::dup2(slave_raw, libc::STDIN_FILENO);
                    libc::dup2(slave_raw, libc::STDOUT_FILENO);
                    libc::dup2(slave_raw, libc::STDERR_FILENO);

                    // Close all fds > 2. This is critical: without it, other
                    // panes' master fds leak into this child, preventing EOF
                    // when siblings exit (their slave fds stay open here).
                    // This matches tmux's use of closefrom(STDERR_FILENO + 1).
                    close_fds_above(libc::STDERR_FILENO);

                    // Change to the working directory
                    libc::chdir(cwd_cstr.as_ptr());

                    // Reset signal handlers to defaults
                    libc::signal(libc::SIGPIPE, libc::SIG_DFL);
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    libc::signal(libc::SIGTERM, libc::SIG_DFL);
                    libc::signal(libc::SIGHUP, libc::SIG_DFL);

                    // Exec the shell as a login shell
                    let argv: [*const libc::c_char; 2] = [login_cstr.as_ptr(), std::ptr::null()];
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

    /// Spawn a command in this PTY via `shell -c "command"`.
    ///
    /// Like `spawn_shell`, but runs a specific command instead of an interactive shell.
    /// When the command exits, the pane becomes dead (EOF on master fd).
    pub fn spawn_command(
        self,
        shell: &str,
        cwd: &str,
        command: &str,
    ) -> Result<SpawnedProcess, PtyError> {
        // Prepare C strings before fork (allocation is not async-signal-safe).
        let shell_cstr = CString::new(shell).map_err(|e| {
            PtyError::Spawn(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;
        let cwd_cstr = CString::new(cwd).map_err(|e| {
            PtyError::Spawn(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;
        let flag_cstr = CString::new("-c").map_err(|e| {
            PtyError::Spawn(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;
        let command_cstr = CString::new(command).map_err(|e| {
            PtyError::Spawn(std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
        })?;

        let Pty { master, slave } = self;
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
                    libc::ioctl(slave_raw, libc::TIOCSCTTY as libc::c_ulong, 0);

                    // Redirect stdin/stdout/stderr to the slave PTY
                    libc::dup2(slave_raw, libc::STDIN_FILENO);
                    libc::dup2(slave_raw, libc::STDOUT_FILENO);
                    libc::dup2(slave_raw, libc::STDERR_FILENO);

                    // Close all fds > 2. This is critical: without it, other
                    // panes' master fds leak into this child, preventing EOF
                    // when siblings exit (their slave fds stay open here).
                    // This matches tmux's use of closefrom(STDERR_FILENO + 1).
                    close_fds_above(libc::STDERR_FILENO);

                    // Change to the working directory
                    libc::chdir(cwd_cstr.as_ptr());

                    // Reset signal handlers to defaults
                    libc::signal(libc::SIGPIPE, libc::SIG_DFL);
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    libc::signal(libc::SIGTERM, libc::SIG_DFL);
                    libc::signal(libc::SIGHUP, libc::SIG_DFL);

                    // Exec the command via shell -c "command"
                    let argv: [*const libc::c_char; 4] = [
                        shell_cstr.as_ptr(),
                        flag_cstr.as_ptr(),
                        command_cstr.as_ptr(),
                        std::ptr::null(),
                    ];
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

/// Close all file descriptors above `lowfd`.
///
/// Must only be called in a forked child before exec (async-signal-safe).
///
/// SAFETY: Caller must ensure this is called after fork in the child process.
/// Uses only async-signal-safe libc functions.
/// SAFETY: Must only be called in a forked child before exec.
/// Uses only async-signal-safe libc functions.
unsafe fn close_fds_above(lowfd: i32) {
    let from = lowfd + 1;
    let max_fd = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
    let max_fd = if max_fd > 0 { max_fd as i32 } else { 1024 };
    for fd in from..max_fd {
        unsafe { libc::close(fd) };
    }
}

/// Set a file descriptor to non-blocking mode.
pub fn set_nonblocking(fd: i32) -> Result<(), PtyError> {
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    use std::os::fd::BorrowedFd;
    // SAFETY: Caller guarantees fd is a valid, open file descriptor.
    let borrowed = unsafe { BorrowedFd::borrow_raw(fd) };
    let flags = fcntl(borrowed, FcntlArg::F_GETFL).map_err(PtyError::Create)?;
    let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
    fcntl(borrowed, FcntlArg::F_SETFL(flags)).map_err(PtyError::Create)?;
    Ok(())
}

/// Get the foreground process name for a PTY file descriptor.
///
/// Uses `tcgetpgrp()` to find the foreground process group, then looks up
/// the process name. Returns `None` if any step fails.
#[must_use]
pub fn foreground_process_name(fd: i32) -> Option<String> {
    // SAFETY: tcgetpgrp is a POSIX function that returns the foreground
    // process group ID for the terminal associated with fd.
    let pgrp = unsafe { libc::tcgetpgrp(fd) };
    if pgrp <= 0 {
        return None;
    }
    process_name(pgrp)
}

/// Get the name of a process by PID using platform-specific APIs.
#[cfg(target_os = "macos")]
fn process_name(pid: libc::pid_t) -> Option<String> {
    // SAFETY: proc_name is a macOS libproc function that writes the process
    // name into a caller-provided buffer. We pass a valid buffer and check
    // the return value (0 = failure, >0 = bytes written).
    unsafe {
        unsafe extern "C" {
            fn proc_name(pid: libc::c_int, buffer: *mut libc::c_char, buffersize: u32) -> i32;
        }
        let mut buf = [0u8; 256];
        let ret = proc_name(pid, buf.as_mut_ptr().cast(), buf.len() as u32);
        if ret <= 0 {
            return None;
        }
        let name = &buf[..ret as usize];
        String::from_utf8(name.to_vec()).ok()
    }
}

/// Get the name of a process by PID using /proc on Linux.
#[cfg(target_os = "linux")]
fn process_name(pid: libc::pid_t) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_string())
}

/// Get the PTY device name (e.g. "/dev/ttys042") for a master fd.
///
/// Uses `ptsname()` on macOS and `/proc/self/fd` readlink on Linux.
#[must_use]
pub fn pty_device_name(master_fd: i32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: ptsname is a POSIX function that returns a pointer to a static
        // string containing the slave device name. The pointer is valid until the
        // next call to ptsname. We immediately copy into a String.
        let ptr = unsafe { libc::ptsname(master_fd) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: ptsname returned a valid C string pointer.
        let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
        cstr.to_str().ok().map(String::from)
    }
    #[cfg(target_os = "linux")]
    {
        // On Linux, use /proc/self/fd/N to find the slave device
        std::fs::read_link(format!("/proc/self/fd/{master_fd}"))
            .ok()
            .and_then(|p| p.to_str().map(String::from))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = master_fd;
        None
    }
}

/// Get the default shell from $SHELL or fall back to /bin/sh.
#[must_use]
pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::wait::{WaitPidFlag, waitpid};
    use std::os::fd::AsRawFd;

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
        let n = nix::unistd::read(&spawned.master, &mut buf);
        assert!(n.is_ok(), "read failed: {n:?}");

        // Wait for child to finish
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

        let status = waitpid(spawned.pid, Some(WaitPidFlag::WNOHANG));
        assert!(status.is_ok());
    }

    #[test]
    fn default_shell_not_empty() {
        let shell = default_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn foreground_process_name_invalid_fd() {
        // Negative fd should return None
        assert!(foreground_process_name(-1).is_none());
        // Unlikely-to-be-valid fd should return None
        assert!(foreground_process_name(9999).is_none());
    }

    #[test]
    fn foreground_process_name_non_pty() {
        // A regular file fd (not a PTY) should return None
        use std::fs::File;
        let f = File::open("/dev/null").unwrap();
        let fd = f.as_raw_fd();
        assert!(foreground_process_name(fd).is_none());
    }

    #[test]
    fn foreground_process_name_spawned_shell() {
        // Spawn a shell and verify we can get its process name
        let pty = Pty::open(80, 24).unwrap();
        let spawned = pty.spawn_shell("/bin/sh", "/tmp").unwrap();

        // Give the shell a moment to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        let name = foreground_process_name(spawned.master_fd());
        // Should get "sh" or similar
        assert!(name.is_some(), "should resolve foreground process name for PTY");
        let name = name.unwrap();
        assert!(!name.is_empty(), "process name should not be empty");

        // Clean up
        nix::unistd::write(&spawned.master, b"exit\n").ok();
        std::thread::sleep(std::time::Duration::from_millis(100));
        waitpid(spawned.pid, Some(WaitPidFlag::WNOHANG)).ok();
    }

    #[test]
    fn process_name_current_process() {
        // Look up the current test process
        let pid = std::process::id() as libc::pid_t;
        let name = process_name(pid);
        assert!(name.is_some(), "should resolve current process name");
        let name = name.unwrap();
        assert!(!name.is_empty(), "process name should not be empty");
    }

    #[test]
    fn process_name_invalid_pid() {
        assert!(process_name(-1).is_none());
        // PID 0 is kernel on most systems, but might not be accessible
        // Very large PID should not exist
        assert!(process_name(999_999_999).is_none());
    }

    #[test]
    fn pty_device_name_valid_pty() {
        let pty = Pty::open(80, 24).unwrap();
        let name = pty_device_name(pty.master_fd());
        assert!(name.is_some(), "should resolve PTY device name");
        let name = name.unwrap();
        assert!(name.starts_with("/dev/"), "PTY name should be a /dev path: {name}");
    }

    #[test]
    fn pty_device_name_invalid_fd() {
        assert!(pty_device_name(-1).is_none());
        assert!(pty_device_name(9999).is_none());
    }

    #[test]
    fn spawn_command_runs_and_exits() {
        let pty = Pty::open(80, 24).unwrap();
        let spawned = pty.spawn_command("/bin/sh", "/tmp", "echo hello").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let status = waitpid(spawned.pid, Some(WaitPidFlag::WNOHANG));
        assert!(status.is_ok());
    }

    #[test]
    fn spawned_process_resize() {
        let pty = Pty::open(80, 24).unwrap();
        let spawned = pty.spawn_shell("/bin/sh", "/tmp").unwrap();
        assert!(spawned.resize(120, 40).is_ok());

        // Clean up
        nix::unistd::write(&spawned.master, b"exit\n").ok();
        std::thread::sleep(std::time::Duration::from_millis(100));
        waitpid(spawned.pid, Some(WaitPidFlag::WNOHANG)).ok();
    }
}
