//! Key sequence parsing.
//!
//! Converts raw byte sequences from the terminal into key codes.
//! Handles escape sequences for function keys, arrow keys, etc.

use crate::mouse;
use rmux_core::key::*;

/// Result of parsing a key that may be a mouse event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// The key code.
    pub key: KeyCode,
    /// Number of bytes consumed.
    pub consumed: usize,
    /// Mouse coordinates (only set for mouse events).
    pub mouse_x: u32,
    /// Mouse y coordinate (only set for mouse events).
    pub mouse_y: u32,
}

/// Parse a byte sequence into a key code, also returning mouse coordinates
/// if the input is a mouse event.
#[must_use]
pub fn parse_key_event(data: &[u8]) -> Option<KeyEvent> {
    // Try mouse parsing first for ESC[ sequences
    if data.len() >= 3 && data[0] == 0x1b && data[1] == b'[' {
        if let Some(parsed) = mouse::try_parse_mouse_csi(&data[2..]) {
            return Some(KeyEvent {
                key: parsed.key,
                consumed: parsed.consumed + 2, // +2 for ESC[
                mouse_x: parsed.x,
                mouse_y: parsed.y,
            });
        }
    }

    let (key, consumed) = parse_key(data)?;
    Some(KeyEvent { key, consumed, mouse_x: 0, mouse_y: 0 })
}

/// Parse a byte sequence into a key code.
///
/// Returns `Some((key, consumed))` on success, where `consumed` is the number
/// of bytes consumed from the input.
/// Returns `None` if more input is needed.
#[must_use]
pub fn parse_key(data: &[u8]) -> Option<(KeyCode, usize)> {
    if data.is_empty() {
        return None;
    }

    match data[0] {
        // ESC sequences
        0x1B => parse_escape(data),
        // C0 controls
        0x00 => Some((keyc_build(KEYC_SPACE, KeyModifiers::CTRL), 1)),
        0x01..=0x1A => {
            let base = (data[0] - 1 + b'a') as u64;
            Some((keyc_build(base, KeyModifiers::CTRL), 1))
        }
        0x7F => Some((KEYC_BACKSPACE, 1)),
        // Space gets its own constant to match string_to_key("Space")
        0x20 => Some((KEYC_SPACE, 1)),
        // Plain ASCII
        0x21..=0x7E => Some((data[0] as KeyCode, 1)),
        // UTF-8 start bytes
        0xC0..=0xDF => parse_utf8(data, 2),
        0xE0..=0xEF => parse_utf8(data, 3),
        0xF0..=0xF7 => parse_utf8(data, 4),
        _ => Some((KEYC_UNKNOWN, 1)),
    }
}

fn parse_escape(data: &[u8]) -> Option<(KeyCode, usize)> {
    if data.len() < 2 {
        return None; // Need more input
    }

    match data[1] {
        b'[' => parse_csi_key(&data[2..]).map(|(k, n)| (k, n + 2)),
        b'O' => parse_ss3_key(&data[2..]).map(|(k, n)| (k, n + 2)),
        // Alt+Space
        0x20 => Some((keyc_build(KEYC_SPACE, KeyModifiers::META), 2)),
        // Alt+key
        0x21..=0x7E => Some((keyc_build(data[1] as KeyCode, KeyModifiers::META), 2)),
        _ => Some((KEYC_ESCAPE, 1)),
    }
}

fn parse_csi_key(data: &[u8]) -> Option<(KeyCode, usize)> {
    if data.is_empty() {
        return None;
    }

    // Check for mouse sequences first (X10: M + 3 bytes, SGR: < + params + M/m)
    if data[0] == b'M' || data[0] == b'<' {
        if let Some(parsed) = mouse::try_parse_mouse_csi(data) {
            return Some((parsed.key, parsed.consumed));
        }
        if data[0] == b'M' && data.len() < 4 {
            return None; // Need more input for X10
        }
        if data[0] == b'<' {
            // Check if we need more input for SGR
            let has_final = data[1..].iter().any(|&b| b == b'M' || b == b'm');
            if !has_final {
                return None; // Need more input
            }
        }
    }

    // Find the final byte
    let mut i = 0;
    while i < data.len() {
        match data[i] {
            0x40..=0x7E => break,  // Final byte
            0x20..=0x3F => i += 1, // Parameter/intermediate
            _ => return Some((KEYC_UNKNOWN, i + 1)),
        }
    }

    if i >= data.len() {
        return None; // Need more input
    }

    let final_byte = data[i];
    let param_data = &data[..i];

    let key = match final_byte {
        b'A' => KEYC_UP,
        b'B' => KEYC_DOWN,
        b'C' => KEYC_RIGHT,
        b'D' => KEYC_LEFT,
        b'H' => KEYC_HOME,
        b'F' => KEYC_END,
        b'~' => {
            // Parse parameter for special keys
            let n: u32 = std::str::from_utf8(param_data)
                .ok()
                .and_then(|s| s.split(';').next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            match n {
                1 => KEYC_HOME,
                2 => KEYC_INSERT,
                3 => KEYC_DELETE,
                4 => KEYC_END,
                5 => KEYC_PPAGE,
                6 => KEYC_NPAGE,
                15 => KEYC_F5,
                17 => KEYC_F6,
                18 => KEYC_F7,
                19 => KEYC_F8,
                20 => KEYC_F9,
                21 => KEYC_F10,
                23 => KEYC_F11,
                24 => KEYC_F12,
                _ => KEYC_UNKNOWN,
            }
        }
        _ => KEYC_UNKNOWN,
    };

    Some((key, i + 1))
}

fn parse_ss3_key(data: &[u8]) -> Option<(KeyCode, usize)> {
    if data.is_empty() {
        return None;
    }

    let key = match data[0] {
        b'A' => KEYC_UP,
        b'B' => KEYC_DOWN,
        b'C' => KEYC_RIGHT,
        b'D' => KEYC_LEFT,
        b'H' => KEYC_HOME,
        b'F' => KEYC_END,
        b'P' => KEYC_F1,
        b'Q' => KEYC_F2,
        b'R' => KEYC_F3,
        b'S' => KEYC_F4,
        _ => KEYC_UNKNOWN,
    };

    Some((key, 1))
}

fn parse_utf8(data: &[u8], needed: usize) -> Option<(KeyCode, usize)> {
    if data.len() < needed {
        return None;
    }
    // Validate UTF-8 and return the codepoint as key
    if let Ok(s) = std::str::from_utf8(&data[..needed]) {
        if let Some(ch) = s.chars().next() {
            return Some((ch as KeyCode, needed));
        }
    }
    Some((KEYC_UNKNOWN, 1))
}

/// Convert a tmux key name string to the raw byte sequence a terminal would emit.
///
/// Supports: "Enter", "Escape", "Space", "Tab", "BSpace", "DC" (Delete),
/// "IC" (Insert), "Home", "End", "PPage", "NPage", "Up", "Down", "Left", "Right",
/// "F1"-"F12", "C-x" (Ctrl+x), "M-x" (Alt+x), and bare printable characters.
///
/// Returns `None` if the key name is not recognized.
#[must_use]
pub fn key_name_to_bytes(name: &str) -> Option<Vec<u8>> {
    // Check for modifier prefixes first
    if let Some(rest) = name.strip_prefix("C-") {
        // Ctrl+letter: compute as letter & 0x1f
        if rest.len() == 1 {
            let ch = rest.as_bytes()[0];
            if ch.is_ascii_alphabetic() {
                return Some(vec![ch.to_ascii_lowercase() & 0x1f]);
            }
        }
        return None;
    }

    if let Some(rest) = name.strip_prefix("M-") {
        // Alt+key: ESC followed by the key
        if rest.len() == 1 {
            return Some(vec![0x1b, rest.as_bytes()[0]]);
        }
        return None;
    }

    match name {
        "Enter" | "CR" => Some(b"\r".to_vec()),
        "Escape" | "Esc" => Some(vec![0x1b]),
        "Space" => Some(b" ".to_vec()),
        "Tab" | "BTab" => Some(b"\t".to_vec()),
        "BSpace" => Some(vec![0x7f]),
        "DC" | "Delete" => Some(b"\x1b[3~".to_vec()),
        "IC" | "Insert" => Some(b"\x1b[2~".to_vec()),
        "Home" => Some(b"\x1b[H".to_vec()),
        "End" => Some(b"\x1b[F".to_vec()),
        "PPage" | "PageUp" => Some(b"\x1b[5~".to_vec()),
        "NPage" | "PageDown" => Some(b"\x1b[6~".to_vec()),
        "Up" => Some(b"\x1b[A".to_vec()),
        "Down" => Some(b"\x1b[B".to_vec()),
        "Right" => Some(b"\x1b[C".to_vec()),
        "Left" => Some(b"\x1b[D".to_vec()),
        "F1" => Some(b"\x1bOP".to_vec()),
        "F2" => Some(b"\x1bOQ".to_vec()),
        "F3" => Some(b"\x1bOR".to_vec()),
        "F4" => Some(b"\x1bOS".to_vec()),
        "F5" => Some(b"\x1b[15~".to_vec()),
        "F6" => Some(b"\x1b[17~".to_vec()),
        "F7" => Some(b"\x1b[18~".to_vec()),
        "F8" => Some(b"\x1b[19~".to_vec()),
        "F9" => Some(b"\x1b[20~".to_vec()),
        "F10" => Some(b"\x1b[21~".to_vec()),
        "F11" => Some(b"\x1b[23~".to_vec()),
        "F12" => Some(b"\x1b[24~".to_vec()),
        _ => {
            // Single printable character
            if name.len() == 1 && name.as_bytes()[0].is_ascii_graphic() {
                Some(name.as_bytes().to_vec())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii() {
        assert_eq!(parse_key(b"a"), Some((b'a' as KeyCode, 1)));
        assert_eq!(parse_key(b"Z"), Some((b'Z' as KeyCode, 1)));
    }

    #[test]
    fn ctrl_c() {
        let (key, len) = parse_key(b"\x03").unwrap();
        assert_eq!(len, 1);
        assert_eq!(keyc_base(key), b'c' as KeyCode);
        assert!(keyc_modifiers(key).contains(KeyModifiers::CTRL));
    }

    #[test]
    fn arrow_keys() {
        assert_eq!(parse_key(b"\x1b[A"), Some((KEYC_UP, 3)));
        assert_eq!(parse_key(b"\x1b[B"), Some((KEYC_DOWN, 3)));
        assert_eq!(parse_key(b"\x1b[C"), Some((KEYC_RIGHT, 3)));
        assert_eq!(parse_key(b"\x1b[D"), Some((KEYC_LEFT, 3)));
    }

    #[test]
    fn function_keys() {
        assert_eq!(parse_key(b"\x1bOP"), Some((KEYC_F1, 3)));
        assert_eq!(parse_key(b"\x1bOQ"), Some((KEYC_F2, 3)));
    }

    #[test]
    fn special_keys() {
        assert_eq!(parse_key(b"\x1b[2~"), Some((KEYC_INSERT, 4)));
        assert_eq!(parse_key(b"\x1b[3~"), Some((KEYC_DELETE, 4)));
        assert_eq!(parse_key(b"\x1b[5~"), Some((KEYC_PPAGE, 4)));
        assert_eq!(parse_key(b"\x1b[6~"), Some((KEYC_NPAGE, 4)));
    }

    #[test]
    fn alt_key() {
        let (key, len) = parse_key(b"\x1ba").unwrap();
        assert_eq!(len, 2);
        assert_eq!(keyc_base(key), b'a' as KeyCode);
        assert!(keyc_modifiers(key).contains(KeyModifiers::META));
    }

    #[test]
    fn incomplete_returns_none() {
        assert_eq!(parse_key(b"\x1b"), None);
        assert_eq!(parse_key(b"\x1b["), None);
    }

    #[test]
    fn backspace() {
        assert_eq!(parse_key(b"\x7f"), Some((KEYC_BACKSPACE, 1)));
    }

    #[test]
    fn key_name_enter() {
        assert_eq!(key_name_to_bytes("Enter"), Some(b"\r".to_vec()));
        assert_eq!(key_name_to_bytes("CR"), Some(b"\r".to_vec()));
    }

    #[test]
    fn key_name_ctrl() {
        assert_eq!(key_name_to_bytes("C-c"), Some(vec![0x03]));
        assert_eq!(key_name_to_bytes("C-a"), Some(vec![0x01]));
        assert_eq!(key_name_to_bytes("C-z"), Some(vec![0x1a]));
    }

    #[test]
    fn key_name_arrows() {
        assert_eq!(key_name_to_bytes("Up"), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_name_to_bytes("Down"), Some(b"\x1b[B".to_vec()));
        assert_eq!(key_name_to_bytes("Left"), Some(b"\x1b[D".to_vec()));
        assert_eq!(key_name_to_bytes("Right"), Some(b"\x1b[C".to_vec()));
    }

    #[test]
    fn key_name_special() {
        assert_eq!(key_name_to_bytes("Escape"), Some(vec![0x1b]));
        assert_eq!(key_name_to_bytes("Space"), Some(b" ".to_vec()));
        assert_eq!(key_name_to_bytes("Tab"), Some(b"\t".to_vec()));
        assert_eq!(key_name_to_bytes("BSpace"), Some(vec![0x7f]));
    }

    #[test]
    fn key_name_function_keys() {
        assert_eq!(key_name_to_bytes("F1"), Some(b"\x1bOP".to_vec()));
        assert_eq!(key_name_to_bytes("F12"), Some(b"\x1b[24~".to_vec()));
    }

    #[test]
    fn key_name_bare_char() {
        assert_eq!(key_name_to_bytes("a"), Some(b"a".to_vec()));
        assert_eq!(key_name_to_bytes("Z"), Some(b"Z".to_vec()));
    }

    #[test]
    fn key_name_unknown() {
        assert_eq!(key_name_to_bytes("FooBar"), None);
    }

    #[test]
    fn key_name_alt() {
        assert_eq!(key_name_to_bytes("M-x"), Some(vec![0x1b, b'x']));
    }

    #[test]
    fn parse_x10_mouse_click() {
        // ESC[M + button(0+32=32) + x(10+33=43) + y(5+33=38)
        let data = b"\x1b[M +&";
        let result = parse_key(data).unwrap();
        assert_eq!(result.0, KEYC_MOUSEDOWN1);
    }

    #[test]
    fn parse_sgr_mouse_click() {
        let data = b"\x1b[<0;11;6M";
        let result = parse_key(data).unwrap();
        assert_eq!(result.0, KEYC_MOUSEDOWN1);
    }

    #[test]
    fn parse_key_event_mouse_coords() {
        let data = b"\x1b[<0;11;6M";
        let event = parse_key_event(data).unwrap();
        assert_eq!(event.key, KEYC_MOUSEDOWN1);
        assert_eq!(event.mouse_x, 10);
        assert_eq!(event.mouse_y, 5);
    }

    #[test]
    fn parse_key_event_non_mouse() {
        let data = b"a";
        let event = parse_key_event(data).unwrap();
        assert_eq!(event.key, b'a' as KeyCode);
        assert_eq!(event.mouse_x, 0);
        assert_eq!(event.mouse_y, 0);
    }

    #[test]
    fn parse_f3_through_f12() {
        // F3 and F4 use SS3 sequences
        assert_eq!(parse_key(b"\x1bOR"), Some((KEYC_F3, 3)));
        assert_eq!(parse_key(b"\x1bOS"), Some((KEYC_F4, 3)));
        // F5-F12 use CSI ~ sequences
        assert_eq!(parse_key(b"\x1b[15~"), Some((KEYC_F5, 5)));
        assert_eq!(parse_key(b"\x1b[17~"), Some((KEYC_F6, 5)));
        assert_eq!(parse_key(b"\x1b[18~"), Some((KEYC_F7, 5)));
        assert_eq!(parse_key(b"\x1b[19~"), Some((KEYC_F8, 5)));
        assert_eq!(parse_key(b"\x1b[20~"), Some((KEYC_F9, 5)));
        assert_eq!(parse_key(b"\x1b[21~"), Some((KEYC_F10, 5)));
        assert_eq!(parse_key(b"\x1b[23~"), Some((KEYC_F11, 5)));
        assert_eq!(parse_key(b"\x1b[24~"), Some((KEYC_F12, 5)));
    }

    #[test]
    fn parse_csi_with_modifier() {
        // CSI sequences with modifier parameters like ESC[1;2A (Shift+Up)
        // The current parser does not extract modifiers from CSI params for
        // arrow keys, so it returns the base key code without modifier bits.
        let result = parse_key(b"\x1b[1;2A");
        let (key, consumed) = result.unwrap();
        assert_eq!(key, KEYC_UP);
        assert_eq!(consumed, 6); // ESC [ 1 ; 2 A = 6 bytes total
    }

    #[test]
    fn parse_utf8_two_byte() {
        // U+00E9 (e with acute accent) = 0xC3 0xA9
        let data = [0xC3, 0xA9];
        let (key, consumed) = parse_key(&data).unwrap();
        assert_eq!(consumed, 2);
        assert_eq!(key, '\u{00E9}' as KeyCode);
    }

    #[test]
    fn parse_utf8_three_byte() {
        // U+4E16 (CJK character, meaning "world") = 0xE4 0xB8 0x96
        let ch = '\u{4E16}';
        let mut buf = [0u8; 3];
        ch.encode_utf8(&mut buf);
        let (key, consumed) = parse_key(&buf).unwrap();
        assert_eq!(consumed, 3);
        assert_eq!(key, ch as KeyCode);
    }

    #[test]
    fn parse_utf8_four_byte() {
        // U+1F600 (grinning face emoji) = 0xF0 0x9F 0x98 0x80
        let ch = '\u{1F600}';
        let mut buf = [0u8; 4];
        ch.encode_utf8(&mut buf);
        let (key, consumed) = parse_key(&buf).unwrap();
        assert_eq!(consumed, 4);
        assert_eq!(key, ch as KeyCode);
    }

    #[test]
    fn key_name_to_bytes_ctrl_all_letters() {
        for ch in b'a'..=b'z' {
            let name = format!("C-{}", ch as char);
            let bytes = key_name_to_bytes(&name).unwrap();
            let expected = ch & 0x1f;
            assert_eq!(
                bytes,
                vec![expected],
                "C-{} should produce byte {:#04x}, got {:#04x}",
                ch as char,
                expected,
                bytes[0]
            );
        }
    }

    #[test]
    fn incomplete_csi_returns_none() {
        // ESC[ with no final byte
        assert_eq!(parse_key(b"\x1b["), None);
        // ESC[1 -- parameter but no final byte
        assert_eq!(parse_key(b"\x1b[1"), None);
        // ESC[1; -- partial modifier parameter
        assert_eq!(parse_key(b"\x1b[1;"), None);
        // ESC[1;2 -- modifier but no final byte
        assert_eq!(parse_key(b"\x1b[1;2"), None);
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_key_never_panics(data in proptest::collection::vec(any::<u8>(), 0..256)) {
                // parse_key should return None or Some without panicking
                let _ = parse_key(&data);
            }

            #[test]
            fn parse_key_event_never_panics(data in proptest::collection::vec(any::<u8>(), 0..256)) {
                let _ = parse_key_event(&data);
            }

            #[test]
            fn ascii_roundtrip(ch in 0x20u8..0x7f) {
                let result = parse_key(&[ch]).unwrap();
                let expected = if ch == 0x20 { KEYC_SPACE } else { ch as KeyCode };
                prop_assert_eq!(result.0, expected);
                prop_assert_eq!(result.1, 1);
            }

            #[test]
            fn key_name_to_bytes_never_panics(name in "[a-zA-Z0-9_-]{1,20}") {
                // Should return Some or None without panicking
                let _ = key_name_to_bytes(&name);
            }

            #[test]
            fn ctrl_keys_produce_ctrl_modifier(ch in b'a'..=b'z') {
                let input = [ch - b'a' + 1]; // Ctrl+a = 0x01, Ctrl+z = 0x1a
                let (key, consumed) = parse_key(&input).unwrap();
                prop_assert_eq!(consumed, 1);
                prop_assert_eq!(keyc_base(key), ch as KeyCode);
                prop_assert!(keyc_modifiers(key).contains(KeyModifiers::CTRL));
            }
        }
    }
}
