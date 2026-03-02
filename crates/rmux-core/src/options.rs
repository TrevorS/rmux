//! Hierarchical options system.
//!
//! Options are organized in a tree: server → session → window → pane.
//! Each level can override parent values. This matches tmux's options system.

use crate::error::{CoreError, CoreResult};
use crate::style::Style;
use std::collections::HashMap;

/// An option value.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    /// String value.
    String(String),
    /// Integer value.
    Number(i64),
    /// Boolean value (on/off).
    Flag(bool),
    /// Style value.
    Style(Style),
    /// Array of strings.
    Array(Vec<String>),
}

impl OptionValue {
    /// Get as string, if it is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptionValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as number, if it is one.
    #[must_use]
    pub fn as_number(&self) -> Option<i64> {
        match self {
            OptionValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Get as flag (bool), if it is one.
    #[must_use]
    pub fn as_flag(&self) -> Option<bool> {
        match self {
            OptionValue::Flag(b) => Some(*b),
            _ => None,
        }
    }

    /// Type name for error messages.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            OptionValue::String(_) => "string",
            OptionValue::Number(_) => "number",
            OptionValue::Flag(_) => "flag",
            OptionValue::Style(_) => "style",
            OptionValue::Array(_) => "array",
        }
    }
}

/// Option scope level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionScope {
    Server,
    Session,
    Window,
    Pane,
}

/// An options table (one per scope level).
#[derive(Debug, Clone, Default)]
pub struct Options {
    values: HashMap<String, OptionValue>,
    parent: Option<Box<Options>>,
}

impl Options {
    /// Create a new empty options table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create options with a parent for inheritance.
    #[must_use]
    pub fn with_parent(parent: Options) -> Self {
        Self { values: HashMap::new(), parent: Some(Box::new(parent)) }
    }

    /// Set an option value.
    pub fn set(&mut self, key: impl Into<String>, value: OptionValue) {
        self.values.insert(key.into(), value);
    }

    /// Get an option value, searching up the parent chain.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&OptionValue> {
        self.values.get(key).or_else(|| self.parent.as_ref().and_then(|p| p.get(key)))
    }

    /// Get a string option.
    pub fn get_string(&self, key: &str) -> CoreResult<&str> {
        match self.get(key) {
            Some(OptionValue::String(s)) => Ok(s),
            Some(other) => Err(CoreError::OptionTypeMismatch {
                key: key.to_string(),
                expected: "string",
                got: other.type_name(),
            }),
            None => Err(CoreError::UnknownOption(key.to_string())),
        }
    }

    /// Get a number option.
    pub fn get_number(&self, key: &str) -> CoreResult<i64> {
        match self.get(key) {
            Some(OptionValue::Number(n)) => Ok(*n),
            Some(other) => Err(CoreError::OptionTypeMismatch {
                key: key.to_string(),
                expected: "number",
                got: other.type_name(),
            }),
            None => Err(CoreError::UnknownOption(key.to_string())),
        }
    }

    /// Get a flag (boolean) option.
    pub fn get_flag(&self, key: &str) -> CoreResult<bool> {
        match self.get(key) {
            Some(OptionValue::Flag(b)) => Ok(*b),
            Some(other) => Err(CoreError::OptionTypeMismatch {
                key: key.to_string(),
                expected: "flag",
                got: other.type_name(),
            }),
            None => Err(CoreError::UnknownOption(key.to_string())),
        }
    }

    /// Remove a locally-set option (parent value will show through).
    pub fn unset(&mut self, key: &str) -> Option<OptionValue> {
        self.values.remove(key)
    }

    /// Check if a key is set at this level (not inherited).
    #[must_use]
    pub fn is_local(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// Iterate over locally-set options.
    pub fn local_iter(&self) -> impl Iterator<Item = (&str, &OptionValue)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Collect all option key-value pairs (including inherited), sorted by key.
    #[must_use]
    pub fn all_entries(&self) -> Vec<(String, String)> {
        let mut map: HashMap<String, String> = HashMap::new();
        self.collect_all(&mut map);
        let mut entries: Vec<(String, String)> = map.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    fn collect_all(&self, map: &mut HashMap<String, String>) {
        if let Some(parent) = &self.parent {
            parent.collect_all(map);
        }
        for (k, v) in &self.values {
            map.insert(k.clone(), v.to_string());
        }
    }

    /// Parse a string value into the correct OptionValue type, based on what's already stored.
    /// If the key exists, match its type. Otherwise, try auto-detection.
    pub fn parse_and_set(&mut self, key: &str, value: &str) {
        let target_type = self.get(key).map(OptionValue::type_name);
        let parsed = match target_type {
            Some("number") => {
                if let Ok(n) = value.parse::<i64>() {
                    OptionValue::Number(n)
                } else {
                    OptionValue::String(value.to_string())
                }
            }
            Some("flag") => match value {
                "on" | "true" | "1" | "yes" => OptionValue::Flag(true),
                "off" | "false" | "0" | "no" => OptionValue::Flag(false),
                _ => OptionValue::String(value.to_string()),
            },
            _ => {
                // Auto-detect type
                if let Ok(n) = value.parse::<i64>() {
                    OptionValue::Number(n)
                } else {
                    match value {
                        "on" | "true" => OptionValue::Flag(true),
                        "off" | "false" => OptionValue::Flag(false),
                        _ => OptionValue::String(value.to_string()),
                    }
                }
            }
        };
        self.set(key, parsed);
    }
}

/// Default server options (matching tmux's defaults).
#[must_use]
pub fn default_server_options() -> Options {
    let mut opts = Options::new();
    opts.set("buffer-limit", OptionValue::Number(50));
    opts.set("escape-time", OptionValue::Number(500));
    opts.set("exit-empty", OptionValue::Flag(true));
    opts.set("exit-unattached", OptionValue::Flag(false));
    opts.set("focus-events", OptionValue::Flag(false));
    opts.set("history-limit", OptionValue::Number(2000));
    opts.set("set-clipboard", OptionValue::String("external".into()));
    opts.set("terminal-overrides", OptionValue::Array(Vec::new()));
    opts
}

/// Default session options.
#[must_use]
pub fn default_session_options() -> Options {
    let mut opts = Options::new();
    opts.set("base-index", OptionValue::Number(0));
    opts.set("default-shell", OptionValue::String("/bin/sh".into()));
    opts.set("default-command", OptionValue::String(String::new()));
    opts.set("prefix", OptionValue::String("C-b".into()));
    opts.set("status", OptionValue::Flag(true));
    opts.set("status-left", OptionValue::String("[#{session_name}] ".into()));
    opts.set("status-right", OptionValue::String("\"#{=21:pane_title}\" %H:%M %d-%b-%y".into()));
    opts.set("mouse", OptionValue::Flag(false));
    opts.set("renumber-windows", OptionValue::Flag(false));
    opts
}

/// Default window options.
#[must_use]
pub fn default_window_options() -> Options {
    let mut opts = Options::new();
    opts.set("mode-keys", OptionValue::String("emacs".into()));
    opts.set("automatic-rename", OptionValue::Flag(true));
    opts.set("aggressive-resize", OptionValue::Flag(false));
    opts.set("allow-rename", OptionValue::Flag(true));
    opts.set("monitor-activity", OptionValue::Flag(false));
    opts.set("pane-border-style", OptionValue::String("default".into()));
    opts.set("pane-active-border-style", OptionValue::String("fg=green".into()));
    opts.set("remain-on-exit", OptionValue::Flag(false));
    opts
}

impl std::fmt::Display for OptionValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptionValue::String(s) => write!(f, "{s}"),
            OptionValue::Number(n) => write!(f, "{n}"),
            OptionValue::Flag(b) => write!(f, "{}", if *b { "on" } else { "off" }),
            OptionValue::Style(s) => write!(f, "{s:?}"),
            OptionValue::Array(a) => write!(f, "{}", a.join(",")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_set_get() {
        let mut opts = Options::new();
        opts.set("foo", OptionValue::String("bar".into()));
        assert_eq!(opts.get_string("foo").unwrap(), "bar");
    }

    #[test]
    fn inheritance() {
        let mut parent = Options::new();
        parent.set("inherited", OptionValue::Number(42));
        let child = Options::with_parent(parent);
        assert_eq!(child.get_number("inherited").unwrap(), 42);
    }

    #[test]
    fn override_parent() {
        let mut parent = Options::new();
        parent.set("key", OptionValue::Number(1));
        let mut child = Options::with_parent(parent);
        child.set("key", OptionValue::Number(2));
        assert_eq!(child.get_number("key").unwrap(), 2);
    }

    #[test]
    fn unknown_option() {
        let opts = Options::new();
        assert!(opts.get("nonexistent").is_none());
    }

    #[test]
    fn type_mismatch() {
        let mut opts = Options::new();
        opts.set("num", OptionValue::Number(42));
        assert!(opts.get_string("num").is_err());
    }

    #[test]
    fn unset_reveals_parent() {
        let mut parent = Options::new();
        parent.set("key", OptionValue::Number(1));
        let mut child = Options::with_parent(parent);
        child.set("key", OptionValue::Number(2));
        assert_eq!(child.get_number("key").unwrap(), 2);
        child.unset("key");
        assert_eq!(child.get_number("key").unwrap(), 1);
    }

    #[test]
    fn default_server_options_valid() {
        let opts = default_server_options();
        assert_eq!(opts.get_number("history-limit").unwrap(), 2000);
        assert!(opts.get_flag("exit-empty").unwrap());
    }
}
