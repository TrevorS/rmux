//! Server event loop.
//!
//! The server listens on a Unix domain socket and accepts client connections.
//! It manages all sessions, windows, and panes using a tokio single-threaded runtime.

use crate::client::{ClientFlags, PromptState, ServerClient};
use crate::command::{self, CommandResult, CommandServer, Direction};
use crate::copymode::{self, CopyModeAction};
use crate::keybind::{KeyBindings, string_to_key};
use crate::navigate;
use crate::pane::Pane;
use crate::render;
use crate::session::SessionManager;
use crate::window::Window;
use rmux_core::layout::{LayoutCell, layout_even_horizontal, layout_even_vertical};
use rmux_core::options::OptionValue;
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
    /// Server-level options.
    pub options: rmux_core::options::Options,
    /// Global paste buffer storage.
    paste_buffers: crate::paste::PasteBufferStore,
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
            options: rmux_core::options::default_server_options(),
            paste_buffers: crate::paste::PasteBufferStore::default(),
        }
    }

    /// Get the default socket path (matching tmux's convention).
    pub fn default_socket_path() -> PathBuf {
        let tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        let uid = nix::unistd::getuid();
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
                    self.handle_pty_output(pane_id, &data).await;
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

    async fn handle_pty_output(&mut self, pane_id: u32, data: &[u8]) {
        if data.is_empty() {
            // EOF sentinel: the pane's process exited.
            self.handle_pane_exit(pane_id).await;
            return;
        }

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

    /// Handle a pane whose process has exited.
    async fn handle_pane_exit(&mut self, pane_id: u32) {
        tracing::info!("pane {pane_id} process exited");

        // Find which session/window owns this pane
        let location = self
            .sessions
            .iter()
            .flat_map(|s| s.windows.iter().map(move |(&widx, w)| (s.id, widx, w)))
            .find(|(_, _, w)| w.panes.contains_key(&pane_id))
            .map(|(sid, widx, w)| (sid, widx, w.panes.len()));

        let Some((session_id, window_idx, pane_count)) = location else {
            // Orphan pane, just clean up
            self.cleanup_pane(pane_id);
            return;
        };

        self.cleanup_pane(pane_id);

        if pane_count <= 1 {
            // Last pane in window — remove the window
            if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                session.windows.remove(&window_idx);

                if session.windows.is_empty() {
                    // Last window in session — remove session and detach clients
                    let sid = session_id;
                    self.sessions.remove(sid);
                    self.detach_session_clients(sid).await;
                    return;
                }

                // Switch to another window if the active one was closed
                if session.active_window == window_idx {
                    if let Some(&next_idx) = session.windows.keys().next() {
                        session.active_window = next_idx;
                    }
                }
            }
        } else {
            // Remove just this pane from the window
            if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                if let Some(window) = session.windows.get_mut(&window_idx) {
                    window.panes.remove(&pane_id);
                    if window.active_pane == pane_id {
                        if let Some(&next) = window.panes.keys().next() {
                            window.active_pane = next;
                        }
                    }
                    // Rebuild layout
                    let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
                    if pane_ids.len() > 1 {
                        window.layout =
                            Some(layout_even_horizontal(window.sx, window.sy, &pane_ids));
                    } else {
                        window.layout = pane_ids.first().map(|&pid| {
                            rmux_core::layout::LayoutCell::new_pane(0, 0, window.sx, window.sy, pid)
                        });
                    }
                }
            }
        }

        self.mark_clients_redraw(session_id);
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
            Ok(CommandResult::RunShell(cmd)) => {
                let output =
                    match tokio::process::Command::new("sh").arg("-c").arg(&cmd).output().await {
                        Ok(out) => {
                            let mut result = out.stdout;
                            result.extend_from_slice(&out.stderr);
                            String::from_utf8_lossy(&result).into_owned()
                        }
                        Err(e) => format!("run-shell: {e}\n"),
                    };
                if let Some(client) = self.clients.get_mut(&client_id) {
                    if !output.is_empty() {
                        client.send(&Message::OutputData(output.into_bytes())).await.ok();
                    }
                    if !client.is_attached() {
                        client.send(&Message::Exit).await.ok();
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("{e}\n");
                if let Some(client) = self.clients.get_mut(&client_id) {
                    if client.is_attached() {
                        // For attached clients, log the error but don't disconnect.
                        // tmux shows errors in the status line; we just log for now.
                        tracing::warn!("command error for attached client {client_id}: {e}");
                    } else {
                        client.send(&Message::ErrorOutput(err_msg.into_bytes())).await.ok();
                        client.send(&Message::Exit).await.ok();
                    }
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

        // Check if client is in command prompt mode
        if client.prompt.is_some() {
            self.handle_prompt_input(client_id, data);
            return;
        }

        // Check for mouse events early (before copy mode or keybinding processing)
        if let Some(event) = rmux_terminal::keys::parse_key_event(data) {
            if rmux_core::key::keyc_is_mouse(event.key) {
                self.handle_mouse_event(client_id, session_id, &event);
                return;
            }
        }

        // Check if active pane is in copy mode
        if self.is_active_pane_in_copy_mode(session_id) {
            self.handle_copy_mode_input(client_id, session_id, data);
            return;
        }

        // Normal mode: process all keys in the buffer. A single read may
        // contain multiple keys (e.g. prefix + command arriving together),
        // so we loop until the buffer is consumed.
        let mut offset = 0;
        while offset < data.len() {
            let remaining = &data[offset..];
            let (action, consumed) = self.keybindings.process_input(remaining);
            match action {
                Some(crate::keybind::KeyAction::SendToPane(bytes)) => {
                    if !bytes.is_empty() {
                        self.write_to_active_pane(session_id, &bytes);
                    }
                }
                Some(crate::keybind::KeyAction::Command(argv)) => {
                    self.queue_command(client_id, argv);
                }
                None => {
                    // Pass through to pane
                    self.write_to_active_pane(session_id, &remaining[..consumed]);
                }
            }
            offset += consumed;
        }
    }

    /// Queue a command for execution via the event channel.
    fn queue_command(&self, client_id: u64, argv: Vec<String>) {
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

    /// Handle a mouse event from a client.
    fn handle_mouse_event(
        &mut self,
        client_id: u64,
        session_id: u32,
        event: &rmux_terminal::keys::KeyEvent,
    ) {
        use rmux_core::key::*;

        // Check if the `mouse` option is enabled
        let mouse_enabled = self.options.get_flag("mouse").ok().unwrap_or(false);

        if !mouse_enabled {
            // Mouse disabled: forward to PTY if the pane has mouse mode flags
            // (for vim, htop, etc.)
            let forward = self.pane_wants_mouse(session_id);
            if forward {
                // Re-encode as SGR and forward to the active pane
                let encoded =
                    rmux_terminal::mouse::encode_sgr_mouse(event.key, event.mouse_x, event.mouse_y);
                self.write_to_active_pane(session_id, &encoded);
            }
            return;
        }

        let base = keyc_base(event.key);
        let mx = event.mouse_x;
        let my = event.mouse_y;

        match base {
            KEYC_MOUSEDOWN1 => {
                // Click: select pane at position, or position cursor in copy mode
                if self.is_active_pane_in_copy_mode(session_id) {
                    // In copy mode: position cursor
                    self.copy_mode_position_cursor(session_id, mx, my);
                } else {
                    // Select the pane at the click position
                    self.select_pane_at_position(session_id, mx, my);
                }
                self.mark_clients_redraw(session_id);
            }
            KEYC_MOUSEDRAG1 => {
                // Drag: begin/extend selection in copy mode
                if !self.is_active_pane_in_copy_mode(session_id) {
                    // Enter copy mode first
                    self.enter_copy_mode_for_active_pane(session_id);
                }
                self.copy_mode_drag_selection(session_id, mx, my);
                self.mark_clients_redraw(session_id);
            }
            KEYC_MOUSEUP1 => {
                // Release after drag: copy selection if any
                if self.is_active_pane_in_copy_mode(session_id) {
                    self.copy_mode_finish_selection(session_id);
                    self.mark_clients_redraw(session_id);
                }
            }
            KEYC_WHEELUP => {
                if !self.is_active_pane_in_copy_mode(session_id) {
                    self.enter_copy_mode_for_active_pane(session_id);
                }
                // Scroll up 3 lines
                self.copy_mode_scroll_up(session_id, 3);
                self.mark_clients_redraw(session_id);
            }
            KEYC_WHEELDOWN => {
                if self.is_active_pane_in_copy_mode(session_id) {
                    self.copy_mode_scroll_down(session_id, 3);
                    // If we scrolled back to the bottom, exit copy mode
                    self.maybe_exit_copy_mode_at_bottom(session_id);
                    self.mark_clients_redraw(session_id);
                }
            }
            _ => {
                // Other mouse events (middle/right click, etc.) - ignore for now
            }
        }

        // Suppress unused warning for client_id; may be needed for per-client mouse state later
        let _ = client_id;
    }

    /// Check if the active pane's screen has mouse mode flags (for vim/htop forwarding).
    fn pane_wants_mouse(&self, session_id: u32) -> bool {
        use rmux_core::screen::ModeFlags;
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return false;
        };
        let Some(window) = session.active_window() else {
            return false;
        };
        let Some(pane) = window.active_pane() else {
            return false;
        };
        let mode = pane.screen.mode;
        mode.intersects(
            ModeFlags::MOUSE_STANDARD
                | ModeFlags::MOUSE_BUTTON
                | ModeFlags::MOUSE_ANY
                | ModeFlags::MOUSE_SGR,
        )
    }

    /// Select the pane at screen coordinates.
    fn select_pane_at_position(&mut self, session_id: u32, x: u32, y: u32) {
        let pane_id = {
            let Some(session) = self.sessions.find_by_id(session_id) else {
                return;
            };
            let Some(window) = session.active_window() else {
                return;
            };
            let Some(layout) = &window.layout else {
                return;
            };
            layout.pane_at(x, y)
        };

        if let Some(pid) = pane_id {
            if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                if let Some(window) = session.active_window_mut() {
                    if window.panes.contains_key(&pid) {
                        window.last_active_pane = Some(window.active_pane);
                        window.active_pane = pid;
                    }
                }
            }
        }
    }

    /// Enter copy mode on the active pane of a session.
    fn enter_copy_mode_for_active_pane(&mut self, session_id: u32) {
        let mode_keys = self.options.get_string("mode-keys").ok().unwrap_or("emacs").to_string();
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    if !pane.is_in_copy_mode() {
                        pane.enter_copy_mode(&mode_keys);
                    }
                }
            }
        }
    }

    /// Position the copy mode cursor at screen coordinates.
    fn copy_mode_position_cursor(&mut self, session_id: u32, x: u32, y: u32) {
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    // Convert screen coords to pane-local coords
                    let px = x.saturating_sub(pane.xoff);
                    let py = y.saturating_sub(pane.yoff);
                    if let Some(cm) = &mut pane.copy_mode {
                        cm.cx = px.min(pane.sx.saturating_sub(1));
                        cm.cy = py.min(pane.sy.saturating_sub(1));
                    }
                }
            }
        }
    }

    /// Extend selection during mouse drag in copy mode.
    fn copy_mode_drag_selection(&mut self, session_id: u32, x: u32, y: u32) {
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    let px = x.saturating_sub(pane.xoff);
                    let py = y.saturating_sub(pane.yoff);
                    if let Some(cm) = &mut pane.copy_mode {
                        if !cm.selecting {
                            // Start selection at current position
                            let hs = pane.screen.grid.history_size();
                            cm.begin_selection(hs);
                        }
                        // Update cursor to drag position (extends selection)
                        cm.cx = px.min(pane.sx.saturating_sub(1));
                        cm.cy = py.min(pane.sy.saturating_sub(1));
                    }
                }
            }
        }
    }

    /// Finish mouse selection and copy to paste buffer.
    fn copy_mode_finish_selection(&mut self, session_id: u32) {
        let copy_data = {
            let Some(session) = self.sessions.find_by_id(session_id) else {
                return;
            };
            let Some(window) = session.active_window() else {
                return;
            };
            let Some(pane) = window.active_pane() else {
                return;
            };
            let Some(cm) = &pane.copy_mode else {
                return;
            };
            if cm.selecting { copymode::copy_selection(&pane.screen, cm) } else { None }
        };

        if let Some(data) = copy_data {
            self.paste_buffers.add(data);
        }

        // Exit copy mode
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    pane.exit_copy_mode();
                }
            }
        }
    }

    /// Scroll up in copy mode.
    fn copy_mode_scroll_up(&mut self, session_id: u32, lines: u32) {
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    if let Some(cm) = &mut pane.copy_mode {
                        let max_oy = pane.screen.grid.history_size();
                        cm.oy = (cm.oy + lines).min(max_oy);
                    }
                }
            }
        }
    }

    /// Scroll down in copy mode.
    fn copy_mode_scroll_down(&mut self, session_id: u32, lines: u32) {
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    if let Some(cm) = &mut pane.copy_mode {
                        cm.oy = cm.oy.saturating_sub(lines);
                    }
                }
            }
        }
    }

    /// Exit copy mode if scrolled back to the live screen.
    fn maybe_exit_copy_mode_at_bottom(&mut self, session_id: u32) {
        let should_exit = {
            let Some(session) = self.sessions.find_by_id(session_id) else {
                return;
            };
            let Some(window) = session.active_window() else {
                return;
            };
            let Some(pane) = window.active_pane() else {
                return;
            };
            pane.copy_mode.as_ref().is_some_and(|cm| cm.oy == 0 && !cm.selecting)
        };

        if should_exit {
            if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                if let Some(window) = session.active_window_mut() {
                    if let Some(pane) = window.active_pane_mut() {
                        pane.exit_copy_mode();
                    }
                }
            }
        }
    }

    /// Check if the active pane of a session is in copy mode.
    fn is_active_pane_in_copy_mode(&self, session_id: u32) -> bool {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return false;
        };
        let Some(window) = session.active_window() else {
            return false;
        };
        let Some(pane) = window.active_pane() else {
            return false;
        };
        pane.is_in_copy_mode()
    }

    /// Handle input when the active pane is in copy mode.
    fn handle_copy_mode_input(&mut self, _client_id: u64, session_id: u32, data: &[u8]) {
        use rmux_terminal::keys::parse_key;

        // Parse the raw input into a key code
        let Some((key, _consumed)) = parse_key(data) else {
            return;
        };

        // Get the key table name from the copy mode state
        let key_table = {
            let Some(session) = self.sessions.find_by_id(session_id) else {
                return;
            };
            let Some(window) = session.active_window() else {
                return;
            };
            let Some(pane) = window.active_pane() else {
                return;
            };
            let Some(cm) = &pane.copy_mode else {
                return;
            };
            cm.key_table.clone()
        };

        // Look up the key in the copy mode key table
        let base = rmux_core::key::keyc_base(key);
        let action_name = self
            .keybindings
            .lookup_in_table(&key_table, base)
            .or_else(|| self.keybindings.lookup_in_table(&key_table, key))
            .map(|argv| argv[0].clone());

        let Some(action_name) = action_name else {
            // Key not bound in copy mode — ignore
            return;
        };

        // Dispatch the action on the pane's copy mode state
        let action = {
            let Some(session) = self.sessions.find_by_id_mut(session_id) else {
                return;
            };
            let Some(window) = session.active_window_mut() else {
                return;
            };
            let Some(pane) = window.active_pane_mut() else {
                return;
            };
            let Some(cm) = &mut pane.copy_mode else {
                return;
            };
            copymode::dispatch_copy_mode_action(&pane.screen, cm, &action_name)
        };

        match action {
            CopyModeAction::Handled => {
                // Redraw needed
                self.mark_clients_redraw(session_id);
            }
            CopyModeAction::Exit { copy_data } => {
                // Add copied data to paste buffer store
                if let Some(data) = copy_data {
                    self.paste_buffers.add(data);
                }
                // Exit copy mode on the pane
                if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                    if let Some(window) = session.active_window_mut() {
                        if let Some(pane) = window.active_pane_mut() {
                            pane.exit_copy_mode();
                        }
                    }
                }
                self.mark_clients_redraw(session_id);
            }
            CopyModeAction::Unhandled => {
                // Not recognized — ignore
            }
        }
    }

    /// Handle input when the client is in command prompt mode.
    fn handle_prompt_input(&mut self, client_id: u64, data: &[u8]) {
        let mut offset = 0;
        while offset < data.len() {
            let remaining = &data[offset..];

            // Handle raw bytes directly for control chars that parse_key
            // encodes as Ctrl+letter (losing the original semantic).
            match remaining[0] {
                // Enter (\r or \n) — parse_key maps these to Ctrl-M / Ctrl-J
                0x0D | 0x0A => {
                    let cmd_str = {
                        let Some(client) = self.clients.get_mut(&client_id) else {
                            return;
                        };
                        let cmd =
                            client.prompt.as_ref().map(|p| p.buffer.clone()).unwrap_or_default();
                        client.prompt = None;
                        cmd
                    };
                    if !cmd_str.is_empty() {
                        let argv: Vec<String> =
                            cmd_str.split_whitespace().map(String::from).collect();
                        self.queue_command(client_id, argv);
                    }
                    self.mark_prompt_redraw(client_id);
                    return; // Enter always ends prompt input
                }
                // Escape — parse_key returns None for bare ESC (wants more bytes)
                0x1B if remaining.len() == 1 || remaining[1] == 0x1B => {
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        client.prompt = None;
                    }
                    self.mark_prompt_redraw(client_id);
                    return; // Escape always ends prompt input
                }
                // Backspace (DEL) or Ctrl-H
                0x7F | 0x08 => {
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        if let Some(prompt) = &mut client.prompt {
                            prompt.buffer.pop();
                        }
                    }
                    self.mark_prompt_redraw(client_id);
                    offset += 1;
                }
                // Ctrl-U — clear the line
                0x15 => {
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        if let Some(prompt) = &mut client.prompt {
                            prompt.buffer.clear();
                        }
                    }
                    self.mark_prompt_redraw(client_id);
                    offset += 1;
                }
                // Printable ASCII
                0x20..=0x7E => {
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        if let Some(prompt) = &mut client.prompt {
                            prompt.buffer.push(remaining[0] as char);
                        }
                    }
                    self.mark_prompt_redraw(client_id);
                    offset += 1;
                }
                // UTF-8 multi-byte sequences — accept printable Unicode in prompt
                0xC2..=0xF4 => {
                    let utf8_len = match remaining[0] {
                        0xC2..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        0xF0..=0xF4 => 4,
                        _ => 1,
                    };
                    if remaining.len() >= utf8_len {
                        if let Ok(s) = std::str::from_utf8(&remaining[..utf8_len]) {
                            if let Some(ch) = s.chars().next() {
                                if !ch.is_control() {
                                    if let Some(client) = self.clients.get_mut(&client_id) {
                                        if let Some(prompt) = &mut client.prompt {
                                            prompt.buffer.push(ch);
                                        }
                                    }
                                    self.mark_prompt_redraw(client_id);
                                }
                            }
                            offset += utf8_len;
                        } else {
                            offset += 1;
                        }
                    } else {
                        break; // Incomplete UTF-8 sequence, wait for more data
                    }
                }
                // ESC sequence (not bare ESC) — skip it, not meaningful in prompt
                0x1B => {
                    // Consume the escape sequence so we don't get stuck
                    let (_, consumed) = rmux_terminal::keys::parse_key(remaining)
                        .unwrap_or((rmux_core::key::KEYC_UNKNOWN, 1));
                    offset += consumed;
                }
                // Other control chars — ignore
                _ => {
                    offset += 1;
                }
            }
        }
    }

    fn mark_prompt_redraw(&mut self, client_id: u64) {
        let session_id = self.clients.get(&client_id).and_then(|c| c.session_id);
        if let Some(sid) = session_id {
            self.mark_clients_redraw(sid);
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

    /// Detach all clients attached to a session (e.g. when session is destroyed).
    async fn detach_session_clients(&mut self, session_id: u32) {
        let client_ids: Vec<u64> = self
            .clients
            .values()
            .filter(|c| c.session_id == Some(session_id) && c.is_attached())
            .map(|c| c.id)
            .collect();

        for cid in client_ids {
            if let Some(client) = self.clients.get_mut(&cid) {
                client.detach();
                client.send(&Message::Exited).await.ok();
            }
        }
    }

    async fn render_clients(&mut self) {
        // Collect client IDs that need redraw, along with their session IDs, sizes, and prompt
        let to_render: Vec<(u64, u32, u32, u32, Option<String>)> = self
            .clients
            .values_mut()
            .filter_map(|c| {
                if c.needs_redraw() {
                    let prompt = c.prompt.as_ref().map(|p| p.buffer.clone());
                    c.session_id.map(|sid| (c.id, sid, c.sx, c.sy, prompt))
                } else {
                    None
                }
            })
            .collect();

        for (client_id, session_id, sx, sy, prompt) in to_render {
            let output = self.render_session(session_id, sx, sy, prompt.as_deref());
            if let Some(client) = self.clients.get_mut(&client_id) {
                client.send(&Message::OutputData(output)).await.ok();
            }
        }
    }

    fn render_session(&self, session_id: u32, sx: u32, sy: u32, prompt: Option<&str>) -> Vec<u8> {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return Vec::new();
        };
        let Some(window) = session.active_window() else {
            return Vec::new();
        };

        // Build window list for status line
        let mut window_list: Vec<render::WindowInfo> = session
            .windows
            .iter()
            .map(|(&idx, w)| render::WindowInfo {
                idx,
                name: w.name.clone(),
                is_active: idx == session.active_window,
            })
            .collect();
        window_list.sort_by_key(|w| w.idx);

        render::render_window(window, &session.name, sx, sy, &window_list, prompt)
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

    // Notify the server that this pane's process exited (empty vec = EOF sentinel).
    tx.send((pane_id, Vec::new())).await.ok();
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

    fn session_name_for_id(&self, id: u32) -> Option<String> {
        self.sessions.find_by_id(id).map(|s| s.name.clone())
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

            // Set up the new pane with the same ID the layout already has
            let mut new_pane = Pane::with_id(new_pane_id, new_sx, new_sy, 2000);
            new_pane.xoff = new_xoff;
            new_pane.yoff = new_yoff;

            window.panes.insert(new_pane_id, new_pane);
            window.active_pane = new_pane_id;

            (new_pane_id, new_sx, new_sy, window.sy)
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

    fn active_window_for(&self, session_id: u32) -> Option<u32> {
        let session = self.sessions.find_by_id(session_id)?;
        Some(session.active_window)
    }

    fn active_pane_id_for(&self, session_id: u32, window_idx: u32) -> Option<u32> {
        let session = self.sessions.find_by_id(session_id)?;
        let window = session.windows.get(&window_idx)?;
        Some(window.active_pane)
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

    // --- PTY I/O ---

    fn write_to_pane(
        &self,
        _session_id: u32,
        _window_idx: u32,
        pane_id: u32,
        data: &[u8],
    ) -> Result<(), ServerError> {
        if let Some(fd) = self.pty_fds.get(&pane_id) {
            nix::unistd::write(fd, data)
                .map_err(|e| ServerError::Io(std::io::Error::other(e.to_string())))?;
            Ok(())
        } else {
            Err(ServerError::Command(format!("pane %{pane_id} has no PTY")))
        }
    }

    // --- Options ---

    fn get_server_option(&self, key: &str) -> Result<String, ServerError> {
        match self.options.get(key) {
            Some(val) => Ok(format_option_value(val)),
            None => Err(ServerError::Command(format!("unknown option: {key}"))),
        }
    }

    fn set_server_option(&mut self, key: &str, value: &str) -> Result<(), ServerError> {
        let val = parse_option_value(value);
        self.options.set(key, val);
        Ok(())
    }

    fn set_session_option(
        &mut self,
        session_id: u32,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let val = parse_option_value(value);
        session.options.set(key, val);
        Ok(())
    }

    fn set_window_option(
        &mut self,
        session_id: u32,
        window_idx: u32,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;
        let val = parse_option_value(value);
        window.options.set(key, val);
        Ok(())
    }

    fn show_options(&self, scope: &str, target_id: Option<u32>) -> Vec<String> {
        let opts: Vec<String> = match scope {
            "server" => self
                .options
                .local_iter()
                .map(|(k, v)| format!("{k} {}", format_option_value(v)))
                .collect(),
            "session" => {
                if let Some(session) = target_id.and_then(|id| self.sessions.find_by_id(id)) {
                    session
                        .options
                        .local_iter()
                        .map(|(k, v)| format!("{k} {}", format_option_value(v)))
                        .collect()
                } else {
                    Vec::new()
                }
            }
            "window" => {
                if let Some(session) = target_id.and_then(|id| self.sessions.find_by_id(id)) {
                    if let Some(window) = session.active_window() {
                        window
                            .options
                            .local_iter()
                            .map(|(k, v)| format!("{k} {}", format_option_value(v)))
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        let mut sorted = opts;
        sorted.sort();
        sorted
    }

    // --- Key bindings ---

    fn add_key_binding(
        &mut self,
        table: &str,
        key_name: &str,
        argv: Vec<String>,
    ) -> Result<(), ServerError> {
        let key = string_to_key(key_name)
            .ok_or_else(|| ServerError::Command(format!("unknown key: {key_name}")))?;
        self.keybindings.add_binding(table, key, argv);
        Ok(())
    }

    fn remove_key_binding(&mut self, table: &str, key_name: &str) -> Result<(), ServerError> {
        let key = string_to_key(key_name)
            .ok_or_else(|| ServerError::Command(format!("unknown key: {key_name}")))?;
        if !self.keybindings.remove_binding(table, key) {
            return Err(ServerError::Command(format!("key not bound: {key_name}")));
        }
        Ok(())
    }

    // --- Config ---

    fn execute_config_commands(&mut self, commands: Vec<Vec<String>>) -> Vec<String> {
        let mut errors = Vec::new();
        for argv in commands {
            if let Err(e) = crate::command::execute_command(&argv, self) {
                errors.push(format!("{e}"));
            }
        }
        errors
    }

    // --- Capture ---

    fn capture_pane(
        &self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<String, ServerError> {
        let session = self
            .sessions
            .find_by_id(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;
        let pane = window
            .panes
            .get(&pane_id)
            .ok_or_else(|| ServerError::Command(format!("pane not found: %{pane_id}")))?;

        let mut lines = Vec::new();
        for y in 0..pane.screen.grid.height() {
            let mut line_buf = String::new();
            for x in 0..pane.screen.grid.width() {
                let cell = pane.screen.grid.get_cell(x, y);
                let bytes = cell.data.as_bytes();
                if let Ok(s) = std::str::from_utf8(bytes) {
                    line_buf.push_str(s);
                } else {
                    line_buf.push(' ');
                }
            }
            // Trim trailing whitespace from each line
            let trimmed = line_buf.trim_end();
            lines.push(trimmed.to_string());
        }

        // Join with newlines and add trailing newline
        Ok(lines.join("\n") + "\n")
    }

    // --- Resize ---

    fn resize_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
        direction: Option<Direction>,
        amount: u32,
    ) -> Result<(), ServerError> {
        use rmux_core::layout::ResizeDirection;

        let dir = direction.ok_or_else(|| {
            ServerError::Command("resize-pane requires a direction (-U/-D/-L/-R)".into())
        })?;

        let resize_dir = match dir {
            Direction::Up => ResizeDirection::Up,
            Direction::Down => ResizeDirection::Down,
            Direction::Left => ResizeDirection::Left,
            Direction::Right => ResizeDirection::Right,
        };

        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

        let layout = window
            .layout
            .as_mut()
            .ok_or_else(|| ServerError::Command("no layout".into()))?;

        if !layout.resize_pane(pane_id, resize_dir, amount) {
            return Err(ServerError::Command("cannot resize pane in that direction".into()));
        }

        // Update pane screen sizes to match new layout dimensions
        for id in layout.pane_ids() {
            if let (Some(lc), Some(pane)) = (layout.find_pane(id), window.panes.get_mut(&id)) {
                pane.screen.resize(lc.sx, lc.sy);
                pane.xoff = lc.x_off;
                pane.yoff = lc.y_off;
            }
        }

        Ok(())
    }

    // --- Swap/Move ---

    fn swap_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        src: u32,
        dst: u32,
    ) -> Result<(), ServerError> {
        {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get_mut(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

            if !window.panes.contains_key(&src) {
                return Err(ServerError::Command(format!("pane not found: %{src}")));
            }
            if !window.panes.contains_key(&dst) {
                return Err(ServerError::Command(format!("pane not found: %{dst}")));
            }

            // Swap the pane screen/parser state but keep their layout positions
            let mut pane_a = window.panes.remove(&src).unwrap();
            let mut pane_b = window.panes.remove(&dst).unwrap();

            // Swap layout positions (offsets and sizes)
            std::mem::swap(&mut pane_a.xoff, &mut pane_b.xoff);
            std::mem::swap(&mut pane_a.yoff, &mut pane_b.yoff);
            std::mem::swap(&mut pane_a.sx, &mut pane_b.sx);
            std::mem::swap(&mut pane_a.sy, &mut pane_b.sy);

            // Re-insert with swapped IDs (pane_a gets dst's slot, pane_b gets src's slot)
            window.panes.insert(src, pane_b);
            window.panes.insert(dst, pane_a);
        }

        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn swap_window(
        &mut self,
        session_id: u32,
        src_idx: u32,
        dst_idx: u32,
    ) -> Result<(), ServerError> {
        {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;

            if !session.windows.contains_key(&src_idx) {
                return Err(ServerError::Command(format!("window not found: {src_idx}")));
            }
            if !session.windows.contains_key(&dst_idx) {
                return Err(ServerError::Command(format!("window not found: {dst_idx}")));
            }

            let window_a = session.windows.remove(&src_idx).unwrap();
            let window_b = session.windows.remove(&dst_idx).unwrap();
            session.windows.insert(src_idx, window_b);
            session.windows.insert(dst_idx, window_a);
        }

        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn move_window(
        &mut self,
        src_session_id: u32,
        src_idx: u32,
        dst_session_id: u32,
        dst_idx: u32,
    ) -> Result<(), ServerError> {
        // Remove window from source session
        let window = {
            let session = self
                .sessions
                .find_by_id_mut(src_session_id)
                .ok_or_else(|| ServerError::Command("source session not found".into()))?;
            session
                .windows
                .remove(&src_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {src_idx}")))?
        };

        // Fix active window in source session if needed
        {
            let session = self.sessions.find_by_id_mut(src_session_id).unwrap();
            if session.active_window == src_idx {
                if let Some(&next) = session.windows.keys().next() {
                    session.active_window = next;
                }
            }
        }

        // Insert into destination session
        {
            let session = self
                .sessions
                .find_by_id_mut(dst_session_id)
                .ok_or_else(|| ServerError::Command("destination session not found".into()))?;
            // If dst_idx is already taken, remove it first
            if session.windows.contains_key(&dst_idx) {
                return Err(ServerError::Command(format!(
                    "window index {dst_idx} already exists in destination session"
                )));
            }
            session.windows.insert(dst_idx, window);
        }

        self.mark_clients_redraw(src_session_id);
        self.mark_clients_redraw(dst_session_id);
        Ok(())
    }

    fn break_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<u32, ServerError> {
        let sx = self.client_sx();
        let sy = self.client_sy();
        let pane_height = sy.saturating_sub(1);

        // Remove the pane from the source window
        let mut pane = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get_mut(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

            if window.panes.len() <= 1 {
                return Err(ServerError::Command("cannot break with only one pane".into()));
            }

            let pane = window
                .panes
                .remove(&pane_id)
                .ok_or_else(|| ServerError::Command(format!("pane not found: %{pane_id}")))?;

            // Update active pane if needed
            if window.active_pane == pane_id {
                if let Some(&next) = window.panes.keys().next() {
                    window.active_pane = next;
                }
            }

            // Rebuild layout for remaining panes
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
            for &pid in &pane_ids {
                if let Some(cell) = layout.find_pane(pid) {
                    if let Some(p) = window.panes.get_mut(&pid) {
                        p.resize(cell.sx, cell.sy);
                        p.xoff = cell.x_off;
                        p.yoff = cell.y_off;
                    }
                }
            }
            window.layout = Some(layout);

            pane
        };

        // Create a new window with this pane
        let new_window_idx = {
            let session = self.sessions.find_by_id_mut(session_id).unwrap();
            let new_idx = session.next_window_index();
            pane.resize(sx, pane_height);
            pane.xoff = 0;
            pane.yoff = 0;
            let mut new_window = Window::new("bash".to_string(), sx, pane_height);
            new_window.active_pane = pane.id;
            new_window.layout = Some(LayoutCell::new_pane(0, 0, sx, pane_height, pane.id));
            new_window.panes.insert(pane.id, pane);
            session.windows.insert(new_idx, new_window);
            new_idx
        };

        // Resize remaining panes' PTYs in original window
        {
            let session = self.sessions.find_by_id(session_id).unwrap();
            if let Some(win) = session.windows.get(&window_idx) {
                for (&pid, p) in &win.panes {
                    if let Some(fd) = self.pty_fds.get(&pid) {
                        pty::Pty::resize_fd(fd.as_raw_fd(), p.sx as u16, p.sy as u16).ok();
                    }
                }
            }
        }

        // Resize the pane PTY in the new window
        if let Some(fd) = self.pty_fds.get(&pane_id) {
            pty::Pty::resize_fd(fd.as_raw_fd(), sx as u16, pane_height as u16).ok();
        }

        self.mark_clients_redraw(session_id);
        Ok(new_window_idx)
    }

    fn join_pane(
        &mut self,
        src_session_id: u32,
        src_window_idx: u32,
        src_pane_id: u32,
        dst_session_id: u32,
        dst_window_idx: u32,
        horizontal: bool,
    ) -> Result<(), ServerError> {
        // Remove pane from source window
        let pane = {
            let session = self
                .sessions
                .find_by_id_mut(src_session_id)
                .ok_or_else(|| ServerError::Command("source session not found".into()))?;
            let window = session.windows.get_mut(&src_window_idx).ok_or_else(|| {
                ServerError::Command(format!("window not found: {src_window_idx}"))
            })?;

            let pane = window
                .panes
                .remove(&src_pane_id)
                .ok_or_else(|| ServerError::Command(format!("pane not found: %{src_pane_id}")))?;

            // Update active pane if needed
            if window.active_pane == src_pane_id {
                if let Some(&next) = window.panes.keys().next() {
                    window.active_pane = next;
                }
            }

            // If window is now empty, remove it
            if window.panes.is_empty() {
                let idx = src_window_idx;
                session.windows.remove(&idx);
                if session.active_window == idx {
                    if let Some(&next) = session.windows.keys().next() {
                        session.active_window = next;
                    }
                }
            } else {
                // Rebuild layout for remaining panes
                let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
                let layout = if horizontal {
                    layout_even_horizontal(window.sx, window.sy, &pane_ids)
                } else {
                    layout_even_vertical(window.sx, window.sy, &pane_ids)
                };
                for &pid in &pane_ids {
                    if let Some(cell) = layout.find_pane(pid) {
                        if let Some(p) = window.panes.get_mut(&pid) {
                            p.resize(cell.sx, cell.sy);
                            p.xoff = cell.x_off;
                            p.yoff = cell.y_off;
                        }
                    }
                }
                window.layout = Some(layout);
            }

            pane
        };

        // Insert pane into destination window
        {
            let session = self
                .sessions
                .find_by_id_mut(dst_session_id)
                .ok_or_else(|| ServerError::Command("destination session not found".into()))?;
            let window = session.windows.get_mut(&dst_window_idx).ok_or_else(|| {
                ServerError::Command(format!("window not found: {dst_window_idx}"))
            })?;

            let pid = pane.id;
            window.panes.insert(pid, pane);

            // Rebuild layout with new pane
            let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
            let layout = if horizontal {
                layout_even_horizontal(window.sx, window.sy, &pane_ids)
            } else {
                layout_even_vertical(window.sx, window.sy, &pane_ids)
            };
            for &id in &pane_ids {
                if let Some(cell) = layout.find_pane(id) {
                    if let Some(p) = window.panes.get_mut(&id) {
                        p.resize(cell.sx, cell.sy);
                        p.xoff = cell.x_off;
                        p.yoff = cell.y_off;
                    }
                }
            }
            window.layout = Some(layout);
            window.active_pane = pid;
        }

        // Resize PTYs in destination window
        {
            let session = self.sessions.find_by_id(dst_session_id).unwrap();
            if let Some(win) = session.windows.get(&dst_window_idx) {
                for (&pid, p) in &win.panes {
                    if let Some(fd) = self.pty_fds.get(&pid) {
                        pty::Pty::resize_fd(fd.as_raw_fd(), p.sx as u16, p.sy as u16).ok();
                    }
                }
            }
        }

        self.mark_clients_redraw(src_session_id);
        if dst_session_id != src_session_id {
            self.mark_clients_redraw(dst_session_id);
        }
        Ok(())
    }

    fn last_pane(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError> {
        let switched = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get_mut(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

            if let Some(last) = window.last_active_pane {
                if window.panes.contains_key(&last) {
                    window.last_active_pane = Some(window.active_pane);
                    window.active_pane = last;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };

        if switched {
            self.mark_clients_redraw(session_id);
            Ok(())
        } else {
            Err(ServerError::Command("no last pane".into()))
        }
    }

    fn rotate_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError> {
        {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get_mut(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

            let mut pane_ids: Vec<u32> = window.panes.keys().copied().collect();
            pane_ids.sort_unstable();

            if pane_ids.len() <= 1 {
                return Ok(());
            }

            // Rotate: collect all position info, shift each pane to the next position
            let positions: Vec<(u32, u32, u32, u32)> = pane_ids
                .iter()
                .map(|&id| {
                    let p = &window.panes[&id];
                    (p.xoff, p.yoff, p.sx, p.sy)
                })
                .collect();

            // Each pane takes the position of the next pane
            for (i, &pid) in pane_ids.iter().enumerate() {
                let next_pos = &positions[(i + 1) % positions.len()];
                if let Some(pane) = window.panes.get_mut(&pid) {
                    pane.xoff = next_pos.0;
                    pane.yoff = next_pos.1;
                    pane.resize(next_pos.2, next_pos.3);
                }
            }

            // Advance active pane
            if let Some(pos) = pane_ids.iter().position(|&id| id == window.active_pane) {
                let next_active = pane_ids[(pos + 1) % pane_ids.len()];
                window.active_pane = next_active;
            }
        }

        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn select_layout(
        &mut self,
        session_id: u32,
        window_idx: u32,
        layout_name: &str,
    ) -> Result<(), ServerError> {
        // Collect pane resize info: (pane_id, new_sx, new_sy)
        let pane_resizes: Vec<(u32, u32, u32)> = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get_mut(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

            let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
            let layout = match layout_name {
                "even-horizontal" | "eh" => layout_even_horizontal(window.sx, window.sy, &pane_ids),
                "even-vertical" | "ev" => layout_even_vertical(window.sx, window.sy, &pane_ids),
                _ => {
                    return Err(ServerError::Command(format!("unknown layout: {layout_name}")));
                }
            };

            let mut resizes = Vec::new();
            // Apply layout positions to panes
            for &pid in &pane_ids {
                if let Some(cell) = layout.find_pane(pid) {
                    if let Some(pane) = window.panes.get_mut(&pid) {
                        pane.resize(cell.sx, cell.sy);
                        pane.xoff = cell.x_off;
                        pane.yoff = cell.y_off;
                        resizes.push((pid, cell.sx, cell.sy));
                    }
                }
            }
            window.layout = Some(layout);
            resizes
        };

        // Resize PTYs
        for (pid, new_sx, new_sy) in pane_resizes {
            if let Some(fd) = self.pty_fds.get(&pid) {
                pty::Pty::resize_fd(fd.as_raw_fd(), new_sx as u16, new_sy as u16).ok();
            }
        }

        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn respawn_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<(), ServerError> {
        // Clean up old PTY
        self.cleanup_pane(pane_id);

        // Get pane dimensions and reset screen
        let (sx, sy, cwd) = {
            let session = self
                .sessions
                .find_by_id_mut(session_id)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            let window = session
                .windows
                .get_mut(&window_idx)
                .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;
            let pane = window
                .panes
                .get_mut(&pane_id)
                .ok_or_else(|| ServerError::Command(format!("pane not found: %{pane_id}")))?;
            let sx = pane.sx;
            let sy = pane.sy;
            pane.screen = rmux_core::screen::Screen::new(sx, sy, 2000);
            let cwd = session.cwd.clone();
            (sx, sy, cwd)
        };

        // Spawn a new shell process
        self.spawn_pane_process(pane_id, sx, sy, &cwd)?;

        self.mark_clients_redraw(session_id);
        Ok(())
    }

    // --- Command prompt ---

    fn enter_command_prompt(&mut self) {
        if let Some(client) = self.clients.get_mut(&self.command_client) {
            client.prompt = Some(PromptState::default());
        }
    }

    // --- Copy mode ---

    fn enter_copy_mode(&mut self) -> Result<(), ServerError> {
        let session_id =
            self.client_session_id().ok_or(ServerError::Command("no session".into()))?;
        let mode_keys = self.pane_mode_keys();
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or(ServerError::Command("session not found".into()))?;
        let window =
            session.active_window_mut().ok_or(ServerError::Command("no active window".into()))?;
        let pane = window.active_pane_mut().ok_or(ServerError::Command("no active pane".into()))?;
        pane.enter_copy_mode(&mode_keys);
        self.mark_clients_redraw(session_id);
        Ok(())
    }

    fn pane_mode_keys(&self) -> String {
        let Some(session_id) = self.client_session_id() else {
            return "emacs".to_string();
        };
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return "emacs".to_string();
        };
        let Some(window) = session.active_window() else {
            return "emacs".to_string();
        };
        window
            .options
            .get("mode-keys")
            .and_then(|v| v.as_str().map(ToString::to_string))
            .unwrap_or_else(|| "emacs".to_string())
    }

    // --- Paste buffers ---

    fn paste_buffer_add(&mut self, data: Vec<u8>) {
        self.paste_buffers.add(data);
    }

    fn paste_buffer(&self, name: Option<&str>) -> Result<(), ServerError> {
        let buf = if let Some(name) = name {
            self.paste_buffers.get_by_name(name)
        } else {
            self.paste_buffers.get_top()
        };
        let buf = buf.ok_or(ServerError::Command("no buffers".into()))?;
        let data = buf.data.clone();

        // Write to active pane's PTY
        let session_id =
            self.client_session_id().ok_or(ServerError::Command("no session".into()))?;
        let session = self
            .sessions
            .find_by_id(session_id)
            .ok_or(ServerError::Command("session not found".into()))?;
        let window =
            session.active_window().ok_or(ServerError::Command("no active window".into()))?;
        let pane = window.active_pane().ok_or(ServerError::Command("no active pane".into()))?;

        // Wrap with bracketed paste if the pane has BRACKETPASTE mode
        if pane.screen.mode.contains(rmux_core::screen::ModeFlags::BRACKETPASTE) {
            let mut wrapped = Vec::with_capacity(data.len() + 12);
            wrapped.extend_from_slice(b"\x1b[200~");
            wrapped.extend_from_slice(&data);
            wrapped.extend_from_slice(b"\x1b[201~");
            self.write_to_pane(session_id, session.active_window, pane.id, &wrapped)?;
        } else {
            self.write_to_pane(session_id, session.active_window, pane.id, &data)?;
        }
        Ok(())
    }

    fn list_buffers(&self) -> Vec<String> {
        self.paste_buffers
            .list()
            .iter()
            .map(|b| {
                let preview: String =
                    String::from_utf8_lossy(&b.data[..b.data.len().min(50)]).into();
                format!("{}: {} bytes: \"{}\"", b.name, b.data.len(), preview)
            })
            .collect()
    }

    fn show_buffer(&self, name: &str) -> Result<String, ServerError> {
        let buf = self
            .paste_buffers
            .get_by_name(name)
            .ok_or(ServerError::Command(format!("buffer not found: {name}")))?;
        Ok(String::from_utf8_lossy(&buf.data).into_owned())
    }

    fn delete_buffer(&mut self, name: &str) -> Result<(), ServerError> {
        if self.paste_buffers.delete(name) {
            Ok(())
        } else {
            Err(ServerError::Command(format!("buffer not found: {name}")))
        }
    }

    fn set_buffer(&mut self, name: &str, data: &str) -> Result<(), ServerError> {
        self.paste_buffers.set(name, data.as_bytes().to_vec());
        Ok(())
    }
}

/// Format an option value as a display string.
fn format_option_value(val: &OptionValue) -> String {
    match val {
        OptionValue::String(s) => s.clone(),
        OptionValue::Number(n) => n.to_string(),
        OptionValue::Flag(b) => if *b { "on" } else { "off" }.to_string(),
        OptionValue::Style(s) => format!("{s:?}"),
        OptionValue::Array(a) => a.join(","),
    }
}

/// Parse a string value into an OptionValue, guessing the type.
fn parse_option_value(value: &str) -> OptionValue {
    match value {
        "on" | "true" | "yes" => OptionValue::Flag(true),
        "off" | "false" | "no" => OptionValue::Flag(false),
        _ => {
            if let Ok(n) = value.parse::<i64>() {
                OptionValue::Number(n)
            } else {
                OptionValue::String(value.to_string())
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
