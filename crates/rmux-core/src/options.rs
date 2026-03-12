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
    opts.set("default-terminal", OptionValue::String("screen".into()));
    opts.set("message-limit", OptionValue::Number(1000));
    opts.set("prefix-timeout", OptionValue::Number(0));
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
    opts.set("automatic-rename", OptionValue::Flag(true));
    opts.set("status-style", OptionValue::String("bg=green,fg=black".into()));
    opts.set("status-position", OptionValue::String("bottom".into()));
    opts.set("window-status-format", OptionValue::String("#I:#W#F".into()));
    opts.set("window-status-current-format", OptionValue::String("#I:#W#F".into()));
    opts.set("display-time", OptionValue::Number(750));
    opts.set("repeat-time", OptionValue::Number(500));
    opts.set("status-interval", OptionValue::Number(15));
    opts.set("status-justify", OptionValue::String("left".into()));
    opts.set("status-keys", OptionValue::String("emacs".into()));
    opts.set("status-left-length", OptionValue::Number(10));
    opts.set("status-right-length", OptionValue::Number(40));
    opts.set("status-left-style", OptionValue::String("default".into()));
    opts.set("status-right-style", OptionValue::String("default".into()));
    opts.set("message-style", OptionValue::String("bg=yellow,fg=black".into()));
    opts.set("message-command-style", OptionValue::String("bg=black,fg=yellow".into()));
    opts.set("set-titles", OptionValue::Flag(false));
    opts.set(
        "set-titles-string",
        OptionValue::String("#S:#I:#W - \"#T\" #{session_alerts}".into()),
    );
    opts.set("prefix2", OptionValue::String("None".into()));
    opts.set("destroy-unattached", OptionValue::Flag(false));
    opts.set("detach-on-destroy", OptionValue::String("on".into()));
    opts.set("word-separators", OptionValue::String(" ".into()));
    opts.set("visual-activity", OptionValue::String("off".into()));
    opts.set("visual-bell", OptionValue::String("off".into()));
    opts.set("visual-silence", OptionValue::String("off".into()));
    opts.set("default-size", OptionValue::String("80x24".into()));
    opts.set("key-table", OptionValue::String("root".into()));
    opts.set("bell-action", OptionValue::String("any".into()));
    opts.set("activity-action", OptionValue::String("other".into()));
    opts.set("silence-action", OptionValue::String("none".into()));
    opts.set("lock-after-time", OptionValue::Number(0)); // 0 = disabled
    opts.set("lock-command", OptionValue::String("lock -np".into()));
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
    opts.set("pane-border-status", OptionValue::String("off".into()));
    opts.set(
        "pane-border-format",
        OptionValue::String(
            "#{?pane_active,#[reverse],}#{pane_index}#[default] \"#{pane_title}\"".into(),
        ),
    );
    opts.set("remain-on-exit", OptionValue::Flag(false));
    opts.set("alternate-screen", OptionValue::Flag(true));
    opts.set("monitor-bell", OptionValue::Flag(true));
    opts.set("monitor-silence", OptionValue::Number(0));
    opts.set("synchronize-panes", OptionValue::Flag(false));
    opts.set("wrap-search", OptionValue::Flag(true));
    opts.set("pane-base-index", OptionValue::Number(0));
    opts.set("main-pane-height", OptionValue::Number(24));
    opts.set("main-pane-width", OptionValue::Number(80));
    opts.set("window-status-style", OptionValue::String("default".into()));
    opts.set("window-status-current-style", OptionValue::String("default".into()));
    opts.set("window-status-last-style", OptionValue::String("default".into()));
    opts.set("window-status-activity-style", OptionValue::String("reverse".into()));
    opts.set("window-status-bell-style", OptionValue::String("reverse".into()));
    opts.set("window-status-separator", OptionValue::String(" ".into()));
    opts.set("window-active-style", OptionValue::String("default".into()));
    opts.set("window-style", OptionValue::String("default".into()));
    opts.set("allow-passthrough", OptionValue::Flag(false));
    opts.set("xterm-keys", OptionValue::Flag(true));
    opts.set("copy-mode-match-style", OptionValue::String("bg=cyan,fg=black".into()));
    opts.set("copy-mode-current-match-style", OptionValue::String("bg=magenta,fg=black".into()));
    opts.set("copy-mode-mark-style", OptionValue::String("bg=red,fg=black".into()));
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

    #[test]
    fn parse_and_set_number() {
        let mut opts = Options::new();
        opts.parse_and_set("count", "42");
        assert_eq!(opts.get("count"), Some(&OptionValue::Number(42)));
    }

    #[test]
    fn parse_and_set_flag() {
        let mut opts = Options::new();
        opts.parse_and_set("enabled", "on");
        assert_eq!(opts.get("enabled"), Some(&OptionValue::Flag(true)));

        opts.parse_and_set("disabled", "off");
        assert_eq!(opts.get("disabled"), Some(&OptionValue::Flag(false)));

        // "true" and "false" should also work via auto-detection.
        opts.parse_and_set("truthy", "true");
        assert_eq!(opts.get("truthy"), Some(&OptionValue::Flag(true)));

        opts.parse_and_set("falsy", "false");
        assert_eq!(opts.get("falsy"), Some(&OptionValue::Flag(false)));
    }

    #[test]
    fn parse_and_set_string() {
        let mut opts = Options::new();
        opts.parse_and_set("name", "hello world");
        assert_eq!(opts.get("name"), Some(&OptionValue::String("hello world".into())));
    }

    #[test]
    fn deep_inheritance_chain() {
        // Three-level parent chain: grandparent -> parent -> child.
        let mut grandparent = Options::new();
        grandparent.set("gp-key", OptionValue::String("from-grandparent".into()));
        grandparent.set("shared", OptionValue::Number(1));

        let mut parent = Options::with_parent(grandparent);
        parent.set("parent-key", OptionValue::String("from-parent".into()));
        parent.set("shared", OptionValue::Number(2)); // Override grandparent.

        let child = Options::with_parent(parent);

        // Child sees grandparent value.
        assert_eq!(child.get_string("gp-key").unwrap(), "from-grandparent");
        // Child sees parent value.
        assert_eq!(child.get_string("parent-key").unwrap(), "from-parent");
        // Child sees parent's override of shared key.
        assert_eq!(child.get_number("shared").unwrap(), 2);
    }

    #[test]
    fn local_iter_only_local() {
        let mut parent = Options::new();
        parent.set("inherited", OptionValue::Number(1));

        let mut child = Options::with_parent(parent);
        child.set("local", OptionValue::String("mine".into()));

        let local_keys: Vec<&str> = child.local_iter().map(|(k, _)| k).collect();
        assert!(local_keys.contains(&"local"));
        assert!(!local_keys.contains(&"inherited"));
        assert_eq!(local_keys.len(), 1);
    }

    #[test]
    fn all_entries_includes_inherited() {
        let mut parent = Options::new();
        parent.set("alpha", OptionValue::String("a".into()));
        parent.set("beta", OptionValue::Number(2));

        let mut child = Options::with_parent(parent);
        child.set("gamma", OptionValue::Flag(true));

        let entries = child.all_entries();
        let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"alpha"));
        assert!(keys.contains(&"beta"));
        assert!(keys.contains(&"gamma"));
        // Entries should be sorted by key.
        assert_eq!(keys, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn type_name_for_each_variant() {
        assert_eq!(OptionValue::String("x".into()).type_name(), "string");
        assert_eq!(OptionValue::Number(0).type_name(), "number");
        assert_eq!(OptionValue::Flag(true).type_name(), "flag");
        assert_eq!(OptionValue::Style(Style::DEFAULT).type_name(), "style");
        assert_eq!(OptionValue::Array(vec![]).type_name(), "array");
    }

    #[test]
    fn get_style_returns_none_for_non_style() {
        let mut opts = Options::new();
        opts.set("num", OptionValue::Number(42));
        opts.set("str", OptionValue::String("hello".into()));
        opts.set("flag", OptionValue::Flag(true));

        // Getting as a different type should return None from the accessor.
        assert!(opts.get("num").unwrap().as_str().is_none());
        assert!(opts.get("num").unwrap().as_flag().is_none());
        assert!(opts.get("str").unwrap().as_number().is_none());
        assert!(opts.get("str").unwrap().as_flag().is_none());
        assert!(opts.get("flag").unwrap().as_str().is_none());
        assert!(opts.get("flag").unwrap().as_number().is_none());
    }

    #[test]
    fn default_session_options_valid() {
        let opts = default_session_options();
        // Should have standard session keys.
        assert_eq!(opts.get_number("base-index").unwrap(), 0);
        assert_eq!(opts.get_string("default-shell").unwrap(), "/bin/sh");
        assert_eq!(opts.get_string("prefix").unwrap(), "C-b");
        assert!(opts.get_flag("status").unwrap());
        assert!(!opts.get_flag("mouse").unwrap());
        assert!(!opts.get_flag("renumber-windows").unwrap());
    }

    #[test]
    fn default_session_has_default_size() {
        let opts = default_session_options();
        assert_eq!(opts.get_string("default-size").unwrap(), "80x24");
    }

    #[test]
    fn default_window_has_monitor_bell() {
        let opts = default_window_options();
        assert!(opts.get_flag("monitor-bell").unwrap());
        assert!(!opts.get_flag("monitor-activity").unwrap());
    }

    #[test]
    fn default_window_options_valid() {
        let opts = default_window_options();
        // Should have standard window keys.
        assert_eq!(opts.get_string("mode-keys").unwrap(), "emacs");
        assert!(opts.get_flag("automatic-rename").unwrap());
        assert!(!opts.get_flag("aggressive-resize").unwrap());
        assert!(opts.get_flag("allow-rename").unwrap());
        assert!(!opts.get_flag("monitor-activity").unwrap());
        assert!(!opts.get_flag("remain-on-exit").unwrap());
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn set_get_roundtrip(
                key in "[a-z-]{1,30}",
                value in "[a-zA-Z0-9 ]{0,100}"
            ) {
                let mut opts = Options::new();
                opts.set(&key, OptionValue::String(value.clone()));
                let got = opts.get(&key);
                prop_assert_eq!(got, Some(&OptionValue::String(value)));
            }

            #[test]
            fn unset_makes_get_return_none(
                key in "[a-z-]{1,30}",
                value in "[a-zA-Z0-9]{1,50}"
            ) {
                let mut opts = Options::new();
                opts.set(&key, OptionValue::String(value));
                opts.unset(&key);
                prop_assert!(opts.get(&key).is_none());
            }

            #[test]
            fn parse_and_set_number_roundtrip(
                n in -1000i64..1000
            ) {
                let mut opts = Options::new();
                opts.parse_and_set("test_key", &n.to_string());
                if let Some(OptionValue::Number(got)) = opts.get("test_key") {
                    prop_assert_eq!(*got, n);
                } else {
                    // parse_and_set auto-detects integers, so this should always be Number
                    prop_assert!(false, "expected OptionValue::Number but got {:?}", opts.get("test_key"));
                }
            }

            #[test]
            fn set_flag_roundtrip(
                key in "[a-z-]{1,30}",
                flag in proptest::bool::ANY,
            ) {
                let mut opts = Options::new();
                opts.set(&key, OptionValue::Flag(flag));
                let got = opts.get_flag(&key).unwrap();
                prop_assert_eq!(got, flag);
            }

            #[test]
            fn set_number_roundtrip(
                key in "[a-z-]{1,30}",
                num in proptest::num::i64::ANY,
            ) {
                let mut opts = Options::new();
                opts.set(&key, OptionValue::Number(num));
                let got = opts.get_number(&key).unwrap();
                prop_assert_eq!(got, num);
            }

            #[test]
            fn child_overrides_parent(
                key in "[a-z-]{1,30}",
                parent_val in "[a-zA-Z]{1,20}",
                child_val in "[a-zA-Z]{1,20}",
            ) {
                let mut parent = Options::new();
                parent.set(&key, OptionValue::String(parent_val));
                let mut child = Options::with_parent(parent);
                child.set(&key, OptionValue::String(child_val.clone()));
                let got = child.get(&key);
                prop_assert_eq!(got, Some(&OptionValue::String(child_val)));
            }

            #[test]
            fn is_local_after_set(
                key in "[a-z-]{1,30}",
                value in "[a-zA-Z0-9]{1,50}",
            ) {
                let mut opts = Options::new();
                prop_assert!(!opts.is_local(&key));
                opts.set(&key, OptionValue::String(value));
                prop_assert!(opts.is_local(&key));
            }
        }
    }

    #[test]
    fn session_defaults_alert_options() {
        let opts = default_session_options();
        assert_eq!(opts.get_string("bell-action").unwrap(), "any");
        assert_eq!(opts.get_string("activity-action").unwrap(), "other");
        assert_eq!(opts.get_string("silence-action").unwrap(), "none");
        assert_eq!(opts.get_string("key-table").unwrap(), "root");
    }

    #[test]
    fn session_defaults_lock_options() {
        let opts = default_session_options();
        assert_eq!(opts.get_number("lock-after-time").unwrap(), 0);
        assert_eq!(opts.get_string("lock-command").unwrap(), "lock -np");
    }

    #[test]
    fn session_defaults_word_separators() {
        let opts = default_session_options();
        assert_eq!(opts.get_string("word-separators").unwrap(), " ");
    }

    #[test]
    fn window_defaults_pane_border_status() {
        let opts = default_window_options();
        assert_eq!(opts.get_string("pane-border-status").unwrap(), "off");
        assert!(opts.get_string("pane-border-format").unwrap().contains("pane_index"));
    }

    #[test]
    fn window_defaults_copy_mode_mark_style() {
        let opts = default_window_options();
        assert_eq!(opts.get_string("copy-mode-mark-style").unwrap(), "bg=red,fg=black");
    }
}
