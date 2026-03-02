//! Mouse protocol encoding/decoding.
//!
//! Supports X10 normal mode (`ESC[M` + 3 bytes) and SGR extended mode
//! (`ESC[<Ps;Px;PyM` or `ESC[<Ps;Px;Pym`).

use rmux_core::key::*;

/// Result of parsing a mouse sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMouse {
    /// The key code representing this mouse event.
    pub key: KeyCode,
    /// Column (0-based).
    pub x: u32,
    /// Row (0-based).
    pub y: u32,
    /// Number of bytes consumed from the input.
    pub consumed: usize,
}

/// Parse an X10 mouse sequence: `ESC[M` followed by 3 bytes (button, x+33, y+33).
///
/// The input `data` should start *after* `ESC[M` (i.e., just the 3 button/x/y bytes).
fn parse_x10_mouse(data: &[u8]) -> Option<ParsedMouse> {
    if data.len() < 3 {
        return None;
    }
    let cb = data[0].wrapping_sub(32) as u32;
    let x = (data[1] as u32).saturating_sub(33);
    let y = (data[2] as u32).saturating_sub(33);

    let key = button_to_keycode(cb, false);

    Some(ParsedMouse { key, x, y, consumed: 3 })
}

/// Parse an SGR mouse sequence: `<Ps;Px;PyM` or `<Ps;Px;Pym`.
///
/// The input `data` should start *after* `ESC[<` (the parameters and final byte).
fn parse_sgr_mouse(data: &[u8]) -> Option<ParsedMouse> {
    // Find the final byte (M = press, m = release)
    let mut end = 0;
    while end < data.len() {
        if data[end] == b'M' || data[end] == b'm' {
            break;
        }
        if !data[end].is_ascii_digit() && data[end] != b';' {
            return None; // Invalid
        }
        end += 1;
    }
    if end >= data.len() {
        return None; // Need more input
    }

    let is_release = data[end] == b'm';
    let params_str = std::str::from_utf8(&data[..end]).ok()?;
    let parts: Vec<&str> = params_str.split(';').collect();
    if parts.len() != 3 {
        return None;
    }

    let cb: u32 = parts[0].parse().ok()?;
    let x: u32 = parts[1].parse::<u32>().ok()?.saturating_sub(1); // SGR is 1-based
    let y: u32 = parts[2].parse::<u32>().ok()?.saturating_sub(1);

    let key = button_to_keycode(cb, is_release);

    Some(ParsedMouse { key, x, y, consumed: end + 1 })
}

/// Convert a button code to a key code.
///
/// For X10, `is_release` is always false (X10 uses 0x03 in button byte for release).
/// For SGR, `is_release` is true when the final byte is `m`.
fn button_to_keycode(cb: u32, is_release: bool) -> KeyCode {
    let button_bits = cb & 0x03;
    let is_drag = cb & 0x20 != 0;
    let is_wheel = cb & 0x40 != 0;

    if is_wheel {
        // Wheel events
        if button_bits == 0 {
            KEYC_WHEELUP
        } else {
            KEYC_WHEELDOWN
        }
    } else if is_drag {
        // Drag events
        match button_bits {
            0 => KEYC_MOUSEDRAG1,
            1 => KEYC_MOUSEDRAG2,
            2 => KEYC_MOUSEDRAG3,
            _ => KEYC_MOUSE,
        }
    } else if is_release {
        // SGR release
        match button_bits {
            0 => KEYC_MOUSEUP1,
            1 => KEYC_MOUSEUP2,
            2 => KEYC_MOUSEUP3,
            _ => KEYC_MOUSE,
        }
    } else if button_bits == 3 {
        // X10 release (all buttons released)
        KEYC_MOUSEUP1
    } else {
        // Press events
        match button_bits {
            0 => KEYC_MOUSEDOWN1,
            1 => KEYC_MOUSEDOWN2,
            2 => KEYC_MOUSEDOWN3,
            _ => KEYC_MOUSE,
        }
    }
}

/// Try to parse a mouse sequence from data that starts after `ESC[`.
///
/// Returns `Some((key, x, y, consumed))` where consumed is bytes after `ESC[`.
pub fn try_parse_mouse_csi(data: &[u8]) -> Option<ParsedMouse> {
    if data.is_empty() {
        return None;
    }

    match data[0] {
        // X10: ESC[M + 3 bytes. data[0] is 'M', data[1..4] is button/x/y
        b'M' => {
            let result = parse_x10_mouse(&data[1..])?;
            Some(ParsedMouse {
                consumed: result.consumed + 1, // +1 for the 'M'
                ..result
            })
        }
        // SGR: ESC[< + params + M/m. data[0] is '<'
        b'<' => {
            let result = parse_sgr_mouse(&data[1..])?;
            Some(ParsedMouse {
                consumed: result.consumed + 1, // +1 for the '<'
                ..result
            })
        }
        _ => None,
    }
}

/// Encode a mouse event as SGR protocol bytes (for forwarding to PTY).
#[must_use]
pub fn encode_sgr_mouse(key: KeyCode, x: u32, y: u32) -> Vec<u8> {
    let base = keyc_base(key);
    let (cb, final_byte) = match base {
        KEYC_MOUSEDOWN1 => (0, b'M'),
        KEYC_MOUSEDOWN2 => (1, b'M'),
        KEYC_MOUSEDOWN3 => (2, b'M'),
        KEYC_MOUSEUP1 => (0, b'm'),
        KEYC_MOUSEUP2 => (1, b'm'),
        KEYC_MOUSEUP3 => (2, b'm'),
        KEYC_MOUSEDRAG1 => (32, b'M'),
        KEYC_MOUSEDRAG2 => (33, b'M'),
        KEYC_MOUSEDRAG3 => (34, b'M'),
        KEYC_WHEELUP => (64, b'M'),
        KEYC_WHEELDOWN => (65, b'M'),
        _ => (0, b'M'),
    };

    format!("\x1b[<{cb};{};{}{}", x + 1, y + 1, final_byte as char).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x10_click() {
        // ESC[M + button(0+32=32) + x(10+33=43) + y(5+33=38)
        let data = [b' ', b'+', b'&']; // 32, 43, 38
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    #[test]
    fn x10_release() {
        // Button 3 = release in X10
        let data = [b'#', b'+', b'&']; // 35=32+3, 43, 38
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEUP1);
    }

    #[test]
    fn x10_wheel_up() {
        let data = [32 + 64, b'+', b'&']; // 96=32+64, 43, 38
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_WHEELUP);
    }

    #[test]
    fn sgr_click() {
        // ESC[<0;11;6M -> button 0, x=10 (11-1), y=5 (6-1), press
        let data = b"0;11;6M";
        let result = parse_sgr_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    #[test]
    fn sgr_release() {
        let data = b"0;11;6m";
        let result = parse_sgr_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEUP1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    #[test]
    fn sgr_drag() {
        let data = b"32;15;20M";
        let result = parse_sgr_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDRAG1);
        assert_eq!(result.x, 14);
        assert_eq!(result.y, 19);
    }

    #[test]
    fn sgr_wheel_down() {
        let data = b"65;5;10M";
        let result = parse_sgr_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_WHEELDOWN);
    }

    #[test]
    fn try_parse_x10_via_csi() {
        let data = [b'M', b' ', b'+', b'&']; // M + 3 bytes
        let result = try_parse_mouse_csi(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.consumed, 4);
    }

    #[test]
    fn try_parse_sgr_via_csi() {
        let data = b"<0;11;6M";
        let result = try_parse_mouse_csi(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.consumed, 8);
    }

    #[test]
    fn encode_sgr_roundtrip() {
        let encoded = encode_sgr_mouse(KEYC_MOUSEDOWN1, 10, 5);
        assert_eq!(encoded, b"\x1b[<0;11;6M");

        // Now parse it back (skip ESC[)
        let result = try_parse_mouse_csi(&encoded[2..]).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    #[test]
    fn encode_sgr_release() {
        let encoded = encode_sgr_mouse(KEYC_MOUSEUP1, 3, 7);
        assert_eq!(encoded, b"\x1b[<0;4;8m");
    }

    #[test]
    fn incomplete_x10_returns_none() {
        assert!(parse_x10_mouse(&[32, 43]).is_none());
    }

    #[test]
    fn incomplete_sgr_returns_none() {
        assert!(parse_sgr_mouse(b"0;11;").is_none());
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn try_parse_mouse_csi_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
                let _ = try_parse_mouse_csi(&data);
            }

            #[test]
            fn parse_x10_never_panics(data in proptest::collection::vec(any::<u8>(), 0..64)) {
                let _ = parse_x10_mouse(&data);
            }

            #[test]
            fn parse_sgr_never_panics(data in proptest::collection::vec(any::<u8>(), 0..64)) {
                let _ = parse_sgr_mouse(&data);
            }

            #[test]
            fn sgr_encode_decode_roundtrip(
                x in 0u32..200,
                y in 0u32..60,
            ) {
                // Encode button 1 press, then parse
                let encoded = encode_sgr_mouse(KEYC_MOUSEDOWN1, x, y);
                // Skip the ESC[ prefix (should be \x1b[)
                if encoded.len() > 2 && encoded[0] == 0x1b && encoded[1] == b'[' {
                    if let Some(parsed) = try_parse_mouse_csi(&encoded[2..]) {
                        prop_assert_eq!(parsed.x, x);
                        prop_assert_eq!(parsed.y, y);
                    }
                }
            }
        }
    }
}
