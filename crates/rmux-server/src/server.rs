//! Server event loop.
//!
//! The server listens on a Unix domain socket and accepts client connections.
//! It manages all sessions, windows, and panes using a tokio single-threaded runtime.

use crate::client::{ClientFlags, PromptState, ServerClient};
use crate::command::{self, CommandResult, CommandServer, Direction, SessionTreeInfo};
use crate::copymode::{self, CopyModeAction};
use crate::keybind::{KeyBindings, string_to_key};
use crate::navigate;
use crate::pane::Pane;
use crate::render;
use crate::session::SessionManager;
use crate::window::Window;
use rmux_core::layout::{LayoutCell, layout_even_horizontal, layout_even_vertical};
use rmux_core::options::OptionValue;
use rmux_core::screen::ModeFlags;
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

/// Format a control mode `%output` notification.
///
/// tmux format: `%output %<pane_id> <octal-escaped-data>\n`
/// Printable ASCII is passed through; backslash is escaped as `\\`;
/// all other bytes are octal-escaped as `\NNN`.
pub fn format_control_output(pane_id: u32, data: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = format!("%output %{pane_id} ");
    for &b in data {
        if b == b'\\' {
            out.push_str("\\\\");
        } else if (0x20..0x7F).contains(&b) {
            out.push(b as char);
        } else {
            write!(out, "\\{b:03o}").ok();
        }
    }
    out.push('\n');
    out
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
    /// Server hooks.
    hooks: crate::hooks::HookStore,
    /// Tick counter for periodic tasks (auto-rename polling).
    tick_count: u32,
    /// Recent server messages for show-messages.
    message_log: std::collections::VecDeque<String>,
    /// Prompt history (most recent first).
    prompt_history: Vec<String>,
    /// Queued control mode notifications to send on next tick.
    control_notifications: Vec<(u32, String)>,
    /// Global (server-level) environment variables.
    global_environ: HashMap<String, String>,
    /// Pending config commands waiting to be executed (startup config loading).
    pending_config: std::collections::VecDeque<Vec<String>>,
    /// Active shell job from `run-shell` during config loading.
    pending_shell_job: Option<tokio::process::Child>,
    /// Directory with `tmux` -> rmux symlink for run-shell compatibility.
    /// Plugins (TPM etc.) call `tmux` commands — this shim redirects to rmux.
    shim_dir: Option<PathBuf>,
    /// Client commands deferred until config loading completes.
    /// Session-creating commands (new-session) must wait so they inherit config options.
    deferred_commands: Vec<(u64, Vec<String>)>,
    /// Hidden variables from `%hidden` directives, propagated across nested source-file calls.
    config_hidden_vars: HashMap<String, String>,
    /// Client IDs queued for detach by `detach_other_clients` (processed after command dispatch).
    pending_detach: Vec<u64>,
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
            hooks: crate::hooks::HookStore::new(),
            tick_count: 0,
            message_log: std::collections::VecDeque::new(),
            prompt_history: Vec::new(),
            control_notifications: Vec::new(),
            global_environ: HashMap::new(),
            pending_config: std::collections::VecDeque::new(),
            pending_shell_job: None,
            shim_dir: None,
            deferred_commands: Vec::new(),
            config_hidden_vars: HashMap::new(),
            pending_detach: Vec::new(),
        }
    }

    /// Get the default socket path (matching tmux's convention).
    pub fn default_socket_path() -> PathBuf {
        let tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        let uid = nix::unistd::getuid();
        PathBuf::from(format!("{tmpdir}/rmux-{uid}/default"))
    }

    /// Load a configuration file, executing all commands it contains.
    /// Errors from individual commands are logged but don't stop the server.
    pub fn load_config(&mut self, path: &str) {
        match crate::config::load_config_file(path) {
            Ok(commands) => {
                tracing::info!("loading config: {path}");
                let errors = self.execute_config_commands(commands);
                for err in errors {
                    tracing::warn!("config error: {err}");
                }
            }
            Err(e) => {
                tracing::warn!("could not load config {path}: {e}");
            }
        }
    }

    /// Load the default configuration file (tmux-compatible paths).
    ///
    /// Checks in order (first found wins):
    /// 1. `~/.tmux.conf`
    /// 2. `$XDG_CONFIG_HOME/tmux/tmux.conf` (defaults to `~/.config/tmux/tmux.conf`)
    pub fn load_default_config(&mut self) {
        let Ok(home) = std::env::var("HOME") else { return };
        let xdg = std::env::var("XDG_CONFIG_HOME").ok();
        if let Some(path) = Self::find_default_config(&home, xdg.as_deref()) {
            self.load_config(&path);
        } else {
            tracing::info!("no default config file found");
        }
    }

    /// Find the default config file path without loading it.
    fn find_default_config(home: &str, xdg_config_home: Option<&str>) -> Option<String> {
        let candidates = [
            format!("{home}/.tmux.conf"),
            xdg_config_home.map_or_else(
                || format!("{home}/.config/tmux/tmux.conf"),
                |xdg| format!("{xdg}/tmux/tmux.conf"),
            ),
        ];

        for path in &candidates {
            if std::path::Path::new(path).exists() {
                return Some(path.clone());
            }
        }
        None
    }

    /// Run the server event loop.
    pub async fn run(&mut self, config_file: Option<&str>) -> Result<(), ServerError> {
        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent).map_err(ServerError::Bind)?;
        }

        // Remove stale socket
        let _ = std::fs::remove_file(&self.socket_path);

        // Bind socket BEFORE loading config — run-shell scripts (like TPM) need to
        // connect back to the server while executing, just like tmux.
        let listener = UnixListener::bind(&self.socket_path).map_err(ServerError::Bind)?;

        tracing::info!("server listening on {:?}", self.socket_path);

        // Create tmux shim so plugins calling `tmux` resolve to rmux.
        self.setup_tmux_shim();

        // Queue config commands for processing inside the event loop.
        // This matches tmux's architecture: config commands execute within the
        // event loop so that run-shell can fork processes that connect back to
        // the server (e.g., TPM plugins calling `tmux set -g ...`).
        self.queue_config(config_file);

        let mut redraw_interval = tokio::time::interval(
            tokio::time::Duration::from_millis(16), // ~60fps
        );
        redraw_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Install signal handlers so we can clean up children on SIGHUP/SIGTERM.
        let mut sig_hup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .map_err(ServerError::Bind)?;
        let mut sig_term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(ServerError::Bind)?;

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

                // Process pending config commands (one per iteration, like tmux's cmdq).
                // Only runs when no shell job is blocking.
                () = std::future::ready(()), if !self.pending_config.is_empty() && self.pending_shell_job.is_none() => {
                    self.process_next_config_command();
                    // Config just finished — flush deferred client commands
                    if !self.config_loading() {
                        self.flush_deferred_commands().await;
                    }
                }

                // Wait for active run-shell job to complete before resuming config.
                // While waiting, the event loop keeps running — new connections from
                // TPM plugin scripts are accepted and handled normally.
                status = async { self.pending_shell_job.as_mut().unwrap().wait().await }, if self.pending_shell_job.is_some() => {
                    match status {
                        Ok(exit) => {
                            if !exit.success() {
                                tracing::debug!("run-shell exited with {exit}");
                            }
                        }
                        Err(e) => tracing::warn!("run-shell wait error: {e}"),
                    }
                    self.pending_shell_job = None;
                    // Shell job was the last config step — flush deferred commands
                    if !self.config_loading() {
                        self.flush_deferred_commands().await;
                    }
                }

                // Periodic redraw
                _ = redraw_interval.tick() => {
                    // Poll foreground process for auto-rename every ~500ms (30 ticks at 60fps)
                    self.tick_count = self.tick_count.wrapping_add(1);
                    if self.tick_count % 30 == 0 {
                        self.update_window_names();
                    }
                    // Status-interval: force status bar refresh (default 15s = 937 ticks)
                    let status_ticks = self.status_interval_ticks();
                    if status_ticks > 0 && self.tick_count % status_ticks == 0 {
                        for client in self.clients.values_mut() {
                            if client.is_attached() {
                                client.mark_redraw();
                            }
                        }
                    }
                    // Expire timed messages (display-time)
                    self.expire_timed_messages();
                    self.flush_control_notifications().await;
                    self.render_clients().await;
                }

                // Graceful shutdown on SIGHUP (e.g., tmux killing our PTY)
                _ = sig_hup.recv() => {
                    tracing::info!("received SIGHUP, shutting down");
                    self.shutdown = true;
                }

                // Graceful shutdown on SIGTERM
                _ = sig_term.recv() => {
                    tracing::info!("received SIGTERM, shutting down");
                    self.shutdown = true;
                }
            }
        }

        // Clean up: kill all child processes before exiting
        self.kill_all_children();
        let _ = std::fs::remove_file(&self.socket_path);
        if let Some(shim) = &self.shim_dir {
            let _ = std::fs::remove_dir_all(shim);
        }
        tracing::info!("server shutting down");
        Ok(())
    }

    /// Queue config file commands for processing in the event loop.
    fn queue_config(&mut self, config_file: Option<&str>) {
        let mut ctx = self.build_config_context();
        let commands = if let Some(path) = config_file {
            match crate::config::load_config_file_with_context(path, &mut ctx) {
                Ok(cmds) => {
                    tracing::info!("loading config: {path}");
                    cmds
                }
                Err(e) => {
                    tracing::warn!("could not load config {path}: {e}");
                    return;
                }
            }
        } else {
            let Ok(home) = std::env::var("HOME") else { return };
            let xdg = std::env::var("XDG_CONFIG_HOME").ok();
            let Some(path) = Self::find_default_config(&home, xdg.as_deref()) else {
                tracing::info!("no default config file found");
                return;
            };
            match crate::config::load_config_file_with_context(&path, &mut ctx) {
                Ok(cmds) => {
                    tracing::info!("loading config: {path}");
                    cmds
                }
                Err(e) => {
                    tracing::warn!("could not load config {path}: {e}");
                    return;
                }
            }
        };

        self.pending_config = commands.into();
    }

    /// Process the next pending config command.
    fn process_next_config_command(&mut self) {
        let Some(argv) = self.pending_config.pop_front() else {
            return;
        };

        match crate::command::execute_command(&argv, self) {
            Ok(CommandResult::RunShell(cmd) | CommandResult::RunShellBackground(cmd)) => {
                tracing::debug!("config run-shell: {cmd}");
                let expanded = crate::config::expand_tilde(&cmd);
                match self.shell_command(&expanded).spawn() {
                    Ok(child) => {
                        self.pending_shell_job = Some(child);
                    }
                    Err(e) => {
                        tracing::warn!("config run-shell spawn error: {e}");
                    }
                }
            }
            Ok(_) => {
                // Other results (Attach, Detach, Overlay, etc.) are silently
                // ignored during config loading, matching tmux behavior.
            }
            Err(e) => {
                tracing::debug!("config command failed: {argv:?} -> {e}");
                tracing::warn!("config error: {e}");
            }
        }
    }

    /// Create a shim directory with a `tmux` symlink pointing to the rmux client binary.
    /// This allows plugins (TPM, etc.) that call `tmux` to transparently use rmux.
    fn setup_tmux_shim(&mut self) {
        // Find the rmux client binary next to rmux-server.
        let rmux_bin = match std::env::current_exe() {
            Ok(server_path) => {
                let dir = server_path.parent().unwrap_or(std::path::Path::new("."));
                let client = dir.join("rmux");
                if client.exists() {
                    client
                } else {
                    // Fallback: maybe we ARE rmux (single binary)
                    server_path
                }
            }
            Err(e) => {
                tracing::debug!("could not determine rmux binary path: {e}");
                return;
            }
        };

        let uid = nix::unistd::getuid();
        let shim_dir = PathBuf::from(format!(
            "{}/rmux-{uid}/shim",
            std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into())
        ));

        if let Err(e) = std::fs::create_dir_all(&shim_dir) {
            tracing::debug!("could not create shim dir: {e}");
            return;
        }

        let tmux_shim = shim_dir.join("tmux");
        // Remove stale symlink
        let _ = std::fs::remove_file(&tmux_shim);
        if let Err(e) = std::os::unix::fs::symlink(&rmux_bin, &tmux_shim) {
            tracing::debug!("could not create tmux shim symlink: {e}");
            return;
        }

        tracing::debug!("tmux shim: {} -> {}", tmux_shim.display(), rmux_bin.display());
        self.shim_dir = Some(shim_dir);
    }

    /// Build a shell command with the right environment for run-shell.
    /// Sets TMUX (socket path) and prepends the shim dir to PATH so that
    /// `tmux` commands in scripts resolve to rmux.
    fn shell_command(&self, cmd: &str) -> tokio::process::Command {
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg(cmd);
        command.env("TMUX", format!("{},0,0", self.socket_path.display()));

        if let Some(shim) = &self.shim_dir {
            let path = std::env::var("PATH").unwrap_or_default();
            command.env("PATH", format!("{}:{path}", shim.display()));
        }

        command
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
        self.log_message(format!("client {client_id} connected"));
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_pty_output(&mut self, pane_id: u32, data: &[u8]) {
        if data.is_empty() {
            // EOF sentinel: the pane's process exited.
            self.handle_pane_exit(pane_id).await;
            return;
        }

        if Self::route_popup_output(&mut self.clients, pane_id, data) {
            return;
        }

        // Find the pane and feed data through its parser
        let mut notifications = Vec::new();
        let mut replies: Option<(i32, Vec<u8>)> = None;
        let mut alert_messages: Vec<(u32, String)> = Vec::new();
        let mut pane_session_id: Option<u32> = None;

        for session in self.sessions.iter_mut() {
            for (&widx, window) in &mut session.windows {
                if let Some(pane) = window.panes.get_mut(&pane_id) {
                    pane.process_input(data);
                    pane.pipe_output(data);
                    notifications = pane.screen.drain_notifications();
                    let reply_data = pane.screen.take_replies();
                    if !reply_data.is_empty() && pane.pty_fd >= 0 {
                        replies = Some((pane.pty_fd, reply_data));
                    }
                    // Note: automatic-rename is handled by update_window_names() which
                    // polls the foreground process name periodically (matching tmux).
                    // OSC 0/2 title changes only affect pane_title (#T), not window_name (#W).
                    let is_active_window = widx == session.active_window;
                    // Bell detection: set flag on non-active windows when monitor-bell is on
                    let has_bell = notifications
                        .iter()
                        .any(|n| matches!(n, rmux_core::screen::Notification::Bell));
                    if has_bell
                        && !is_active_window
                        && window.options.get_flag("monitor-bell").unwrap_or(true)
                    {
                        window.has_bell = true;
                        // Check bell-action to show alert message
                        let ba = session.options.get_string("bell-action").unwrap_or("any");
                        if ba == "any" || ba == "other" {
                            alert_messages.push((session.id, format!("Bell in window {widx}")));
                        }
                    }
                    if has_bell && is_active_window {
                        let ba = session.options.get_string("bell-action").unwrap_or("any");
                        if ba == "any" || ba == "current" {
                            alert_messages.push((session.id, format!("Bell in window {widx}")));
                        }
                    }
                    // Activity detection: set flag on non-active windows when monitor-activity is on
                    if !is_active_window
                        && window.options.get_flag("monitor-activity").unwrap_or(false)
                    {
                        window.has_activity = true;
                        let aa = session.options.get_string("activity-action").unwrap_or("other");
                        if aa == "any" || aa == "other" {
                            alert_messages.push((session.id, format!("Activity in window {widx}")));
                        }
                    }
                    // Filter out Bell notifications (handled above, not needed downstream)
                    notifications.retain(|n| !matches!(n, rmux_core::screen::Notification::Bell));
                    pane_session_id = Some(session.id);
                    // Mark attached clients for redraw — but defer if
                    // the pane is in synchronized output mode (mode 2026).
                    let in_sync =
                        pane.screen.mode.contains(rmux_core::screen::ModeFlags::SYNC_OUTPUT);
                    if !in_sync {
                        for client in self.clients.values_mut() {
                            if client.session_id == Some(session.id) && client.is_attached() {
                                client.mark_redraw();
                            }
                        }
                    }
                    break;
                }
            }
        }
        for notification in notifications {
            self.handle_screen_notification(notification);
        }
        // Write replies (e.g., CPR) back to the PTY
        if let Some((raw_fd, reply_bytes)) = replies {
            // SAFETY: raw_fd is a valid PTY master fd owned by the pane.
            let fd = unsafe { BorrowedFd::borrow_raw(raw_fd) };
            nix::unistd::write(fd, &reply_bytes).ok();
        }
        // Send %output to control mode clients
        if let Some(sid) = pane_session_id {
            self.send_control_output(sid, pane_id, data).await;
        }
        // Show alert messages as timed messages on attached clients
        let display_time_ms = self
            .sessions
            .iter()
            .next()
            .map_or(750, |s| s.options.get_number("display-time").unwrap_or(750) as u64);
        let expiry = std::time::Instant::now() + std::time::Duration::from_millis(display_time_ms);
        for (session_id, msg) in alert_messages {
            for client in self.clients.values_mut() {
                if client.session_id == Some(session_id) && client.is_attached() {
                    client.timed_message = Some((msg.clone(), expiry));
                }
            }
        }
    }

    /// Handle a screen notification (side-channel event from escape sequences).
    fn handle_screen_notification(&mut self, notification: rmux_core::screen::Notification) {
        use base64::Engine;
        use rmux_core::screen::Notification;
        match notification {
            Notification::SetClipboard(base64_data) => {
                // Decode base64 and store in paste buffer (respects set-clipboard option)
                let clipboard_mode =
                    self.options.get_string("set-clipboard").unwrap_or("external").to_string();
                if clipboard_mode == "off" {
                    return;
                }
                let engine = base64::engine::general_purpose::STANDARD;
                if let Ok(decoded) = engine.decode(&base64_data) {
                    self.paste_buffers.add(decoded);
                }
            }
            Notification::SetPaletteColor(_, _, _, _)
            | Notification::SetForegroundColor(_)
            | Notification::SetBackgroundColor(_)
            | Notification::ResetCursorColor
            | Notification::ResetPaletteColor(_)
            | Notification::ResetForegroundColor
            | Notification::ResetBackgroundColor
            | Notification::Bell => {
                // Color notifications stored for future use.
                // Bell handled in handle_pty_output where we have window context.
            }
        }
    }

    /// Handle a pane whose process has exited.
    async fn handle_pane_exit(&mut self, pane_id: u32) {
        tracing::info!("pane {pane_id} process exited");

        // Check if this is a popup pane — close the popup overlay
        let popup_client = self.clients.iter().find_map(|(&cid, c)| {
            if let Some(crate::overlay::OverlayState::Popup(popup)) = &c.overlay {
                if popup.pane_id == pane_id {
                    return Some(cid);
                }
            }
            None
        });
        if let Some(client_id) = popup_client {
            self.cleanup_pane(pane_id);
            if let Some(client) = self.clients.get_mut(&client_id) {
                client.overlay = None;
                client.mark_redraw();
            }
            return;
        }

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
            let mut pty_resizes: Vec<(u32, u32, u32)> = Vec::new();
            if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                if let Some(window) = session.windows.get_mut(&window_idx) {
                    window.panes.remove(&pane_id);
                    if window.active_pane == pane_id {
                        if let Some(&next) = window.panes.keys().next() {
                            window.active_pane = next;
                        }
                    }
                    // Rebuild layout and resize remaining panes
                    let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
                    if pane_ids.len() > 1 {
                        window.layout =
                            Some(layout_even_horizontal(window.sx, window.sy, &pane_ids));
                        if let Some(ref layout) = window.layout {
                            for &pid in &pane_ids {
                                if let Some(cell) = layout.find_pane(pid) {
                                    let (cx, cy, csx, csy) =
                                        (cell.x_off, cell.y_off, cell.sx, cell.sy);
                                    if let Some(pane) = window.panes.get_mut(&pid) {
                                        pane.resize(csx, csy);
                                        pane.xoff = cx;
                                        pane.yoff = cy;
                                        pty_resizes.push((pid, csx, csy));
                                    }
                                }
                            }
                        }
                    } else if let Some(&only_id) = pane_ids.first() {
                        window.layout =
                            Some(LayoutCell::new_pane(0, 0, window.sx, window.sy, only_id));
                        if let Some(pane) = window.panes.get_mut(&only_id) {
                            pane.resize(window.sx, window.sy);
                            pane.xoff = 0;
                            pane.yoff = 0;
                            pty_resizes.push((only_id, window.sx, window.sy));
                        }
                    }
                }
            }
            // Resize PTYs so shells know the new dimensions
            for (pid, new_sx, new_sy) in pty_resizes {
                if let Some(fd) = self.pty_fds.get(&pid) {
                    pty::Pty::resize_fd(fd.as_raw_fd(), new_sx as u16, new_sy as u16).ok();
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
                    client.control_mode = (client.identify.flags
                        & rmux_protocol::identify::flags::IDENTIFY_CONTROL)
                        != 0;
                    tracing::info!(
                        "client {client_id} identified: term={}, cwd={}, control={}",
                        client.identify.term,
                        client.identify.cwd,
                        client.control_mode,
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

    /// Returns true if config is still being loaded (pending commands or active shell job).
    fn config_loading(&self) -> bool {
        !self.pending_config.is_empty() || self.pending_shell_job.is_some()
    }

    /// Returns true if a command creates or attaches to sessions and should
    /// be deferred until config loading is complete.
    fn is_session_command(argv: &[String]) -> bool {
        matches!(
            argv.first().map(String::as_str),
            Some("new-session" | "new" | "attach-session" | "attach" | "a")
        )
    }

    /// Process all commands that were deferred while config was loading.
    async fn flush_deferred_commands(&mut self) {
        let commands = std::mem::take(&mut self.deferred_commands);
        for (client_id, argv) in commands {
            tracing::info!("executing deferred command from client {client_id}: {argv:?}");
            self.handle_command(client_id, &argv).await;
        }
    }

    async fn handle_command(&mut self, client_id: u64, argv: &[String]) {
        if argv.is_empty() {
            return;
        }

        // Defer session-creating commands until config loading is done,
        // so sessions inherit configured options (prefix, status bar, etc.).
        if self.config_loading() && Self::is_session_command(argv) {
            tracing::debug!("deferring command until config loaded: {argv:?}");
            self.deferred_commands.push((client_id, argv.to_vec()));
            return;
        }

        tracing::info!("client {client_id} command: {argv:?}");

        // Set the command client context
        self.command_client = client_id;

        // Execute command
        let result = command::execute_command(argv, self);
        self.dispatch_command_result(client_id, result).await;
    }

    #[allow(clippy::too_many_lines)]
    async fn dispatch_command_result(
        &mut self,
        client_id: u64,
        result: Result<CommandResult, ServerError>,
    ) {
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
            Ok(CommandResult::Suspend) => {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    client.send(&Message::Suspend).await.ok();
                }
            }
            Ok(CommandResult::TimedMessage(msg)) => {
                // Show message in status bar for display-time milliseconds
                if let Some(client) = self.clients.get_mut(&client_id) {
                    let display_time_ms = client
                        .session_id
                        .and_then(|sid| {
                            self.sessions
                                .find_by_id(sid)
                                .and_then(|s| s.options.get_number("display-time").ok())
                        })
                        .unwrap_or(750) as u64;
                    let expiry = std::time::Instant::now()
                        + std::time::Duration::from_millis(display_time_ms);
                    client.timed_message = Some((msg, expiry));
                    client.mark_redraw();
                    if !client.is_attached() {
                        client.send(&Message::Exit).await.ok();
                    }
                }
            }
            Ok(CommandResult::Overlay(overlay_state)) => {
                self.set_client_overlay(client_id, overlay_state);
            }
            Ok(CommandResult::SpawnPopup(config)) => {
                self.spawn_popup(client_id, config);
            }
            Ok(CommandResult::RunShell(cmd)) => {
                let output = match self.shell_command(&cmd).output().await {
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
            Ok(CommandResult::RunShellBackground(cmd)) => {
                // Fire and forget — don't capture output or block client
                match self.shell_command(&cmd).spawn() {
                    Ok(_child) => {
                        tracing::debug!("run-shell -b: spawned background: {cmd}");
                    }
                    Err(e) => {
                        tracing::warn!("run-shell -b spawn error: {e}");
                    }
                }
                if let Some(client) = self.clients.get_mut(&client_id) {
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
                        self.log_message(format!("error: {e}"));
                    } else {
                        client.send(&Message::ErrorOutput(err_msg.into_bytes())).await.ok();
                        client.send(&Message::Exit).await.ok();
                    }
                }
            }
        }

        // Process any clients queued for detach by detach_other_clients().
        let pending = std::mem::take(&mut self.pending_detach);
        for cid in pending {
            self.detach_client(cid).await;
        }
    }

    fn handle_input_data(&mut self, client_id: u64, data: &[u8]) {
        // Update activity timestamps
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let Some(client) = self.clients.get_mut(&client_id) else {
            return;
        };
        client.activity = now;
        let Some(session_id) = client.session_id else {
            return;
        };
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            session.activity = now;
        }

        // Check if client has an active overlay
        if client.overlay.is_some() {
            self.handle_overlay_input(client_id, data);
            return;
        }

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
                    // If the keybinding system stayed in prefix mode (repeatable
                    // binding), set the repeat timeout from the session option.
                    if self.keybindings.in_prefix() {
                        let repeat_ms = self
                            .sessions
                            .find_by_id(session_id)
                            .and_then(|s| s.options.get_number("repeat-time").ok())
                            .unwrap_or(500) as u64;
                        self.keybindings.set_repeat_timeout(repeat_ms);
                    }
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

    /// Fire a named hook, executing all registered commands.
    fn fire_hook(&mut self, hook_name: &str) {
        let commands = match self.hooks.get(hook_name) {
            Some(cmds) => cmds.to_vec(),
            None => return,
        };
        for argv in commands {
            let _ = crate::command::execute_command(&argv, self);
        }
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
                // Track click count for double/triple-click detection
                let click_count = if let Some(client) = self.clients.get_mut(&client_id) {
                    client.click_state.register_click(mx, my)
                } else {
                    1
                };

                match click_count {
                    2 => {
                        // Double-click: select word at position
                        if !self.is_active_pane_in_copy_mode(session_id) {
                            self.enter_copy_mode_for_active_pane(session_id);
                        }
                        self.copy_mode_select_word(session_id, mx, my);
                    }
                    3 => {
                        // Triple-click: select line
                        if !self.is_active_pane_in_copy_mode(session_id) {
                            self.enter_copy_mode_for_active_pane(session_id);
                        }
                        self.copy_mode_select_line(session_id, my);
                    }
                    _ => {
                        // Single click: select pane or position cursor
                        if self.is_active_pane_in_copy_mode(session_id) {
                            self.copy_mode_position_cursor(session_id, mx, my);
                        } else {
                            self.select_pane_at_position(session_id, mx, my);
                        }
                    }
                }
                self.mark_clients_redraw(session_id);
            }
            KEYC_MOUSEDOWN2 => {
                // Middle-click: paste from top buffer
                self.paste_top_buffer_to_active_pane(session_id);
            }
            KEYC_MOUSEDOWN3 => {
                // Right-click: select pane at position (no context menu yet)
                self.select_pane_at_position(session_id, mx, my);
                self.mark_clients_redraw(session_id);
            }
            KEYC_MOUSEDRAG1 => {
                // Drag: begin/extend selection in copy mode
                if !self.is_active_pane_in_copy_mode(session_id) {
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
                // Alternate scroll: send arrow keys in alternate screen
                if self.active_pane_has_alt_scroll(session_id) {
                    for _ in 0..3 {
                        self.write_to_active_pane(session_id, b"\x1b[A");
                    }
                } else {
                    if !self.is_active_pane_in_copy_mode(session_id) {
                        self.enter_copy_mode_for_active_pane(session_id);
                    }
                    self.copy_mode_scroll_up(session_id, 3);
                    self.mark_clients_redraw(session_id);
                }
            }
            KEYC_WHEELDOWN => {
                if self.active_pane_has_alt_scroll(session_id) {
                    for _ in 0..3 {
                        self.write_to_active_pane(session_id, b"\x1b[B");
                    }
                } else if self.is_active_pane_in_copy_mode(session_id) {
                    self.copy_mode_scroll_down(session_id, 3);
                    self.maybe_exit_copy_mode_at_bottom(session_id);
                    self.mark_clients_redraw(session_id);
                }
            }
            _ => {}
        }
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

    /// Double-click: select the word at the given screen position.
    fn copy_mode_select_word(&mut self, session_id: u32, x: u32, y: u32) {
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            let word_seps =
                session.options.get_string("word-separators").unwrap_or(" ").to_string();
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    if let Some(cm) = &mut pane.copy_mode {
                        cm.cx = x;
                        cm.cy = y;
                        // Find word boundaries using word-separators option
                        let abs_y = cm.absolute_y(pane.screen.grid.history_size());
                        if let Some(line) = pane.screen.grid.get_line_absolute(abs_y) {
                            let max = line.cell_count();
                            let is_sep = |cell_data: &[u8]| -> bool {
                                if cell_data.is_empty() {
                                    return true;
                                }
                                // Check each char against word-separators
                                if let Ok(s) = std::str::from_utf8(cell_data) {
                                    s.chars().any(|c| word_seps.contains(c))
                                } else {
                                    false
                                }
                            };
                            // Find word start
                            let mut start = x;
                            while start > 0 {
                                let cell = line.get_cell(start - 1);
                                if is_sep(cell.data.as_bytes()) {
                                    break;
                                }
                                start -= 1;
                            }
                            // Find word end
                            let mut end = x;
                            while end + 1 < max {
                                let cell = line.get_cell(end + 1);
                                if is_sep(cell.data.as_bytes()) {
                                    break;
                                }
                                end += 1;
                            }
                            // Set selection
                            let hs = pane.screen.grid.history_size();
                            cm.selecting = true;
                            cm.sel_type = rmux_core::screen::selection::SelectionType::Normal;
                            cm.sel_start_x = start;
                            cm.sel_start_y = cm.absolute_y(hs);
                            cm.cx = end;
                        }
                    }
                }
            }
        }
    }

    /// Triple-click: select the entire line at the given row.
    fn copy_mode_select_line(&mut self, session_id: u32, y: u32) {
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    if let Some(cm) = &mut pane.copy_mode {
                        cm.cy = y;
                        let hs = pane.screen.grid.history_size();
                        cm.select_line(hs);
                    }
                }
            }
        }
    }

    /// Middle-click: paste the top buffer to the active pane.
    fn paste_top_buffer_to_active_pane(&self, session_id: u32) {
        let Some(buf) = self.paste_buffers.get_top() else {
            return;
        };
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return;
        };
        let Some(window) = session.active_window() else {
            return;
        };
        let Some(pane) = window.active_pane() else {
            return;
        };
        if let Some(fd) = self.pty_fds.get(&pane.id) {
            let _ = nix::unistd::write(fd, &buf.data);
        }
    }

    /// Check if the active pane is in alternate screen with alternate scroll enabled.
    fn active_pane_has_alt_scroll(&self, session_id: u32) -> bool {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return false;
        };
        let Some(window) = session.active_window() else {
            return false;
        };
        let Some(pane) = window.active_pane() else {
            return false;
        };
        pane.screen.alternate.is_some()
            && pane.screen.mode.contains(rmux_core::screen::ModeFlags::ALT_SCROLL)
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

    /// Execute a search in the active pane's copy mode.
    fn copy_mode_search(&mut self, client_id: u64, needle: &str, forward: bool) {
        let Some(session_id) = self.clients.get(&client_id).and_then(|c| c.session_id) else {
            return;
        };
        let Some(session) = self.sessions.find_by_id_mut(session_id) else {
            return;
        };
        let Some(window) = session.active_window_mut() else {
            return;
        };
        let Some(pane) = window.active_pane_mut() else {
            return;
        };
        if let Some(cm) = &mut pane.copy_mode {
            if forward {
                cm.search_forward_for(&pane.screen, needle);
            } else {
                cm.search_backward_for(&pane.screen, needle);
            }
        }
        self.mark_clients_redraw(session_id);
    }

    /// Go to a specific line number in the active pane's copy mode.
    fn copy_mode_goto_line(&mut self, client_id: u64, line: u32) {
        let Some(session_id) = self.clients.get(&client_id).and_then(|c| c.session_id) else {
            return;
        };
        let Some(session) = self.sessions.find_by_id_mut(session_id) else {
            return;
        };
        let Some(window) = session.active_window_mut() else {
            return;
        };
        let Some(pane) = window.active_pane_mut() else {
            return;
        };
        if let Some(cm) = &mut pane.copy_mode {
            cm.goto_line(&pane.screen, line);
        }
        self.mark_clients_redraw(session_id);
    }

    /// Handle input when the active pane is in copy mode.
    fn handle_copy_mode_input(&mut self, client_id: u64, session_id: u32, data: &[u8]) {
        use rmux_terminal::keys::parse_key;

        let Some((key, _consumed)) = parse_key(data) else {
            return;
        };

        // Check for pending jump (f/F/t/T waiting for a character)
        if self.handle_copy_mode_pending_jump(session_id, key) {
            return;
        }

        // Look up binding and dispatch
        let Some((_key_table, argv)) = self.copy_mode_lookup(session_id, key) else {
            return;
        };

        let action_name = &argv[0];

        // Handle copy-pipe variants directly (they need the command arg)
        if action_name == "copy-pipe" || action_name == "copy-pipe-and-cancel" {
            let command = argv.get(1).cloned().unwrap_or_default();
            let cancel = action_name == "copy-pipe-and-cancel";
            let copy_data = {
                let Some(session) = self.sessions.find_by_id_mut(session_id) else { return };
                let Some(window) = session.active_window_mut() else { return };
                let Some(pane) = window.active_pane_mut() else { return };
                let Some(cm) = &mut pane.copy_mode else { return };
                copymode::copy_selection(&pane.screen, cm)
            };
            let action = CopyModeAction::CopyPipe { copy_data, command, cancel };
            self.handle_copy_mode_action(client_id, session_id, action);
            return;
        }

        let action = {
            let Some(session) = self.sessions.find_by_id_mut(session_id) else { return };
            let Some(window) = session.active_window_mut() else { return };
            let Some(pane) = window.active_pane_mut() else { return };
            let Some(cm) = &mut pane.copy_mode else { return };
            copymode::dispatch_copy_mode_action(&pane.screen, cm, action_name)
        };

        self.handle_copy_mode_action(client_id, session_id, action);
    }

    /// If copy mode has a pending jump, handle the character and return true.
    fn handle_copy_mode_pending_jump(
        &mut self,
        session_id: u32,
        key: rmux_core::key::KeyCode,
    ) -> bool {
        let pending = self
            .sessions
            .find_by_id(session_id)
            .and_then(|s| s.active_window())
            .and_then(|w| w.active_pane())
            .and_then(|p| p.copy_mode.as_ref())
            .and_then(|cm| cm.pending_jump);

        let Some(jump_type) = pending else { return false };
        let base = rmux_core::key::keyc_base(key);
        if base >= 128 {
            return false;
        }

        let ch = base as u8 as char;
        if let Some(session) = self.sessions.find_by_id_mut(session_id) {
            if let Some(window) = session.active_window_mut() {
                if let Some(pane) = window.active_pane_mut() {
                    if let Some(cm) = &mut pane.copy_mode {
                        cm.pending_jump = None;
                        match jump_type {
                            copymode::JumpType::Forward => cm.jump_forward(&pane.screen, ch),
                            copymode::JumpType::Backward => cm.jump_backward(&pane.screen, ch),
                            copymode::JumpType::ForwardTill => {
                                cm.jump_forward_till(&pane.screen, ch);
                            }
                            copymode::JumpType::BackwardTill => {
                                cm.jump_backward_till(&pane.screen, ch);
                            }
                        }
                    }
                }
            }
        }
        self.mark_clients_redraw(session_id);
        true
    }

    /// Look up a copy-mode key binding and return (table_name, argv).
    fn copy_mode_lookup(
        &self,
        session_id: u32,
        key: rmux_core::key::KeyCode,
    ) -> Option<(String, Vec<String>)> {
        let key_table = self
            .sessions
            .find_by_id(session_id)?
            .active_window()?
            .active_pane()?
            .copy_mode
            .as_ref()?
            .key_table
            .clone();

        let base = rmux_core::key::keyc_base(key);
        let argv = self
            .keybindings
            .lookup_in_table(&key_table, base)
            .or_else(|| self.keybindings.lookup_in_table(&key_table, key))
            .cloned()?;

        Some((key_table, argv))
    }

    /// Handle the result of dispatching a copy-mode action.
    fn handle_copy_mode_action(&mut self, client_id: u64, session_id: u32, action: CopyModeAction) {
        match action {
            CopyModeAction::Handled => {
                self.mark_clients_redraw(session_id);
            }
            CopyModeAction::Exit { copy_data } => {
                if let Some(data) = copy_data {
                    self.paste_buffers.add(data);
                }
                if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                    if let Some(window) = session.active_window_mut() {
                        if let Some(pane) = window.active_pane_mut() {
                            pane.exit_copy_mode();
                        }
                    }
                }
                self.mark_clients_redraw(session_id);
            }
            CopyModeAction::SearchPrompt { forward } => {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    use crate::client::PromptType;
                    client.prompt = Some(PromptState {
                        prompt_type: if forward {
                            PromptType::SearchForward
                        } else {
                            PromptType::SearchBackward
                        },
                        ..PromptState::default()
                    });
                }
                self.mark_clients_redraw(session_id);
            }
            CopyModeAction::JumpPrompt { jump_type } => {
                if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                    if let Some(window) = session.active_window_mut() {
                        if let Some(pane) = window.active_pane_mut() {
                            if let Some(cm) = &mut pane.copy_mode {
                                cm.pending_jump = Some(jump_type);
                            }
                        }
                    }
                }
            }
            CopyModeAction::GotoLinePrompt => {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    use crate::client::PromptType;
                    client.prompt = Some(PromptState {
                        prompt_type: PromptType::GotoLine,
                        ..PromptState::default()
                    });
                }
                self.mark_clients_redraw(session_id);
            }
            CopyModeAction::CopyPipe { copy_data, command, cancel } => {
                if let Some(data) = &copy_data {
                    self.paste_buffers.add(data.clone());
                    // Pipe to the command
                    if !command.is_empty() {
                        Self::pipe_data_to_command(data, &command);
                    }
                }
                if cancel {
                    if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                        if let Some(window) = session.active_window_mut() {
                            if let Some(pane) = window.active_pane_mut() {
                                pane.exit_copy_mode();
                            }
                        }
                    }
                }
                self.mark_clients_redraw(session_id);
            }
            CopyModeAction::Unhandled => {}
        }
    }

    /// Pipe data to a shell command's stdin.
    fn pipe_data_to_command(data: &[u8], command: &str) {
        use std::io::Write;
        let child = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(data).ok();
            }
            // Don't wait — let it run in the background
        }
    }

    /// Submit the current prompt (called when Enter is pressed).
    fn submit_prompt(&mut self, client_id: u64) {
        use crate::client::PromptType;
        let (input, prompt_type, template) = {
            let Some(client) = self.clients.get_mut(&client_id) else {
                return;
            };
            let (buf, pt, tmpl) = client
                .prompt
                .as_ref()
                .map(|p| (p.buffer.clone(), p.prompt_type.clone(), p.template.clone()))
                .unwrap_or_default();
            client.prompt = None;
            (buf, pt, tmpl)
        };
        if !input.is_empty() {
            // Add to prompt history
            self.prompt_history.insert(0, input.clone());
            self.prompt_history.truncate(100);
            match prompt_type {
                PromptType::Command => {
                    // If a template is set, substitute %% with the input
                    let cmd_str = if let Some(tmpl) = &template {
                        tmpl.replace("%%", &input)
                    } else {
                        input.clone()
                    };
                    let argv = crate::config::tokenize_command(&cmd_str);
                    if !argv.is_empty() {
                        self.queue_command(client_id, argv);
                    }
                }
                PromptType::SearchForward => {
                    self.copy_mode_search(client_id, &input, true);
                }
                PromptType::SearchBackward => {
                    self.copy_mode_search(client_id, &input, false);
                }
                PromptType::GotoLine => {
                    if let Ok(line) = input.parse::<u32>() {
                        self.copy_mode_goto_line(client_id, line);
                    }
                }
            }
        }
        self.mark_prompt_redraw(client_id);
    }

    /// Handle input when the client is in command prompt mode.
    fn set_client_overlay(&mut self, client_id: u64, overlay: crate::overlay::OverlayState) {
        if let Some(client) = self.clients.get_mut(&client_id) {
            if client.is_attached() {
                client.overlay = Some(overlay);
                client.mark_redraw();
            }
        }
    }

    /// Rebuild the choose-tree overlay, preserving collapsed state per session.
    fn rebuild_tree_overlay(&mut self, client_id: u64) {
        use crate::overlay::{ListItem, ListKind, ListOverlay, OverlayState};

        let Some(client) = self.clients.get(&client_id) else { return };
        // Collect which sessions are collapsed from the current overlay
        let mut collapsed_sessions = std::collections::HashSet::new();
        let mut prev_selected = 0;
        if let Some(OverlayState::List(list)) = &client.overlay {
            prev_selected = list.selected;
            for item in &list.items {
                if item.indent == 0 && item.collapsed {
                    // Extract session name from display (before ':')
                    if let Some(name) = item.display.split(':').next() {
                        collapsed_sessions.insert(name.to_string());
                    }
                }
            }
        }

        let tree_info = self.session_tree_info();
        let mut items = Vec::new();

        for (session_name, attached, windows) in tree_info {
            let is_collapsed = collapsed_sessions.contains(&session_name);
            let attached_str =
                if attached > 0 { format!(" (attached: {attached})") } else { String::new() };
            let win_count = windows.len();
            items.push(ListItem {
                display: format!("{session_name}: {win_count} windows{attached_str}"),
                command: vec!["switch-client".into(), "-t".into(), session_name.clone()],
                indent: 0,
                collapsed: is_collapsed,
                hidden_children: if is_collapsed { win_count } else { 0 },
                deletable: true,
                delete_command: vec!["kill-session".into(), "-t".into(), session_name.clone()],
            });

            if !is_collapsed {
                for (idx, win_name, is_active, pane_count) in &windows {
                    let active_str = if *is_active { "*" } else { "" };
                    let panes_str = if *pane_count > 1 {
                        format!(" ({pane_count} panes)")
                    } else {
                        String::new()
                    };
                    items.push(ListItem {
                        display: format!("{idx}: {win_name}{active_str}{panes_str}"),
                        command: vec![
                            "select-window".into(),
                            "-t".into(),
                            format!("{session_name}:{idx}"),
                        ],
                        indent: 1,
                        collapsed: false,
                        hidden_children: 0,
                        deletable: true,
                        delete_command: vec![
                            "kill-window".into(),
                            "-t".into(),
                            format!("{session_name}:{idx}"),
                        ],
                    });
                }
            }
        }

        let selected = prev_selected.min(items.len().saturating_sub(1));
        let overlay = OverlayState::List(ListOverlay {
            items,
            selected,
            scroll_offset: 0,
            filter: String::new(),
            filtering: false,
            title: "choose-tree".into(),
            kind: ListKind::Tree,
        });
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.overlay = Some(overlay);
            client.mark_redraw();
        }
    }

    fn handle_overlay_input(&mut self, client_id: u64, data: &[u8]) {
        use crate::overlay::{OverlayAction, OverlayState, process_list_input, process_menu_input};

        let mut offset = 0;
        while offset < data.len() {
            let remaining = &data[offset..];

            let Some(client) = self.clients.get_mut(&client_id) else {
                return;
            };
            let Some(overlay) = &mut client.overlay else {
                return;
            };

            let (action, consumed) = match overlay {
                OverlayState::List(list) => process_list_input(list, remaining),
                OverlayState::Menu(menu) => process_menu_input(menu, remaining),
                OverlayState::Popup(popup) => {
                    // Forward input to the popup's PTY
                    if popup.pty_fd >= 0 {
                        // SAFETY: pty_fd is a valid open fd managed by the popup lifecycle
                        let fd = unsafe { BorrowedFd::borrow_raw(popup.pty_fd) };
                        let _ = nix::unistd::write(fd, remaining);
                    }
                    crate::overlay::process_popup_input(popup, remaining)
                }
            };

            match action {
                OverlayAction::Select { command } => {
                    // Dismiss overlay and execute the selected command
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        client.overlay = None;
                        client.mark_redraw();
                    }
                    if !command.is_empty() {
                        self.queue_command(client_id, command);
                    }
                    return;
                }
                OverlayAction::Cancel => {
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        client.overlay = None;
                        client.mark_redraw();
                    }
                    return;
                }
                OverlayAction::Delete { command } => {
                    // Execute the delete command, then redraw
                    if !command.is_empty() {
                        self.queue_command(client_id, command);
                    }
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        client.mark_redraw();
                    }
                    offset += consumed;
                }
                OverlayAction::RebuildTree => {
                    self.rebuild_tree_overlay(client_id);
                    offset += consumed;
                }
                OverlayAction::Handled => {
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        client.mark_redraw();
                    }
                    offset += consumed;
                }
                OverlayAction::Unhandled => {
                    offset += consumed.max(1);
                }
            }
        }
    }

    fn handle_prompt_input(&mut self, client_id: u64, data: &[u8]) {
        use crate::client::{PromptAction, process_prompt_input};

        let mut offset = 0;
        while offset < data.len() {
            let remaining = &data[offset..];

            let Some(client) = self.clients.get_mut(&client_id) else {
                return;
            };
            let Some(prompt) = &mut client.prompt else {
                return;
            };

            let (action, consumed) = process_prompt_input(prompt, remaining);

            match action {
                PromptAction::Submit => {
                    self.submit_prompt(client_id);
                    return;
                }
                PromptAction::Cancel => {
                    if let Some(client) = self.clients.get_mut(&client_id) {
                        client.prompt = None;
                    }
                    self.mark_prompt_redraw(client_id);
                    return;
                }
                PromptAction::Changed => {
                    self.mark_prompt_redraw(client_id);
                    offset += consumed;
                }
                PromptAction::Ignored => {
                    offset += consumed;
                }
                PromptAction::NeedMore => {
                    break;
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

        // When synchronize-panes is on, send input to all panes in the window
        let sync = window.options.get_flag("synchronize-panes").unwrap_or(false);
        if sync {
            for pane in window.panes.values() {
                if let Some(fd) = self.pty_fds.get(&pane.id) {
                    nix::unistd::write(fd, data).ok();
                }
            }
        } else {
            let Some(pane) = window.active_pane() else {
                return;
            };
            if let Some(fd) = self.pty_fds.get(&pane.id) {
                nix::unistd::write(fd, data).ok();
            }
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
            self.log_message(format!("client {client_id} disconnected"));
        }

        // If no more sessions and no more clients, shut down.
        // Don't exit while config is still loading (run-shell may spawn clients
        // that connect and disconnect before any session is created).
        if self.clients.is_empty() && self.sessions.is_empty() && !self.config_loading() {
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

    /// Log a server message (for show-messages). Caps at message-limit.
    /// Parse the `default-size` option (e.g. "80x24") into (width, height).
    ///
    /// Checks the current command client's session options first, then falls
    /// back to the global session defaults.
    fn default_size(&self) -> (u32, u32) {
        let s = self
            .client_session_id()
            .and_then(|sid| self.sessions.find_by_id(sid))
            .and_then(|session| session.options.get_string("default-size").ok())
            .unwrap_or("80x24");
        if let Some((w, h)) = s.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                return (w, h);
            }
        }
        (80, 24)
    }

    fn log_message(&mut self, msg: String) {
        let limit = self.options.get_number("message-limit").unwrap_or(1000) as usize;
        self.message_log.push_back(msg);
        while self.message_log.len() > limit {
            self.message_log.pop_front();
        }
    }

    /// Calculate how many 16ms ticks equal the status-interval (seconds).
    fn status_interval_ticks(&self) -> u32 {
        // Get status-interval from the first attached session, or fall back to server default
        let interval_secs = self
            .clients
            .values()
            .find_map(|c| {
                c.session_id.and_then(|sid| {
                    self.sessions
                        .find_by_id(sid)
                        .and_then(|s| s.options.get_number("status-interval").ok())
                })
            })
            .unwrap_or(15) as u32;
        if interval_secs == 0 {
            return 0;
        }
        // 1 second = ~62.5 ticks at 16ms
        interval_secs.saturating_mul(62)
    }

    /// Expire any timed status messages whose display-time has elapsed.
    fn expire_timed_messages(&mut self) {
        let now = std::time::Instant::now();
        for client in self.clients.values_mut() {
            if let Some((_, expiry)) = &client.timed_message {
                if now >= *expiry {
                    client.timed_message = None;
                    client.mark_redraw();
                }
            }
        }
    }

    /// Poll PTY foreground processes and update window names for auto-rename.
    fn update_window_names(&mut self) {
        let global_auto = self.options.get_flag("automatic-rename").unwrap_or(true);
        for session in self.sessions.iter_mut() {
            let auto_rename = session.options.get_flag("automatic-rename").unwrap_or(global_auto);
            if !auto_rename {
                continue;
            }
            for window in session.windows.values_mut() {
                let auto_rename_window =
                    window.options.get_flag("automatic-rename").unwrap_or(true);
                if !auto_rename_window {
                    continue;
                }
                // Get the active pane's PTY fd
                if let Some(pane) = window.panes.get(&window.active_pane) {
                    if pane.pty_fd >= 0 {
                        if let Some(name) = pty::foreground_process_name(pane.pty_fd) {
                            if name != window.name {
                                window.name = name;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn render_clients(&mut self) {
        // Collect client IDs that need redraw, along with their render info
        let to_render: Vec<(u64, u32, u32, u32, Option<String>, bool)> = self
            .clients
            .values_mut()
            .filter_map(|c| {
                if c.needs_redraw() && !c.control_mode {
                    // Timed message takes precedence over prompt
                    let prompt = if let Some((msg, _)) = &c.timed_message {
                        Some(msg.clone())
                    } else {
                        c.prompt.as_ref().map(|p| {
                            use crate::client::PromptType;
                            match p.prompt_type {
                                PromptType::SearchForward => format!("/{}", p.buffer),
                                PromptType::SearchBackward => format!("?{}", p.buffer),
                                PromptType::Command | PromptType::GotoLine => {
                                    if let Some(ps) = &p.prompt_str {
                                        format!("{ps}{}", p.buffer)
                                    } else {
                                        format!(":{}", p.buffer)
                                    }
                                }
                            }
                        })
                    };
                    let has_overlay = c.overlay.is_some();
                    c.session_id.map(|sid| (c.id, sid, c.sx, c.sy, prompt, has_overlay))
                } else {
                    None
                }
            })
            .collect();

        for (client_id, session_id, sx, sy, prompt, has_overlay) in to_render {
            // Borrow overlay from the client for rendering
            let overlay_ref = if has_overlay {
                self.clients.get(&client_id).and_then(|c| c.overlay.as_ref())
            } else {
                None
            };
            let output = self.render_session(session_id, sx, sy, prompt.as_deref(), overlay_ref);
            if let Some(client) = self.clients.get_mut(&client_id) {
                client.send(&Message::OutputData(output)).await.ok();
            }
        }
    }

    fn render_session(
        &self,
        session_id: u32,
        sx: u32,
        sy: u32,
        prompt: Option<&str>,
        overlay: Option<&crate::overlay::OverlayState>,
    ) -> Vec<u8> {
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
            .map(|(&idx, w)| {
                let mut flags = render::WindowFlags::empty();
                if idx == session.active_window {
                    flags |= render::WindowFlags::ACTIVE;
                }
                if session.last_window == Some(idx) {
                    flags |= render::WindowFlags::LAST;
                }
                if w.has_bell {
                    flags |= render::WindowFlags::BELL;
                }
                if w.has_activity {
                    flags |= render::WindowFlags::ACTIVITY;
                }
                // Gather active pane info for format expansion
                let active_pane = w.panes.get(&w.active_pane);
                let pane_current_command = active_pane
                    .map(|p| {
                        if p.pty_fd >= 0 {
                            pty::foreground_process_name(p.pty_fd).unwrap_or_else(|| w.name.clone())
                        } else {
                            w.name.clone()
                        }
                    })
                    .unwrap_or_default();
                let pane_current_path = active_pane
                    .and_then(|p| p.screen.path.clone())
                    .unwrap_or_else(|| session.cwd.clone());
                let pane_title = active_pane.map(|p| p.screen.title.clone()).unwrap_or_default();
                let pane_id = active_pane.map_or(0, |p| p.id);
                render::WindowInfo {
                    idx,
                    name: w.name.clone(),
                    flags,
                    pane_current_command,
                    pane_current_path,
                    pane_title,
                    pane_id,
                    pane_count: w.pane_count(),
                    window_id: w.id,
                }
            })
            .collect();
        window_list.sort_by_key(|w| w.idx);

        let status_config = self.build_status_config(session, window);

        // Collect @-prefixed user options for format expansion in the status line.
        // Merge server-level and session-level options (session overrides server).
        let user_options: std::collections::HashMap<String, String> = self
            .options
            .local_iter()
            .chain(session.options.local_iter())
            .filter(|(k, _)| k.starts_with('@'))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        render::render_window(
            window,
            &session.name,
            sx,
            sy,
            &window_list,
            prompt,
            Some(&status_config),
            overlay,
            Some(&user_options),
        )
    }

    /// Build `StatusConfig` from session and window options, falling back to server options.
    fn build_status_config(
        &self,
        session: &crate::session::Session,
        window: &crate::window::Window,
    ) -> render::StatusConfig {
        // Helper: check session options first, then server (global) options.
        let opt_str = |key: &str, default: &str| -> String {
            session
                .options
                .get_string(key)
                .or_else(|_| self.options.get_string(key))
                .unwrap_or(default)
                .to_string()
        };
        let opt_flag = |key: &str, default: bool| -> bool {
            session.options.get_flag(key).or_else(|_| self.options.get_flag(key)).unwrap_or(default)
        };
        let opt_num = |key: &str, default: i64| -> i64 {
            session
                .options
                .get_number(key)
                .or_else(|_| self.options.get_number(key))
                .unwrap_or(default)
        };
        let opt_style = |key: &str, default: &str| -> rmux_core::style::Style {
            let s = session
                .options
                .get_string(key)
                .or_else(|_| self.options.get_string(key))
                .unwrap_or(default);
            rmux_core::style::parse_style(s)
        };

        render::StatusConfig {
            left: opt_str("status-left", "[#{session_name}] "),
            right: opt_str("status-right", ""),
            window_status_format: opt_str("window-status-format", "#I:#W#F"),
            window_status_current_format: opt_str("window-status-current-format", "#I:#W#F"),
            status_style: opt_style("status-style", "bg=green,fg=black"),
            pane_border_style: rmux_core::style::parse_style(
                window.options.get_string("pane-border-style").unwrap_or("default"),
            ),
            pane_active_border_style: rmux_core::style::parse_style(
                window.options.get_string("pane-active-border-style").unwrap_or("fg=green"),
            ),
            status_position_top: opt_str("status-position", "bottom") == "top",
            status_enabled: opt_flag("status", true),
            status_justify: opt_str("status-justify", "left"),
            status_left_length: opt_num("status-left-length", 10) as usize,
            status_right_length: opt_num("status-right-length", 40) as usize,
            window_status_separator: opt_str("window-status-separator", " "),
            window_status_style: opt_style("window-status-style", "default"),
            window_status_current_style: opt_style("window-status-current-style", "default"),
            set_titles: opt_flag("set-titles", false),
            set_titles_string: opt_str("set-titles-string", "#S:#I:#W"),
            pane_border_status: window
                .options
                .get_string("pane-border-status")
                .unwrap_or("off")
                .to_string(),
            pane_border_format: window
                .options
                .get_string("pane-border-format")
                .unwrap_or("#{pane_index}")
                .to_string(),
        }
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

        // Update pane with PID and start command
        for session in self.sessions.iter_mut() {
            for window in session.windows.values_mut() {
                if let Some(pane) = window.panes.get_mut(&pane_id) {
                    pane.pid = spawned.pid.as_raw() as u32;
                    pane.pty_fd = master_raw;
                    pane.start_command.clone_from(&shell);
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

    /// Queue a control mode notification for a session's control clients.
    fn queue_control_notification(&mut self, session_id: u32, notification: String) {
        self.control_notifications.push((session_id, notification));
    }

    /// Drain and send all queued control notifications.
    async fn flush_control_notifications(&mut self) {
        let notifications: Vec<(u32, String)> = self.control_notifications.drain(..).collect();
        for (session_id, text) in notifications {
            let cids: Vec<u64> = self
                .clients
                .values()
                .filter(|c| c.control_mode && c.session_id == Some(session_id) && c.is_attached())
                .map(|c| c.id)
                .collect();
            if !cids.is_empty() {
                let msg = Message::OutputData(text.into_bytes());
                for cid in cids {
                    if let Some(client) = self.clients.get_mut(&cid) {
                        client.send(&msg).await.ok();
                    }
                }
            }
        }
    }

    /// Send `%output` notification to all control mode clients for a session.
    async fn send_control_output(&mut self, session_id: u32, pane_id: u32, data: &[u8]) {
        let control_clients: Vec<u64> = self
            .clients
            .values()
            .filter(|c| c.control_mode && c.session_id == Some(session_id) && c.is_attached())
            .map(|c| c.id)
            .collect();
        if control_clients.is_empty() {
            return;
        }
        let notification = Self::format_control_output(pane_id, data);
        let msg = Message::OutputData(notification.into_bytes());
        for cid in control_clients {
            if let Some(client) = self.clients.get_mut(&cid) {
                client.send(&msg).await.ok();
            }
        }
    }

    /// Format a control mode `%output` notification.
    ///
    /// tmux format: `%output %<pane_id> <octal-escaped-data>\n`
    fn format_control_output(pane_id: u32, data: &[u8]) -> String {
        format_control_output(pane_id, data)
    }

    /// Route PTY output to a popup pane if one matches. Returns true if handled.
    fn route_popup_output(
        clients: &mut HashMap<u64, ServerClient>,
        pane_id: u32,
        data: &[u8],
    ) -> bool {
        for client in clients.values_mut() {
            if let Some(crate::overlay::OverlayState::Popup(popup)) = &mut client.overlay {
                if popup.pane_id == pane_id {
                    popup.parser.parse(data, &mut popup.screen);
                    client.mark_redraw();
                    return true;
                }
            }
        }
        false
    }

    /// Spawn a popup window for a client with an embedded PTY.
    fn spawn_popup(&mut self, client_id: u64, config: command::PopupConfig) {
        use crate::overlay::{OverlayState, PopupOverlay};

        let pane_id = crate::pane::Pane::new(config.width, config.height, 0).id;

        // Get CWD from current session
        let cwd = self
            .clients
            .get(&client_id)
            .and_then(|c| c.session_id)
            .and_then(|sid| self.sessions.find_by_id(sid))
            .map_or_else(|| "/tmp".to_string(), |s| s.cwd.clone());

        // Spawn PTY process
        let default_shell = pty::default_shell();
        let shell = config.command.as_deref().unwrap_or(&default_shell);
        let pty_result = pty::Pty::open(config.width as u16, config.height as u16);
        let Ok(pty_pair) = pty_result else {
            tracing::warn!("popup: failed to open PTY");
            return;
        };

        let Ok(spawned) = pty_pair.spawn_shell(shell, &cwd) else {
            tracing::warn!("popup: failed to spawn process");
            return;
        };

        let master_raw = spawned.master_fd();
        if let Err(e) = pty::set_nonblocking(master_raw) {
            tracing::warn!("popup: failed to set non-blocking: {e}");
            return;
        }

        // Store PTY fd
        self.pty_fds.insert(pane_id, spawned.master);

        // Spawn async read task
        let tx = self.pty_tx.clone();
        let handle = tokio::spawn(async move {
            pty_read_task(master_raw, pane_id, tx).await;
        });
        self.pty_tasks.insert(pane_id, handle);

        // Create the popup overlay
        let popup = PopupOverlay {
            x: config.x,
            y: config.y,
            width: config.width,
            height: config.height,
            title: config.title,
            has_border: config.has_border,
            close_on_exit: config.close_on_exit,
            pane_id,
            screen: rmux_core::screen::Screen::new(config.width, config.height, 0),
            parser: rmux_terminal::input::InputParser::new(),
            pty_fd: master_raw,
            pid: spawned.pid.as_raw() as u32,
        };

        if let Some(client) = self.clients.get_mut(&client_id) {
            client.overlay = Some(OverlayState::Popup(Box::new(popup)));
            client.mark_redraw();
        }
    }

    /// Clean up PTY resources for a pane.
    fn cleanup_pane(&mut self, pane_id: u32) {
        if let Some(task) = self.pty_tasks.remove(&pane_id) {
            task.abort();
        }
        // Kill the child process (SIGHUP, like tmux does on pane close).
        // Must happen before dropping the OwnedFd so the signal arrives
        // while the process still has a controlling terminal.
        let pid = self
            .sessions
            .iter()
            .flat_map(|s| s.windows.values())
            .flat_map(|w| w.panes.values())
            .find(|p| p.id == pane_id)
            .map(|p| p.pid);
        if let Some(pid) = pid.filter(|&p| p > 0) {
            if let Ok(pid_i32) = i32::try_from(pid) {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid_i32),
                    nix::sys::signal::Signal::SIGHUP,
                );
            }
        }
        self.pty_fds.remove(&pane_id);
    }

    /// Send SIGHUP to all child processes on server shutdown.
    fn kill_all_children(&self) {
        for session in self.sessions.iter() {
            for window in session.windows.values() {
                for pane in window.panes.values() {
                    if pane.pid > 0 {
                        if let Ok(pid_i32) = i32::try_from(pane.pid) {
                            let _ = nix::sys::signal::kill(
                                nix::unistd::Pid::from_raw(pid_i32),
                                nix::sys::signal::Signal::SIGHUP,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Set pane-related format variables on the context.
    fn set_pane_format_vars(
        ctx: &mut crate::format::FormatContext,
        pane: &crate::pane::Pane,
        window_name: &str,
        session_cwd: &str,
    ) {
        ctx.set("pane_id", format!("%{}", pane.id));
        ctx.set("pane_index", pane.id.to_string());
        ctx.set("pane_title", &*pane.screen.title);
        ctx.set("pane_width", pane.screen.width().to_string());
        ctx.set("pane_height", pane.screen.height().to_string());
        ctx.set("pane_active", "1");
        ctx.set("pane_dead", if pane.dead { "1" } else { "0" });
        let current_cmd = if pane.pty_fd >= 0 {
            pty::foreground_process_name(pane.pty_fd).unwrap_or_else(|| window_name.to_string())
        } else {
            window_name.to_string()
        };
        ctx.set("pane_current_command", current_cmd);
        let path = pane.screen.path.as_deref().unwrap_or(session_cwd);
        ctx.set("pane_current_path", path);
        ctx.set("pane_pid", pane.pid.to_string());
        if !pane.start_command.is_empty() {
            ctx.set("pane_start_command", &*pane.start_command);
        }
        if pane.pty_fd >= 0 {
            if let Some(tty) = pty::pty_device_name(pane.pty_fd) {
                ctx.set("pane_tty", tty);
            }
        }
        ctx.set("cursor_x", pane.screen.cursor.x.to_string());
        ctx.set("cursor_y", pane.screen.cursor.y.to_string());
        ctx.set("pane_in_mode", if pane.copy_mode.is_some() { "1" } else { "0" });
        ctx.set("alternate_on", if pane.screen.alternate.is_some() { "1" } else { "0" });
        let mode = pane.screen.mode;
        ctx.set("cursor_flag", if mode.contains(ModeFlags::CURSOR_KEYS) { "1" } else { "0" });
        ctx.set("insert_flag", if mode.contains(ModeFlags::INSERT) { "1" } else { "0" });
        ctx.set("keypad_flag", if mode.contains(ModeFlags::KEYPAD) { "1" } else { "0" });
        ctx.set(
            "mouse_any_flag",
            if mode.intersects(ModeFlags::MOUSE_BUTTON | ModeFlags::MOUSE_ANY) { "1" } else { "0" },
        );
    }
}

/// Get the default window name from the user's shell (e.g. "zsh", "bash").
fn default_window_name() -> String {
    let shell = pty::default_shell();
    std::path::Path::new(&shell)
        .file_name()
        .map_or_else(|| shell.clone(), |n| n.to_string_lossy().into_owned())
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
            nix::unistd::read(inner.as_fd(), &mut buf).map_err(std::io::Error::from)
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

    fn client_last_session_id(&self) -> Option<u32> {
        self.clients.get(&self.command_client).and_then(|c| c.last_session_id)
    }

    fn detach_other_clients(&mut self) -> Result<(), ServerError> {
        let session_id = self
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let my_id = self.command_client;
        let others: Vec<u64> = self
            .clients
            .values()
            .filter(|c| c.id != my_id && c.session_id == Some(session_id) && c.is_attached())
            .map(|c| c.id)
            .collect();
        self.pending_detach.extend(others);
        Ok(())
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
        self.clients.get(&self.command_client).map_or_else(|| self.default_size().0, |c| c.sx)
    }

    fn client_sy(&self) -> u32 {
        self.clients.get(&self.command_client).map_or_else(|| self.default_size().1, |c| c.sy)
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

        // Inherit global options so session reads (status-style, base-index, etc.)
        // fall back to server-level options set by config.
        session.options = rmux_core::options::Options::with_parent(self.options.clone());

        // Create initial window with one pane
        // Reserve 1 row for status line
        let pane_height = sy.saturating_sub(1);
        let mut window = Window::new(default_window_name(), sx, pane_height);
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

        self.fire_hook("after-new-session");
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
        let sid = session.id;
        session.name = new_name.to_string();
        self.queue_control_notification(sid, format!("%session-renamed {new_name}\n"));
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

        let window_name = name.map_or_else(default_window_name, str::to_string);
        let mut window = Window::new(window_name, sx, pane_height);
        let pane = Pane::new(sx, pane_height, 2000);
        let pane_id = pane.id;
        window.active_pane = pane_id;
        window.layout = Some(LayoutCell::new_pane(0, 0, sx, pane_height, pane_id));
        window.panes.insert(pane_id, pane);

        let window_idx = session.next_window_index();
        session.windows.insert(window_idx, window);

        self.spawn_pane_process(pane_id, sx, pane_height, cwd)?;

        self.fire_hook("after-new-window");
        self.queue_control_notification(session_id, format!("%window-add @{window_idx}\n"));
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
        self.queue_control_notification(session_id, format!("%window-close @{window_idx}\n"));

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
        self.fire_hook("after-select-window");
        self.queue_control_notification(session_id, format!("%window-changed @{window_idx}\n"));
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
        self.queue_control_notification(
            session_id,
            format!("%window-renamed @{window_idx} {name}\n"),
        );
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
        _size: Option<command::SplitSize>,
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
            // Splitting cancels zoom (tmux behavior)
            window.zoomed_pane = None;

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

        // Clear zoom if the zoomed pane was killed
        if window.zoomed_pane == Some(pane_id) {
            window.zoomed_pane = None;
        }

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

    fn list_key_bindings_with_notes(&self) -> Vec<String> {
        self.keybindings.list_bindings_with_notes(true)
    }

    fn show_messages(&self) -> Vec<String> {
        self.message_log.iter().cloned().collect()
    }

    #[allow(clippy::too_many_lines)]
    fn build_format_context(&self) -> crate::format::FormatContext {
        let mut ctx = crate::format::FormatContext::new();
        // version — rmux version string (tracks tmux for plugin compat)
        ctx.set("version", env!("CARGO_PKG_VERSION"));
        if let Some(session_id) = self.client_session_id() {
            if let Some(session) = self.sessions.find_by_id(session_id) {
                ctx.set("session_name", &*session.name);
                ctx.set("session_id", format!("${session_id}"));
                ctx.set("session_windows", session.windows.len().to_string());
                ctx.set("session_attached", session.attached.to_string());
                ctx.set("session_created", session.created.to_string());
                ctx.set("session_activity", session.activity.to_string());
                // session_alerts: list windows with bell/activity flags
                let alerts: Vec<String> = session
                    .sorted_window_indices()
                    .iter()
                    .filter_map(|&idx| {
                        let w = session.windows.get(&idx)?;
                        let mut flags = String::new();
                        if w.has_bell {
                            flags.push('#');
                        }
                        if w.has_activity {
                            flags.push('!');
                        }
                        if flags.is_empty() { None } else { Some(format!("{idx}:{flags}")) }
                    })
                    .collect();
                ctx.set("session_alerts", alerts.join(", "));
                if let Some(widx) = self.client_active_window() {
                    ctx.set("window_index", widx.to_string());
                    // Window flags
                    let mut wflags = render::WindowFlags::ACTIVE;
                    if session.last_window == Some(widx) {
                        wflags |= render::WindowFlags::LAST;
                    }
                    ctx.set("window_flags", wflags.to_flag_string());
                    // window_last_flag
                    ctx.set(
                        "window_last_flag",
                        if session.last_window == Some(widx) { "1" } else { "0" },
                    );
                    if let Some(window) = session.windows.get(&widx) {
                        ctx.set("window_name", &*window.name);
                        ctx.set("window_id", format!("@{}", window.id));
                        ctx.set("window_panes", window.pane_count().to_string());
                        ctx.set("window_active", "1");
                        ctx.set(
                            "window_zoomed_flag",
                            if window.zoomed_pane.is_some() { "1" } else { "0" },
                        );
                        // pane_synchronized
                        let sync = window.options.get_flag("synchronize-panes").unwrap_or(false);
                        ctx.set("pane_synchronized", if sync { "1" } else { "0" });
                        // Window layout name
                        if let Some(layout) = &window.layout {
                            let layout_name = match layout.cell_type {
                                rmux_core::layout::LayoutType::TopBottom => "even-vertical",
                                rmux_core::layout::LayoutType::LeftRight
                                | rmux_core::layout::LayoutType::Pane => "even-horizontal",
                            };
                            ctx.set("window_layout", layout_name);
                        }
                        if let Some(pane) = window.active_pane() {
                            Self::set_pane_format_vars(&mut ctx, pane, &window.name, &session.cwd);
                        }
                    }
                }
            }
        }
        // Client info
        let client_id = self.command_client;
        ctx.set("client_name", format!("client-{client_id}"));
        ctx.set("client_tty", "/dev/tty");
        ctx.set("client_prefix", if self.keybindings.in_prefix() { "1" } else { "0" });
        if let Some(client) = self.clients.get(&client_id) {
            ctx.set("client_width", client.sx.to_string());
            ctx.set("client_height", client.sy.to_string());
            ctx.set("client_activity", client.activity.to_string());
            if let Some(sid) = client.session_id {
                if let Some(session) = self.sessions.find_by_id(sid) {
                    ctx.set("client_session", &*session.name);
                }
            }
        }
        if let Ok(hostname) = nix::unistd::gethostname() {
            let h = hostname.to_string_lossy().to_string();
            if let Some(short) = h.split('.').next() {
                ctx.set("host_short", short);
            }
            ctx.set("host", h);
        }
        // current_file — path of config file being sourced (if any)
        if let Ok(cf) = self.options.get_string("current_file") {
            ctx.set("current_file", cf);
        }
        // Collect @user options for #{@option} format expansion
        let mut user_opts: HashMap<String, String> = HashMap::new();
        for (k, v) in self.options.local_iter() {
            if k.starts_with('@') {
                user_opts.insert(k.to_string(), format_option_value(v));
            }
        }
        // Also include session-level @options
        if let Some(session_id) = self.client_session_id() {
            if let Some(session) = self.sessions.find_by_id(session_id) {
                for (k, v) in session.options.local_iter() {
                    if k.starts_with('@') {
                        user_opts.insert(k.to_string(), format_option_value(v));
                    }
                }
            }
        }
        if !user_opts.is_empty() {
            ctx.set_option_lookup(move |key| user_opts.get(key).cloned());
        }
        ctx
    }

    // --- Layout ---

    fn current_layout_name(&self, session_id: u32, window_idx: u32) -> String {
        if let Some(session) = self.sessions.find_by_id(session_id) {
            if let Some(window) = session.windows.get(&window_idx) {
                if let Some(layout) = &window.layout {
                    return match layout.cell_type {
                        rmux_core::layout::LayoutType::TopBottom => "even-vertical".to_string(),
                        rmux_core::layout::LayoutType::LeftRight
                        | rmux_core::layout::LayoutType::Pane => "even-horizontal".to_string(),
                    };
                }
            }
        }
        "even-horizontal".to_string()
    }

    // --- Misc ---

    fn execute_command(
        &mut self,
        argv: &[String],
    ) -> Result<crate::command::CommandResult, ServerError> {
        crate::command::execute_command(argv, self)
    }

    fn send_bytes_to_pane(&self, bytes: &[u8]) -> Result<(), ServerError> {
        let session_id =
            self.client_session_id().ok_or_else(|| ServerError::Command("no session".into()))?;
        let window_idx =
            self.client_active_window().ok_or_else(|| ServerError::Command("no window".into()))?;
        let pane_id =
            self.client_active_pane_id().ok_or_else(|| ServerError::Command("no pane".into()))?;
        let _ = self.write_to_pane(session_id, window_idx, pane_id, bytes);
        Ok(())
    }

    fn clear_history(&mut self) -> Result<(), ServerError> {
        let session_id =
            self.client_session_id().ok_or_else(|| ServerError::Command("no session".into()))?;
        let window_idx =
            self.client_active_window().ok_or_else(|| ServerError::Command("no window".into()))?;
        let pane_id =
            self.client_active_pane_id().ok_or_else(|| ServerError::Command("no pane".into()))?;

        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command("window not found".into()))?;
        let pane = window
            .panes
            .get_mut(&pane_id)
            .ok_or_else(|| ServerError::Command("pane not found".into()))?;
        pane.screen.grid.clear_history();
        Ok(())
    }

    // --- Redraw ---

    fn mark_clients_redraw(&mut self, session_id: u32) {
        for client in self.clients.values_mut() {
            if client.session_id == Some(session_id) && client.is_attached() {
                client.mark_redraw();
            }
        }
    }

    // --- Pipe ---

    fn pipe_pane(&mut self, command: Option<&str>) -> Result<(), ServerError> {
        let session_id =
            self.client_session_id().ok_or_else(|| ServerError::Command("no session".into()))?;
        let window_idx =
            self.client_active_window().ok_or_else(|| ServerError::Command("no window".into()))?;
        let pane_id =
            self.client_active_pane_id().ok_or_else(|| ServerError::Command("no pane".into()))?;

        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command("window not found".into()))?;
        let pane = window
            .panes
            .get_mut(&pane_id)
            .ok_or_else(|| ServerError::Command("pane not found".into()))?;

        if let Some(cmd) = command {
            pane.start_pipe(cmd).map_err(|e| ServerError::Command(format!("pipe-pane: {e}")))?;
        } else {
            pane.stop_pipe();
        }
        Ok(())
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
        self.options.set(key, parse_option_value_for_key(key, value));
        // Update the prefix key in the keybindings when the option changes
        if key == "prefix" || key == "prefix2" {
            if let Some(keycode) = crate::keybind::string_to_key(value) {
                if key == "prefix" {
                    self.keybindings.set_prefix(keycode);
                }
            }
        }
        Ok(())
    }

    fn unset_server_option(&mut self, key: &str) -> Result<(), ServerError> {
        self.options.unset(key);
        Ok(())
    }

    fn append_server_option(&mut self, key: &str, value: &str) -> Result<(), ServerError> {
        let current = self.options.get(key).map(format_option_value).unwrap_or_default();
        let new_value = format!("{current}{value}");
        self.options.set(key, parse_option_value_for_key(key, &new_value));
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
        session.options.set(key, parse_option_value_for_key(key, value));
        Ok(())
    }

    fn unset_session_option(&mut self, session_id: u32, key: &str) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        session.options.unset(key);
        Ok(())
    }

    fn append_session_option(
        &mut self,
        session_id: u32,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let current = session.options.get(key).map(format_option_value).unwrap_or_default();
        let new_value = format!("{current}{value}");
        session.options.set(key, parse_option_value_for_key(key, &new_value));
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
        window.options.set(key, parse_option_value_for_key(key, value));
        Ok(())
    }

    fn unset_window_option(
        &mut self,
        session_id: u32,
        window_idx: u32,
        key: &str,
    ) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;
        window.options.unset(key);
        Ok(())
    }

    fn append_window_option(
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
        let current = window.options.get(key).map(format_option_value).unwrap_or_default();
        let new_value = format!("{current}{value}");
        window.options.set(key, parse_option_value_for_key(key, &new_value));
        Ok(())
    }

    fn has_server_option(&self, key: &str) -> bool {
        self.options.get(key).is_some()
    }

    fn has_session_option(&self, session_id: u32, key: &str) -> bool {
        self.sessions.find_by_id(session_id).is_some_and(|s| s.options.is_local(key))
    }

    fn has_window_option(&self, session_id: u32, window_idx: u32, key: &str) -> bool {
        self.sessions
            .find_by_id(session_id)
            .is_some_and(|s| s.windows.get(&window_idx).is_some_and(|w| w.options.is_local(key)))
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
        repeatable: bool,
        note: Option<String>,
    ) -> Result<(), ServerError> {
        let key = string_to_key(key_name)
            .ok_or_else(|| ServerError::Command(format!("unknown key: {key_name}")))?;
        self.keybindings.add_binding_with_opts(table, key, argv, repeatable, note);
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

    fn clear_key_table(&mut self, table: &str) {
        self.keybindings.clear_table(table);
    }

    // --- Config ---

    fn build_config_context(&self) -> crate::config::ConfigContext {
        let mut ctx = crate::config::ConfigContext::new();
        // Collect all @user options for format expansion in %if conditions
        let mut user_opts: HashMap<String, String> = HashMap::new();
        for (k, v) in self.options.local_iter() {
            if k.starts_with('@') {
                user_opts.insert(k.to_string(), format_option_value(v));
            }
        }
        if let Some(session_id) = self.client_session_id() {
            if let Some(session) = self.sessions.find_by_id(session_id) {
                for (k, v) in session.options.local_iter() {
                    if k.starts_with('@') {
                        user_opts.insert(k.to_string(), format_option_value(v));
                    }
                }
            }
        }
        // Seed with persisted hidden vars from parent source-file calls
        for (k, v) in &self.config_hidden_vars {
            ctx.hidden_vars.insert(k.clone(), v.clone());
        }
        ctx.set_format_expand(move |expr| {
            let mut fctx = crate::format::FormatContext::new();
            if !user_opts.is_empty() {
                let opts = user_opts.clone();
                fctx.set_option_lookup(move |key| opts.get(key).cloned());
            }
            crate::format::format_expand(expr, &fctx)
        });
        ctx
    }

    fn get_config_hidden_vars(&self) -> HashMap<String, String> {
        self.config_hidden_vars.clone()
    }

    fn set_config_hidden_vars(&mut self, vars: HashMap<String, String>) {
        self.config_hidden_vars = vars;
    }

    fn execute_config_commands(&mut self, commands: Vec<Vec<String>>) -> Vec<String> {
        let mut errors = Vec::new();
        for argv in commands {
            if let Err(e) = crate::command::execute_command(&argv, self) {
                tracing::debug!("config command failed: {argv:?} -> {e}");
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

    fn resize_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        sx: Option<u32>,
        sy: Option<u32>,
    ) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;

        let new_sx = sx.unwrap_or(window.sx);
        let new_sy = sy.unwrap_or(window.sy);
        window.sx = new_sx;
        window.sy = new_sy;

        // Rebuild layout with new dimensions
        let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
        if pane_ids.len() <= 1 {
            if let Some((&pid, pane)) = window.panes.iter_mut().next() {
                pane.resize(new_sx, new_sy);
                pane.xoff = 0;
                pane.yoff = 0;
                window.layout = Some(LayoutCell::new_pane(0, 0, new_sx, new_sy, pid));
            }
        } else {
            let layout = if window
                .layout
                .as_ref()
                .is_some_and(|l| l.cell_type == rmux_core::layout::LayoutType::LeftRight)
            {
                layout_even_horizontal(new_sx, new_sy, &pane_ids)
            } else {
                layout_even_vertical(new_sx, new_sy, &pane_ids)
            };
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
        }

        self.mark_clients_redraw(session_id);
        Ok(())
    }

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

        let layout =
            window.layout.as_mut().ok_or_else(|| ServerError::Command("no layout".into()))?;

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

    fn toggle_zoom(
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

        if window.zoomed_pane == Some(pane_id) {
            // Unzoom: restore normal layout
            window.zoomed_pane = None;
        } else {
            // Zoom: set the zoomed pane
            if !window.panes.contains_key(&pane_id) {
                return Err(ServerError::Command(format!("pane not found: {pane_id}")));
            }
            window.zoomed_pane = Some(pane_id);
        }
        Ok(())
    }

    fn unzoom_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let window = session
            .windows
            .get_mut(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;
        window.zoomed_pane = None;
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
            let mut new_window = Window::new(default_window_name(), sx, pane_height);
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

    fn rotate_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        reverse: bool,
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

            // Each pane takes the position of the next/previous pane
            let n = positions.len();
            for (i, &pid) in pane_ids.iter().enumerate() {
                let target_pos =
                    if reverse { &positions[(i + n - 1) % n] } else { &positions[(i + 1) % n] };
                if let Some(pane) = window.panes.get_mut(&pid) {
                    pane.xoff = target_pos.0;
                    pane.yoff = target_pos.1;
                    pane.resize(target_pos.2, target_pos.3);
                }
            }

            // Advance/retreat active pane
            if let Some(pos) = pane_ids.iter().position(|&id| id == window.active_pane) {
                let next_active =
                    if reverse { pane_ids[(pos + n - 1) % n] } else { pane_ids[(pos + 1) % n] };
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
                "main-horizontal" | "mh" => {
                    rmux_core::layout::layout_main_horizontal(window.sx, window.sy, &pane_ids)
                }
                "main-vertical" | "mv" => {
                    rmux_core::layout::layout_main_vertical(window.sx, window.sy, &pane_ids)
                }
                "tiled" => rmux_core::layout::layout_tiled(window.sx, window.sy, &pane_ids),
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

    fn enter_command_prompt_with(
        &mut self,
        initial_text: Option<&str>,
        prompt_str: Option<&str>,
        template: Option<&str>,
    ) {
        if let Some(client) = self.clients.get_mut(&self.command_client) {
            let mut state = PromptState::default();
            if let Some(text) = initial_text {
                state.buffer = text.to_string();
                state.cursor_pos = text.len();
            }
            state.prompt_str = prompt_str.map(String::from);
            state.template = template.map(String::from);
            client.prompt = Some(state);
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

    fn dispatch_copy_mode_command(&mut self, command: &str) -> Result<bool, ServerError> {
        let session_id =
            self.client_session_id().ok_or(ServerError::Command("no session".into()))?;
        let client_id = self.command_client;

        // Check if active pane is in copy mode
        let in_copy_mode = self
            .sessions
            .find_by_id(session_id)
            .and_then(|s| s.active_window())
            .and_then(|w| w.active_pane())
            .is_some_and(|p| p.copy_mode.is_some());

        if !in_copy_mode {
            return Ok(false);
        }

        // Handle copy-pipe variants
        if command.starts_with("copy-pipe") {
            let parts: Vec<&str> = command.splitn(2, ' ').collect();
            let action_name = parts[0];
            let pipe_cmd = parts.get(1).copied().unwrap_or_default().to_string();
            let cancel = action_name == "copy-pipe-and-cancel";
            let copy_data = {
                let Some(session) = self.sessions.find_by_id_mut(session_id) else {
                    return Ok(false);
                };
                let Some(window) = session.active_window_mut() else { return Ok(false) };
                let Some(pane) = window.active_pane_mut() else { return Ok(false) };
                let Some(cm) = &mut pane.copy_mode else { return Ok(false) };
                copymode::copy_selection(&pane.screen, cm)
            };
            let action = CopyModeAction::CopyPipe { copy_data, command: pipe_cmd, cancel };
            self.handle_copy_mode_action(client_id, session_id, action);
            return Ok(true);
        }

        let action = {
            let Some(session) = self.sessions.find_by_id_mut(session_id) else {
                return Ok(false);
            };
            let Some(window) = session.active_window_mut() else { return Ok(false) };
            let Some(pane) = window.active_pane_mut() else { return Ok(false) };
            let Some(cm) = &mut pane.copy_mode else { return Ok(false) };
            copymode::dispatch_copy_mode_action(&pane.screen, cm, command)
        };

        self.handle_copy_mode_action(client_id, session_id, action);
        Ok(true)
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

    // --- Client switching ---

    fn switch_client(&mut self, session_id: u32) -> Result<(), ServerError> {
        if self.sessions.find_by_id(session_id).is_none() {
            return Err(ServerError::Command("session not found".into()));
        }
        let client_id = self.command_client;
        if let Some(client) = self.clients.get_mut(&client_id) {
            // Detach from old session
            if let Some(old_id) = client.session_id {
                client.last_session_id = Some(old_id);
                if let Some(old_session) = self.sessions.find_by_id_mut(old_id) {
                    old_session.attached = old_session.attached.saturating_sub(1);
                }
            }
            client.session_id = Some(session_id);
            client.mark_redraw();
            if let Some(session) = self.sessions.find_by_id_mut(session_id) {
                session.attached += 1;
            }
        }
        if let Some(name) = self.sessions.find_by_id(session_id).map(|s| s.name.clone()) {
            self.queue_control_notification(
                session_id,
                format!("%session-changed ${session_id} {name}\n"),
            );
        }
        Ok(())
    }

    // --- Environment ---

    fn set_environment(
        &mut self,
        session_id: Option<u32>,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError> {
        if let Some(sid) = session_id {
            let session = self
                .sessions
                .find_by_id_mut(sid)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            session.environ.insert(key.to_string(), value.to_string());
        } else {
            self.global_environ.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }

    fn unset_environment(&mut self, session_id: Option<u32>, key: &str) -> Result<(), ServerError> {
        if let Some(sid) = session_id {
            let session = self
                .sessions
                .find_by_id_mut(sid)
                .ok_or_else(|| ServerError::Command("session not found".into()))?;
            session.environ.remove(key);
        } else {
            self.global_environ.remove(key);
        }
        Ok(())
    }

    fn show_environment(&self, session_id: Option<u32>) -> Vec<String> {
        if let Some(sid) = session_id {
            if let Some(session) = self.sessions.find_by_id(sid) {
                let mut env: Vec<String> =
                    session.environ.iter().map(|(k, v)| format!("{k}={v}")).collect();
                env.sort();
                env
            } else {
                Vec::new()
            }
        } else {
            let mut env: Vec<String> =
                self.global_environ.iter().map(|(k, v)| format!("{k}={v}")).collect();
            env.sort();
            env
        }
    }

    // --- Buffer file I/O ---

    fn save_buffer(&self, name: Option<&str>, path: &str) -> Result<(), ServerError> {
        let buf = if let Some(name) = name {
            self.paste_buffers.get_by_name(name)
        } else {
            self.paste_buffers.get_top()
        };
        let buf = buf.ok_or(ServerError::Command("no buffers".into()))?;
        std::fs::write(path, &buf.data)
            .map_err(|e| ServerError::Command(format!("save-buffer: {e}")))?;
        Ok(())
    }

    fn load_buffer(&mut self, name: Option<&str>, path: &str) -> Result<(), ServerError> {
        let data =
            std::fs::read(path).map_err(|e| ServerError::Command(format!("load-buffer: {e}")))?;
        if let Some(name) = name {
            self.paste_buffers.set(name, data);
        } else {
            self.paste_buffers.add(data);
        }
        Ok(())
    }

    // --- Window search ---

    fn find_windows(&self, session_id: u32, pattern: &str) -> Vec<String> {
        let Some(session) = self.sessions.find_by_id(session_id) else {
            return Vec::new();
        };
        let mut results = Vec::new();
        for (&idx, window) in &session.windows {
            if window.name.contains(pattern) {
                results.push(format!("{idx}: {}", window.name));
            }
        }
        results.sort();
        results
    }

    // --- Client redraw ---

    fn refresh_client(&mut self) {
        let client_id = self.command_client;
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.mark_redraw();
        }
    }

    // --- Hooks ---

    fn set_hook(&mut self, hook_name: &str, argv: Vec<String>) {
        self.hooks.set(hook_name, argv);
    }

    fn remove_hook(&mut self, hook_name: &str) -> bool {
        self.hooks.remove(hook_name)
    }

    fn show_hooks(&self) -> Vec<String> {
        self.hooks.list()
    }

    // --- Prompt history ---

    fn show_prompt_history(&self) -> Vec<String> {
        self.prompt_history.clone()
    }

    fn clear_prompt_history(&mut self) {
        self.prompt_history.clear();
    }

    fn add_prompt_history(&mut self, entry: String) {
        // Don't add duplicates of the most recent entry
        if self.prompt_history.first().is_none_or(|last| *last != entry) {
            self.prompt_history.insert(0, entry);
            // Cap at 100 entries
            self.prompt_history.truncate(100);
        }
    }

    fn session_info_list(&self) -> Vec<(String, usize, usize)> {
        self.sessions
            .iter()
            .map(|s| (s.name.clone(), s.windows.len(), s.attached as usize))
            .collect()
    }

    fn session_tree_info(&self) -> Vec<SessionTreeInfo> {
        self.sessions
            .iter()
            .map(|s| {
                let mut windows: Vec<(u32, String, bool, usize)> = s
                    .windows
                    .iter()
                    .map(|(&idx, w)| (idx, w.name.clone(), idx == s.active_window, w.pane_count()))
                    .collect();
                windows.sort_by_key(|&(idx, _, _, _)| idx);
                (s.name.clone(), s.attached as usize, windows)
            })
            .collect()
    }

    fn buffer_info_list(&self) -> Vec<(String, usize, String)> {
        self.paste_buffers
            .list()
            .into_iter()
            .map(|buf| {
                let preview: String = String::from_utf8_lossy(&buf.data)
                    .chars()
                    .take(50)
                    .map(|c| if c.is_control() { '.' } else { c })
                    .collect();
                (buf.name.clone(), buf.data.len(), preview)
            })
            .collect()
    }

    fn client_info_list(&self) -> Vec<(u64, String, String)> {
        self.clients
            .values()
            .filter(|c| c.is_attached())
            .map(|c| {
                let session_name = c
                    .session_id
                    .and_then(|sid| self.sessions.find_by_id(sid))
                    .map_or("(none)".to_string(), |s| s.name.clone());
                let size = format!("{}x{}", c.sx, c.sy);
                (c.id, session_name, size)
            })
            .collect()
    }

    fn close_popup(&mut self) {
        let client_id = self.command_client;
        let popup_pane_id = self.clients.get(&client_id).and_then(|c| {
            if let Some(crate::overlay::OverlayState::Popup(popup)) = &c.overlay {
                Some(popup.pane_id)
            } else {
                None
            }
        });
        if let Some(pane_id) = popup_pane_id {
            self.cleanup_pane(pane_id);
        }
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.overlay = None;
            client.mark_redraw();
        }
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

/// Parse an option value, skipping type coercion for @-prefixed user options.
/// tmux only interprets types (Flag, Number) for built-in options.
fn parse_option_value_for_key(key: &str, value: &str) -> OptionValue {
    if key.starts_with('@') {
        OptionValue::String(value.to_string())
    } else {
        parse_option_value(value)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_name_is_shell_basename() {
        let name = default_window_name();
        // Should be a short name like "bash", "zsh", "sh" — not a full path
        assert!(!name.is_empty());
        assert!(!name.contains('/'), "should be basename, not full path: {name}");
    }

    // ============================================================
    // format_control_output
    // ============================================================

    #[test]
    fn control_output_printable_ascii() {
        let result = format_control_output(42, b"hello world");
        assert_eq!(result, "%output %42 hello world\n");
    }

    #[test]
    fn control_output_empty_data() {
        let result = format_control_output(0, b"");
        assert_eq!(result, "%output %0 \n");
    }

    #[test]
    fn control_output_backslash_escaped() {
        let result = format_control_output(1, b"a\\b");
        assert_eq!(result, "%output %1 a\\\\b\n");
    }

    #[test]
    fn control_output_control_chars_octal() {
        let result = format_control_output(5, b"\x1b[0m");
        assert_eq!(result, "%output %5 \\033[0m\n");
    }

    #[test]
    fn control_output_null_byte_octal() {
        let result = format_control_output(0, &[0x00]);
        assert_eq!(result, "%output %0 \\000\n");
    }

    #[test]
    fn control_output_newline_octal() {
        let result = format_control_output(0, b"line1\nline2");
        assert_eq!(result, "%output %0 line1\\012line2\n");
    }

    #[test]
    fn control_output_tab_octal() {
        let result = format_control_output(0, b"a\tb");
        assert_eq!(result, "%output %0 a\\011b\n");
    }

    #[test]
    fn control_output_del_octal() {
        // DEL (0x7F) is not printable, should be octal
        let result = format_control_output(0, &[0x7F]);
        assert_eq!(result, "%output %0 \\177\n");
    }

    #[test]
    fn control_output_high_bytes_octal() {
        let result = format_control_output(0, &[0xFF, 0x80]);
        assert_eq!(result, "%output %0 \\377\\200\n");
    }

    #[test]
    fn control_output_mixed_content() {
        // Mix of printable, backslash, control, and high bytes
        let result = format_control_output(99, b"OK\x1b\\END\xff");
        assert_eq!(result, "%output %99 OK\\033\\\\END\\377\n");
    }

    #[test]
    fn control_output_space_is_printable() {
        let result = format_control_output(0, b" ");
        assert_eq!(result, "%output %0  \n");
    }

    #[test]
    fn control_output_tilde_is_printable() {
        // 0x7E (~) is the last printable ASCII char
        let result = format_control_output(0, b"~");
        assert_eq!(result, "%output %0 ~\n");
    }

    // ============================================================
    // Config loading
    // ============================================================

    #[test]
    fn load_config_applies_commands() {
        let tmp = "/tmp/rmux_test_load_config.conf";
        std::fs::write(tmp, "set-option -g history-limit 7777\n").unwrap();

        let mut server = Server::new(PathBuf::from("/tmp/rmux-test-dummy/default"));
        server.load_config(tmp);

        let val = server.options.get_number("history-limit").unwrap();
        assert_eq!(val, 7777, "load_config should apply set-option commands");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn load_config_nonexistent_does_not_panic() {
        let mut server = Server::new(PathBuf::from("/tmp/rmux-test-dummy/default"));
        server.load_config("/tmp/rmux_nonexistent_config_12345.conf");
    }

    #[test]
    fn load_config_with_errors_continues() {
        let tmp = "/tmp/rmux_test_load_config_errors.conf";
        std::fs::write(tmp, "bogus-command foo\nset-option -g history-limit 3333\n").unwrap();

        let mut server = Server::new(PathBuf::from("/tmp/rmux-test-dummy/default"));
        server.load_config(tmp);

        let val = server.options.get_number("history-limit").unwrap();
        assert_eq!(val, 3333, "valid commands should apply even when others fail");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn find_default_config_prefers_tmux_conf() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_str().unwrap();

        // Create both ~/.tmux.conf and ~/.config/tmux/tmux.conf
        std::fs::write(dir.path().join(".tmux.conf"), "").unwrap();
        let config_dir = dir.path().join(".config/tmux");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("tmux.conf"), "").unwrap();

        let result = Server::find_default_config(home, None);
        assert_eq!(result, Some(format!("{home}/.tmux.conf")), "~/.tmux.conf should take priority");
    }

    #[test]
    fn find_default_config_falls_back_to_xdg_default() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_str().unwrap();

        // Only ~/.config/tmux/tmux.conf exists (no ~/.tmux.conf)
        let config_dir = dir.path().join(".config/tmux");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("tmux.conf"), "").unwrap();

        let result = Server::find_default_config(home, None);
        assert_eq!(
            result,
            Some(format!("{home}/.config/tmux/tmux.conf")),
            "should fall back to ~/.config/tmux/tmux.conf"
        );
    }

    #[test]
    fn find_default_config_uses_xdg_config_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_str().unwrap();
        let xdg_dir = dir.path().join("custom-xdg/tmux");
        std::fs::create_dir_all(&xdg_dir).unwrap();
        std::fs::write(xdg_dir.join("tmux.conf"), "").unwrap();

        let xdg = dir.path().join("custom-xdg");
        let result = Server::find_default_config(home, Some(xdg.to_str().unwrap()));
        assert_eq!(
            result,
            Some(format!("{}/tmux/tmux.conf", xdg.display())),
            "should use $XDG_CONFIG_HOME/tmux/tmux.conf"
        );
    }

    #[test]
    fn find_default_config_returns_none_when_no_config() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_str().unwrap();

        let result = Server::find_default_config(home, None);
        assert_eq!(result, None, "should return None when no config files exist");
    }

    #[test]
    fn load_config_with_bind_key() {
        let tmp = "/tmp/rmux_test_load_config_bind.conf";
        std::fs::write(tmp, "bind-key z kill-session\n").unwrap();

        let mut server = Server::new(PathBuf::from("/tmp/rmux-test-dummy/default"));
        server.load_config(tmp);

        let bindings = server.keybindings.list_bindings();
        assert!(
            bindings.iter().any(|b| b.contains('z') && b.contains("kill-session")),
            "bind-key from config should be applied"
        );

        std::fs::remove_file(tmp).ok();
    }
}
