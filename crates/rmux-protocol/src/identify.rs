//! Client identification handshake.
//!
//! When a client connects, it sends a series of IDENTIFY messages to tell the
//! server about its terminal type, capabilities, environment, and flags.

use crate::message::Message;

/// Client identification flags (matching tmux's IDENTIFY_* in tmux.h).
pub mod flags {
    /// Client supports 256 colors.
    pub const IDENTIFY_256COLOURS: i64 = 0x1;
    /// Client requests control mode.
    pub const IDENTIFY_CONTROL: i64 = 0x2;
    /// Terminal type override.
    pub const IDENTIFY_TERMINALOVERRIDES: i64 = 0x4;
    /// Client supports UTF-8.
    pub const IDENTIFY_UTF8: i64 = 0x8;
    /// Client wants terminal features.
    pub const IDENTIFY_FEATURES: i64 = 0x10;
    /// Client is in a nested tmux session.
    pub const IDENTIFY_NESTED: i64 = 0x20;
}

/// Build the sequence of identify messages a client sends on connection.
pub fn build_identify_sequence(
    client_flags: i64,
    term: &str,
    ttyname: &str,
    cwd: &str,
    pid: i32,
    environ: &[(String, String)],
) -> Vec<Message> {
    let mut msgs = vec![
        Message::IdentifyLongFlags(client_flags),
        Message::IdentifyTerm(term.to_string()),
        Message::IdentifyTtyName(ttyname.to_string()),
        Message::IdentifyCwd(cwd.to_string()),
        Message::IdentifyClientPid(pid),
    ];

    // Environment variables
    for (key, value) in environ {
        msgs.push(Message::IdentifyEnviron(format!("{key}={value}")));
    }

    // Done
    msgs.push(Message::IdentifyDone);

    msgs
}

/// State machine for processing incoming identify messages on the server side.
#[derive(Debug, Default)]
pub struct IdentifyState {
    /// Client flags.
    pub flags: i64,
    /// Terminal type.
    pub term: String,
    /// TTY name.
    pub ttyname: String,
    /// Current working directory.
    pub cwd: String,
    /// Client PID.
    pub pid: i32,
    /// Environment variables.
    pub environ: Vec<(String, String)>,
    /// Terminal features.
    pub features: String,
    /// Whether identification is complete.
    pub done: bool,
}

impl IdentifyState {
    /// Process an identify message. Returns true when identification is complete.
    pub fn process(&mut self, msg: &Message) -> bool {
        match msg {
            Message::IdentifyFlags(f) => self.flags = *f,
            Message::IdentifyLongFlags(f) => self.flags = *f,
            Message::IdentifyTerm(t) => self.term.clone_from(t),
            Message::IdentifyTtyName(n) => self.ttyname.clone_from(n),
            Message::IdentifyCwd(c) => self.cwd.clone_from(c),
            Message::IdentifyClientPid(p) => self.pid = *p,
            Message::IdentifyEnviron(e) => {
                if let Some((key, value)) = e.split_once('=') {
                    self.environ.push((key.to_string(), value.to_string()));
                }
            }
            Message::IdentifyFeatures(f) => self.features.clone_from(f),
            Message::IdentifyDone => {
                self.done = true;
                return true;
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_process_identify() {
        let msgs = build_identify_sequence(
            flags::IDENTIFY_UTF8 | flags::IDENTIFY_256COLOURS,
            "xterm-256color",
            "/dev/pts/0",
            "/home/user",
            1234,
            &[("TERM".to_string(), "xterm".to_string())],
        );

        let mut state = IdentifyState::default();
        for msg in &msgs {
            state.process(msg);
        }

        assert!(state.done);
        assert_eq!(state.term, "xterm-256color");
        assert_eq!(state.ttyname, "/dev/pts/0");
        assert_eq!(state.cwd, "/home/user");
        assert_eq!(state.pid, 1234);
        assert_eq!(state.environ.len(), 1);
        assert_eq!(state.environ[0], ("TERM".to_string(), "xterm".to_string()));
    }
}
