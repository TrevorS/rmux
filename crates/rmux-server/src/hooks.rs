//! Server hooks — run commands in response to events.
//!
//! tmux fires hooks like `after-new-session`, `after-new-window`, etc.
//! Each hook name maps to a list of command arrays to execute.

use std::collections::HashMap;

/// Known hook names (not exhaustive — users can define arbitrary names).
pub const KNOWN_HOOKS: &[&str] = &[
    "after-new-session",
    "after-new-window",
    "after-select-window",
    "after-select-pane",
    "after-split-window",
    "after-kill-pane",
    "after-resize-pane",
    "after-copy-mode",
    "client-attached",
    "client-detached",
    "client-session-changed",
    "session-created",
    "session-closed",
    "window-linked",
    "window-renamed",
    "pane-exited",
];

/// Storage for hook command lists.
#[derive(Debug, Clone, Default)]
pub struct HookStore {
    /// Map of hook name → list of command argv vectors.
    hooks: HashMap<String, Vec<Vec<String>>>,
}

impl HookStore {
    /// Create an empty hook store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a command to a hook. The command will be appended to the existing list.
    pub fn add(&mut self, hook_name: &str, argv: Vec<String>) {
        self.hooks.entry(hook_name.to_string()).or_default().push(argv);
    }

    /// Set a hook, replacing all existing commands for that hook name.
    pub fn set(&mut self, hook_name: &str, argv: Vec<String>) {
        self.hooks.insert(hook_name.to_string(), vec![argv]);
    }

    /// Remove all commands for a hook name.
    pub fn remove(&mut self, hook_name: &str) -> bool {
        self.hooks.remove(hook_name).is_some()
    }

    /// Get the commands registered for a hook.
    #[must_use]
    pub fn get(&self, hook_name: &str) -> Option<&[Vec<String>]> {
        self.hooks.get(hook_name).map(Vec::as_slice)
    }

    /// List all hooks as formatted strings.
    #[must_use]
    pub fn list(&self) -> Vec<String> {
        let mut result = Vec::new();
        let mut names: Vec<&String> = self.hooks.keys().collect();
        names.sort();
        for name in names {
            if let Some(commands) = self.hooks.get(name.as_str()) {
                for (i, argv) in commands.iter().enumerate() {
                    let cmd = argv.join(" ");
                    result.push(format!("{name}[{i}]: {cmd}"));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_get_hook() {
        let mut store = HookStore::new();
        store.add("after-new-session", vec!["display-message".into(), "hello".into()]);
        let hooks = store.get("after-new-session").unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0], vec!["display-message", "hello"]);
    }

    #[test]
    fn add_multiple_commands() {
        let mut store = HookStore::new();
        store.add("after-new-session", vec!["cmd1".into()]);
        store.add("after-new-session", vec!["cmd2".into()]);
        let hooks = store.get("after-new-session").unwrap();
        assert_eq!(hooks.len(), 2);
    }

    #[test]
    fn set_replaces() {
        let mut store = HookStore::new();
        store.add("test-hook", vec!["old".into()]);
        store.add("test-hook", vec!["old2".into()]);
        store.set("test-hook", vec!["new".into()]);
        let hooks = store.get("test-hook").unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0], vec!["new"]);
    }

    #[test]
    fn remove_hook() {
        let mut store = HookStore::new();
        store.add("test-hook", vec!["cmd".into()]);
        assert!(store.remove("test-hook"));
        assert!(store.get("test-hook").is_none());
        assert!(!store.remove("nonexistent"));
    }

    #[test]
    fn list_hooks() {
        let mut store = HookStore::new();
        store.add("alpha", vec!["cmd1".into(), "arg1".into()]);
        store.add("beta", vec!["cmd2".into()]);
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], "alpha[0]: cmd1 arg1");
        assert_eq!(list[1], "beta[0]: cmd2");
    }

    #[test]
    fn get_nonexistent() {
        let store = HookStore::new();
        assert!(store.get("nope").is_none());
    }
}
