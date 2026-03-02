//! Protocol message types.
//!
//! Exactly matching tmux's `tmux-protocol.h` (PROTOCOL_VERSION = 8).

/// Protocol version. Must match the tmux server we're connecting to.
pub const PROTOCOL_VERSION: u32 = 8;

/// Message types matching tmux's `enum msgtype`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum MessageType {
    /// Protocol version exchange.
    Version = 12,

    // Identification messages (client → server during handshake)
    /// Client flags (int).
    IdentifyFlags = 100,
    /// Terminal type (string).
    IdentifyTerm = 101,
    /// TTY name (string).
    IdentifyTtyName = 102,
    /// Old CWD (unused).
    IdentifyOldCwd = 103,
    /// Stdin fd.
    IdentifyStdin = 104,
    /// Environment variable (string: KEY=VALUE).
    IdentifyEnviron = 105,
    /// Identification complete.
    IdentifyDone = 106,
    /// Client PID (pid_t).
    IdentifyClientPid = 107,
    /// Current working directory (string).
    IdentifyCwd = 108,
    /// Terminal features (string).
    IdentifyFeatures = 109,
    /// Stdout fd.
    IdentifyStdout = 110,
    /// Long flags (int64).
    IdentifyLongFlags = 111,
    /// Terminfo entries (binary).
    IdentifyTerminfo = 112,

    // Command/control messages
    /// Execute command (followed by packed argv).
    Command = 200,
    /// Detach client.
    Detach = 201,
    /// Detach and kill.
    DetachKill = 202,
    /// Exit.
    Exit = 203,
    /// Client has exited.
    Exited = 204,
    /// Client is exiting.
    Exiting = 205,
    /// Lock client.
    Lock = 206,
    /// Server is ready.
    Ready = 207,
    /// Terminal resize.
    Resize = 208,
    /// Shell command.
    Shell = 209,
    /// Shutdown server.
    Shutdown = 210,
    /// Old stderr (unused).
    OldStderr = 211,
    /// Old stdin (unused).
    OldStdin = 212,
    /// Old stdout (unused).
    OldStdout = 213,
    /// Suspend client.
    Suspend = 214,
    /// Unlock client.
    Unlock = 215,
    /// Wake up client.
    Wakeup = 216,
    /// Execute program.
    Exec = 217,
    /// Client flags update.
    Flags = 218,

    // File I/O messages
    /// Open file for reading.
    ReadOpen = 300,
    /// Read data.
    Read = 301,
    /// Read complete.
    ReadDone = 302,
    /// Open file for writing.
    WriteOpen = 303,
    /// Write data.
    Write = 304,
    /// Write ready.
    WriteReady = 305,
    /// Close write stream.
    WriteClose = 306,
    /// Cancel read.
    ReadCancel = 307,

    // rmux extensions (400+)
    /// Terminal output data from server to attached client.
    OutputData = 400,
    /// Keyboard/input data from attached client to server.
    InputData = 401,
    /// Error output from server (written to stderr by client).
    ErrorOutput = 402,
}

impl MessageType {
    /// Convert from raw u32 value.
    #[must_use]
    pub fn from_raw(val: u32) -> Option<Self> {
        match val {
            12 => Some(Self::Version),
            100 => Some(Self::IdentifyFlags),
            101 => Some(Self::IdentifyTerm),
            102 => Some(Self::IdentifyTtyName),
            103 => Some(Self::IdentifyOldCwd),
            104 => Some(Self::IdentifyStdin),
            105 => Some(Self::IdentifyEnviron),
            106 => Some(Self::IdentifyDone),
            107 => Some(Self::IdentifyClientPid),
            108 => Some(Self::IdentifyCwd),
            109 => Some(Self::IdentifyFeatures),
            110 => Some(Self::IdentifyStdout),
            111 => Some(Self::IdentifyLongFlags),
            112 => Some(Self::IdentifyTerminfo),
            200 => Some(Self::Command),
            201 => Some(Self::Detach),
            202 => Some(Self::DetachKill),
            203 => Some(Self::Exit),
            204 => Some(Self::Exited),
            205 => Some(Self::Exiting),
            206 => Some(Self::Lock),
            207 => Some(Self::Ready),
            208 => Some(Self::Resize),
            209 => Some(Self::Shell),
            210 => Some(Self::Shutdown),
            211 => Some(Self::OldStderr),
            212 => Some(Self::OldStdin),
            213 => Some(Self::OldStdout),
            214 => Some(Self::Suspend),
            215 => Some(Self::Unlock),
            216 => Some(Self::Wakeup),
            217 => Some(Self::Exec),
            218 => Some(Self::Flags),
            300 => Some(Self::ReadOpen),
            301 => Some(Self::Read),
            302 => Some(Self::ReadDone),
            303 => Some(Self::WriteOpen),
            304 => Some(Self::Write),
            305 => Some(Self::WriteReady),
            306 => Some(Self::WriteClose),
            307 => Some(Self::ReadCancel),
            400 => Some(Self::OutputData),
            401 => Some(Self::InputData),
            402 => Some(Self::ErrorOutput),
            _ => None,
        }
    }
}

/// Command message data (MSG_COMMAND).
#[derive(Debug, Clone)]
pub struct MsgCommand {
    /// Number of arguments.
    pub argc: i32,
    /// Packed argument strings (null-separated).
    pub argv: Vec<String>,
}

/// Read-open message (MSG_READ_OPEN).
#[derive(Debug, Clone)]
pub struct MsgReadOpen {
    pub stream: i32,
    pub fd: i32,
    pub path: String,
}

/// Read data message (MSG_READ).
#[derive(Debug, Clone)]
pub struct MsgReadData {
    pub stream: i32,
}

/// Read done message (MSG_READ_DONE).
#[derive(Debug, Clone)]
pub struct MsgReadDone {
    pub stream: i32,
    pub error: i32,
}

/// Read cancel message (MSG_READ_CANCEL).
#[derive(Debug, Clone)]
pub struct MsgReadCancel {
    pub stream: i32,
}

/// Write-open message (MSG_WRITE_OPEN).
#[derive(Debug, Clone)]
pub struct MsgWriteOpen {
    pub stream: i32,
    pub fd: i32,
    pub flags: i32,
    pub path: String,
}

/// Write data message (MSG_WRITE).
#[derive(Debug, Clone)]
pub struct MsgWriteData {
    pub stream: i32,
    pub data: Vec<u8>,
}

/// Write ready message (MSG_WRITE_READY).
#[derive(Debug, Clone)]
pub struct MsgWriteReady {
    pub stream: i32,
    pub error: i32,
}

/// Write close message (MSG_WRITE_CLOSE).
#[derive(Debug, Clone)]
pub struct MsgWriteClose {
    pub stream: i32,
}

/// A decoded protocol message.
#[derive(Debug, Clone)]
pub enum Message {
    Version {
        version: u32,
    },
    IdentifyFlags(i64),
    IdentifyTerm(String),
    IdentifyTtyName(String),
    IdentifyStdin,
    IdentifyStdout,
    IdentifyEnviron(String),
    IdentifyDone,
    IdentifyClientPid(i32),
    IdentifyCwd(String),
    IdentifyFeatures(String),
    IdentifyLongFlags(i64),
    IdentifyTerminfo(Vec<u8>),
    Command(MsgCommand),
    Detach,
    DetachKill,
    Exit,
    Exited,
    Exiting,
    Lock,
    Ready,
    Resize {
        sx: u32,
        sy: u32,
        xpixel: u32,
        ypixel: u32,
    },
    Shell(String),
    Shutdown,
    Suspend,
    Unlock,
    Wakeup,
    Exec(String),
    Flags(i64),
    ReadOpen(MsgReadOpen),
    Read(MsgReadData),
    ReadDone(MsgReadDone),
    ReadCancel(MsgReadCancel),
    WriteOpen(MsgWriteOpen),
    Write(MsgWriteData),
    WriteReady(MsgWriteReady),
    WriteClose(MsgWriteClose),
    /// Terminal output data from server to attached client.
    OutputData(Vec<u8>),
    /// Keyboard/input data from attached client to server.
    InputData(Vec<u8>),
    /// Error output from server (written to stderr by client).
    ErrorOutput(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_type_from_raw() {
        assert_eq!(MessageType::from_raw(12), Some(MessageType::Version));
        assert_eq!(MessageType::from_raw(200), Some(MessageType::Command));
        assert_eq!(MessageType::from_raw(300), Some(MessageType::ReadOpen));
        assert_eq!(MessageType::from_raw(999), None);
    }

    #[test]
    fn message_type_values_match_tmux() {
        // Verify critical values match tmux-protocol.h exactly
        assert_eq!(MessageType::Version as u32, 12);
        assert_eq!(MessageType::IdentifyFlags as u32, 100);
        assert_eq!(MessageType::IdentifyDone as u32, 106);
        assert_eq!(MessageType::Command as u32, 200);
        assert_eq!(MessageType::Ready as u32, 207);
        assert_eq!(MessageType::ReadOpen as u32, 300);
    }
}
