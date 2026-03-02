//! Session management.

use crate::window::Window;
use rmux_core::options::{Options, default_session_options};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(0);

/// A tmux session.
#[derive(Debug)]
pub struct Session {
    /// Unique session ID.
    pub id: u32,
    /// Session name.
    pub name: String,
    /// Current working directory.
    pub cwd: String,
    /// Windows in this session, keyed by window index.
    pub windows: HashMap<u32, Window>,
    /// Active window index.
    pub active_window: u32,
    /// Last active window index (for `last-window` command).
    pub last_window: Option<u32>,
    /// Session options.
    pub options: Options,
    /// Number of attached clients.
    pub attached: u32,
}

impl Session {
    /// Create a new session with the given name.
    #[must_use]
    pub fn new(name: String, cwd: String) -> Self {
        Self {
            id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            name,
            cwd,
            windows: HashMap::new(),
            active_window: 0,
            last_window: None,
            options: default_session_options(),
            attached: 0,
        }
    }

    /// Get the active window.
    #[must_use]
    pub fn active_window(&self) -> Option<&Window> {
        self.windows.get(&self.active_window)
    }

    /// Get the active window mutably.
    pub fn active_window_mut(&mut self) -> Option<&mut Window> {
        self.windows.get_mut(&self.active_window)
    }

    /// Next available window index.
    #[must_use]
    pub fn next_window_index(&self) -> u32 {
        let base = self.options.get_number("base-index").unwrap_or(0) as u32;
        let mut idx = base;
        while self.windows.contains_key(&idx) {
            idx += 1;
        }
        idx
    }

    /// Switch to a window by index, updating last_window.
    pub fn select_window(&mut self, idx: u32) -> bool {
        if !self.windows.contains_key(&idx) || idx == self.active_window {
            return false;
        }
        self.last_window = Some(self.active_window);
        self.active_window = idx;
        true
    }

    /// Get sorted window indices.
    pub fn sorted_window_indices(&self) -> Vec<u32> {
        let mut indices: Vec<u32> = self.windows.keys().copied().collect();
        indices.sort_unstable();
        indices
    }

    /// Get the next window index (wrapping around).
    pub fn next_window_after(&self, current: u32) -> Option<u32> {
        let indices = self.sorted_window_indices();
        if indices.len() < 2 {
            return None;
        }
        let pos = indices.iter().position(|&i| i == current)?;
        let next = (pos + 1) % indices.len();
        Some(indices[next])
    }

    /// Get the previous window index (wrapping around).
    pub fn prev_window_before(&self, current: u32) -> Option<u32> {
        let indices = self.sorted_window_indices();
        if indices.len() < 2 {
            return None;
        }
        let pos = indices.iter().position(|&i| i == current)?;
        let prev = if pos == 0 { indices.len() - 1 } else { pos - 1 };
        Some(indices[prev])
    }
}

/// Manages all sessions on the server.
#[derive(Debug, Default)]
pub struct SessionManager {
    sessions: HashMap<u32, Session>,
}

impl SessionManager {
    /// Create a new session manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new session.
    pub fn create(&mut self, name: String, cwd: String) -> &mut Session {
        let session = Session::new(name, cwd);
        let id = session.id;
        self.sessions.insert(id, session);
        self.sessions.get_mut(&id).unwrap()
    }

    /// Find a session by name.
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<&Session> {
        self.sessions.values().find(|s| s.name == name)
    }

    /// Find a session by name (mutable).
    pub fn find_by_name_mut(&mut self, name: &str) -> Option<&mut Session> {
        self.sessions.values_mut().find(|s| s.name == name)
    }

    /// Find a session by ID.
    #[must_use]
    pub fn find_by_id(&self, id: u32) -> Option<&Session> {
        self.sessions.get(&id)
    }

    /// Find a session by ID (mutable).
    pub fn find_by_id_mut(&mut self, id: u32) -> Option<&mut Session> {
        self.sessions.get_mut(&id)
    }

    /// Remove a session.
    pub fn remove(&mut self, id: u32) -> Option<Session> {
        self.sessions.remove(&id)
    }

    /// Number of sessions.
    #[must_use]
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Iterate over all sessions.
    pub fn iter(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values()
    }

    /// Iterate over all sessions mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Session> {
        self.sessions.values_mut()
    }

    /// Is the session list empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_session() {
        let mut mgr = SessionManager::new();
        let session = mgr.create("test".into(), "/home".into());
        assert_eq!(session.name, "test");
        assert_eq!(mgr.count(), 1);
    }

    #[test]
    fn find_by_name() {
        let mut mgr = SessionManager::new();
        mgr.create("foo".into(), "/".into());
        mgr.create("bar".into(), "/".into());
        assert!(mgr.find_by_name("foo").is_some());
        assert!(mgr.find_by_name("baz").is_none());
    }

    #[test]
    fn remove_session() {
        let mut mgr = SessionManager::new();
        let session = mgr.create("test".into(), "/".into());
        let id = session.id;
        assert_eq!(mgr.count(), 1);
        mgr.remove(id);
        assert_eq!(mgr.count(), 0);
    }
}
