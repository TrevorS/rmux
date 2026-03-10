//! imsg-compatible message encoding and decoding.
//!
//! tmux uses OpenBSD's imsg protocol over Unix domain sockets. Each message has a
//! fixed-size header followed by variable-length data.
//!
//! TODO: File descriptor passing via SCM_RIGHTS ancillary data is not yet implemented.

use crate::message::{Message, MessageType, MsgCommand};
use bytes::{BufMut, BytesMut};

/// imsg header size (matching OpenBSD imsg.h).
/// struct imsg_hdr { u_int32_t type; u_int16_t len; u_int16_t flags; u_int32_t peerid; u_int32_t pid; }
pub const IMSG_HEADER_SIZE: usize = 16;

/// Maximum imsg data size.
pub const IMSG_MAX_DATA: usize = 16384;

/// Protocol errors.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("incomplete message: need {needed} bytes, have {have}")]
    Incomplete { needed: usize, have: usize },
    #[error("message too large: {size} bytes (max {IMSG_MAX_DATA})")]
    TooLarge { size: usize },
    #[error("unknown message type: {0}")]
    UnknownType(u32),
    #[error("deprecated message type: {0:?}")]
    DeprecatedType(MessageType),
    #[error("invalid message data for type {msg_type:?}")]
    InvalidData { msg_type: MessageType },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Encode a message into a buffer.
///
/// Returns the number of bytes written.
pub fn encode_message(msg: &Message, buf: &mut BytesMut) -> Result<usize, CodecError> {
    let msg_type = message_to_type(msg);

    // Reserve space for the header (we'll fill it after encoding data)
    let header_pos = buf.len();
    buf.put_bytes(0, IMSG_HEADER_SIZE);

    // Encode message data
    encode_message_data(msg, buf);

    let total_len = buf.len() - header_pos;
    if total_len - IMSG_HEADER_SIZE > IMSG_MAX_DATA {
        return Err(CodecError::TooLarge { size: total_len - IMSG_HEADER_SIZE });
    }

    // Fill in the header
    let header = &mut buf[header_pos..header_pos + IMSG_HEADER_SIZE];
    // type (u32 LE)
    header[0..4].copy_from_slice(&(msg_type as u32).to_le_bytes());
    // len (u16 LE)
    header[4..6].copy_from_slice(&(total_len as u16).to_le_bytes());
    // flags (u16 LE) - 0
    header[6..8].copy_from_slice(&0u16.to_le_bytes());
    // peerid (u32 LE) - 0
    header[8..12].copy_from_slice(&0u32.to_le_bytes());
    // pid (u32 LE) - our pid
    header[12..16].copy_from_slice(&(std::process::id()).to_le_bytes());

    Ok(total_len)
}

/// Decode a message from a buffer.
///
/// Returns `None` if the buffer doesn't contain a complete message.
pub fn decode_message(buf: &mut BytesMut) -> Result<Option<Message>, CodecError> {
    if buf.len() < IMSG_HEADER_SIZE {
        return Ok(None);
    }

    // Parse header
    let msg_type_raw = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let msg_len = u16::from_le_bytes([buf[4], buf[5]]) as usize;

    if msg_len < IMSG_HEADER_SIZE {
        return Err(match MessageType::from_raw(msg_type_raw) {
            Some(msg_type) => CodecError::InvalidData { msg_type },
            None => CodecError::UnknownType(msg_type_raw),
        });
    }

    if buf.len() < msg_len {
        return Ok(None); // Need more data
    }

    let msg_type =
        MessageType::from_raw(msg_type_raw).ok_or(CodecError::UnknownType(msg_type_raw))?;

    // Extract data portion
    let _ = buf.split_to(IMSG_HEADER_SIZE); // Skip header
    let data_len = msg_len - IMSG_HEADER_SIZE;
    let data = buf.split_to(data_len);

    decode_message_data(msg_type, &data).map(Some)
}

fn encode_message_data(msg: &Message, buf: &mut BytesMut) {
    match msg {
        Message::Version { version } => {
            buf.put_i32_le(*version as i32);
        }
        Message::IdentifyFlags(flags) => {
            buf.put_i64_le(*flags);
        }
        Message::IdentifyTerm(term) => {
            buf.put_slice(term.as_bytes());
            buf.put_u8(0);
        }
        Message::IdentifyTtyName(name) => {
            buf.put_slice(name.as_bytes());
            buf.put_u8(0);
        }
        Message::IdentifyEnviron(env) => {
            buf.put_slice(env.as_bytes());
            buf.put_u8(0);
        }
        Message::IdentifyDone | Message::IdentifyStdin | Message::IdentifyStdout => {}
        Message::IdentifyClientPid(pid) => {
            buf.put_i32_le(*pid);
        }
        Message::IdentifyCwd(cwd) => {
            buf.put_slice(cwd.as_bytes());
            buf.put_u8(0);
        }
        Message::IdentifyFeatures(features) => {
            buf.put_slice(features.as_bytes());
            buf.put_u8(0);
        }
        Message::IdentifyLongFlags(flags) => {
            buf.put_i64_le(*flags);
        }
        Message::IdentifyTerminfo(data) => {
            buf.put_slice(data);
        }
        Message::Command(cmd) => {
            buf.put_i32_le(cmd.argc);
            for arg in &cmd.argv {
                buf.put_slice(arg.as_bytes());
                buf.put_u8(0);
            }
        }
        Message::Resize { sx, sy, xpixel, ypixel } => {
            buf.put_u32_le(*sx);
            buf.put_u32_le(*sy);
            buf.put_u32_le(*xpixel);
            buf.put_u32_le(*ypixel);
        }
        Message::Flags(flags) => {
            buf.put_i64_le(*flags);
        }
        // Simple messages with no data
        Message::Detach
        | Message::DetachKill
        | Message::Exit
        | Message::Exited
        | Message::Exiting
        | Message::Lock
        | Message::Ready
        | Message::Shutdown
        | Message::Suspend
        | Message::Unlock
        | Message::Wakeup => {}
        Message::Shell(cmd) | Message::Exec(cmd) => {
            buf.put_slice(cmd.as_bytes());
            buf.put_u8(0);
        }
        Message::ReadOpen(m) => {
            buf.put_i32_le(m.stream);
            buf.put_i32_le(m.fd);
            buf.put_slice(m.path.as_bytes());
            buf.put_u8(0);
        }
        Message::Read(m) => {
            buf.put_i32_le(m.stream);
        }
        Message::ReadDone(m) => {
            buf.put_i32_le(m.stream);
            buf.put_i32_le(m.error);
        }
        Message::ReadCancel(m) => {
            buf.put_i32_le(m.stream);
        }
        Message::WriteOpen(m) => {
            buf.put_i32_le(m.stream);
            buf.put_i32_le(m.fd);
            buf.put_i32_le(m.flags);
            buf.put_slice(m.path.as_bytes());
            buf.put_u8(0);
        }
        Message::Write(m) => {
            buf.put_i32_le(m.stream);
            buf.put_slice(&m.data);
        }
        Message::WriteReady(m) => {
            buf.put_i32_le(m.stream);
            buf.put_i32_le(m.error);
        }
        Message::WriteClose(m) => {
            buf.put_i32_le(m.stream);
        }
        Message::OutputData(data) | Message::InputData(data) | Message::ErrorOutput(data) => {
            buf.put_slice(data);
        }
    }
}

fn decode_message_data(msg_type: MessageType, data: &[u8]) -> Result<Message, CodecError> {
    let err = || CodecError::InvalidData { msg_type };
    match msg_type {
        MessageType::Version => {
            if data.len() < 4 {
                return Err(err());
            }
            let version = i32::from_le_bytes([data[0], data[1], data[2], data[3]]) as u32;
            Ok(Message::Version { version })
        }
        MessageType::IdentifyFlags => {
            if data.len() < 8 {
                return Err(err());
            }
            let flags = i64::from_le_bytes(data[..8].try_into().map_err(|_| err())?);
            Ok(Message::IdentifyFlags(flags))
        }
        MessageType::IdentifyTerm => Ok(Message::IdentifyTerm(decode_cstring(data))),
        MessageType::IdentifyTtyName => Ok(Message::IdentifyTtyName(decode_cstring(data))),
        MessageType::IdentifyEnviron => Ok(Message::IdentifyEnviron(decode_cstring(data))),
        MessageType::IdentifyDone => Ok(Message::IdentifyDone),
        MessageType::IdentifyStdin => Ok(Message::IdentifyStdin),
        MessageType::IdentifyStdout => Ok(Message::IdentifyStdout),
        MessageType::IdentifyClientPid => {
            if data.len() < 4 {
                return Err(err());
            }
            let pid = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok(Message::IdentifyClientPid(pid))
        }
        MessageType::IdentifyCwd => Ok(Message::IdentifyCwd(decode_cstring(data))),
        MessageType::IdentifyFeatures => Ok(Message::IdentifyFeatures(decode_cstring(data))),
        MessageType::IdentifyLongFlags => {
            if data.len() < 8 {
                return Err(err());
            }
            let flags = i64::from_le_bytes(data[..8].try_into().map_err(|_| err())?);
            Ok(Message::IdentifyLongFlags(flags))
        }
        MessageType::IdentifyTerminfo => Ok(Message::IdentifyTerminfo(data.to_vec())),
        MessageType::Command => {
            if data.len() < 4 {
                return Err(err());
            }
            let argc = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if argc < 0 {
                return Err(err());
            }
            let argv_data = &data[4..];
            let argv: Vec<String> = argv_data
                .split(|&b| b == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect();
            if argv.len() != argc as usize {
                return Err(err());
            }
            Ok(Message::Command(MsgCommand { argc, argv }))
        }
        MessageType::Ready => Ok(Message::Ready),
        MessageType::Detach => Ok(Message::Detach),
        MessageType::DetachKill => Ok(Message::DetachKill),
        MessageType::Exit => Ok(Message::Exit),
        MessageType::Exited => Ok(Message::Exited),
        MessageType::Exiting => Ok(Message::Exiting),
        MessageType::Shutdown => Ok(Message::Shutdown),
        MessageType::Suspend => Ok(Message::Suspend),
        MessageType::Lock => Ok(Message::Lock),
        MessageType::Unlock => Ok(Message::Unlock),
        MessageType::Wakeup => Ok(Message::Wakeup),
        MessageType::Shell => Ok(Message::Shell(decode_cstring(data))),
        MessageType::Exec => Ok(Message::Exec(decode_cstring(data))),
        MessageType::Resize => {
            if data.len() < 16 {
                return Err(err());
            }
            Ok(Message::Resize {
                sx: u32::from_le_bytes(data[0..4].try_into().map_err(|_| err())?),
                sy: u32::from_le_bytes(data[4..8].try_into().map_err(|_| err())?),
                xpixel: u32::from_le_bytes(data[8..12].try_into().map_err(|_| err())?),
                ypixel: u32::from_le_bytes(data[12..16].try_into().map_err(|_| err())?),
            })
        }
        MessageType::Flags => {
            if data.len() < 8 {
                return Err(err());
            }
            let flags = i64::from_le_bytes(data[..8].try_into().map_err(|_| err())?);
            Ok(Message::Flags(flags))
        }
        // Deprecated message types — reject as protocol errors
        MessageType::IdentifyOldCwd
        | MessageType::OldStderr
        | MessageType::OldStdin
        | MessageType::OldStdout => Err(CodecError::DeprecatedType(msg_type)),
        MessageType::ReadOpen => {
            if data.len() < 8 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let fd = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let path = decode_cstring(&data[8..]);
            Ok(Message::ReadOpen(crate::message::MsgReadOpen { stream, fd, path }))
        }
        MessageType::Read => {
            if data.len() < 4 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok(Message::Read(crate::message::MsgReadData { stream }))
        }
        MessageType::ReadDone => {
            if data.len() < 8 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let error = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            Ok(Message::ReadDone(crate::message::MsgReadDone { stream, error }))
        }
        MessageType::ReadCancel => {
            if data.len() < 4 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok(Message::ReadCancel(crate::message::MsgReadCancel { stream }))
        }
        MessageType::WriteOpen => {
            if data.len() < 12 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let fd = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let flags = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
            let path = decode_cstring(&data[12..]);
            Ok(Message::WriteOpen(crate::message::MsgWriteOpen { stream, fd, flags, path }))
        }
        MessageType::Write => {
            if data.len() < 4 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let write_data = data[4..].to_vec();
            Ok(Message::Write(crate::message::MsgWriteData { stream, data: write_data }))
        }
        MessageType::WriteReady => {
            if data.len() < 8 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let error = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            Ok(Message::WriteReady(crate::message::MsgWriteReady { stream, error }))
        }
        MessageType::WriteClose => {
            if data.len() < 4 {
                return Err(err());
            }
            let stream = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            Ok(Message::WriteClose(crate::message::MsgWriteClose { stream }))
        }
        // rmux extensions
        MessageType::OutputData => Ok(Message::OutputData(data.to_vec())),
        MessageType::InputData => Ok(Message::InputData(data.to_vec())),
        MessageType::ErrorOutput => Ok(Message::ErrorOutput(data.to_vec())),
    }
}

fn message_to_type(msg: &Message) -> MessageType {
    match msg {
        Message::Version { .. } => MessageType::Version,
        Message::IdentifyFlags(_) => MessageType::IdentifyFlags,
        Message::IdentifyTerm(_) => MessageType::IdentifyTerm,
        Message::IdentifyTtyName(_) => MessageType::IdentifyTtyName,
        Message::IdentifyStdin => MessageType::IdentifyStdin,
        Message::IdentifyStdout => MessageType::IdentifyStdout,
        Message::IdentifyEnviron(_) => MessageType::IdentifyEnviron,
        Message::IdentifyDone => MessageType::IdentifyDone,
        Message::IdentifyClientPid(_) => MessageType::IdentifyClientPid,
        Message::IdentifyCwd(_) => MessageType::IdentifyCwd,
        Message::IdentifyFeatures(_) => MessageType::IdentifyFeatures,
        Message::IdentifyLongFlags(_) => MessageType::IdentifyLongFlags,
        Message::IdentifyTerminfo(_) => MessageType::IdentifyTerminfo,
        Message::Command(_) => MessageType::Command,
        Message::Detach => MessageType::Detach,
        Message::DetachKill => MessageType::DetachKill,
        Message::Exit => MessageType::Exit,
        Message::Exited => MessageType::Exited,
        Message::Exiting => MessageType::Exiting,
        Message::Lock => MessageType::Lock,
        Message::Ready => MessageType::Ready,
        Message::Resize { .. } => MessageType::Resize,
        Message::Shell(_) => MessageType::Shell,
        Message::Shutdown => MessageType::Shutdown,
        Message::Suspend => MessageType::Suspend,
        Message::Unlock => MessageType::Unlock,
        Message::Wakeup => MessageType::Wakeup,
        Message::Exec(_) => MessageType::Exec,
        Message::Flags(_) => MessageType::Flags,
        Message::ReadOpen(_) => MessageType::ReadOpen,
        Message::Read(_) => MessageType::Read,
        Message::ReadDone(_) => MessageType::ReadDone,
        Message::ReadCancel(_) => MessageType::ReadCancel,
        Message::WriteOpen(_) => MessageType::WriteOpen,
        Message::Write(_) => MessageType::Write,
        Message::WriteReady(_) => MessageType::WriteReady,
        Message::WriteClose(_) => MessageType::WriteClose,
        Message::OutputData(_) => MessageType::OutputData,
        Message::InputData(_) => MessageType::InputData,
        Message::ErrorOutput(_) => MessageType::ErrorOutput,
    }
}

// --- Async message I/O ---

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

/// Async message reader wrapping the read half of a Unix stream.
pub struct MessageReader {
    stream: OwnedReadHalf,
    buf: BytesMut,
}

impl MessageReader {
    /// Create a new reader from a stream read half.
    pub fn new(stream: OwnedReadHalf) -> Self {
        Self { stream, buf: BytesMut::with_capacity(8192) }
    }

    /// Read the next message from the stream.
    ///
    /// Returns `None` on EOF (connection closed).
    pub async fn read_message(&mut self) -> Result<Option<Message>, CodecError> {
        loop {
            // Try to decode from existing buffer
            if let Some(msg) = decode_message(&mut self.buf)? {
                return Ok(Some(msg));
            }

            // Read more data
            let n = self.stream.read_buf(&mut self.buf).await.map_err(CodecError::Io)?;
            if n == 0 {
                return Ok(None); // EOF
            }
        }
    }
}

/// Async message writer wrapping the write half of a Unix stream.
pub struct MessageWriter {
    stream: OwnedWriteHalf,
    buf: BytesMut,
}

impl MessageWriter {
    /// Create a new writer from a stream write half.
    pub fn new(stream: OwnedWriteHalf) -> Self {
        Self { stream, buf: BytesMut::with_capacity(8192) }
    }

    /// Write a message to the stream.
    pub async fn write_message(&mut self, msg: &Message) -> Result<(), CodecError> {
        encode_message(msg, &mut self.buf)?;
        let data = self.buf.split();
        self.stream.write_all(&data).await.map_err(CodecError::Io)?;
        Ok(())
    }

    /// Flush the underlying stream.
    pub async fn flush(&mut self) -> Result<(), CodecError> {
        self.stream.flush().await.map_err(CodecError::Io)
    }
}

fn decode_cstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::PROTOCOL_VERSION;

    #[test]
    fn encode_decode_version() {
        let msg = Message::Version { version: PROTOCOL_VERSION };
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::Version { version } => assert_eq!(version, PROTOCOL_VERSION),
            other => panic!("expected Version, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_identify_term() {
        let msg = Message::IdentifyTerm("xterm-256color".to_string());
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::IdentifyTerm(term) => assert_eq!(term, "xterm-256color"),
            other => panic!("expected IdentifyTerm, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_command() {
        let msg = Message::Command(MsgCommand {
            argc: 3,
            argv: vec!["new-session".to_string(), "-s".to_string(), "test".to_string()],
        });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::Command(cmd) => {
                assert_eq!(cmd.argc, 3);
                assert_eq!(cmd.argv, vec!["new-session", "-s", "test"]);
            }
            other => panic!("expected Command, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_resize() {
        let msg = Message::Resize { sx: 120, sy: 40, xpixel: 960, ypixel: 640 };
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::Resize { sx, sy, xpixel, ypixel } => {
                assert_eq!((sx, sy, xpixel, ypixel), (120, 40, 960, 640));
            }
            other => panic!("expected Resize, got {other:?}"),
        }
    }

    #[test]
    fn incomplete_message_returns_none() {
        let mut buf = BytesMut::from(&[0u8; 4][..]);
        assert!(decode_message(&mut buf).unwrap().is_none());
    }

    #[test]
    fn multiple_messages_in_buffer() {
        let mut buf = BytesMut::new();
        encode_message(&Message::Ready, &mut buf).unwrap();
        encode_message(&Message::Detach, &mut buf).unwrap();

        let msg1 = decode_message(&mut buf).unwrap().unwrap();
        assert!(matches!(msg1, Message::Ready));

        let msg2 = decode_message(&mut buf).unwrap().unwrap();
        assert!(matches!(msg2, Message::Detach));
    }

    #[test]
    fn encode_decode_flags() {
        let msg = Message::Flags(0x1234_5678_9ABC_DEF0_i64);
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::Flags(f) => assert_eq!(f, 0x1234_5678_9ABC_DEF0_i64),
            other => panic!("expected Flags, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_identify_cwd() {
        let msg = Message::IdentifyCwd("/home/user/projects".to_string());
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::IdentifyCwd(c) => assert_eq!(c, "/home/user/projects"),
            other => panic!("expected IdentifyCwd, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_identify_client_pid() {
        let msg = Message::IdentifyClientPid(12345);
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::IdentifyClientPid(pid) => assert_eq!(pid, 12345),
            other => panic!("expected IdentifyClientPid, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_identify_long_flags() {
        let msg = Message::IdentifyLongFlags(0xFF);
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::IdentifyLongFlags(f) => assert_eq!(f, 0xFF),
            other => panic!("expected IdentifyLongFlags, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_shell() {
        let msg = Message::Shell("/bin/bash".to_string());
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::Shell(s) => assert_eq!(s, "/bin/bash"),
            other => panic!("expected Shell, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_output_data() {
        let data = b"Hello World\x1b[0m".to_vec();
        let msg = Message::OutputData(data.clone());
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::OutputData(d) => assert_eq!(d, data),
            other => panic!("expected OutputData, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_input_data() {
        let data = vec![0x1b, b'[', b'A'];
        let msg = Message::InputData(data.clone());
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::InputData(d) => assert_eq!(d, data),
            other => panic!("expected InputData, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_simple_messages() {
        for msg in [
            Message::Detach,
            Message::DetachKill,
            Message::Exit,
            Message::Exited,
            Message::Exiting,
            Message::Lock,
            Message::Ready,
            Message::Shutdown,
            Message::Suspend,
            Message::Unlock,
            Message::Wakeup,
        ] {
            let mut buf = BytesMut::new();
            encode_message(&msg, &mut buf).unwrap();
            let decoded = decode_message(&mut buf).unwrap().unwrap();
            // Just verify we can roundtrip without errors
            assert!(!format!("{decoded:?}").is_empty());
        }
    }

    #[test]
    fn unknown_message_type_returns_error() {
        let mut buf = BytesMut::new();
        // Manually write a header with an unknown message type
        buf.put_u32_le(9999); // type
        buf.put_u16_le(IMSG_HEADER_SIZE as u16); // len = header only
        buf.put_u16_le(0); // flags
        buf.put_u32_le(0); // peerid
        buf.put_u32_le(0); // pid
        let result = decode_message(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn decode_cstring_nul_terminated() {
        assert_eq!(decode_cstring(b"hello\x00world"), "hello");
    }

    #[test]
    fn decode_cstring_no_nul() {
        assert_eq!(decode_cstring(b"hello"), "hello");
    }

    #[test]
    fn encode_decode_read_open() {
        use crate::message::MsgReadOpen;
        let msg =
            Message::ReadOpen(MsgReadOpen { stream: 7, fd: 3, path: "/tmp/test.txt".to_string() });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::ReadOpen(m) => {
                assert_eq!(m.stream, 7);
                assert_eq!(m.fd, 3);
                assert_eq!(m.path, "/tmp/test.txt");
            }
            other => panic!("expected ReadOpen, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_write_open() {
        use crate::message::MsgWriteOpen;
        let msg = Message::WriteOpen(MsgWriteOpen {
            stream: 42,
            fd: 1,
            flags: 0x0602,
            path: "/var/log/output.log".to_string(),
        });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::WriteOpen(m) => {
                assert_eq!(m.stream, 42);
                assert_eq!(m.fd, 1);
                assert_eq!(m.flags, 0x0602);
                assert_eq!(m.path, "/var/log/output.log");
            }
            other => panic!("expected WriteOpen, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_read_data() {
        use crate::message::MsgReadData;
        let msg = Message::Read(MsgReadData { stream: 99 });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::Read(m) => {
                assert_eq!(m.stream, 99);
            }
            other => panic!("expected Read, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_write_data() {
        use crate::message::MsgWriteData;
        let data = vec![0x41, 0x42, 0x43, 0x00, 0xFF];
        let msg = Message::Write(MsgWriteData { stream: 5, data: data.clone() });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::Write(m) => {
                assert_eq!(m.stream, 5);
                assert_eq!(m.data, data);
            }
            other => panic!("expected Write, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_read_done() {
        use crate::message::MsgReadDone;
        let msg = Message::ReadDone(MsgReadDone { stream: 12, error: -1 });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::ReadDone(m) => {
                assert_eq!(m.stream, 12);
                assert_eq!(m.error, -1);
            }
            other => panic!("expected ReadDone, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_write_ready() {
        use crate::message::MsgWriteReady;
        let msg = Message::WriteReady(MsgWriteReady { stream: 33, error: 0 });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::WriteReady(m) => {
                assert_eq!(m.stream, 33);
                assert_eq!(m.error, 0);
            }
            other => panic!("expected WriteReady, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_write_close() {
        use crate::message::MsgWriteClose;
        let msg = Message::WriteClose(MsgWriteClose { stream: 77 });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::WriteClose(m) => {
                assert_eq!(m.stream, 77);
            }
            other => panic!("expected WriteClose, got {other:?}"),
        }
    }

    #[test]
    fn encode_decode_read_cancel() {
        use crate::message::MsgReadCancel;
        let msg = Message::ReadCancel(MsgReadCancel { stream: 55 });
        let mut buf = BytesMut::new();
        encode_message(&msg, &mut buf).unwrap();
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::ReadCancel(m) => {
                assert_eq!(m.stream, 55);
            }
            other => panic!("expected ReadCancel, got {other:?}"),
        }
    }

    #[test]
    fn message_at_max_size() {
        // Create a message with data at exactly IMSG_MAX_DATA bytes
        let data = vec![0xAB; IMSG_MAX_DATA];
        let msg = Message::OutputData(data.clone());
        let mut buf = BytesMut::new();
        let result = encode_message(&msg, &mut buf);
        assert!(result.is_ok());
        let decoded = decode_message(&mut buf).unwrap().unwrap();
        match decoded {
            Message::OutputData(d) => assert_eq!(d.len(), IMSG_MAX_DATA),
            other => panic!("expected OutputData, got {other:?}"),
        }
    }

    #[test]
    fn message_exceeding_max_size() {
        // Create a message with data exceeding IMSG_MAX_DATA bytes
        let data = vec![0xAB; IMSG_MAX_DATA + 1];
        let msg = Message::OutputData(data);
        let mut buf = BytesMut::new();
        let result = encode_message(&msg, &mut buf);
        assert!(result.is_err());
        match result.unwrap_err() {
            CodecError::TooLarge { size } => assert_eq!(size, IMSG_MAX_DATA + 1),
            other => panic!("expected TooLarge error, got {other:?}"),
        }
    }

    #[test]
    fn empty_buffer_returns_none() {
        let mut buf = BytesMut::new();
        let result = decode_message(&mut buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn decode_command_negative_argc_fails() {
        let mut buf = BytesMut::new();
        // Manually build a Command message with negative argc
        let mut data = BytesMut::new();
        data.put_i32_le(-1); // negative argc
        data.put_slice(b"arg\x00");

        buf.put_u32_le(MessageType::Command as u32);
        let total_len = IMSG_HEADER_SIZE + data.len();
        buf.put_u16_le(total_len as u16);
        buf.put_u16_le(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        buf.put_slice(&data);

        let result = decode_message(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn decode_command_argc_mismatch_fails() {
        let mut buf = BytesMut::new();
        // argc says 5 but only 2 actual args
        let mut data = BytesMut::new();
        data.put_i32_le(5);
        data.put_slice(b"one\x00two\x00");

        buf.put_u32_le(MessageType::Command as u32);
        let total_len = IMSG_HEADER_SIZE + data.len();
        buf.put_u16_le(total_len as u16);
        buf.put_u16_le(0);
        buf.put_u32_le(0);
        buf.put_u32_le(0);
        buf.put_slice(&data);

        let result = decode_message(&mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn deprecated_message_type_returns_error() {
        for msg_type in [
            MessageType::IdentifyOldCwd,
            MessageType::OldStderr,
            MessageType::OldStdin,
            MessageType::OldStdout,
        ] {
            let mut buf = BytesMut::new();
            buf.put_u32_le(msg_type as u32);
            buf.put_u16_le(IMSG_HEADER_SIZE as u16);
            buf.put_u16_le(0);
            buf.put_u32_le(0);
            buf.put_u32_le(0);
            let result = decode_message(&mut buf);
            assert!(result.is_err(), "expected error for deprecated type {msg_type:?}");
        }
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn version_roundtrip(version in 0u32..1000) {
                let msg = Message::Version { version };
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::Version { version: v } => prop_assert_eq!(v, version),
                    other => prop_assert!(false, "expected Version, got {:?}", other),
                }
            }

            #[test]
            fn identify_term_roundtrip(term in "[a-zA-Z0-9_-]{1,100}") {
                let msg = Message::IdentifyTerm(term.clone());
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::IdentifyTerm(t) => prop_assert_eq!(t, term),
                    other => prop_assert!(false, "expected IdentifyTerm, got {:?}", other),
                }
            }

            #[test]
            fn resize_roundtrip(sx in 1u32..10000, sy in 1u32..10000, xpixel in 0u32..10000, ypixel in 0u32..10000) {
                let msg = Message::Resize { sx, sy, xpixel, ypixel };
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::Resize { sx: s, sy: t, xpixel: xp, ypixel: yp } => {
                        prop_assert_eq!(s, sx);
                        prop_assert_eq!(t, sy);
                        prop_assert_eq!(xp, xpixel);
                        prop_assert_eq!(yp, ypixel);
                    }
                    other => prop_assert!(false, "expected Resize, got {:?}", other),
                }
            }

            #[test]
            fn command_roundtrip(
                args in proptest::collection::vec("[a-zA-Z0-9_-]{1,50}", 1..5)
            ) {
                let msg = Message::Command(MsgCommand {
                    argc: args.len() as i32,
                    argv: args.clone(),
                });
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::Command(cmd) => {
                        prop_assert_eq!(cmd.argc, args.len() as i32);
                        prop_assert_eq!(cmd.argv, args);
                    }
                    other => prop_assert!(false, "expected Command, got {:?}", other),
                }
            }

            #[test]
            fn flags_roundtrip(flags in proptest::num::i64::ANY) {
                let msg = Message::Flags(flags);
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::Flags(f) => prop_assert_eq!(f, flags),
                    other => prop_assert!(false, "expected Flags, got {:?}", other),
                }
            }

            #[test]
            fn output_data_roundtrip(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
                let msg = Message::OutputData(data.clone());
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::OutputData(d) => prop_assert_eq!(d, data),
                    other => prop_assert!(false, "expected OutputData, got {:?}", other),
                }
            }

            #[test]
            fn input_data_roundtrip(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
                let msg = Message::InputData(data.clone());
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::InputData(d) => prop_assert_eq!(d, data),
                    other => prop_assert!(false, "expected InputData, got {:?}", other),
                }
            }

            #[test]
            fn shell_roundtrip(shell in "[/a-zA-Z0-9._-]{1,200}") {
                let msg = Message::Shell(shell.clone());
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::Shell(s) => prop_assert_eq!(s, shell),
                    other => prop_assert!(false, "expected Shell, got {:?}", other),
                }
            }

            #[test]
            fn identify_cwd_roundtrip(cwd in "[/a-zA-Z0-9._-]{1,200}") {
                let msg = Message::IdentifyCwd(cwd.clone());
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::IdentifyCwd(c) => prop_assert_eq!(c, cwd),
                    other => prop_assert!(false, "expected IdentifyCwd, got {:?}", other),
                }
            }

            #[test]
            fn client_pid_roundtrip(pid in any::<i32>()) {
                let msg = Message::IdentifyClientPid(pid);
                let mut buf = BytesMut::new();
                encode_message(&msg, &mut buf).unwrap();
                let decoded = decode_message(&mut buf).unwrap().unwrap();
                match decoded {
                    Message::IdentifyClientPid(p) => prop_assert_eq!(p, pid),
                    other => prop_assert!(false, "expected IdentifyClientPid, got {:?}", other),
                }
            }

            #[test]
            fn decode_arbitrary_bytes_never_panics(data in proptest::collection::vec(any::<u8>(), 0..256)) {
                let mut buf = BytesMut::from(data.as_slice());
                // Should never panic, only return Ok or Err
                let _ = decode_message(&mut buf);
            }
        }
    }
}
