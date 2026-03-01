//! Server event loop.
//!
//! The server listens on a Unix domain socket and accepts client connections.
//! It manages all sessions, windows, and panes.

use crate::session::SessionManager;
use rmux_protocol::codec;
use std::path::PathBuf;

/// Server error type.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("failed to bind socket: {0}")]
    Bind(std::io::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] codec::CodecError),
}

/// The rmux server.
pub struct Server {
    /// Socket path.
    socket_path: PathBuf,
    /// Session manager.
    sessions: SessionManager,
}

impl Server {
    /// Create a new server.
    #[must_use]
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            sessions: SessionManager::new(),
        }
    }

    /// Get the default socket path (matching tmux's convention).
    #[must_use]
    pub fn default_socket_path() -> PathBuf {
        let tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        let uid = nix::unistd::getpid();
        PathBuf::from(format!("{tmpdir}/rmux-{uid}/default"))
    }

    /// Socket path.
    #[must_use]
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Session manager.
    #[must_use]
    pub fn sessions(&self) -> &SessionManager {
        &self.sessions
    }

    /// Mutable session manager.
    pub fn sessions_mut(&mut self) -> &mut SessionManager {
        &mut self.sessions
    }
}
