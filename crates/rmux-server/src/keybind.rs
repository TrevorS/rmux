//! Key binding tables and prefix mode handling.
//!
//! Implements tmux-compatible key bindings with a prefix key (default: Ctrl-b)
//! and named key tables (prefix, root, copy-mode-vi, copy-mode-emacs).

use rmux_core::key::*;
use rmux_terminal::keys::parse_key;
use std::collections::HashMap;

/// Action to take for a key binding.
#[derive(Debug)]
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
    /// Named key tables: "prefix", "root", "copy-mode-vi", "copy-mode-emacs".
    tables: HashMap<String, HashMap<KeyCode, Vec<String>>>,
}

impl KeyBindings {
    /// Create default key bindings matching tmux.
    pub fn default_bindings() -> Self {
        let prefix = keyc_build(b'b'.into(), KeyModifiers::CTRL);

        let mut prefix_table: HashMap<KeyCode, Vec<String>> = HashMap::new();

        // Detach
        prefix_table.insert(b'd' as KeyCode, vec!["detach-client".into()]);

        // Window management
        prefix_table.insert(b'c' as KeyCode, vec!["new-window".into()]);
        prefix_table.insert(b'n' as KeyCode, vec!["next-window".into()]);
        prefix_table.insert(b'p' as KeyCode, vec!["previous-window".into()]);
        prefix_table.insert(b'l' as KeyCode, vec!["last-window".into()]);
        prefix_table.insert(b'&' as KeyCode, vec!["kill-window".into()]);

        // Pane splitting
        prefix_table.insert(b'"' as KeyCode, vec!["split-window".into()]);
        prefix_table.insert(b'%' as KeyCode, vec!["split-window".into(), "-h".into()]);

        // Pane navigation
        prefix_table.insert(b'o' as KeyCode, vec!["select-pane".into(), "-t".into(), "+".into()]);
        prefix_table.insert(b'x' as KeyCode, vec!["kill-pane".into()]);

        // Arrow key pane navigation
        prefix_table.insert(KEYC_UP, vec!["select-pane".into(), "-U".into()]);
        prefix_table.insert(KEYC_DOWN, vec!["select-pane".into(), "-D".into()]);
        prefix_table.insert(KEYC_LEFT, vec!["select-pane".into(), "-L".into()]);
        prefix_table.insert(KEYC_RIGHT, vec!["select-pane".into(), "-R".into()]);

        // Window selection by number (0-9)
        for i in 0u8..=9 {
            prefix_table.insert(
                (b'0' + i) as KeyCode,
                vec!["select-window".into(), "-t".into(), i.to_string()],
            );
        }

        // Command prompt
        prefix_table.insert(b':' as KeyCode, vec!["command-prompt".into()]);

        // Copy mode
        prefix_table.insert(b'[' as KeyCode, vec!["copy-mode".into()]);
        // Paste buffer
        prefix_table.insert(b']' as KeyCode, vec!["paste-buffer".into()]);

        // Send prefix (C-b C-b sends literal C-b to pane)
        prefix_table
            .insert(keyc_build(b'b'.into(), KeyModifiers::CTRL), vec!["send-prefix".into()]);

        // Layout cycling
        prefix_table.insert(KEYC_SPACE, vec!["next-layout".into()]);

        // Break pane out to its own window
        prefix_table.insert(b'!' as KeyCode, vec!["break-pane".into()]);

        // Last pane
        prefix_table.insert(b';' as KeyCode, vec!["last-pane".into()]);

        // Swap pane
        prefix_table.insert(b'{' as KeyCode, vec!["swap-pane".into(), "-U".into()]);
        prefix_table.insert(b'}' as KeyCode, vec!["swap-pane".into(), "-D".into()]);

        // Rename window
        prefix_table.insert(
            b',' as KeyCode,
            vec!["command-prompt".into(), "-I".into(), "#W".into(), "rename-window -- '%%'".into()],
        );

        // List keys
        prefix_table.insert(b'?' as KeyCode, vec!["list-keys".into()]);

        // Resize pane with Ctrl+arrows (by 1 cell)
        prefix_table.insert(
            keyc_build(KEYC_UP, KeyModifiers::CTRL),
            vec!["resize-pane".into(), "-U".into()],
        );
        prefix_table.insert(
            keyc_build(KEYC_DOWN, KeyModifiers::CTRL),
            vec!["resize-pane".into(), "-D".into()],
        );
        prefix_table.insert(
            keyc_build(KEYC_LEFT, KeyModifiers::CTRL),
            vec!["resize-pane".into(), "-L".into()],
        );
        prefix_table.insert(
            keyc_build(KEYC_RIGHT, KeyModifiers::CTRL),
            vec!["resize-pane".into(), "-R".into()],
        );

        // Resize pane with Meta+arrows (by 5 cells)
        prefix_table.insert(
            keyc_build(KEYC_UP, KeyModifiers::META),
            vec!["resize-pane".into(), "-U".into(), "5".into()],
        );
        prefix_table.insert(
            keyc_build(KEYC_DOWN, KeyModifiers::META),
            vec!["resize-pane".into(), "-D".into(), "5".into()],
        );
        prefix_table.insert(
            keyc_build(KEYC_LEFT, KeyModifiers::META),
            vec!["resize-pane".into(), "-L".into(), "5".into()],
        );
        prefix_table.insert(
            keyc_build(KEYC_RIGHT, KeyModifiers::META),
            vec!["resize-pane".into(), "-R".into(), "5".into()],
        );

        let mut tables = HashMap::new();
        tables.insert("prefix".to_string(), prefix_table);
        tables.insert("root".to_string(), HashMap::new());
        tables.insert("copy-mode-vi".to_string(), default_copy_mode_vi());
        tables.insert("copy-mode-emacs".to_string(), default_copy_mode_emacs());

        Self { prefix, in_prefix: false, tables }
    }

    /// Add a key binding to the given table.
    pub fn add_binding(&mut self, table: &str, key: KeyCode, argv: Vec<String>) {
        self.tables.entry(table.to_string()).or_default().insert(key, argv);
    }

    /// Remove a key binding from the given table.
    pub fn remove_binding(&mut self, table: &str, key: KeyCode) -> bool {
        self.tables.get_mut(table).is_some_and(|t| t.remove(&key).is_some())
    }

    /// Look up a binding in a specific table.
    ///
    /// Checks exact key (with modifiers) first, then falls back to base key.
    pub fn lookup_in_table(&self, table: &str, key: KeyCode) -> Option<&Vec<String>> {
        let t = self.tables.get(table)?;
        let base = keyc_base(key);
        t.get(&key).or_else(|| t.get(&base))
    }

    /// Process input bytes and return the action to take.
    ///
    /// Returns `(Some(action), consumed)` if the input was handled by the
    /// keybinding system, or `(None, consumed)` if the bytes should be passed
    /// through to the pane. The caller must advance by `consumed` bytes and
    /// call again for the remainder of the buffer.
    pub fn process_input(&mut self, data: &[u8]) -> (Option<KeyAction>, usize) {
        // Parse the input into a key code
        let Some((key, consumed)) = parse_key(data) else {
            if self.in_prefix {
                self.in_prefix = false;
            }
            // Can't parse — consume 1 byte to avoid infinite loop
            return (None, 1.min(data.len()));
        };

        if self.in_prefix {
            self.in_prefix = false;

            // Check if this key has a binding in the prefix table.
            // Try exact match (with modifiers) first, then base key.
            let base = keyc_base(key);
            if let Some(argv) =
                self.tables.get("prefix").and_then(|t| t.get(&key).or_else(|| t.get(&base)))
            {
                return (Some(KeyAction::Command(argv.clone())), consumed);
            }

            // Unknown binding, ignore
            return (None, consumed);
        }

        // Check root table bindings (no prefix needed)
        let base = keyc_base(key);
        if let Some(argv) =
            self.tables.get("root").and_then(|t| t.get(&base).or_else(|| t.get(&key)))
        {
            return (Some(KeyAction::Command(argv.clone())), consumed);
        }

        // Check if this is the prefix key
        if key == self.prefix {
            self.in_prefix = true;
            return (Some(KeyAction::SendToPane(Vec::new())), consumed); // Consume the prefix key
        }

        // Not handled - pass through to pane
        (None, consumed)
    }

    /// List all key bindings as human-readable strings.
    pub fn list_bindings(&self) -> Vec<String> {
        let mut result = Vec::new();

        for (table_name, table) in &self.tables {
            for (&key, argv) in table {
                let key_name = key_to_string(key);
                let cmd = argv.join(" ");
                result.push(format!("bind-key -T {table_name} {key_name} {cmd}"));
            }
        }

        result.sort();
        result
    }
}

/// Default copy-mode-vi key bindings.
fn default_copy_mode_vi() -> HashMap<KeyCode, Vec<String>> {
    let mut m = HashMap::new();

    // Navigation
    m.insert(b'h' as KeyCode, vec!["cursor-left".into()]);
    m.insert(b'j' as KeyCode, vec!["cursor-down".into()]);
    m.insert(b'k' as KeyCode, vec!["cursor-up".into()]);
    m.insert(b'l' as KeyCode, vec!["cursor-right".into()]);
    m.insert(KEYC_UP, vec!["cursor-up".into()]);
    m.insert(KEYC_DOWN, vec!["cursor-down".into()]);
    m.insert(KEYC_LEFT, vec!["cursor-left".into()]);
    m.insert(KEYC_RIGHT, vec!["cursor-right".into()]);

    // Page movement
    m.insert(KEYC_PPAGE, vec!["page-up".into()]);
    m.insert(KEYC_NPAGE, vec!["page-down".into()]);
    m.insert(keyc_build(b'b'.into(), KeyModifiers::CTRL), vec!["page-up".into()]);
    m.insert(keyc_build(b'f'.into(), KeyModifiers::CTRL), vec!["page-down".into()]);

    // Half page
    m.insert(keyc_build(b'u'.into(), KeyModifiers::CTRL), vec!["halfpage-up".into()]);
    m.insert(keyc_build(b'd'.into(), KeyModifiers::CTRL), vec!["halfpage-down".into()]);

    // Top/bottom
    m.insert(b'g' as KeyCode, vec!["history-top".into()]);
    m.insert(b'G' as KeyCode, vec!["history-bottom".into()]);

    // Line movement
    m.insert(b'0' as KeyCode, vec!["start-of-line".into()]);
    m.insert(b'$' as KeyCode, vec!["end-of-line".into()]);
    m.insert(b'^' as KeyCode, vec!["back-to-indentation".into()]);
    m.insert(KEYC_HOME, vec!["start-of-line".into()]);
    m.insert(KEYC_END, vec!["end-of-line".into()]);

    // Word movement
    m.insert(b'w' as KeyCode, vec!["next-word".into()]);
    m.insert(b'b' as KeyCode, vec!["previous-word".into()]);
    m.insert(b'e' as KeyCode, vec!["next-word-end".into()]);

    // Selection
    m.insert(b'v' as KeyCode, vec!["begin-selection".into()]);
    m.insert(KEYC_SPACE, vec!["begin-selection".into()]);
    m.insert(b'V' as KeyCode, vec!["select-line".into()]);
    m.insert(keyc_build(b'v'.into(), KeyModifiers::CTRL), vec!["rectangle-toggle".into()]);

    // Copy/exit
    m.insert(KEYC_RETURN, vec!["copy-selection-and-cancel".into()]);
    m.insert(b'y' as KeyCode, vec!["copy-selection-and-cancel".into()]);

    // Cancel
    m.insert(b'q' as KeyCode, vec!["cancel".into()]);
    m.insert(KEYC_ESCAPE, vec!["cancel".into()]);

    m
}

/// Default copy-mode-emacs key bindings.
fn default_copy_mode_emacs() -> HashMap<KeyCode, Vec<String>> {
    let mut m = HashMap::new();

    // Navigation
    m.insert(KEYC_UP, vec!["cursor-up".into()]);
    m.insert(KEYC_DOWN, vec!["cursor-down".into()]);
    m.insert(KEYC_LEFT, vec!["cursor-left".into()]);
    m.insert(KEYC_RIGHT, vec!["cursor-right".into()]);
    m.insert(keyc_build(b'p'.into(), KeyModifiers::CTRL), vec!["cursor-up".into()]);
    m.insert(keyc_build(b'n'.into(), KeyModifiers::CTRL), vec!["cursor-down".into()]);
    m.insert(keyc_build(b'b'.into(), KeyModifiers::CTRL), vec!["cursor-left".into()]);
    m.insert(keyc_build(b'f'.into(), KeyModifiers::CTRL), vec!["cursor-right".into()]);

    // Page movement
    m.insert(KEYC_PPAGE, vec!["page-up".into()]);
    m.insert(KEYC_NPAGE, vec!["page-down".into()]);
    m.insert(keyc_build(b'v'.into(), KeyModifiers::META), vec!["page-up".into()]);
    m.insert(keyc_build(b'v'.into(), KeyModifiers::CTRL), vec!["page-down".into()]);

    // Line movement
    m.insert(keyc_build(b'a'.into(), KeyModifiers::CTRL), vec!["start-of-line".into()]);
    m.insert(keyc_build(b'e'.into(), KeyModifiers::CTRL), vec!["end-of-line".into()]);

    // Word movement
    m.insert(keyc_build(b'f'.into(), KeyModifiers::META), vec!["next-word".into()]);
    m.insert(keyc_build(b'b'.into(), KeyModifiers::META), vec!["previous-word".into()]);

    // Selection
    m.insert(keyc_build(KEYC_SPACE, KeyModifiers::CTRL), vec!["begin-selection".into()]);

    // Copy
    m.insert(keyc_build(b'w'.into(), KeyModifiers::META), vec!["copy-selection-and-cancel".into()]);

    // Cancel
    m.insert(keyc_build(b'g'.into(), KeyModifiers::CTRL), vec!["cancel".into()]);
    m.insert(KEYC_ESCAPE, vec!["cancel".into()]);

    m
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

    if mods.is_empty() { Some(base) } else { Some(keyc_build(base, mods)) }
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
        let (result, consumed) = kb.process_input(b"\x02");
        assert!(result.is_some());
        assert_eq!(consumed, 1);
        assert!(kb.in_prefix);
    }

    #[test]
    fn prefix_d_detaches() {
        let mut kb = KeyBindings::default_bindings();
        // Send prefix
        let _ = kb.process_input(b"\x02");
        assert!(kb.in_prefix);

        // Send 'd'
        let (result, _) = kb.process_input(b"d");
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
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"%");
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
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"\"");
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
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"n");
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
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"0");
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
        let (result, consumed) = kb.process_input(b"a");
        assert!(result.is_none());
        assert_eq!(consumed, 1);
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

    #[test]
    fn copy_mode_vi_bindings_exist() {
        let kb = KeyBindings::default_bindings();
        // Check key lookups for copy-mode-vi
        let action = kb.lookup_in_table("copy-mode-vi", b'h' as KeyCode);
        assert_eq!(action, Some(&vec!["cursor-left".to_string()]));

        let action = kb.lookup_in_table("copy-mode-vi", b'q' as KeyCode);
        assert_eq!(action, Some(&vec!["cancel".to_string()]));

        let action = kb.lookup_in_table("copy-mode-vi", b'y' as KeyCode);
        assert_eq!(action, Some(&vec!["copy-selection-and-cancel".to_string()]));
    }

    #[test]
    fn copy_mode_emacs_bindings_exist() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("copy-mode-emacs", KEYC_ESCAPE);
        assert_eq!(action, Some(&vec!["cancel".to_string()]));
    }

    #[test]
    fn prefix_bracket_enters_copy_mode() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"[");
        match result {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["copy-mode"]);
            }
            _ => panic!("expected Command for copy-mode"),
        }
    }

    #[test]
    fn prefix_close_bracket_pastes() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"]");
        match result {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["paste-buffer"]);
            }
            _ => panic!("expected Command for paste-buffer"),
        }
    }

    #[test]
    fn add_custom_binding() {
        let mut kb = KeyBindings::default_bindings();
        kb.add_binding("prefix", b'z' as KeyCode, vec!["custom-command".into()]);
        let action = kb.lookup_in_table("prefix", b'z' as KeyCode);
        assert_eq!(action, Some(&vec!["custom-command".to_string()]));
    }

    #[test]
    fn remove_binding_returns_true() {
        let mut kb = KeyBindings::default_bindings();
        // 'd' is bound to detach-client in prefix table
        let removed = kb.remove_binding("prefix", b'd' as KeyCode);
        assert!(removed);
        // Verify it's gone
        let action = kb.lookup_in_table("prefix", b'd' as KeyCode);
        assert!(action.is_none());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut kb = KeyBindings::default_bindings();
        let removed = kb.remove_binding("prefix", b'Z' as KeyCode);
        assert!(!removed);
    }

    #[test]
    fn overwrite_binding() {
        let mut kb = KeyBindings::default_bindings();
        // 'd' is initially detach-client
        kb.add_binding("prefix", b'd' as KeyCode, vec!["new-command".into()]);
        let action = kb.lookup_in_table("prefix", b'd' as KeyCode);
        assert_eq!(action, Some(&vec!["new-command".to_string()]));
    }

    #[test]
    fn string_to_key_basic() {
        assert_eq!(string_to_key("Enter"), Some(KEYC_RETURN));
        assert_eq!(string_to_key("Space"), Some(KEYC_SPACE));
        let ctrl_a = string_to_key("C-a");
        assert_eq!(ctrl_a, Some(keyc_build(b'a' as KeyCode, KeyModifiers::CTRL)));
    }

    #[test]
    fn key_to_string_roundtrip() {
        let names = ["Enter", "Space", "Up", "Down", "Left", "Right", "Escape", "Tab", "BSpace"];
        for name in names {
            let key = string_to_key(name).unwrap();
            let result = key_to_string(key);
            assert_eq!(result, name, "roundtrip failed for {name}");
        }
    }

    #[test]
    fn window_selection_0_through_9() {
        let kb = KeyBindings::default_bindings();
        for i in 0u8..=9 {
            let key = (b'0' + i) as KeyCode;
            let action = kb.lookup_in_table("prefix", key);
            assert!(action.is_some(), "expected binding for window {i}");
            let argv = action.unwrap();
            assert_eq!(argv, &vec!["select-window".to_string(), "-t".to_string(), i.to_string()]);
        }
    }

    #[test]
    fn copy_mode_vi_all_nav_keys() {
        let kb = KeyBindings::default_bindings();
        for key_char in [b'h', b'j', b'k', b'l'] {
            let action = kb.lookup_in_table("copy-mode-vi", key_char as KeyCode);
            assert!(action.is_some(), "expected copy-mode-vi binding for '{}'", key_char as char);
        }
    }

    #[test]
    fn prefix_space_next_layout() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        // Space is a special key, need to send it as raw
        let action = kb.lookup_in_table("prefix", KEYC_SPACE);
        assert_eq!(action, Some(&vec!["next-layout".to_string()]));
    }

    #[test]
    fn prefix_bang_break_pane() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("prefix", b'!' as KeyCode);
        assert_eq!(action, Some(&vec!["break-pane".to_string()]));
    }

    #[test]
    fn prefix_semicolon_last_pane() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("prefix", b';' as KeyCode);
        assert_eq!(action, Some(&vec!["last-pane".to_string()]));
    }

    #[test]
    fn prefix_braces_swap_pane() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("prefix", b'{' as KeyCode);
        assert_eq!(action, Some(&vec!["swap-pane".to_string(), "-U".to_string()]));
        let action = kb.lookup_in_table("prefix", b'}' as KeyCode);
        assert_eq!(action, Some(&vec!["swap-pane".to_string(), "-D".to_string()]));
    }

    #[test]
    fn prefix_question_list_keys() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("prefix", b'?' as KeyCode);
        assert_eq!(action, Some(&vec!["list-keys".to_string()]));
    }

    #[test]
    fn prefix_comma_rename_window() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("prefix", b',' as KeyCode);
        assert!(action.is_some());
        assert_eq!(action.unwrap()[0], "command-prompt");
    }

    #[test]
    fn prefix_ctrl_b_send_prefix() {
        let kb = KeyBindings::default_bindings();
        let ctrl_b = keyc_build(b'b'.into(), KeyModifiers::CTRL);
        let action = kb.lookup_in_table("prefix", ctrl_b);
        assert_eq!(action, Some(&vec!["send-prefix".to_string()]));
    }

    #[test]
    fn prefix_ctrl_arrows_resize_by_1() {
        let kb = KeyBindings::default_bindings();
        for (arrow, dir) in
            [(KEYC_UP, "-U"), (KEYC_DOWN, "-D"), (KEYC_LEFT, "-L"), (KEYC_RIGHT, "-R")]
        {
            let key = keyc_build(arrow, KeyModifiers::CTRL);
            let action = kb.lookup_in_table("prefix", key);
            assert_eq!(
                action,
                Some(&vec!["resize-pane".to_string(), dir.to_string()]),
                "C-{dir} should resize by 1"
            );
        }
    }

    #[test]
    fn prefix_meta_arrows_resize_by_5() {
        let kb = KeyBindings::default_bindings();
        for (arrow, dir) in
            [(KEYC_UP, "-U"), (KEYC_DOWN, "-D"), (KEYC_LEFT, "-L"), (KEYC_RIGHT, "-R")]
        {
            let key = keyc_build(arrow, KeyModifiers::META);
            let action = kb.lookup_in_table("prefix", key);
            assert_eq!(
                action,
                Some(&vec!["resize-pane".to_string(), dir.to_string(), "5".to_string()]),
                "M-{dir} should resize by 5"
            );
        }
    }

    #[test]
    fn ctrl_arrow_not_confused_with_plain_arrow() {
        let kb = KeyBindings::default_bindings();
        // Plain Up → select-pane -U
        let plain = kb.lookup_in_table("prefix", KEYC_UP);
        assert_eq!(plain, Some(&vec!["select-pane".to_string(), "-U".to_string()]));
        // Ctrl-Up → resize-pane -U
        let ctrl = kb.lookup_in_table("prefix", keyc_build(KEYC_UP, KeyModifiers::CTRL));
        assert_eq!(ctrl, Some(&vec!["resize-pane".to_string(), "-U".to_string()]));
    }

    #[test]
    fn lookup_in_nonexistent_table() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("nonexistent-table", b'a' as KeyCode);
        assert!(action.is_none());
    }

    #[test]
    fn prefix_and_command_in_single_buffer() {
        let mut kb = KeyBindings::default_bindings();

        // Simulate \x02d arriving in one read (prefix + detach)
        let data = b"\x02d";

        // First call: consumes the prefix byte
        let (action1, consumed1) = kb.process_input(data);
        assert!(matches!(action1, Some(KeyAction::SendToPane(ref b)) if b.is_empty()));
        assert_eq!(consumed1, 1);
        assert!(kb.in_prefix);

        // Second call on remaining bytes: processes 'd' as prefix command
        let (action2, consumed2) = kb.process_input(&data[consumed1..]);
        assert_eq!(consumed2, 1);
        match action2 {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["detach-client"]);
            }
            _ => panic!("expected Command(detach-client), got {action2:?}"),
        }
    }
}
