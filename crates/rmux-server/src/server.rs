//! Server event loop.
//!
//! The server listens on a Unix domain socket and accepts client connections.
//! It manages all sessions, windows, and panes using a tokio single-threaded runtime.

use crate::client::{ClientFlags, ServerClient};
use crate::command::{self, CommandResult, CommandServer};
use crate::keybind::KeyBindings;
use crate::pane::Pane;
use crate::render;
use crate::session::SessionManager;
use crate::window::Window;
use rmux_protocol::codec::{self, MessageReader, MessageWriter};
use rmux_protocol::message::Message;
use rmux_terminal::pty;
use std::collections::HashMap;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::path::PathBuf;
use tokio::io::unix::AsyncFd;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

/// Server error type.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("failed to bind socket: {0}")]
    Bind(std::io::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    Protocol(#[from] codec::CodecError),
    #[error("PTY error: {0}")]
    Pty(#[from] pty::PtyError),
    #[error("command error: {0}")]
    Command(String),
}

/// Events from client read tasks.
pub enum ClientEvent {
    /// A protocol message was received.
    Message(Message),
    /// The client disconnected.
    Disconnected,
}

/// Wrapper for a raw fd so we can use it with AsyncFd without owning the fd.
struct RawFdRef(i32);

impl AsRawFd for RawFdRef {
    fn as_raw_fd(&self) -> i32 {
        self.0
    }
}

impl AsFd for RawFdRef {
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: The fd is kept alive by the pty_fds HashMap in the Server struct.
        unsafe { BorrowedFd::borrow_raw(self.0) }
    }
}

/// The rmux server.
pub struct Server {
    /// Socket path.
    socket_path: PathBuf,
    /// Session manager.
    sessions: SessionManager,
    /// Connected clients.
    clients: HashMap<u64, ServerClient>,
    /// Next client ID.
    next_client_id: u64,
    /// Key bindings.
    keybindings: KeyBindings,
    /// Sender for PTY output events (pane_id, data).
    pty_tx: mpsc::Sender<(u32, Vec<u8>)>,
    /// Receiver for PTY output events.
    pty_rx: mpsc::Receiver<(u32, Vec<u8>)>,
    /// Sender for client events (client_id, event).
    client_tx: mpsc::Sender<(u64, ClientEvent)>,
    /// Receiver for client events.
    client_rx: mpsc::Receiver<(u64, ClientEvent)>,
    /// PTY master fds (pane_id -> OwnedFd), kept alive so async tasks can read.
    pty_fds: HashMap<u32, std::os::fd::OwnedFd>,
    /// Active PTY read tasks.
    pty_tasks: HashMap<u32, tokio::task::JoinHandle<()>>,
    /// Whether the server should shut down.
    shutdown: bool,
}

impl Server {
    /// Create a new server.
    pub fn new(socket_path: PathBuf) -> Self {
        let (pty_tx, pty_rx) = mpsc::channel(256);
        let (client_tx, client_rx) = mpsc::channel(256);

        Self {
            socket_path,
            sessions: SessionManager::new(),
            clients: HashMap::new(),
            next_client_id: 1,
            keybindings: KeyBindings::default_bindings(),
            pty_tx,
            pty_rx,
            client_tx,
            client_rx,
            pty_fds: HashMap::new(),
            pty_tasks: HashMap::new(),
            shutdown: false,
        }
    }

    /// Get the default socket path (matching tmux's convention).
    pub fn default_socket_path() -> PathBuf {
        let tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        let uid = nix::unistd::getpid();
        PathBuf::from(format!("{tmpdir}/rmux-{uid}/default"))
    }

    /// Run the server event loop.
    pub async fn run(&mut self) -> Result<(), ServerError> {
        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent).map_err(ServerError::Bind)?;
        }

        // Remove stale socket
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)
            .map_err(ServerError::Bind)?;

        tracing::info!("server listening on {:?}", self.socket_path);

        let mut redraw_interval = tokio::time::interval(
            tokio::time::Duration::from_millis(16), // ~60fps
        );
        redraw_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        while !self.shutdown {
            tokio::select! {
                // Accept new client connections
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            self.handle_new_client(stream);
                        }
                        Err(e) => {
                            tracing::error!("accept error: {e}");
                        }
                    }
                }

                // PTY output from any pane
                Some((pane_id, data)) = self.pty_rx.recv() => {
                    self.handle_pty_output(pane_id, &data);
                }

                // Client events (messages or disconnections)
                Some((client_id, event)) = self.client_rx.recv() => {
                    self.handle_client_event(client_id, event).await;
                }

                // Periodic redraw
                _ = redraw_interval.tick() => {
                    self.render_clients().await;
                }
            }
        }

        // Clean up
        let _ = std::fs::remove_file(&self.socket_path);
        tracing::info!("server shutting down");
        Ok(())
    }

    fn handle_new_client(&mut self, stream: UnixStream) {
        let client_id = self.next_client_id;
        self.next_client_id += 1;

        let (read_half, write_half) = stream.into_split();
        let writer = MessageWriter::new(write_half);
        let client = ServerClient::new(client_id, writer);

        self.clients.insert(client_id, client);

        // Spawn reader task
        let tx = self.client_tx.clone();
        tokio::spawn(async move {
            let mut reader = MessageReader::new(read_half);
            loop {
                if let Ok(Some(msg)) = reader.read_message().await {
                    if tx.send((client_id, ClientEvent::Message(msg))).await.is_err() {
                        break;
                    }
                } else {
                    tx.send((client_id, ClientEvent::Disconnected)).await.ok();
                    break;
                }
            }
        });

        tracing::info!("client {client_id} connected");
    }

    fn handle_pty_output(&mut self, pane_id: u32, data: &[u8]) {
        // Find the pane and feed data through its parser
        for session in self.sessions.iter_mut() {
            for window in session.windows.values_mut() {
                if let Some(pane) = window.panes.get_mut(&pane_id) {
                    pane.process_input(data);
                    // Mark attached clients for redraw
                    for client in self.clients.values_mut() {
                        if client.session_id == Some(session.id) && client.is_attached() {
                            client.mark_redraw();
                        }
                    }
                    return;
                }
            }
        }
    }

    async fn handle_client_event(&mut self, client_id: u64, event: ClientEvent) {
        match event {
            ClientEvent::Message(msg) => {
                self.handle_client_message(client_id, msg).await;
            }
            ClientEvent::Disconnected => {
                self.handle_client_disconnect(client_id);
            }
        }
    }

    async fn handle_client_message(&mut self, client_id: u64, msg: Message) {
        // Get client, process identify if not yet identified
        let identified = {
            let Some(client) = self.clients.get_mut(&client_id) else {
                return;
            };
            if !client.is_identified() {
                let done = client.identify.process(&msg);
                if done {
                    client.flags.insert(ClientFlags::IDENTIFIED);
                    tracing::info!(
                        "client {client_id} identified: term={}, cwd={}",
                        client.identify.term,
                        client.identify.cwd,
                    );
                }
                return;
            }
            true
        };

        if !identified {
            return;
        }

        match msg {
            Message::Command(cmd) => {
                self.handle_command(client_id, &cmd.argv).await;
            }
            Message::InputData(data) => {
                self.handle_input_data(client_id, &data);
            }
            Message::Resize { sx, sy, .. } => {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    client.set_size(sx, sy);
                }
            }
            Message::Exiting => {
                self.handle_client_disconnect(client_id);
            }
            _ => {
                tracing::debug!("unhandled message from client {client_id}: {msg:?}");
            }
        }
    }

    async fn handle_command(&mut self, client_id: u64, argv: &[String]) {
        if argv.is_empty() {
            return;
        }

        tracing::info!("client {client_id} command: {argv:?}");

        // Execute command
        let result = command::execute_command(argv, self);

        match result {
            Ok(CommandResult::Ok) => {
                // Send exit to non-attached clients
                if let Some(client) = self.clients.get_mut(&client_id) {
                    if !client.is_attached() {
                        client.send(&Message::Exit).await.ok();
                    }
                }
            }
            Ok(CommandResult::Output(text)) => {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    client.send(&Message::OutputData(text.into_bytes())).await.ok();
                    client.send(&Message::Exit).await.ok();
                }
            }
            Ok(CommandResult::Attach(session_id)) => {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    client.attach(session_id);
                    client.send(&Message::Ready).await.ok();
                    // Increment attached count
                    if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                        session.attached += 1;
                    }
                }
            }
            Ok(CommandResult::Detach) => {
                self.detach_client(client_id).await;
            }
            Ok(CommandResult::Exit) => {
                self.shutdown = true;
            }
            Err(e) => {
                let err_msg = format!("{e}\n");
                if let Some(client) = self.clients.get_mut(&client_id) {
                    client.send(&Message::OutputData(err_msg.into_bytes())).await.ok();
                    client.send(&Message::Exit).await.ok();
                }
            }
        }
    }

    fn handle_input_data(&mut self, client_id: u64, data: &[u8]) {
        let Some(client) = self.clients.get(&client_id) else {
            return;
        };
        let Some(session_id) = client.session_id else {
            return;
        };

        // Check for prefix key
        if let Some(key_data) = self.keybindings.process_input(data) {
            match key_data {
                crate::keybind::KeyAction::SendToPane(bytes) => {
                    self.write_to_active_pane(session_id, &bytes);
                }
                crate::keybind::KeyAction::Command(argv) => {
                    // Queue command execution (can't call async from here)
                    // For now, handle common keybindings inline
                    if argv.first().map(String::as_str) == Some("detach-client") {
                        // Will be handled in the next event cycle
                        let tx = self.client_tx.clone();
                        let msg = Message::Command(rmux_protocol::message::MsgCommand {
                            argc: 1,
                            argv,
                        });
                        tokio::spawn(async move {
                            tx.send((client_id, ClientEvent::Message(msg))).await.ok();
                        });
                    }
                }
            }
        } else {
            // No prefix handling, send directly to pane
            self.write_to_active_pane(session_id, data);
        }
    }

    fn write_to_active_pane(&self, session_id: u32, data: &[u8]) {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return;
        };
        let Some(window) = session.active_window() else {
            return;
        };
        let Some(pane) = window.active_pane() else {
            return;
        };

        // Write to PTY master fd
        if let Some(fd) = self.pty_fds.get(&pane.id) {
            nix::unistd::write(fd, data).ok();
        }
    }

    fn handle_client_disconnect(&mut self, client_id: u64) {
        if let Some(client) = self.clients.remove(&client_id) {
            // Decrement attached count
            if let Some(session_id) = client.session_id {
                if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                    session.attached = session.attached.saturating_sub(1);
                }
            }
            tracing::info!("client {client_id} disconnected");
        }

        // If no more sessions and no more clients, shut down
        if self.clients.is_empty() && self.sessions.is_empty() {
            self.shutdown = true;
        }
    }

    async fn detach_client(&mut self, client_id: u64) {
        if let Some(client) = self.clients.get_mut(&client_id) {
            let session_id = client.session_id;
            client.detach();
            client.send(&Message::Detach).await.ok();

            // Decrement attached count
            if let Some(sid) = session_id {
                if let Some(session) = self.sessions.find_by_id_mut(sid) {
                    session.attached = session.attached.saturating_sub(1);
                }
            }
        }
    }

    async fn render_clients(&mut self) {
        // Collect client IDs that need redraw, along with their session IDs and sizes
        let to_render: Vec<(u64, u32, u32, u32)> = self.clients.values_mut()
            .filter_map(|c| {
                if c.needs_redraw() {
                    c.session_id.map(|sid| (c.id, sid, c.sx, c.sy))
                } else {
                    None
                }
            })
            .collect();

        for (client_id, session_id, sx, sy) in to_render {
            let output = self.render_session(session_id, sx, sy);
            if let Some(client) = self.clients.get_mut(&client_id) {
                client.send(&Message::OutputData(output)).await.ok();
            }
        }
    }

    fn render_session(&self, session_id: u32, sx: u32, sy: u32) -> Vec<u8> {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return Vec::new();
        };
        let Some(window) = session.active_window() else {
            return Vec::new();
        };

        render::render_window(
            window,
            &session.name,
            session.active_window,
            sx,
            sy,
        )
    }

    /// Spawn a shell process for a pane.
    fn spawn_pane_process(&mut self, pane_id: u32, sx: u32, sy: u32, cwd: &str) -> Result<(), ServerError> {
        let shell = pty::default_shell();
        let pty_pair = pty::Pty::open(sx as u16, sy as u16)?;
        let spawned = pty_pair.spawn_shell(&shell, cwd)?;

        let master_raw = spawned.master_fd();

        // Set non-blocking for async reads
        pty::set_nonblocking(master_raw)?;

        // Store the master fd
        self.pty_fds.insert(pane_id, spawned.master);

        // Update pane with PID
        for session in self.sessions.iter_mut() {
            for window in session.windows.values_mut() {
                if let Some(pane) = window.panes.get_mut(&pane_id) {
                    pane.pid = spawned.pid.as_raw() as u32;
                    pane.pty_fd = master_raw;
                }
            }
        }

        // Spawn async read task for PTY output
        let tx = self.pty_tx.clone();
        let handle = tokio::spawn(async move {
            pty_read_task(master_raw, pane_id, tx).await;
        });
        self.pty_tasks.insert(pane_id, handle);

        Ok(())
    }
}

/// Background task that reads from a PTY master fd and sends data through a channel.
async fn pty_read_task(raw_fd: i32, pane_id: u32, tx: mpsc::Sender<(u32, Vec<u8>)>) {
    let fd = RawFdRef(raw_fd);
    let async_fd: AsyncFd<RawFdRef> = match AsyncFd::new(fd) {
        Ok(afd) => afd,
        Err(e) => {
            tracing::error!("failed to create AsyncFd for pane {pane_id}: {e}");
            return;
        }
    };

    let mut buf = vec![0u8; 8192];
    loop {
        let Ok(mut guard) = async_fd.readable().await else {
            break;
        };

        match guard.try_io(|inner: &AsyncFd<RawFdRef>| {
            let raw = inner.as_raw_fd();
            // SAFETY: fd is valid (kept alive by pty_fds HashMap), buf is valid.
            let n = unsafe { nix::libc::read(raw, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(n as usize)
            }
        }) {
            Ok(Ok(0)) => break, // EOF - process exited
            Ok(Ok(n)) => {
                if tx.send((pane_id, buf[..n].to_vec())).await.is_err() {
                    break; // Receiver dropped
                }
            }
            Ok(Err(e)) => {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    tracing::debug!("PTY read error for pane {pane_id}: {e}");
                    break;
                }
            }
            Err(_would_block) => {}
        }
    }

    tracing::debug!("PTY read task for pane {pane_id} exiting");
}

// Implement CommandServer for Server
impl CommandServer for Server {
    fn create_session(&mut self, name: &str, cwd: &str, sx: u32, sy: u32) -> Result<u32, ServerError> {
        let session = self.sessions.create(name.to_string(), cwd.to_string());
        let session_id = session.id;

        // Create initial window with one pane
        // Reserve 1 row for status line
        let pane_height = sy.saturating_sub(1);
        let mut window = Window::new("0".to_string(), sx, pane_height);
        let pane = Pane::new(sx, pane_height, 2000);
        let pane_id = pane.id;
        window.active_pane = pane_id;
        window.panes.insert(pane_id, pane);

        let window_idx = session.next_window_index();
        session.active_window = window_idx;
        session.windows.insert(window_idx, window);

        // We need to get the session back as immutable first
        // Spawn the shell process
        self.spawn_pane_process(pane_id, sx, pane_height, cwd)?;

        Ok(session_id)
    }

    fn kill_session(&mut self, name: &str) -> Result<(), ServerError> {
        let session = self.sessions.find_by_name(name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {name}")))?;
        let id = session.id;

        // Clean up PTY tasks for all panes
        let session = self.sessions.find_by_id(id).unwrap();
        let pane_ids: Vec<u32> = session.windows.values()
            .flat_map(|w| w.panes.keys())
            .copied()
            .collect();

        for pane_id in &pane_ids {
            if let Some(task) = self.pty_tasks.remove(pane_id) {
                task.abort();
            }
            self.pty_fds.remove(pane_id);
        }

        self.sessions.remove(id);
        Ok(())
    }

    fn has_session(&self, name: &str) -> bool {
        self.sessions.find_by_name(name).is_some()
    }

    fn list_sessions(&self) -> Vec<String> {
        self.sessions.iter()
            .map(|s| {
                let windows = s.windows.len();
                let attached = if s.attached > 0 { " (attached)" } else { "" };
                format!("{}: {} windows{attached}", s.name, windows)
            })
            .collect()
    }

    fn find_session_id(&self, name: &str) -> Option<u32> {
        self.sessions.find_by_name(name).map(|s| s.id)
    }
}
