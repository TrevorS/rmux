//! Session management.

use crate::window::Window;
use rmux_core::options::{default_session_options, Options};
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
        let base = self
            .options
            .get_number("base-index")
            .unwrap_or(0) as u32;
        let mut idx = base;
        while self.windows.contains_key(&idx) {
            idx += 1;
        }
        idx
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
