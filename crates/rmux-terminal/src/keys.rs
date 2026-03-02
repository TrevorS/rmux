//! Key sequence parsing.
//!
//! Converts raw byte sequences from the terminal into key codes.
//! Handles escape sequences for function keys, arrow keys, etc.

use rmux_core::key::*;

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
        0x00 => Some((keyc_build(b' '.into(), KeyModifiers::CTRL), 1)),
        0x01..=0x1A => {
            let base = (data[0] - 1 + b'a') as u64;
            Some((keyc_build(base, KeyModifiers::CTRL), 1))
        }
        0x7F => Some((KEYC_BACKSPACE, 1)),
        // Plain ASCII
        0x20..=0x7E => Some((data[0] as KeyCode, 1)),
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
        // Alt+key
        0x20..=0x7E => Some((keyc_build(data[1] as KeyCode, KeyModifiers::META), 2)),
        _ => Some((KEYC_ESCAPE, 1)),
    }
}

fn parse_csi_key(data: &[u8]) -> Option<(KeyCode, usize)> {
    if data.is_empty() {
        return None;
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
}
