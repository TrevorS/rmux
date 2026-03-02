//! Key binding tables and prefix mode handling.
//!
//! Implements tmux-compatible key bindings with a prefix key (default: Ctrl-b).

use rmux_core::key::*;
use rmux_terminal::keys::parse_key;
use std::collections::HashMap;

/// Action to take for a key binding.
pub enum KeyAction {
    /// Send raw bytes to the active pane's PTY.
    SendToPane(Vec<u8>),
    /// Execute a command.
    Command(Vec<String>),
}

/// Key binding tables and prefix mode state.
pub struct KeyBindings {
    /// Prefix key (default: Ctrl-b).
    prefix: KeyCode,
    /// Whether we're waiting for a key after the prefix.
    in_prefix: bool,
    /// Key -> command bindings for prefix mode.
    bindings: HashMap<KeyCode, Vec<String>>,
    /// Key -> command bindings for root table (no prefix needed).
    root_bindings: HashMap<KeyCode, Vec<String>>,
}

impl KeyBindings {
    /// Create default key bindings matching tmux.
    pub fn default_bindings() -> Self {
        let prefix = keyc_build(b'b'.into(), KeyModifiers::CTRL);

        let mut bindings: HashMap<KeyCode, Vec<String>> = HashMap::new();

        // Detach
        bindings.insert(b'd' as KeyCode, vec!["detach-client".into()]);

        // Window management
        bindings.insert(b'c' as KeyCode, vec!["new-window".into()]);
        bindings.insert(b'n' as KeyCode, vec!["next-window".into()]);
        bindings.insert(b'p' as KeyCode, vec!["previous-window".into()]);
        bindings.insert(b'l' as KeyCode, vec!["last-window".into()]);
        bindings.insert(b'&' as KeyCode, vec!["kill-window".into()]);

        // Pane splitting
        bindings.insert(b'"' as KeyCode, vec!["split-window".into()]);
        bindings.insert(b'%' as KeyCode, vec!["split-window".into(), "-h".into()]);

        // Pane navigation
        bindings.insert(b'o' as KeyCode, vec!["select-pane".into(), "-t".into(), "+".into()]);
        bindings.insert(b'x' as KeyCode, vec!["kill-pane".into()]);

        // Arrow key pane navigation
        bindings.insert(KEYC_UP, vec!["select-pane".into(), "-U".into()]);
        bindings.insert(KEYC_DOWN, vec!["select-pane".into(), "-D".into()]);
        bindings.insert(KEYC_LEFT, vec!["select-pane".into(), "-L".into()]);
        bindings.insert(KEYC_RIGHT, vec!["select-pane".into(), "-R".into()]);

        // Window selection by number (0-9)
        for i in 0u8..=9 {
            bindings.insert(
                (b'0' + i) as KeyCode,
                vec!["select-window".into(), "-t".into(), i.to_string()],
            );
        }

        // Command prompt
        bindings.insert(b':' as KeyCode, vec!["command-prompt".into()]);

        Self { prefix, in_prefix: false, bindings, root_bindings: HashMap::new() }
    }

    /// Add a key binding to the given table.
    pub fn add_binding(&mut self, table: &str, key: KeyCode, argv: Vec<String>) {
        match table {
            "root" => { self.root_bindings.insert(key, argv); }
            _ => { self.bindings.insert(key, argv); }
        }
    }

    /// Remove a key binding from the given table.
    pub fn remove_binding(&mut self, table: &str, key: KeyCode) -> bool {
        match table {
            "root" => self.root_bindings.remove(&key).is_some(),
            _ => self.bindings.remove(&key).is_some(),
        }
    }

    /// Process input bytes and return the action to take.
    ///
    /// Returns `Some(action)` if the input was handled,
    /// `None` if the input should be passed through to the pane.
    pub fn process_input(&mut self, data: &[u8]) -> Option<KeyAction> {
        // Parse the input into a key code
        let Some((key, _consumed)) = parse_key(data) else {
            if self.in_prefix {
                self.in_prefix = false;
                return None;
            }
            return None;
        };

        if self.in_prefix {
            self.in_prefix = false;

            // Check if this key has a binding
            let base = keyc_base(key);
            if let Some(argv) = self.bindings.get(&base) {
                return Some(KeyAction::Command(argv.clone()));
            }

            // If the key after prefix is the prefix key itself, send the prefix key to the pane
            if key == self.prefix {
                // Send Ctrl-b to the pane
                return Some(KeyAction::SendToPane(vec![0x02]));
            }

            // Unknown binding, ignore
            return None;
        }

        // Check root table bindings (no prefix needed)
        let base = keyc_base(key);
        if let Some(argv) = self.root_bindings.get(&base).or_else(|| self.root_bindings.get(&key)) {
            return Some(KeyAction::Command(argv.clone()));
        }

        // Check if this is the prefix key
        if key == self.prefix {
            self.in_prefix = true;
            return Some(KeyAction::SendToPane(Vec::new())); // Consume the prefix key
        }

        // Not handled - pass through to pane
        None
    }

    /// List all key bindings as human-readable strings.
    pub fn list_bindings(&self) -> Vec<String> {
        let mut result: Vec<String> = self
            .bindings
            .iter()
            .map(|(&key, argv)| {
                let key_name = key_to_string(key);
                let cmd = argv.join(" ");
                format!("bind-key -T prefix {key_name} {cmd}")
            })
            .collect();

        for (&key, argv) in &self.root_bindings {
            let key_name = key_to_string(key);
            let cmd = argv.join(" ");
            result.push(format!("bind-key -T root {key_name} {cmd}"));
        }

        result.sort();
        result
    }
}

/// Convert a key name string to a KeyCode.
///
/// This is the reverse of `key_to_string`. Used by bind-key/unbind-key.
pub fn string_to_key(name: &str) -> Option<KeyCode> {
    // Check for modifier prefixes
    let (mods, base_name) = if let Some(rest) = name.strip_prefix("C-") {
        (KeyModifiers::CTRL, rest)
    } else if let Some(rest) = name.strip_prefix("M-") {
        (KeyModifiers::META, rest)
    } else if let Some(rest) = name.strip_prefix("S-") {
        (KeyModifiers::SHIFT, rest)
    } else {
        (KeyModifiers::empty(), name)
    };

    let base = match base_name {
        "Up" => KEYC_UP,
        "Down" => KEYC_DOWN,
        "Left" => KEYC_LEFT,
        "Right" => KEYC_RIGHT,
        "Home" => KEYC_HOME,
        "End" => KEYC_END,
        "IC" | "Insert" => KEYC_INSERT,
        "DC" | "Delete" => KEYC_DELETE,
        "PPage" | "PageUp" => KEYC_PPAGE,
        "NPage" | "PageDown" => KEYC_NPAGE,
        "BSpace" => KEYC_BACKSPACE,
        "Tab" => KEYC_TAB,
        "Enter" | "CR" => KEYC_RETURN,
        "Escape" | "Esc" => KEYC_ESCAPE,
        "Space" => KEYC_SPACE,
        s if s.starts_with('F') && s.len() > 1 => {
            if let Ok(n) = s[1..].parse::<u64>() {
                if (1..=12).contains(&n) {
                    KEYC_F1 + n - 1
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        s if s.len() == 1 => {
            let ch = s.as_bytes()[0];
            ch as KeyCode
        }
        _ => return None,
    };

    if mods.is_empty() {
        Some(base)
    } else {
        Some(keyc_build(base, mods))
    }
}

/// Convert a key code to a human-readable string.
fn key_to_string(key: KeyCode) -> String {
    let base = keyc_base(key);
    let mods = keyc_modifiers(key);

    let mut pfx = String::new();
    if mods.contains(KeyModifiers::CTRL) {
        pfx.push_str("C-");
    }
    if mods.contains(KeyModifiers::META) {
        pfx.push_str("M-");
    }
    if mods.contains(KeyModifiers::SHIFT) {
        pfx.push_str("S-");
    }

    let name = match base {
        KEYC_UP => "Up".to_string(),
        KEYC_DOWN => "Down".to_string(),
        KEYC_LEFT => "Left".to_string(),
        KEYC_RIGHT => "Right".to_string(),
        KEYC_HOME => "Home".to_string(),
        KEYC_END => "End".to_string(),
        KEYC_INSERT => "IC".to_string(),
        KEYC_PPAGE => "PPage".to_string(),
        KEYC_NPAGE => "NPage".to_string(),
        KEYC_BACKSPACE => "BSpace".to_string(),
        KEYC_TAB => "Tab".to_string(),
        KEYC_RETURN => "Enter".to_string(),
        KEYC_ESCAPE => "Escape".to_string(),
        KEYC_SPACE => "Space".to_string(),
        KEYC_DELETE => "DC".to_string(),
        b if (KEYC_F1..=KEYC_F12).contains(&b) => format!("F{}", b - KEYC_F1 + 1),
        b if b < 128 => {
            let ch = b as u8 as char;
            if ch.is_ascii_graphic() || ch == ' ' { ch.to_string() } else { format!("0x{b:02x}") }
        }
        other => format!("0x{other:x}"),
    };

    format!("{pfx}{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_key_detection() {
        let mut kb = KeyBindings::default_bindings();
        // Ctrl-b is 0x02
        let result = kb.process_input(b"\x02");
        assert!(result.is_some());
        assert!(kb.in_prefix);
    }

    #[test]
    fn prefix_d_detaches() {
        let mut kb = KeyBindings::default_bindings();
        // Send prefix
        kb.process_input(b"\x02");
        assert!(kb.in_prefix);

        // Send 'd'
        let result = kb.process_input(b"d");
        assert!(!kb.in_prefix);
        match result {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["detach-client"]);
            }
            _ => panic!("expected Command"),
        }
    }

    #[test]
    fn prefix_percent_splits_horizontal() {
        let mut kb = KeyBindings::default_bindings();
        kb.process_input(b"\x02");
        let result = kb.process_input(b"%");
        match result {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["split-window", "-h"]);
            }
            _ => panic!("expected Command"),
        }
    }

    #[test]
    fn prefix_quote_splits_vertical() {
        let mut kb = KeyBindings::default_bindings();
        kb.process_input(b"\x02");
        let result = kb.process_input(b"\"");
        match result {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["split-window"]);
            }
            _ => panic!("expected Command"),
        }
    }

    #[test]
    fn prefix_n_next_window() {
        let mut kb = KeyBindings::default_bindings();
        kb.process_input(b"\x02");
        let result = kb.process_input(b"n");
        match result {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["next-window"]);
            }
            _ => panic!("expected Command"),
        }
    }

    #[test]
    fn prefix_0_selects_window_0() {
        let mut kb = KeyBindings::default_bindings();
        kb.process_input(b"\x02");
        let result = kb.process_input(b"0");
        match result {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["select-window", "-t", "0"]);
            }
            _ => panic!("expected Command"),
        }
    }

    #[test]
    fn normal_input_passes_through() {
        let mut kb = KeyBindings::default_bindings();
        let result = kb.process_input(b"a");
        assert!(result.is_none());
    }

    #[test]
    fn list_bindings_returns_sorted() {
        let kb = KeyBindings::default_bindings();
        let bindings = kb.list_bindings();
        assert!(!bindings.is_empty());
        // Should be sorted
        let mut sorted = bindings.clone();
        sorted.sort();
        assert_eq!(bindings, sorted);
    }
}
