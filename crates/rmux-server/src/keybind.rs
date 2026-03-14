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

/// A single key binding entry.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    /// The command argv to execute.
    pub argv: Vec<String>,
    /// Whether this binding is repeatable (-r flag).
    pub repeatable: bool,
}

/// Key binding tables and prefix mode state.
pub struct KeyBindings {
    /// Prefix key (default: Ctrl-b).
    prefix: KeyCode,
    /// Whether we're waiting for a key after the prefix.
    in_prefix: bool,
    /// Instant when prefix mode should expire (for repeat-time).
    prefix_expiry: Option<std::time::Instant>,
    /// Named key tables: "prefix", "root", "copy-mode-vi", "copy-mode-emacs".
    tables: HashMap<String, HashMap<KeyCode, KeyBinding>>,
}

impl KeyBindings {
    /// Create default key bindings matching tmux.
    pub fn default_bindings() -> Self {
        let prefix = keyc_build(b'b'.into(), KeyModifiers::CTRL);
        let prefix_table = default_prefix_table();

        let mut tables = HashMap::new();
        tables.insert("prefix".to_string(), prefix_table);
        tables.insert("root".to_string(), HashMap::new());
        tables.insert("copy-mode-vi".to_string(), default_copy_mode_vi());
        tables.insert("copy-mode-emacs".to_string(), default_copy_mode_emacs());

        Self { prefix, in_prefix: false, prefix_expiry: None, tables }
    }

    /// Add a key binding to the given table.
    pub fn add_binding(&mut self, table: &str, key: KeyCode, argv: Vec<String>) {
        self.add_binding_with_repeat(table, key, argv, false);
    }

    /// Add a key binding with repeat flag.
    pub fn add_binding_with_repeat(
        &mut self,
        table: &str,
        key: KeyCode,
        argv: Vec<String>,
        repeatable: bool,
    ) {
        self.tables
            .entry(table.to_string())
            .or_default()
            .insert(key, KeyBinding { argv, repeatable });
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
        t.get(&key).or_else(|| t.get(&base)).map(|b| &b.argv)
    }

    /// Look up a binding in a specific table, returning the full binding.
    fn lookup_binding(&self, table: &str, key: KeyCode) -> Option<&KeyBinding> {
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
                self.prefix_expiry = None;
            }
            // Can't parse — consume 1 byte to avoid infinite loop
            return (None, 1.min(data.len()));
        };

        if self.in_prefix {
            // Check if prefix has expired (repeat-time)
            if let Some(expiry) = self.prefix_expiry {
                if std::time::Instant::now() >= expiry {
                    self.in_prefix = false;
                    self.prefix_expiry = None;
                    // Fall through to normal processing
                }
            }
        }

        if self.in_prefix {
            // Check if this key has a binding in the prefix table.
            let binding = self.lookup_binding("prefix", key).cloned();
            if let Some(binding) = binding {
                // If repeatable, stay in prefix mode with a timeout
                if binding.repeatable {
                    // Keep in_prefix = true, expiry will be set by caller
                    // via set_repeat_timeout()
                } else {
                    self.in_prefix = false;
                    self.prefix_expiry = None;
                }
                return (Some(KeyAction::Command(binding.argv)), consumed);
            }

            // Unknown binding, exit prefix mode
            self.in_prefix = false;
            self.prefix_expiry = None;
            return (None, consumed);
        }

        // Check root table bindings (no prefix needed)
        if let Some(binding) = self.lookup_binding("root", key).cloned() {
            return (Some(KeyAction::Command(binding.argv)), consumed);
        }

        // Check if this is the prefix key
        if key == self.prefix {
            self.in_prefix = true;
            self.prefix_expiry = None;
            return (Some(KeyAction::SendToPane(Vec::new())), consumed);
        }

        // Not handled - pass through to pane
        (None, consumed)
    }

    /// Update the prefix key.
    pub fn set_prefix(&mut self, key: KeyCode) {
        self.prefix = key;
    }

    /// Whether we're currently in prefix mode.
    pub fn in_prefix(&self) -> bool {
        self.in_prefix
    }

    /// Set the repeat timeout (called after dispatching a repeatable binding).
    pub fn set_repeat_timeout(&mut self, repeat_time_ms: u64) {
        if repeat_time_ms > 0 {
            self.prefix_expiry =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(repeat_time_ms));
        }
    }

    /// List all key bindings as human-readable strings.
    pub fn list_bindings(&self) -> Vec<String> {
        let mut result = Vec::new();

        for (table_name, table) in &self.tables {
            for (&key, binding) in table {
                let key_name = key_to_string(key);
                let cmd = binding.argv.join(" ");
                let repeat = if binding.repeatable { " -r" } else { "" };
                result.push(format!("bind-key{repeat} -T {table_name} {key_name} {cmd}"));
            }
        }

        result.sort();
        result
    }
}

/// Helper to create a non-repeatable key binding.
fn bind(argv: Vec<String>) -> KeyBinding {
    KeyBinding { argv, repeatable: false }
}

/// Helper to create a repeatable key binding (-r).
fn bind_r(argv: Vec<String>) -> KeyBinding {
    KeyBinding { argv, repeatable: true }
}

/// Default prefix key bindings matching tmux.
fn default_prefix_table() -> HashMap<KeyCode, KeyBinding> {
    let mut t: HashMap<KeyCode, KeyBinding> = HashMap::new();

    // Detach
    t.insert(b'd' as KeyCode, bind(vec!["detach-client".into()]));

    // Window management
    t.insert(b'c' as KeyCode, bind(vec!["new-window".into()]));
    t.insert(b'n' as KeyCode, bind(vec!["next-window".into()]));
    t.insert(b'p' as KeyCode, bind(vec!["previous-window".into()]));
    t.insert(b'l' as KeyCode, bind(vec!["last-window".into()]));
    t.insert(b'&' as KeyCode, bind(vec!["kill-window".into()]));

    // Pane splitting
    t.insert(b'"' as KeyCode, bind(vec!["split-window".into()]));
    t.insert(b'%' as KeyCode, bind(vec!["split-window".into(), "-h".into()]));

    // Pane navigation
    t.insert(b'o' as KeyCode, bind(vec!["select-pane".into(), "-t".into(), "+".into()]));
    t.insert(b'x' as KeyCode, bind(vec!["kill-pane".into()]));

    // Arrow key pane navigation
    t.insert(KEYC_UP, bind(vec!["select-pane".into(), "-U".into()]));
    t.insert(KEYC_DOWN, bind(vec!["select-pane".into(), "-D".into()]));
    t.insert(KEYC_LEFT, bind(vec!["select-pane".into(), "-L".into()]));
    t.insert(KEYC_RIGHT, bind(vec!["select-pane".into(), "-R".into()]));

    // Window selection by number (0-9)
    for i in 0u8..=9 {
        t.insert(
            (b'0' + i) as KeyCode,
            bind(vec!["select-window".into(), "-t".into(), i.to_string()]),
        );
    }

    // Command prompt & copy/paste
    t.insert(b':' as KeyCode, bind(vec!["command-prompt".into()]));
    t.insert(b'[' as KeyCode, bind(vec!["copy-mode".into()]));
    t.insert(b']' as KeyCode, bind(vec!["paste-buffer".into()]));
    t.insert(keyc_build(b'b'.into(), KeyModifiers::CTRL), bind(vec!["send-prefix".into()]));
    t.insert(KEYC_SPACE, bind(vec!["next-layout".into()]));
    t.insert(b'!' as KeyCode, bind(vec!["break-pane".into()]));
    t.insert(b';' as KeyCode, bind(vec!["last-pane".into()]));
    t.insert(b'{' as KeyCode, bind(vec!["swap-pane".into(), "-U".into()]));
    t.insert(b'}' as KeyCode, bind(vec!["swap-pane".into(), "-D".into()]));

    // Prompts
    t.insert(
        b',' as KeyCode,
        bind(vec![
            "command-prompt".into(),
            "-I".into(),
            "#W".into(),
            "rename-window -- '%%'".into(),
        ]),
    );
    t.insert(
        b'$' as KeyCode,
        bind(vec![
            "command-prompt".into(),
            "-I".into(),
            "#S".into(),
            "rename-session -- '%%'".into(),
        ]),
    );
    t.insert(
        b'\'' as KeyCode,
        bind(vec![
            "command-prompt".into(),
            "-p".into(),
            "index".into(),
            "select-window -t '%%'".into(),
        ]),
    );
    t.insert(b'.' as KeyCode, bind(vec!["command-prompt".into(), "move-window -t '%%'".into()]));
    t.insert(b'f' as KeyCode, bind(vec!["command-prompt".into(), "find-window -- '%%'".into()]));

    // Info & display
    t.insert(b'?' as KeyCode, bind(vec!["list-keys".into()]));
    t.insert(b'w' as KeyCode, bind(vec!["choose-tree".into()]));
    t.insert(b's' as KeyCode, bind(vec!["choose-tree".into()]));
    t.insert(b'=' as KeyCode, bind(vec!["choose-buffer".into()]));
    t.insert(b'D' as KeyCode, bind(vec!["choose-client".into()]));
    t.insert(b'~' as KeyCode, bind(vec!["show-messages".into()]));
    t.insert(b'#' as KeyCode, bind(vec!["list-buffers".into()]));
    t.insert(b't' as KeyCode, bind(vec!["clock-mode".into()]));
    t.insert(b'q' as KeyCode, bind(vec!["display-panes".into()]));
    t.insert(b'i' as KeyCode, bind(vec!["display-message".into()]));
    t.insert(b'r' as KeyCode, bind(vec!["refresh-client".into()]));
    t.insert(b'z' as KeyCode, bind(vec!["resize-pane".into(), "-Z".into()]));

    // Session switching
    t.insert(b'(' as KeyCode, bind(vec!["switch-client".into(), "-p".into()]));
    t.insert(b')' as KeyCode, bind(vec!["switch-client".into(), "-n".into()]));

    // Rotate window
    t.insert(keyc_build(b'o'.into(), KeyModifiers::CTRL), bind(vec!["rotate-window".into()]));
    t.insert(
        keyc_build(b'o'.into(), KeyModifiers::META),
        bind(vec!["rotate-window".into(), "-D".into()]),
    );

    // Page up enters copy mode
    t.insert(KEYC_PPAGE, bind(vec!["copy-mode".into(), "-u".into()]));

    default_prefix_resize(&mut t);
    default_prefix_layouts(&mut t);

    t
}

/// Resize bindings for the prefix table.
fn default_prefix_resize(t: &mut HashMap<KeyCode, KeyBinding>) {
    for (arrow, dir) in [(KEYC_UP, "-U"), (KEYC_DOWN, "-D"), (KEYC_LEFT, "-L"), (KEYC_RIGHT, "-R")]
    {
        t.insert(
            keyc_build(arrow, KeyModifiers::CTRL),
            bind_r(vec!["resize-pane".into(), dir.into()]),
        );
        t.insert(
            keyc_build(arrow, KeyModifiers::META),
            bind_r(vec!["resize-pane".into(), dir.into(), "5".into()]),
        );
    }
}

/// Layout selection bindings (M-1..5) for the prefix table.
fn default_prefix_layouts(t: &mut HashMap<KeyCode, KeyBinding>) {
    let layouts = [
        (b'1', "even-horizontal"),
        (b'2', "even-vertical"),
        (b'3', "main-horizontal"),
        (b'4', "main-vertical"),
        (b'5', "tiled"),
    ];
    for (digit, name) in layouts {
        t.insert(
            keyc_build(digit.into(), KeyModifiers::META),
            bind(vec!["select-layout".into(), name.into()]),
        );
    }
}

/// Default copy-mode-vi key bindings.
fn default_copy_mode_vi() -> HashMap<KeyCode, KeyBinding> {
    let mut m = HashMap::new();

    // Navigation
    m.insert(b'h' as KeyCode, bind(vec!["cursor-left".into()]));
    m.insert(b'j' as KeyCode, bind(vec!["cursor-down".into()]));
    m.insert(b'k' as KeyCode, bind(vec!["cursor-up".into()]));
    m.insert(b'l' as KeyCode, bind(vec!["cursor-right".into()]));
    m.insert(KEYC_UP, bind(vec!["cursor-up".into()]));
    m.insert(KEYC_DOWN, bind(vec!["cursor-down".into()]));
    m.insert(KEYC_LEFT, bind(vec!["cursor-left".into()]));
    m.insert(KEYC_RIGHT, bind(vec!["cursor-right".into()]));

    // Page movement
    m.insert(KEYC_PPAGE, bind(vec!["page-up".into()]));
    m.insert(KEYC_NPAGE, bind(vec!["page-down".into()]));
    m.insert(keyc_build(b'b'.into(), KeyModifiers::CTRL), bind(vec!["page-up".into()]));
    m.insert(keyc_build(b'f'.into(), KeyModifiers::CTRL), bind(vec!["page-down".into()]));

    // Half page
    m.insert(keyc_build(b'u'.into(), KeyModifiers::CTRL), bind(vec!["halfpage-up".into()]));
    m.insert(keyc_build(b'd'.into(), KeyModifiers::CTRL), bind(vec!["halfpage-down".into()]));

    // Top/bottom
    m.insert(b'g' as KeyCode, bind(vec!["history-top".into()]));
    m.insert(b'G' as KeyCode, bind(vec!["history-bottom".into()]));

    // Line movement
    m.insert(b'0' as KeyCode, bind(vec!["start-of-line".into()]));
    m.insert(b'$' as KeyCode, bind(vec!["end-of-line".into()]));
    m.insert(b'^' as KeyCode, bind(vec!["back-to-indentation".into()]));
    m.insert(KEYC_HOME, bind(vec!["start-of-line".into()]));
    m.insert(KEYC_END, bind(vec!["end-of-line".into()]));

    // Word movement
    m.insert(b'w' as KeyCode, bind(vec!["next-word".into()]));
    m.insert(b'b' as KeyCode, bind(vec!["previous-word".into()]));
    m.insert(b'e' as KeyCode, bind(vec!["next-word-end".into()]));

    // Selection
    m.insert(b'v' as KeyCode, bind(vec!["begin-selection".into()]));
    m.insert(KEYC_SPACE, bind(vec!["begin-selection".into()]));
    m.insert(b'V' as KeyCode, bind(vec!["select-line".into()]));
    m.insert(keyc_build(b'v'.into(), KeyModifiers::CTRL), bind(vec!["rectangle-toggle".into()]));

    // Jump to character
    m.insert(b'f' as KeyCode, bind(vec!["jump-forward".into()]));
    m.insert(b'F' as KeyCode, bind(vec!["jump-backward".into()]));
    m.insert(b't' as KeyCode, bind(vec!["jump-to-forward".into()]));
    m.insert(b'T' as KeyCode, bind(vec!["jump-to-backward".into()]));
    m.insert(b';' as KeyCode, bind(vec!["jump-again".into()]));
    m.insert(b',' as KeyCode, bind(vec!["jump-reverse".into()]));

    // Mark
    m.insert(b'm' as KeyCode, bind(vec!["set-mark".into()]));
    m.insert(keyc_build(b'm'.into(), KeyModifiers::META), bind(vec!["swap-mark".into()]));

    // Search
    m.insert(b'/' as KeyCode, bind(vec!["search-forward".into()]));
    m.insert(b'?' as KeyCode, bind(vec!["search-backward".into()]));
    m.insert(b'n' as KeyCode, bind(vec!["search-again".into()]));
    m.insert(b'N' as KeyCode, bind(vec!["search-reverse".into()]));

    // Copy/exit
    m.insert(KEYC_RETURN, bind(vec!["copy-selection-and-cancel".into()]));
    m.insert(b'y' as KeyCode, bind(vec!["copy-selection-and-cancel".into()]));

    // Go to line
    m.insert(b':' as KeyCode, bind(vec!["goto-line".into()]));

    // Cancel
    m.insert(b'q' as KeyCode, bind(vec!["cancel".into()]));
    m.insert(KEYC_ESCAPE, bind(vec!["cancel".into()]));

    m
}

/// Default copy-mode-emacs key bindings.
fn default_copy_mode_emacs() -> HashMap<KeyCode, KeyBinding> {
    let mut m = HashMap::new();

    // Navigation
    m.insert(KEYC_UP, bind(vec!["cursor-up".into()]));
    m.insert(KEYC_DOWN, bind(vec!["cursor-down".into()]));
    m.insert(KEYC_LEFT, bind(vec!["cursor-left".into()]));
    m.insert(KEYC_RIGHT, bind(vec!["cursor-right".into()]));
    m.insert(keyc_build(b'p'.into(), KeyModifiers::CTRL), bind(vec!["cursor-up".into()]));
    m.insert(keyc_build(b'n'.into(), KeyModifiers::CTRL), bind(vec!["cursor-down".into()]));
    m.insert(keyc_build(b'b'.into(), KeyModifiers::CTRL), bind(vec!["cursor-left".into()]));
    m.insert(keyc_build(b'f'.into(), KeyModifiers::CTRL), bind(vec!["cursor-right".into()]));

    // Page movement
    m.insert(KEYC_PPAGE, bind(vec!["page-up".into()]));
    m.insert(KEYC_NPAGE, bind(vec!["page-down".into()]));
    m.insert(keyc_build(b'v'.into(), KeyModifiers::META), bind(vec!["page-up".into()]));
    m.insert(keyc_build(b'v'.into(), KeyModifiers::CTRL), bind(vec!["page-down".into()]));

    // Line movement
    m.insert(keyc_build(b'a'.into(), KeyModifiers::CTRL), bind(vec!["start-of-line".into()]));
    m.insert(keyc_build(b'e'.into(), KeyModifiers::CTRL), bind(vec!["end-of-line".into()]));

    // Word movement
    m.insert(keyc_build(b'f'.into(), KeyModifiers::META), bind(vec!["next-word".into()]));
    m.insert(keyc_build(b'b'.into(), KeyModifiers::META), bind(vec!["previous-word".into()]));

    // Selection
    m.insert(keyc_build(KEYC_SPACE, KeyModifiers::CTRL), bind(vec!["begin-selection".into()]));

    // Copy
    m.insert(
        keyc_build(b'w'.into(), KeyModifiers::META),
        bind(vec!["copy-selection-and-cancel".into()]),
    );

    // Cancel
    m.insert(keyc_build(b'g'.into(), KeyModifiers::CTRL), bind(vec!["cancel".into()]));
    m.insert(KEYC_ESCAPE, bind(vec!["cancel".into()]));

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

    #[test]
    fn repeatable_binding_stays_in_prefix() {
        let mut kb = KeyBindings::default_bindings();
        // Add a repeatable binding for 'z' in prefix table
        kb.add_binding_with_repeat("prefix", b'z' as KeyCode, vec!["test-repeat".into()], true);

        // Enter prefix mode
        let _ = kb.process_input(b"\x02");
        assert!(kb.in_prefix());

        // Press 'z' which is repeatable
        let (result, _) = kb.process_input(b"z");
        assert!(matches!(result, Some(KeyAction::Command(ref argv)) if argv == &["test-repeat"]));
        // Should still be in prefix mode (repeatable)
        assert!(kb.in_prefix());

        // Set a repeat timeout
        kb.set_repeat_timeout(500);
        assert!(kb.in_prefix());
    }

    #[test]
    fn non_repeatable_binding_exits_prefix() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        assert!(kb.in_prefix());

        // 'd' (detach) is NOT repeatable
        let (result, _) = kb.process_input(b"d");
        assert!(matches!(result, Some(KeyAction::Command(_))));
        assert!(!kb.in_prefix());
    }

    #[test]
    fn repeat_timeout_expires() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        // Set an already-expired timeout
        kb.set_repeat_timeout(0);
        kb.prefix_expiry =
            Some(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1)).unwrap());

        // Next key should see prefix expired and fall through to normal processing
        let (result, _) = kb.process_input(b"a");
        // 'a' is not in root table, so passes through
        assert!(result.is_none());
        assert!(!kb.in_prefix());
    }

    #[test]
    fn copy_mode_vi_goto_line_binding() {
        let kb = KeyBindings::default_bindings();
        let action = kb.lookup_in_table("copy-mode-vi", b':' as KeyCode);
        assert_eq!(action, Some(&vec!["goto-line".to_string()]));
    }

    #[test]
    fn add_binding_with_repeat_flag() {
        let mut kb = KeyBindings::default_bindings();
        kb.add_binding_with_repeat("prefix", b'z' as KeyCode, vec!["test-cmd".into()], true);
        let binding = kb.lookup_binding("prefix", b'z' as KeyCode);
        assert!(binding.is_some());
        assert!(binding.unwrap().repeatable);
    }

    #[test]
    fn list_bindings_shows_repeat_flag() {
        let mut kb = KeyBindings::default_bindings();
        kb.add_binding_with_repeat("prefix", b'z' as KeyCode, vec!["test-cmd".into()], true);
        let bindings = kb.list_bindings();
        let z_binding = bindings.iter().find(|b| b.contains("test-cmd")).unwrap();
        assert!(z_binding.contains(" -r"), "expected -r flag in: {z_binding}");
    }

    #[test]
    fn copy_mode_vi_has_set_mark_binding() {
        let kb = KeyBindings::default_bindings();
        let table = kb.tables.get("copy-mode-vi").expect("copy-mode-vi table");
        let binding = table.get(&(b'm' as KeyCode));
        assert!(binding.is_some(), "m should be bound in copy-mode-vi");
        assert_eq!(binding.unwrap().argv, vec!["set-mark"]);
    }

    #[test]
    fn copy_mode_vi_has_swap_mark_binding() {
        let kb = KeyBindings::default_bindings();
        let table = kb.tables.get("copy-mode-vi").expect("copy-mode-vi table");
        // M-m = Meta + m
        let meta_m = keyc_build(b'm'.into(), KeyModifiers::META);
        let binding = table.get(&meta_m);
        assert!(binding.is_some(), "M-m should be bound in copy-mode-vi");
        assert_eq!(binding.unwrap().argv, vec!["swap-mark"]);
    }
}
