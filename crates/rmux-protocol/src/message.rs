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
    /// Protocol version exchange.
    ///
    /// tmux 3.4+ sends a null-terminated version string (e.g. "3.6a").
    /// Older tmux sends a 4-byte integer (protocol version number).
    /// We store the string form for maximum compatibility.
    Version {
        version: String,
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

    #[test]
    fn all_message_type_raw_values_unique() {
        let all_variants: Vec<(u32, &str)> = vec![
            (MessageType::Version as u32, "Version"),
            (MessageType::IdentifyFlags as u32, "IdentifyFlags"),
            (MessageType::IdentifyTerm as u32, "IdentifyTerm"),
            (MessageType::IdentifyTtyName as u32, "IdentifyTtyName"),
            (MessageType::IdentifyOldCwd as u32, "IdentifyOldCwd"),
            (MessageType::IdentifyStdin as u32, "IdentifyStdin"),
            (MessageType::IdentifyEnviron as u32, "IdentifyEnviron"),
            (MessageType::IdentifyDone as u32, "IdentifyDone"),
            (MessageType::IdentifyClientPid as u32, "IdentifyClientPid"),
            (MessageType::IdentifyCwd as u32, "IdentifyCwd"),
            (MessageType::IdentifyFeatures as u32, "IdentifyFeatures"),
            (MessageType::IdentifyStdout as u32, "IdentifyStdout"),
            (MessageType::IdentifyLongFlags as u32, "IdentifyLongFlags"),
            (MessageType::IdentifyTerminfo as u32, "IdentifyTerminfo"),
            (MessageType::Command as u32, "Command"),
            (MessageType::Detach as u32, "Detach"),
            (MessageType::DetachKill as u32, "DetachKill"),
            (MessageType::Exit as u32, "Exit"),
            (MessageType::Exited as u32, "Exited"),
            (MessageType::Exiting as u32, "Exiting"),
            (MessageType::Lock as u32, "Lock"),
            (MessageType::Ready as u32, "Ready"),
            (MessageType::Resize as u32, "Resize"),
            (MessageType::Shell as u32, "Shell"),
            (MessageType::Shutdown as u32, "Shutdown"),
            (MessageType::OldStderr as u32, "OldStderr"),
            (MessageType::OldStdin as u32, "OldStdin"),
            (MessageType::OldStdout as u32, "OldStdout"),
            (MessageType::Suspend as u32, "Suspend"),
            (MessageType::Unlock as u32, "Unlock"),
            (MessageType::Wakeup as u32, "Wakeup"),
            (MessageType::Exec as u32, "Exec"),
            (MessageType::Flags as u32, "Flags"),
            (MessageType::ReadOpen as u32, "ReadOpen"),
            (MessageType::Read as u32, "Read"),
            (MessageType::ReadDone as u32, "ReadDone"),
            (MessageType::WriteOpen as u32, "WriteOpen"),
            (MessageType::Write as u32, "Write"),
            (MessageType::WriteReady as u32, "WriteReady"),
            (MessageType::WriteClose as u32, "WriteClose"),
            (MessageType::ReadCancel as u32, "ReadCancel"),
            (MessageType::OutputData as u32, "OutputData"),
            (MessageType::InputData as u32, "InputData"),
            (MessageType::ErrorOutput as u32, "ErrorOutput"),
        ];
        let mut seen = std::collections::HashMap::new();
        for (raw, name) in &all_variants {
            if let Some(existing) = seen.insert(raw, name) {
                panic!("Duplicate raw value {raw}: {existing} and {name}");
            }
        }
    }

    #[test]
    fn message_type_from_raw_all_known() {
        // Test every variant's raw value maps back correctly via from_raw
        let mappings: Vec<(u32, MessageType)> = vec![
            (12, MessageType::Version),
            (100, MessageType::IdentifyFlags),
            (101, MessageType::IdentifyTerm),
            (102, MessageType::IdentifyTtyName),
            (103, MessageType::IdentifyOldCwd),
            (104, MessageType::IdentifyStdin),
            (105, MessageType::IdentifyEnviron),
            (106, MessageType::IdentifyDone),
            (107, MessageType::IdentifyClientPid),
            (108, MessageType::IdentifyCwd),
            (109, MessageType::IdentifyFeatures),
            (110, MessageType::IdentifyStdout),
            (111, MessageType::IdentifyLongFlags),
            (112, MessageType::IdentifyTerminfo),
            (200, MessageType::Command),
            (201, MessageType::Detach),
            (202, MessageType::DetachKill),
            (203, MessageType::Exit),
            (204, MessageType::Exited),
            (205, MessageType::Exiting),
            (206, MessageType::Lock),
            (207, MessageType::Ready),
            (208, MessageType::Resize),
            (209, MessageType::Shell),
            (210, MessageType::Shutdown),
            (211, MessageType::OldStderr),
            (212, MessageType::OldStdin),
            (213, MessageType::OldStdout),
            (214, MessageType::Suspend),
            (215, MessageType::Unlock),
            (216, MessageType::Wakeup),
            (217, MessageType::Exec),
            (218, MessageType::Flags),
            (300, MessageType::ReadOpen),
            (301, MessageType::Read),
            (302, MessageType::ReadDone),
            (303, MessageType::WriteOpen),
            (304, MessageType::Write),
            (305, MessageType::WriteReady),
            (306, MessageType::WriteClose),
            (307, MessageType::ReadCancel),
            (400, MessageType::OutputData),
            (401, MessageType::InputData),
            (402, MessageType::ErrorOutput),
        ];
        for (raw, expected) in mappings {
            assert_eq!(
                MessageType::from_raw(raw),
                Some(expected),
                "from_raw({raw}) should return {expected:?}"
            );
        }
    }

    #[test]
    fn message_type_from_raw_unknown() {
        // Values that should not map to any known variant
        let unknown_values: Vec<u32> = vec![
            0,
            1,
            11,
            13,
            50,
            99,
            113,
            150,
            199,
            219,
            250,
            299,
            308,
            350,
            399,
            403,
            500,
            1000,
            u32::MAX,
        ];
        for val in unknown_values {
            assert_eq!(MessageType::from_raw(val), None, "from_raw({val}) should return None");
        }
    }

    #[test]
    fn all_message_types_from_raw_roundtrip() {
        // Verify that casting each variant to u32 and back via from_raw recovers the variant
        let variants: &[(MessageType, u32)] = &[
            (MessageType::Version, 12),
            (MessageType::IdentifyFlags, 100),
            (MessageType::IdentifyTerm, 101),
            (MessageType::IdentifyTtyName, 102),
            (MessageType::IdentifyOldCwd, 103),
            (MessageType::IdentifyStdin, 104),
            (MessageType::IdentifyEnviron, 105),
            (MessageType::IdentifyDone, 106),
            (MessageType::IdentifyClientPid, 107),
            (MessageType::IdentifyCwd, 108),
            (MessageType::IdentifyFeatures, 109),
            (MessageType::IdentifyStdout, 110),
            (MessageType::IdentifyLongFlags, 111),
            (MessageType::IdentifyTerminfo, 112),
            (MessageType::Command, 200),
            (MessageType::Detach, 201),
            (MessageType::DetachKill, 202),
            (MessageType::Exit, 203),
            (MessageType::Exited, 204),
            (MessageType::Exiting, 205),
            (MessageType::Lock, 206),
            (MessageType::Ready, 207),
            (MessageType::Resize, 208),
            (MessageType::Shell, 209),
            (MessageType::Shutdown, 210),
            (MessageType::OldStderr, 211),
            (MessageType::OldStdin, 212),
            (MessageType::OldStdout, 213),
            (MessageType::Suspend, 214),
            (MessageType::Unlock, 215),
            (MessageType::Wakeup, 216),
            (MessageType::Exec, 217),
            (MessageType::Flags, 218),
            (MessageType::ReadOpen, 300),
            (MessageType::Read, 301),
            (MessageType::ReadDone, 302),
            (MessageType::WriteOpen, 303),
            (MessageType::Write, 304),
            (MessageType::WriteReady, 305),
            (MessageType::WriteClose, 306),
            (MessageType::ReadCancel, 307),
            (MessageType::OutputData, 400),
            (MessageType::InputData, 401),
            (MessageType::ErrorOutput, 402),
        ];
        for &(variant, raw) in variants {
            // Verify the enum discriminant matches the expected raw value
            assert_eq!(variant as u32, raw, "{variant:?} as u32 should be {raw}");
            // Verify from_raw roundtrip
            assert_eq!(
                MessageType::from_raw(variant as u32),
                Some(variant),
                "from_raw({raw}) roundtrip failed for {variant:?}"
            );
        }
    }

    #[test]
    fn unknown_raw_returns_none() {
        assert_eq!(MessageType::from_raw(0), None);
        assert_eq!(MessageType::from_raw(1), None);
        assert_eq!(MessageType::from_raw(13), None);
        assert_eq!(MessageType::from_raw(99), None);
        assert_eq!(MessageType::from_raw(113), None);
        assert_eq!(MessageType::from_raw(199), None);
        assert_eq!(MessageType::from_raw(219), None);
        assert_eq!(MessageType::from_raw(299), None);
        assert_eq!(MessageType::from_raw(308), None);
        assert_eq!(MessageType::from_raw(399), None);
        assert_eq!(MessageType::from_raw(403), None);
        assert_eq!(MessageType::from_raw(u32::MAX), None);
    }

    #[test]
    fn protocol_version_is_8() {
        assert_eq!(PROTOCOL_VERSION, 8);
    }
}
