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
    /// Key → command bindings for prefix mode.
    bindings: HashMap<KeyCode, Vec<String>>,
}

impl KeyBindings {
    /// Create default key bindings matching tmux.
    pub fn default_bindings() -> Self {
        let prefix = keyc_build(b'b'.into(), KeyModifiers::CTRL);

        let mut bindings = HashMap::new();

        // d - detach
        bindings.insert(b'd' as KeyCode, vec!["detach-client".into()]);

        // c - new window (not yet implemented, placeholder)
        bindings.insert(b'c' as KeyCode, vec!["new-window".into()]);

        // : - command prompt (not yet implemented)
        bindings.insert(b':' as KeyCode, vec!["command-prompt".into()]);

        Self {
            prefix,
            in_prefix: false,
            bindings,
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

        // Check if this is the prefix key
        if key == self.prefix {
            self.in_prefix = true;
            return Some(KeyAction::SendToPane(Vec::new())); // Consume the prefix key
        }

        // Not handled - pass through to pane
        None
    }
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
    fn normal_input_passes_through() {
        let mut kb = KeyBindings::default_bindings();
        let result = kb.process_input(b"a");
        assert!(result.is_none());
    }
}
