//! Mock CommandServer implementation for testing command handlers.
//!
//! Provides a fully functional mock server that supports all CommandServer trait
//! methods with in-memory state management (sessions, windows, panes, options,
//! key bindings).

use crate::command::{CommandServer, Direction, SessionTreeInfo};
use crate::keybind::KeyBindings;
use crate::pane::Pane;
use crate::paste::PasteBufferStore;
use crate::server::ServerError;
use crate::session::SessionManager;
use crate::window::Window;
use rmux_core::layout::{LayoutCell, layout_even_horizontal, layout_even_vertical};
use rmux_core::options::{OptionValue, Options, default_server_options};
use std::collections::HashMap;

/// A mock implementation of CommandServer for testing command handlers
/// without needing a real server event loop, sockets, or PTYs.
pub struct MockCommandServer {
    pub sessions: SessionManager,
    pub options: Options,
    pub keybindings: KeyBindings,
    pub command_client: u64,
    /// Simulated client session attachment.
    pub client_session_id: Option<u32>,
    /// Last session ID (for switch-client -l).
    pub last_session_id: Option<u32>,
    /// Simulated client terminal dimensions.
    pub client_sx: u32,
    pub client_sy: u32,
    /// Bytes written to panes via write_to_pane (for send-keys testing).
    #[allow(dead_code)]
    pub pane_writes: HashMap<u32, Vec<Vec<u8>>>,
    /// Whether command-prompt was entered.
    pub prompt_entered: bool,
    /// Redraw calls for tracking.
    pub redraw_sessions: Vec<u32>,
    /// Paste buffer store for testing.
    pub paste_buffers: PasteBufferStore,
    /// Whether copy mode was entered.
    pub copy_mode_entered: bool,
    /// Hook store for testing.
    pub hooks: crate::hooks::HookStore,
    /// Prompt history.
    pub prompt_history: Vec<String>,
    /// Global environment variables.
    pub global_environ: HashMap<String, String>,
    /// Hidden vars from %hidden directives, propagated across nested source-file calls.
    pub config_hidden_vars: HashMap<String, String>,
}

impl MockCommandServer {
    /// Create a new mock server with default options.
    pub fn new() -> Self {
        Self {
            sessions: SessionManager::new(),
            options: default_server_options(),
            keybindings: KeyBindings::default_bindings(),
            command_client: 1,
            client_session_id: None,
            last_session_id: None,
            client_sx: 80,
            client_sy: 24,
            pane_writes: HashMap::new(),
            prompt_entered: false,
            redraw_sessions: Vec::new(),
            paste_buffers: PasteBufferStore::default(),
            copy_mode_entered: false,
            hooks: crate::hooks::HookStore::new(),
            prompt_history: Vec::new(),
            global_environ: HashMap::new(),
            config_hidden_vars: HashMap::new(),
        }
    }

    /// Helper: create a session with one window and one pane.
    /// Returns (session_id, window_idx, pane_id).
    pub fn create_test_session(&mut self, name: &str) -> (u32, u32, u32) {
        let session = self.sessions.create(name.to_string(), "/tmp".to_string());
        let session_id = session.id;

        let pane_height = self.client_sy.saturating_sub(1);
        let mut window = Window::new("bash".to_string(), self.client_sx, pane_height);
        let pane = Pane::new(self.client_sx, pane_height, 2000);
        let pane_id = pane.id;
        window.active_pane = pane_id;
        window.layout = Some(LayoutCell::new_pane(0, 0, self.client_sx, pane_height, pane_id));
        window.panes.insert(pane_id, pane);

        let window_idx = session.next_window_index();
        session.active_window = window_idx;
        session.windows.insert(window_idx, window);

        // Attach mock client to this session
        self.client_session_id = Some(session_id);

        (session_id, window_idx, pane_id)
    }

    /// Helper: add a second pane to a window (simulating split-window).
    /// Returns the new pane_id.
    pub fn add_pane_to_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        horizontal: bool,
    ) -> u32 {
        let session = self.sessions.find_by_id_mut(session_id).unwrap();
        let window = session.windows.get_mut(&window_idx).unwrap();

        let pane = Pane::new(
            if horizontal { window.sx / 2 } else { window.sx },
            if horizontal { window.sy } else { window.sy / 2 },
            2000,
        );
        let new_pane_id = pane.id;
        window.panes.insert(new_pane_id, pane);

        // Rebuild layout
        let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
        let layout = if horizontal {
            layout_even_horizontal(window.sx, window.sy, &pane_ids)
        } else {
            layout_even_vertical(window.sx, window.sy, &pane_ids)
        };

        // Apply layout positions to panes
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
        new_pane_id
    }

    /// Helper: add a window to a session.
    /// Returns (window_idx, pane_id).
    pub fn add_window_to_session(&mut self, session_id: u32, name: &str) -> (u32, u32) {
        let session = self.sessions.find_by_id_mut(session_id).unwrap();
        let pane_height = self.client_sy.saturating_sub(1);
        let mut window = Window::new(name.to_string(), self.client_sx, pane_height);
        let pane = Pane::new(self.client_sx, pane_height, 2000);
        let pane_id = pane.id;
        window.active_pane = pane_id;
        window.layout = Some(LayoutCell::new_pane(0, 0, self.client_sx, pane_height, pane_id));
        window.panes.insert(pane_id, pane);

        let window_idx = session.next_window_index();
        session.windows.insert(window_idx, window);
        (window_idx, pane_id)
    }
}

fn format_option_value(val: &OptionValue) -> String {
    match val {
        OptionValue::String(s) => s.clone(),
        OptionValue::Number(n) => n.to_string(),
        OptionValue::Flag(b) => (if *b { "on" } else { "off" }).to_string(),
        OptionValue::Style(s) => format!("{s:?}"),
        OptionValue::Array(a) => a.join(","),
    }
}

fn parse_option_value(value: &str) -> OptionValue {
    if let Ok(n) = value.parse::<i64>() {
        return OptionValue::Number(n);
    }
    match value {
        "on" | "true" => OptionValue::Flag(true),
        "off" | "false" => OptionValue::Flag(false),
        _ => OptionValue::String(value.to_string()),
    }
}

/// Parse an option value, skipping type coercion for @-prefixed user options.
fn parse_option_value_for_key(key: &str, value: &str) -> OptionValue {
    if key.starts_with('@') {
        OptionValue::String(value.to_string())
    } else {
        parse_option_value(value)
    }
}

impl CommandServer for MockCommandServer {
    fn set_command_client(&mut self, client_id: u64) {
        self.command_client = client_id;
    }

    fn command_client_id(&self) -> u64 {
        self.command_client
    }

    fn client_session_id(&self) -> Option<u32> {
        self.client_session_id
    }

    fn client_last_session_id(&self) -> Option<u32> {
        self.last_session_id
    }

    fn client_active_window(&self) -> Option<u32> {
        let session_id = self.client_session_id?;
        let session = self.sessions.find_by_id(session_id)?;
        Some(session.active_window)
    }

    fn client_active_pane_id(&self) -> Option<u32> {
        let session_id = self.client_session_id?;
        let session = self.sessions.find_by_id(session_id)?;
        let window = session.active_window()?;
        Some(window.active_pane)
    }

    fn client_sx(&self) -> u32 {
        self.client_sx
    }

    fn client_sy(&self) -> u32 {
        self.client_sy
    }

    // --- Session operations ---

    fn create_session(
        &mut self,
        name: &str,
        _cwd: &str,
        _sx: u32,
        sy: u32,
    ) -> Result<u32, ServerError> {
        let session = self.sessions.create(name.to_string(), "/tmp".to_string());
        let session_id = session.id;

        let pane_height = sy.saturating_sub(1);
        let sx = self.client_sx;
        let mut window = Window::new("bash".to_string(), sx, pane_height);
        let pane = Pane::new(sx, pane_height, 2000);
        let pane_id = pane.id;
        window.active_pane = pane_id;
        window.layout = Some(LayoutCell::new_pane(0, 0, sx, pane_height, pane_id));
        window.panes.insert(pane_id, pane);

        let window_idx = session.next_window_index();
        session.active_window = window_idx;
        session.windows.insert(window_idx, window);

        Ok(session_id)
    }

    fn kill_session(&mut self, name: &str) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_name(name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {name}")))?;
        let id = session.id;
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
        _cwd: &str,
    ) -> Result<(u32, u32), ServerError> {
        let sx = self.client_sx;
        let sy = self.client_sy;
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

        Ok((window_idx, pane_id))
    }

    fn kill_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        session
            .windows
            .remove(&window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {window_idx}")))?;
        if session.active_window == window_idx {
            if let Some(&next_idx) = session.windows.keys().next() {
                session.active_window = next_idx;
            }
        }
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
        Ok(())
    }

    fn next_window(&mut self, session_id: u32) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let current = session.active_window;
        if let Some(next) = session.next_window_after(current) {
            session.select_window(next);
        }
        Ok(())
    }

    fn previous_window(&mut self, session_id: u32) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        let current = session.active_window;
        if let Some(prev) = session.prev_window_before(current) {
            session.select_window(prev);
        }
        Ok(())
    }

    fn last_window(&mut self, session_id: u32) -> Result<(), ServerError> {
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or_else(|| ServerError::Command("session not found".into()))?;
        if let Some(last) = session.last_window {
            if session.windows.contains_key(&last) {
                session.select_window(last);
            }
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
        _cwd: &str,
        _size: Option<crate::command::SplitSize>,
    ) -> Result<u32, ServerError> {
        let new_pane_id = self.add_pane_to_window(session_id, window_idx, horizontal);
        let session = self.sessions.find_by_id_mut(session_id).unwrap();
        let window = session.windows.get_mut(&window_idx).unwrap();
        window.last_active_pane = Some(window.active_pane);
        window.active_pane = new_pane_id;
        window.zoomed_pane = None; // split cancels zoom
        Ok(new_pane_id)
    }

    fn kill_pane(
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

        if window.panes.len() <= 1 {
            // Kill the window instead
            session.windows.remove(&window_idx);
            if session.active_window == window_idx {
                if let Some(&next) = session.windows.keys().next() {
                    session.active_window = next;
                }
            }
            return Ok(());
        }

        window.panes.remove(&pane_id);
        if window.active_pane == pane_id {
            if let Some(&next) = window.panes.keys().next() {
                window.active_pane = next;
            }
        }

        // Rebuild layout
        let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
        let layout = layout_even_horizontal(window.sx, window.sy, &pane_ids);
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
        window.last_active_pane = Some(window.active_pane);
        window.active_pane = pane_id;
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
                return Ok(());
            };
            let nav_dir = match direction {
                Direction::Up => crate::navigate::Direction::Up,
                Direction::Down => crate::navigate::Direction::Down,
                Direction::Left => crate::navigate::Direction::Left,
                Direction::Right => crate::navigate::Direction::Right,
            };
            crate::navigate::find_pane_in_direction(layout, window.active_pane, nav_dir)
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
        self.sessions.find_by_id(session_id).map(|s| s.active_window)
    }

    fn active_pane_id_for(&self, session_id: u32, window_idx: u32) -> Option<u32> {
        let session = self.sessions.find_by_id(session_id)?;
        let window = session.windows.get(&window_idx)?;
        Some(window.active_pane)
    }

    // --- PTY I/O ---

    fn write_to_pane(
        &self,
        _session_id: u32,
        _window_idx: u32,
        pane_id: u32,
        data: &[u8],
    ) -> Result<(), ServerError> {
        // In mock, record the writes for assertion
        // We need to use interior mutability here since the trait method takes &self
        // Instead, we'll accept this limitation in mock -- data is lost.
        // Tests that need to verify writes should check behavior differently.
        let _ = (pane_id, data);
        Ok(())
    }

    // --- Options ---

    fn get_server_option(&self, key: &str) -> Result<String, ServerError> {
        self.options
            .get(key)
            .map(format_option_value)
            .ok_or_else(|| ServerError::Command(format!("unknown option: {key}")))
    }

    fn set_server_option(&mut self, key: &str, value: &str) -> Result<(), ServerError> {
        self.options.set(key, parse_option_value_for_key(key, value));
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
        let key = crate::keybind::string_to_key(key_name)
            .ok_or_else(|| ServerError::Command(format!("unknown key: {key_name}")))?;
        self.keybindings.add_binding_with_opts(table, key, argv, repeatable, note);
        Ok(())
    }

    fn remove_key_binding(&mut self, table: &str, key_name: &str) -> Result<(), ServerError> {
        let key = crate::keybind::string_to_key(key_name)
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
        // Collect @user options for format expansion
        let mut user_opts: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (k, v) in self.options.local_iter() {
            if k.starts_with('@') {
                user_opts.insert(
                    k.to_string(),
                    match v {
                        rmux_core::options::OptionValue::String(s) => s.clone(),
                        rmux_core::options::OptionValue::Number(n) => n.to_string(),
                        rmux_core::options::OptionValue::Flag(b) => {
                            if *b { "on" } else { "off" }.to_string()
                        }
                        rmux_core::options::OptionValue::Style(s) => format!("{s:?}"),
                        rmux_core::options::OptionValue::Array(a) => a.join(","),
                    },
                );
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
            let trimmed = line_buf.trim_end();
            lines.push(trimmed.to_string());
        }
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
        if let Some(w) = sx {
            window.sx = w;
        }
        if let Some(h) = sy {
            window.sy = h;
        }
        Ok(())
    }

    fn resize_pane(
        &mut self,
        session_id: u32,
        _window_idx: u32,
        _pane_id: u32,
        _direction: Option<Direction>,
        _amount: u32,
    ) -> Result<(), ServerError> {
        self.redraw_sessions.push(session_id);
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
            window.zoomed_pane = None;
        } else {
            window.zoomed_pane = Some(pane_id);
        }
        self.redraw_sessions.push(session_id);
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
        let mut pane_a = window.panes.remove(&src).unwrap();
        let mut pane_b = window.panes.remove(&dst).unwrap();
        std::mem::swap(&mut pane_a.xoff, &mut pane_b.xoff);
        std::mem::swap(&mut pane_a.yoff, &mut pane_b.yoff);
        std::mem::swap(&mut pane_a.sx, &mut pane_b.sx);
        std::mem::swap(&mut pane_a.sy, &mut pane_b.sy);
        window.panes.insert(src, pane_b);
        window.panes.insert(dst, pane_a);
        Ok(())
    }

    fn swap_window(
        &mut self,
        session_id: u32,
        src_idx: u32,
        dst_idx: u32,
    ) -> Result<(), ServerError> {
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
        Ok(())
    }

    fn move_window(
        &mut self,
        src_session_id: u32,
        src_idx: u32,
        dst_session_id: u32,
        dst_idx: u32,
    ) -> Result<(), ServerError> {
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
        {
            let session = self.sessions.find_by_id_mut(src_session_id).unwrap();
            if session.active_window == src_idx {
                if let Some(&next) = session.windows.keys().next() {
                    session.active_window = next;
                }
            }
        }
        let session = self
            .sessions
            .find_by_id_mut(dst_session_id)
            .ok_or_else(|| ServerError::Command("destination session not found".into()))?;
        if session.windows.contains_key(&dst_idx) {
            return Err(ServerError::Command(format!(
                "window index {dst_idx} already exists in destination session"
            )));
        }
        session.windows.insert(dst_idx, window);
        Ok(())
    }

    fn break_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<u32, ServerError> {
        let pane = {
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
            if window.active_pane == pane_id {
                if let Some(&next) = window.panes.keys().next() {
                    window.active_pane = next;
                }
            }
            // Rebuild layout
            let pane_ids: Vec<u32> = window.panes.keys().copied().collect();
            let layout = layout_even_horizontal(window.sx, window.sy, &pane_ids);
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

        let new_window_idx = {
            let session = self.sessions.find_by_id_mut(session_id).unwrap();
            let new_idx = session.next_window_index();
            let sx = self.client_sx;
            let pane_height = self.client_sy.saturating_sub(1);
            let mut new_window = Window::new("bash".to_string(), sx, pane_height);
            new_window.active_pane = pane.id;
            new_window.layout = Some(LayoutCell::new_pane(0, 0, sx, pane_height, pane.id));
            new_window.panes.insert(pane.id, pane);
            session.windows.insert(new_idx, new_window);
            new_idx
        };

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
            if window.active_pane == src_pane_id {
                if let Some(&next) = window.panes.keys().next() {
                    window.active_pane = next;
                }
            }
            if window.panes.is_empty() {
                session.windows.remove(&src_window_idx);
                if session.active_window == src_window_idx {
                    if let Some(&next) = session.windows.keys().next() {
                        session.active_window = next;
                    }
                }
            } else {
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

        let session = self
            .sessions
            .find_by_id_mut(dst_session_id)
            .ok_or_else(|| ServerError::Command("destination session not found".into()))?;
        let window = session
            .windows
            .get_mut(&dst_window_idx)
            .ok_or_else(|| ServerError::Command(format!("window not found: {dst_window_idx}")))?;
        let pid = pane.id;
        window.panes.insert(pid, pane);
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
        Ok(())
    }

    fn last_pane(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError> {
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
                return Ok(());
            }
        }
        Err(ServerError::Command("no last pane".into()))
    }

    fn rotate_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        reverse: bool,
    ) -> Result<(), ServerError> {
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
        let positions: Vec<(u32, u32, u32, u32)> = pane_ids
            .iter()
            .map(|&id| {
                let p = &window.panes[&id];
                (p.xoff, p.yoff, p.sx, p.sy)
            })
            .collect();
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
        if let Some(pos) = pane_ids.iter().position(|&id| id == window.active_pane) {
            let next_active =
                if reverse { pane_ids[(pos + n - 1) % n] } else { pane_ids[(pos + 1) % n] };
            window.active_pane = next_active;
        }
        Ok(())
    }

    fn select_layout(
        &mut self,
        session_id: u32,
        window_idx: u32,
        layout_name: &str,
    ) -> Result<(), ServerError> {
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
            _ => return Err(ServerError::Command(format!("unknown layout: {layout_name}"))),
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
        Ok(())
    }

    fn respawn_pane(
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
        let pane = window
            .panes
            .get_mut(&pane_id)
            .ok_or_else(|| ServerError::Command(format!("pane not found: %{pane_id}")))?;
        // Reset screen
        let sx = pane.sx;
        let sy = pane.sy;
        pane.screen = rmux_core::screen::Screen::new(sx, sy, 2000);
        Ok(())
    }

    // --- Command prompt ---

    fn enter_command_prompt_with(
        &mut self,
        _initial_text: Option<&str>,
        _prompt_str: Option<&str>,
        _template: Option<&str>,
    ) {
        self.prompt_entered = true;
    }

    // --- Copy mode ---

    fn enter_copy_mode(&mut self) -> Result<(), ServerError> {
        self.copy_mode_entered = true;
        let session_id = self.client_session_id.ok_or(ServerError::Command("no session".into()))?;
        let mode_keys = self.pane_mode_keys();
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or(ServerError::Command("session not found".into()))?;
        let window =
            session.windows.values_mut().next().ok_or(ServerError::Command("no window".into()))?;
        let pane =
            window.panes.values_mut().next().ok_or(ServerError::Command("no pane".into()))?;
        pane.enter_copy_mode(&mode_keys);
        Ok(())
    }

    fn dispatch_copy_mode_command(&mut self, command: &str) -> Result<bool, ServerError> {
        use crate::copymode;
        let session_id = self.client_session_id.ok_or(ServerError::Command("no session".into()))?;
        let session = self
            .sessions
            .find_by_id_mut(session_id)
            .ok_or(ServerError::Command("session not found".into()))?;
        let window = session.active_window_mut().ok_or(ServerError::Command("no window".into()))?;
        let pane = window.active_pane_mut().ok_or(ServerError::Command("no pane".into()))?;
        let Some(cm) = &mut pane.copy_mode else {
            return Ok(false);
        };
        let action = copymode::dispatch_copy_mode_action(&pane.screen, cm, command);
        if let copymode::CopyModeAction::Exit { copy_data } = action {
            pane.copy_mode = None;
            if let Some(data) = copy_data {
                self.paste_buffers.add(data);
            }
        }
        Ok(true)
    }

    fn pane_mode_keys(&self) -> String {
        if let Some(session_id) = self.client_session_id {
            if let Some(session) = self.sessions.find_by_id(session_id) {
                if let Some(window) = session.windows.values().next() {
                    if let Some(val) = window.options.get("mode-keys") {
                        if let Some(s) = val.as_str() {
                            return s.to_string();
                        }
                    }
                }
            }
        }
        "emacs".to_string()
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
        buf.ok_or(ServerError::Command("no buffers".into()))?;
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

    // --- Info ---

    fn list_clients(&self) -> Vec<String> {
        vec![format!("client {}: {}x{}", self.command_client, self.client_sx, self.client_sy)]
    }

    fn list_all_commands(&self) -> Vec<String> {
        crate::command::builtins::COMMANDS
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
        Vec::new()
    }

    fn build_format_context(&self) -> crate::format::FormatContext {
        let mut ctx = crate::format::FormatContext::new();
        ctx.set("version", env!("CARGO_PKG_VERSION"));
        if let Some(session_id) = self.client_session_id() {
            if let Some(session) = self.sessions.find_by_id(session_id) {
                ctx.set("session_name", &*session.name);
                ctx.set("session_windows", session.windows.len().to_string());
                ctx.set("session_created", session.created.to_string());
                ctx.set("session_activity", session.activity.to_string());
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
                    ctx.set(
                        "window_last_flag",
                        if session.last_window == Some(widx) { "1" } else { "0" },
                    );
                    if let Some(window) = session.windows.get(&widx) {
                        ctx.set("window_name", &*window.name);
                        ctx.set("window_panes", window.pane_count().to_string());
                        ctx.set(
                            "window_zoomed_flag",
                            if window.zoomed_pane.is_some() { "1" } else { "0" },
                        );
                        let sync = window.options.get_flag("synchronize-panes").unwrap_or(false);
                        ctx.set("pane_synchronized", if sync { "1" } else { "0" });
                        if let Some(pane) = window.active_pane() {
                            ctx.set("pane_id", format!("%{}", pane.id));
                            ctx.set("pane_index", pane.id.to_string());
                            ctx.set("pane_title", &*pane.screen.title);
                            ctx.set("pane_width", pane.screen.width().to_string());
                            ctx.set("pane_height", pane.screen.height().to_string());
                            ctx.set("pane_active", "1");
                            ctx.set("pane_dead", if pane.dead { "1" } else { "0" });
                            ctx.set("pane_start_command", &*pane.start_command);
                        }
                    }
                }
            }
        }
        ctx.set("client_prefix", "0");
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

    // --- Redraw ---

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

    fn execute_command(
        &mut self,
        argv: &[String],
    ) -> Result<crate::command::CommandResult, ServerError> {
        crate::command::execute_command(argv, self)
    }

    fn send_bytes_to_pane(&self, _bytes: &[u8]) -> Result<(), ServerError> {
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

    // --- Client switching ---

    fn switch_client(&mut self, session_id: u32) -> Result<(), ServerError> {
        if self.sessions.find_by_id(session_id).is_none() {
            return Err(ServerError::Command("session not found".into()));
        }
        self.last_session_id = self.client_session_id;
        self.client_session_id = Some(session_id);
        Ok(())
    }

    fn detach_other_clients(&mut self) -> Result<(), ServerError> {
        // Mock: no-op (only one simulated client)
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
        if let Some(session_id) = self.client_session_id {
            self.redraw_sessions.push(session_id);
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

    fn mark_clients_redraw(&mut self, session_id: u32) {
        self.redraw_sessions.push(session_id);
    }

    fn pipe_pane(&mut self, _command: Option<&str>) -> Result<(), ServerError> {
        Ok(())
    }

    fn show_prompt_history(&self) -> Vec<String> {
        self.prompt_history.clone()
    }

    fn clear_prompt_history(&mut self) {
        self.prompt_history.clear();
    }

    fn add_prompt_history(&mut self, entry: String) {
        if self.prompt_history.first().is_none_or(|last| *last != entry) {
            self.prompt_history.insert(0, entry);
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
        // Mock: return a single client entry
        if let Some(sid) = self.client_session_id {
            let session_name =
                self.sessions.find_by_id(sid).map_or("(none)".to_string(), |s| s.name.clone());
            let size = format!("{}x{}", self.client_sx, self.client_sy);
            vec![(self.command_client, session_name, size)]
        } else {
            vec![]
        }
    }

    fn close_popup(&mut self) {
        // Mock: no-op
    }
}
