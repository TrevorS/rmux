//! Per-client state management.
//!
//! Each connected client has a `ServerClient` that tracks its identification
//! state, attached session, terminal size, and provides message I/O.

use bitflags::bitflags;
use rmux_protocol::codec::MessageWriter;
use rmux_protocol::identify::IdentifyState;
use rmux_protocol::message::Message;

bitflags! {
    /// Client state flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct ClientFlags: u32 {
        /// Client has completed identification.
        const IDENTIFIED   = 0x0001;
        /// Client is attached to a session.
        const ATTACHED     = 0x0002;
        /// Client needs a redraw.
        const REDRAW       = 0x0004;
        /// Client is exiting.
        const EXITING      = 0x0008;
    }
}

/// State for the interactive command prompt (:).
#[derive(Debug, Clone, Default)]
pub struct PromptState {
    /// Current input buffer.
    pub buffer: String,
    /// Cursor position in the buffer.
    pub cursor_pos: usize,
}

/// A connected client on the server side.
pub struct ServerClient {
    /// Unique client ID.
    pub id: u64,
    /// Message writer for sending data to this client.
    pub writer: MessageWriter,
    /// Identification state machine.
    pub identify: IdentifyState,
    /// Client flags.
    pub flags: ClientFlags,
    /// Attached session ID (if any).
    pub session_id: Option<u32>,
    /// Terminal width.
    pub sx: u32,
    /// Terminal height.
    pub sy: u32,
    /// Command prompt state (Some = prompt mode active).
    pub prompt: Option<PromptState>,
}

impl ServerClient {
    /// Create a new client from a message writer.
    pub fn new(id: u64, writer: MessageWriter) -> Self {
        Self {
            id,
            writer,
            identify: IdentifyState::default(),
            flags: ClientFlags::empty(),
            session_id: None,
            sx: 80,
            sy: 24,
            prompt: None,
        }
    }

    /// Whether this client has completed identification.
    pub fn is_identified(&self) -> bool {
        self.flags.contains(ClientFlags::IDENTIFIED)
    }

    /// Whether this client is attached to a session.
    pub fn is_attached(&self) -> bool {
        self.flags.contains(ClientFlags::ATTACHED)
    }

    /// Mark client as needing a redraw.
    pub fn mark_redraw(&mut self) {
        self.flags.insert(ClientFlags::REDRAW);
    }

    /// Check and clear the redraw flag.
    pub fn needs_redraw(&mut self) -> bool {
        let needs = self.flags.contains(ClientFlags::REDRAW);
        self.flags.remove(ClientFlags::REDRAW);
        needs
    }

    /// Set the terminal size.
    pub fn set_size(&mut self, sx: u32, sy: u32) {
        if self.sx != sx || self.sy != sy {
            self.sx = sx;
            self.sy = sy;
            self.mark_redraw();
        }
    }

    /// Attach to a session.
    pub fn attach(&mut self, session_id: u32) {
        self.session_id = Some(session_id);
        self.flags.insert(ClientFlags::ATTACHED);
        self.mark_redraw();
    }

    /// Detach from the current session.
    pub fn detach(&mut self) {
        self.session_id = None;
        self.flags.remove(ClientFlags::ATTACHED);
    }

    /// Send a message to this client.
    pub async fn send(&mut self, msg: &Message) -> Result<(), rmux_protocol::codec::CodecError> {
        self.writer.write_message(msg).await
    }
}
