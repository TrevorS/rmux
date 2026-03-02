//! Server event loop.
//!
//! The server listens on a Unix domain socket and accepts client connections.
//! It manages all sessions, windows, and panes using a tokio single-threaded runtime.

use crate::client::{ClientFlags, ServerClient};
use crate::command::{self, CommandResult, CommandServer, Direction};
use crate::keybind::KeyBindings;
use crate::navigate;
use crate::pane::Pane;
use crate::render;
use crate::session::SessionManager;
use crate::window::Window;
use rmux_core::layout::{LayoutCell, layout_even_horizontal, layout_even_vertical};
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
    /// Client ID for the current command execution context.
    command_client: u64,
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
            command_client: 0,
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

        let listener = UnixListener::bind(&self.socket_path).map_err(ServerError::Bind)?;

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
                self.handle_resize(client_id, sx, sy);
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

        // Set the command client context
        self.command_client = client_id;

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
                    if !bytes.is_empty() {
                        self.write_to_active_pane(session_id, &bytes);
                    }
                }
                crate::keybind::KeyAction::Command(argv) => {
                    // Queue command execution via the event channel
                    let tx = self.client_tx.clone();
                    let msg = Message::Command(rmux_protocol::message::MsgCommand {
                        #[allow(clippy::cast_possible_wrap)]
                        argc: argv.len() as i32,
                        argv,
                    });
                    tokio::spawn(async move {
                        tx.send((client_id, ClientEvent::Message(msg))).await.ok();
                    });
                }
            }
        } else {
            // No prefix handling, send directly to pane
            self.write_to_active_pane(session_id, data);
        }
    }

    fn handle_resize(&mut self, client_id: u64, sx: u32, sy: u32) {
        let session_id = {
            let Some(client) = self.clients.get_mut(&client_id) else {
                return;
            };
            client.set_size(sx, sy);
            client.session_id
        };

        // Propagate resize to all windows in the attached session
        if let Some(session_id) = session_id {
            self.resize_session_windows(session_id, sx, sy);
        }
    }

    fn resize_session_windows(&mut self, session_id: u32, sx: u32, sy: u32) {
        let pane_height = sy.saturating_sub(1); // Reserve status line

        // Collect pane resize info: (pane_id, new_sx, new_sy)
        let mut pane_resizes: Vec<(u32, u32, u32)> = Vec::new();

        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            for window in session.windows.values_mut() {
                window.sx = sx;
                window.sy = pane_height;

                // Rebuild layout with new dimensions
                let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
                if pane_ids.len() <= 1 {
                    // Single pane: just resize to full window
                    if let Some((&pid, pane)) = window.panes.iter_mut().next() {
                        pane.resize(sx, pane_height);
                        pane.xoff = 0;
                        pane.yoff = 0;
                        window.layout = Some(LayoutCell::new_pane(0, 0, sx, pane_height, pid));
                        pane_resizes.push((pid, sx, pane_height));
                    }
                } else {
                    // Multi-pane: rebuild even layout
                    let layout =
                        if window.layout.as_ref().is_some_and(|l| {
                            l.cell_type == rmux_core::layout::LayoutType::LeftRight
                        }) {
                            layout_even_horizontal(sx, pane_height, &pane_ids)
                        } else {
                            layout_even_vertical(sx, pane_height, &pane_ids)
                        };

                    // Apply layout positions to panes
                    for &pid in &pane_ids {
                        if let Some(cell) = layout.find_pane(pid) {
                            if let Some(pane) = window.panes.get_mut(&pid) {
                                pane.resize(cell.sx, cell.sy);
                                pane.xoff = cell.x_off;
                                pane.yoff = cell.y_off;
                                pane_resizes.push((pid, cell.sx, cell.sy));
                            }
                        }
                    }

                    window.layout = Some(layout);
                }
            }
        }

        // Resize PTYs
        for (pane_id, new_sx, new_sy) in pane_resizes {
            if let Some(fd) = self.pty_fds.get(&pane_id) {
                let raw = fd.as_raw_fd();
                pty::Pty::resize_fd(raw, new_sx as u16, new_sy as u16).ok();
            }
        }

        // Mark clients for redraw
        self.mark_clients_redraw(session_id);
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
        let to_render: Vec<(u64, u32, u32, u32)> = self
            .clients
            .values_mut()
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

        render::render_window(window, &session.name, session.active_window, sx, sy)
    }

    /// Spawn a shell process for a pane.
    fn spawn_pane_process(
        &mut self,
        pane_id: u32,
        sx: u32,
        sy: u32,
        cwd: &str,
    ) -> Result<(), ServerError> {
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

    /// Clean up PTY resources for a pane.
    fn cleanup_pane(&mut self, pane_id: u32) {
        if let Some(task) = self.pty_tasks.remove(&pane_id) {
            task.abort();
        }
        self.pty_fds.remove(&pane_id);
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
            if n < 0 { Err(std::io::Error::last_os_error()) } else { Ok(n as usize) }
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
    // --- Client context ---

    fn set_command_client(&mut self, client_id: u64) {
        self.command_client = client_id;
    }

    fn command_client_id(&self) -> u64 {
        self.command_client
    }

    fn client_session_id(&self) -> Option<u32> {
        self.clients.get(&self.command_client).and_then(|c| c.session_id)
    }

    fn client_active_window(&self) -> Option<u32> {
        let session_id = self.client_session_id()?;
        let session = self.sessions.find_by_id(session_id)?;
        Some(session.active_window)
    }

    fn client_active_pane_id(&self) -> Option<u32> {
        let session_id = self.client_session_id()?;
        let session = self.sessions.find_by_id(session_id)?;
        let window = session.active_window()?;
        Some(window.active_pane)
    }

    fn client_sx(&self) -> u32 {
        self.clients.get(&self.command_client).map_or(80, |c| c.sx)
    }

    fn client_sy(&self) -> u32 {
        self.clients.get(&self.command_client).map_or(24, |c| c.sy)
    }

    // --- Session operations ---

    fn create_session(
        &mut self,
        name: &str,
        cwd: &str,
        sx: u32,
        sy: u32,
    ) -> Result<u32, ServerError> {
        let session = self.sessions.create(name.to_string(), cwd.to_string());
        let session_id = session.id;

        // Create initial window with one pane
        // Reserve 1 row for status line
        let pane_height = sy.saturating_sub(1);
        let mut window = Window::new("0".to_string(), sx, pane_height);
        let pane = Pane::new(sx, pane_height, 2000);
        let pane_id = pane.id;
        window.active_pane = pane_id;
        window.layout = Some(LayoutCell::new_pane(0, 0, sx, pane_height, pane_id));
        window.panes.insert(pane_id, pane);

        let window_idx = session.next_window_index();
        session.active_window = window_idx;
        session.windows.insert(window_idx, window);

        // Spawn the shell process
        self.spawn_pane_process(pane_id, sx, pane_height, cwd)?;

        Ok(session_id)
    }

    fn kill_session(&mut self, name: &str) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_name(name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {name}")))?;
        let id = session.id;

        // Clean up PTY tasks for all panes
        let session = self.sessions.find_by_id(id).unwrap();
        let pane_ids: Vec<u32> =
            session.windows.values().flat_map(|w| w.panes.keys()).copied().collect();

        for pane_id in &pane_ids {
            self.cleanup_pane(*pane_id);
        }

        self.sessions.remove(id);
        Ok(())
    }

    fn has_session(&self, name: &str) -> bool {
        self.sessions.find_by_name(name).is_some()
    }

    fn list_sessions(&self) -> Vec<String> {
        self.sessions
            .iter()
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

    fn rename_session(&mut self, name: &str, new_name: &str) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_name_mut(name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {name}")))?;
        session.name = new_name.to_string();
        Ok(())
    }

    // --- Window operations ---

    fn create_window(
        &mut self,
        session_id: u32,
        name: Option<&str>,
        cwd: &str,
    ) -> Result<(u32, u32), ServerError> {
        let sx = self.client_sx();
        let sy = self.client_sy();
        let pane_height = sy.saturating_sub(1);

        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;

        let window_name = name.unwrap_or("bash").to_string();
        let mut window = Window::new(window_name, sx, pane_height);
        let pane = Pane::new(sx, pane_height, 2000);
        let pane_id = pane.id;
        window.active_pane = pane_id;
        window.layout = Some(LayoutCell::new_pane(0, 0, sx, pane_height, pane_id));
        window.panes.insert(pane_id, pane);

        let window_idx = session.next_window_index();
        session.windows.insert(window_idx, window);

        self.spawn_pane_process(pane_id, sx, pane_height, cwd)?;

        Ok((window_idx, pane_id))
    }

    fn kill_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;

        let window = session
            .windows
            .remove(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

        // Clean up all panes
        let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
        for pane_id in pane_ids {
            self.cleanup_pane(pane_id);
        }

        // If the active window was killed, switch to another
        let session = self.sessions.find_by_id_mut(session_id).unwrap();
        if session.active_window == window_idx {
            if let Some(&next_idx) = session.windows.keys().next() {
                session.active_window = next_idx;
            }
        }

        // Mark clients for redraw
        self.mark_clients_redraw(session_id);

        Ok(())
    }

    fn select_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;

        if !session.windows.contains_key(&window_idx) {
            return Err(ServerError::Command(format!("window not found: {window_idx}")));
        }

        session.select_window(window_idx);
        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn next_window(&mut self, session_id: u32) -> Result<(), ServerError> {
        let changed = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let current = session.active_window;
            if let Some(next) = session.next_window_after(current) {
                session.select_window(next);
                true
            } else {
                false
            }
        };
        if changed {
            self.mark_clients_redraw(session_id);
        }
        Ok(())
    }

    fn previous_window(&mut self, session_id: u32) -> Result<(), ServerError> {
        let changed = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let current = session.active_window;
            if let Some(prev) = session.prev_window_before(current) {
                session.select_window(prev);
                true
            } else {
                false
            }
        };
        if changed {
            self.mark_clients_redraw(session_id);
        }
        Ok(())
    }

    fn last_window(&mut self, session_id: u32) -> Result<(), ServerError> {
        let changed = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            if let Some(last) = session.last_window {
                if session.windows.contains_key(&last) {
                    session.select_window(last);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if changed {
            self.mark_clients_redraw(session_id);
        }
        Ok(())
    }

    fn rename_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        name: &str,
    ) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;

        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

        window.name = name.to_string();
        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn list_windows(&self, session_id: u32) -> Vec<String> {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return Vec::new();
        };

        let mut indices: Vec<u32> = session.windows.keys().copied().collect();
        indices.sort_unstable();

        indices
            .iter()
            .map(|&idx| {
                let window = &session.windows[&idx];
                let active = if idx == session.active_window { "*" } else { "-" };
                let panes = window.pane_count();
                format!("{idx}: {}{active} ({panes} panes)", window.name)
            })
            .collect()
    }

    // --- Pane operations ---

    fn split_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        horizontal: bool,
        cwd: &str,
    ) -> Result<u32, ServerError> {
        let (pane_id, sx, sy, _pane_height) = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get_mut(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

            let active_pane_id = window.active_pane;

            // Create new pane (dimensions will be set after layout split)
            let new_pane = Pane::new(1, 1, 2000); // temporary dimensions
            let new_pane_id = new_pane.id;

            // Split the layout
            let layout = window.layout.get_or_insert_with(|| {
                LayoutCell::new_pane(0, 0, window.sx, window.sy, active_pane_id)
            });

            // Find the active pane's cell and split it
            let split_result = if horizontal {
                split_pane_in_layout(layout, active_pane_id, new_pane_id, true)
            } else {
                split_pane_in_layout(layout, active_pane_id, new_pane_id, false)
            };

            if !split_result {
                return Err(ServerError::Command("pane too small to split".into()));
            }

            // Get dimensions for both panes from the updated layout
            let old_cell = layout
                .find_pane(active_pane_id)
                .ok_or_else(|| ServerError::Command("layout error".into()))?;
            let old_sx = old_cell.sx;
            let old_sy = old_cell.sy;
            let old_xoff = old_cell.x_off;
            let old_yoff = old_cell.y_off;

            let new_cell = layout
                .find_pane(new_pane_id)
                .ok_or_else(|| ServerError::Command("layout error".into()))?;
            let new_sx = new_cell.sx;
            let new_sy = new_cell.sy;
            let new_xoff = new_cell.x_off;
            let new_yoff = new_cell.y_off;

            // Resize the existing pane
            if let Some(pane) = window.panes.get_mut(&active_pane_id) {
                pane.resize(old_sx, old_sy);
                pane.xoff = old_xoff;
                pane.yoff = old_yoff;
            }

            // Set up the new pane with correct dimensions
            let mut new_pane = Pane::new(new_sx, new_sy, 2000);
            // Override the ID to match what we registered in the layout
            // The Pane::new already generated a new ID, but we need the one from above
            // Actually new_pane already has new_pane_id since we captured it before
            new_pane.xoff = new_xoff;
            new_pane.yoff = new_yoff;
            let actual_id = new_pane.id;
            // We need to fix the layout to use the actual pane ID
            fix_pane_id_in_layout(window.layout.as_mut().unwrap(), new_pane_id, actual_id);

            window.panes.insert(actual_id, new_pane);
            window.active_pane = actual_id;

            (actual_id, new_sx, new_sy, window.sy)
        };

        // Resize old pane's PTY
        {
            let session = self.sessions.find_by_id(session_id).unwrap();
            let window = &session.windows[&window_idx];
            let active_orig = window.panes.keys().find(|&&id| id != pane_id).copied();
            if let Some(orig_id) = active_orig {
                if let Some(fd) = self.pty_fds.get(&orig_id) {
                    if let Some(pane) = window.panes.get(&orig_id) {
                        pty::Pty::resize_fd(fd.as_raw_fd(), pane.sx as u16, pane.sy as u16).ok();
                    }
                }
            }
        }

        // Spawn shell in new pane
        self.spawn_pane_process(pane_id, sx, sy, cwd)?;

        // Mark clients for redraw
        self.mark_clients_redraw(session_id);

        Ok(pane_id)
    }

    fn kill_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<(), ServerError> {
        // Check if only one pane - if so, kill the window instead
        {
            let session = self
                .sessions
                .find_by_id(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;
            if window.panes.len() <= 1 {
                self.cleanup_pane(pane_id);
                return self.kill_window(session_id, window_idx);
            }
        }

        // Clean up PTY
        self.cleanup_pane(pane_id);

        let session = self.sessions.find_by_id_mut(session_id).unwrap();
        let window = session.windows.get_mut(&window_idx).unwrap();

        window.panes.remove(&pane_id);

        // Update active pane if needed
        if window.active_pane == pane_id {
            if let Some(&next) = window.panes.keys().next() {
                window.active_pane = next;
            }
        }

        // Rebuild layout
        let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
        let was_horizontal = window
            .layout
            .as_ref()
            .is_some_and(|l| l.cell_type == rmux_core::layout::LayoutType::LeftRight);

        let layout = if was_horizontal {
            layout_even_horizontal(window.sx, window.sy, &pane_ids)
        } else {
            layout_even_vertical(window.sx, window.sy, &pane_ids)
        };

        // Apply new layout positions to panes
        for &pid in &pane_ids {
            if let Some(cell) = layout.find_pane(pid) {
                if let Some(pane) = window.panes.get_mut(&pid) {
                    pane.resize(cell.sx, cell.sy);
                    pane.xoff = cell.x_off;
                    pane.yoff = cell.y_off;
                }
            }
        }

        window.layout = Some(layout);

        // Resize PTYs for remaining panes
        for &pid in &pane_ids {
            if let Some(fd) = self.pty_fds.get(&pid) {
                if let Some(session) = self.sessions.find_by_id(session_id) {
                    if let Some(win) = session.windows.get(&window_idx) {
                        if let Some(pane) = win.panes.get(&pid) {
                            pty::Pty::resize_fd(fd.as_raw_fd(), pane.sx as u16, pane.sy as u16)
                                .ok();
                        }
                    }
                }
            }
        }

        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn select_pane_id(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

        if !window.panes.contains_key(&pane_id) {
            return Err(ServerError::Command(format!("pane not found: %{pane_id}")));
        }

        window.active_pane = pane_id;
        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn select_pane_direction(
        &mut self,
        session_id: u32,
        window_idx: u32,
        direction: Direction,
    ) -> Result<(), ServerError> {
        let target = {
            let session = self
                .sessions
                .find_by_id(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

            let Some(layout) = &window.layout else {
                return Ok(()); // No layout, nothing to navigate
            };

            let nav_dir = match direction {
                Direction::Up => navigate::Direction::Up,
                Direction::Down => navigate::Direction::Down,
                Direction::Left => navigate::Direction::Left,
                Direction::Right => navigate::Direction::Right,
            };

            navigate::find_pane_in_direction(layout, window.active_pane, nav_dir)
        };

        if let Some(target) = target {
            self.select_pane_id(session_id, window_idx, target)?;
        }

        Ok(())
    }

    fn list_panes(&self, session_id: u32, window_idx: u32) -> Vec<String> {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return Vec::new();
        };
        let Some(window) = session.windows.get(&window_idx) else {
            return Vec::new();
        };

        window
            .panes
            .values()
            .map(|pane| {
                let active = if pane.id == window.active_pane { " (active)" } else { "" };
                format!(
                    "%{}: [{}x{}] [offset {},{} ]{}",
                    pane.id, pane.sx, pane.sy, pane.xoff, pane.yoff, active
                )
            })
            .collect()
    }

    // --- Info ---

    fn list_clients(&self) -> Vec<String> {
        self.clients
            .values()
            .map(|c| {
                let session = c.session_id.map_or_else(
                    || "(unattached)".to_string(),
                    |sid| {
                        self.sessions
                            .find_by_id(sid)
                            .map_or_else(|| format!("(session {sid})"), |s| s.name.clone())
                    },
                );
                format!("client {}: {}x{} {}", c.id, c.sx, c.sy, session)
            })
            .collect()
    }

    fn list_all_commands(&self) -> Vec<String> {
        command::builtins::COMMANDS
            .iter()
            .map(|cmd| format!("{} {}", cmd.name, cmd.usage))
            .collect()
    }

    fn list_key_bindings(&self) -> Vec<String> {
        self.keybindings.list_bindings()
    }

    // --- Redraw ---

    fn mark_clients_redraw(&mut self, session_id: u32) {
        for client in self.clients.values_mut() {
            if client.session_id == Some(session_id) && client.is_attached() {
                client.mark_redraw();
            }
        }
    }
}

/// Split a pane in the layout tree. Returns true if successful.
fn split_pane_in_layout(
    layout: &mut LayoutCell,
    target_pane_id: u32,
    new_pane_id: u32,
    horizontal: bool,
) -> bool {
    if layout.is_pane() && layout.pane_id == Some(target_pane_id) {
        if horizontal {
            return layout.split_horizontal(new_pane_id).is_some();
        }
        return layout.split_vertical(new_pane_id).is_some();
    }

    for child in &mut layout.children {
        if split_pane_in_layout(child, target_pane_id, new_pane_id, horizontal) {
            return true;
        }
    }

    false
}

/// Fix a pane ID in the layout tree (when the actual Pane::new ID differs from the placeholder).
fn fix_pane_id_in_layout(layout: &mut LayoutCell, old_id: u32, new_id: u32) {
    if layout.pane_id == Some(old_id) {
        layout.pane_id = Some(new_id);
    }
    for child in &mut layout.children {
        fix_pane_id_in_layout(child, old_id, new_id);
    }
}
