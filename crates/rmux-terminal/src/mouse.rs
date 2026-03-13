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
        if button_bits == 0 { KEYC_WHEELUP } else { KEYC_WHEELDOWN }
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

/// Parse a UTF-8 mouse sequence (mode 1005): `ESC[M` + button + UTF-8 x + UTF-8 y.
///
/// Like X10 but coordinates are encoded as UTF-8 characters (allowing values > 223).
/// The input `data` should start *after* `ESC[M` (the button and coordinate bytes).
fn parse_utf8_mouse(data: &[u8]) -> Option<ParsedMouse> {
    if data.is_empty() {
        return None;
    }
    let cb = data[0].wrapping_sub(32) as u32;
    let (x_char, x_len) = decode_utf8_char(&data[1..])?;
    let (y_char, y_len) = decode_utf8_char(&data[1 + x_len..])?;

    let x = x_char.saturating_sub(33);
    let y = y_char.saturating_sub(33);
    let key = button_to_keycode(cb, false);

    Some(ParsedMouse { key, x, y, consumed: 1 + x_len + y_len })
}

/// Decode a single UTF-8 character, returning (codepoint as u32, bytes consumed).
fn decode_utf8_char(data: &[u8]) -> Option<(u32, usize)> {
    if data.is_empty() {
        return None;
    }
    let b0 = data[0];
    if b0 < 0x80 {
        Some((u32::from(b0), 1))
    } else if b0 & 0xE0 == 0xC0 {
        if data.len() < 2 {
            return None;
        }
        let cp = (u32::from(b0 & 0x1F) << 6) | u32::from(data[1] & 0x3F);
        Some((cp, 2))
    } else if b0 & 0xF0 == 0xE0 {
        if data.len() < 3 {
            return None;
        }
        let cp = (u32::from(b0 & 0x0F) << 12)
            | (u32::from(data[1] & 0x3F) << 6)
            | u32::from(data[2] & 0x3F);
        Some((cp, 3))
    } else {
        None // 4-byte sequences not used in mouse protocol
    }
}

/// Parse a urxvt mouse sequence (mode 1015): `Ps;Px;PyM`.
///
/// Like SGR but without `<` prefix. Ps is button+32, coordinates are 1-based.
/// Only `M` as final byte (no `m` for release — uses X10-style button=3 for release).
/// The input `data` should start *after* `ESC[` (the decimal parameters and final byte).
fn parse_urxvt_mouse(data: &[u8]) -> Option<ParsedMouse> {
    // Find the final byte 'M'
    let mut end = 0;
    while end < data.len() {
        if data[end] == b'M' {
            break;
        }
        if !data[end].is_ascii_digit() && data[end] != b';' {
            return None;
        }
        end += 1;
    }
    if end >= data.len() {
        return None;
    }

    let params_str = std::str::from_utf8(&data[..end]).ok()?;
    let parts: Vec<&str> = params_str.split(';').collect();
    if parts.len() != 3 {
        return None;
    }

    let raw_button: u32 = parts[0].parse().ok()?;
    let cb = raw_button.wrapping_sub(32); // urxvt sends cb+32
    let x: u32 = parts[1].parse::<u32>().ok()?.saturating_sub(1); // 1-based
    let y: u32 = parts[2].parse::<u32>().ok()?.saturating_sub(1);

    let key = button_to_keycode(cb, false);

    Some(ParsedMouse { key, x, y, consumed: end + 1 })
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

/// Try to parse a mouse sequence using UTF-8 encoding (mode 1005).
///
/// The input `data` should start after `ESC[M` (button + UTF-8 coords).
pub fn try_parse_mouse_utf8(data: &[u8]) -> Option<ParsedMouse> {
    parse_utf8_mouse(data)
}

/// Try to parse a urxvt mouse sequence (mode 1015) from data after `ESC[`.
///
/// The format is `Ps;Px;PyM` where Ps is button+32 and coords are 1-based.
pub fn try_parse_mouse_urxvt(data: &[u8]) -> Option<ParsedMouse> {
    parse_urxvt_mouse(data)
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

    #[test]
    fn parse_x10_button1_click() {
        // Button 1 press: cb=0+32=32, x=5+33=38, y=10+33=43
        let data = [32u8, 38, 43];
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 5);
        assert_eq!(result.y, 10);
        assert_eq!(result.consumed, 3);
    }

    #[test]
    fn parse_x10_button2_click() {
        // Button 2 press: cb=1+32=33, x=20+33=53, y=15+33=48
        let data = [33u8, 53, 48];
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN2);
        assert_eq!(result.x, 20);
        assert_eq!(result.y, 15);
    }

    #[test]
    fn parse_x10_button3_click() {
        // Button 3 press: cb=2+32=34, x=0+33=33, y=0+33=33
        let data = [34u8, 33, 33];
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN3);
        assert_eq!(result.x, 0);
        assert_eq!(result.y, 0);
    }

    #[test]
    fn parse_x10_scroll_up() {
        // Wheel up: cb=64+0+32=96, x=10+33=43, y=5+33=38
        let data = [96u8, 43, 38];
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_WHEELUP);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    #[test]
    fn parse_x10_scroll_down() {
        // Wheel down: cb=64+1+32=97, x=10+33=43, y=5+33=38
        let data = [97u8, 43, 38];
        let result = parse_x10_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_WHEELDOWN);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    #[test]
    fn parse_sgr_release_button1() {
        // SGR release for button 1: ESC[<0;11;6m (lowercase 'm' = release)
        let data = b"0;11;6m";
        let result = parse_sgr_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEUP1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
        assert_eq!(result.consumed, 7);
    }

    #[test]
    fn encode_x10_roundtrip() {
        // There is no X10 encode function, but we can encode via SGR and verify
        // the X10 parse produces the same logical event.
        // Manually create an X10 sequence for button 1 at (10, 5):
        // cb=0+32=32, x=10+33=43, y=5+33=38
        let x10_data = [32u8, 43, 38];
        let x10_result = parse_x10_mouse(&x10_data).unwrap();
        assert_eq!(x10_result.key, KEYC_MOUSEDOWN1);
        assert_eq!(x10_result.x, 10);
        assert_eq!(x10_result.y, 5);

        // Encode as SGR and parse to verify same logical event
        let sgr_encoded = encode_sgr_mouse(KEYC_MOUSEDOWN1, 10, 5);
        let sgr_result = try_parse_mouse_csi(&sgr_encoded[2..]).unwrap();
        assert_eq!(sgr_result.key, x10_result.key);
        assert_eq!(sgr_result.x, x10_result.x);
        assert_eq!(sgr_result.y, x10_result.y);
    }

    #[test]
    fn encode_sgr_roundtrip_all_buttons() {
        let test_cases = [
            (KEYC_MOUSEDOWN1, 5, 3),
            (KEYC_MOUSEDOWN2, 0, 0),
            (KEYC_MOUSEDOWN3, 100, 50),
            (KEYC_MOUSEUP1, 20, 10),
            (KEYC_MOUSEUP2, 30, 15),
            (KEYC_MOUSEUP3, 1, 1),
            (KEYC_MOUSEDRAG1, 50, 25),
            (KEYC_WHEELUP, 10, 5),
            (KEYC_WHEELDOWN, 10, 5),
        ];
        for (key, x, y) in test_cases {
            let encoded = encode_sgr_mouse(key, x, y);
            // All SGR encodings start with ESC[
            assert_eq!(&encoded[..2], b"\x1b[");
            let parsed = try_parse_mouse_csi(&encoded[2..]).unwrap();
            assert_eq!(parsed.key, key, "key mismatch for {key:#x}");
            assert_eq!(parsed.x, x, "x mismatch for {key:#x}");
            assert_eq!(parsed.y, y, "y mismatch for {key:#x}");
        }
    }

    #[test]
    fn invalid_x10_too_short() {
        // X10 needs 3 bytes after ESC[M; less than 3 returns None
        assert!(parse_x10_mouse(&[]).is_none());
        assert!(parse_x10_mouse(&[32]).is_none());
        assert!(parse_x10_mouse(&[32, 43]).is_none());
    }

    // ============================================================
    // UTF-8 mouse (mode 1005) tests
    // ============================================================

    #[test]
    fn utf8_mouse_basic_click() {
        // Button 1 press at (10, 5): cb=0+32=32, x=10+33=43, y=5+33=38
        // All ASCII, same as X10
        let data = [32u8, 43, 38];
        let result = parse_utf8_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
        assert_eq!(result.consumed, 3);
    }

    #[test]
    fn utf8_mouse_large_x() {
        // x=200, so x+33=233 -> UTF-8: 0xC3 0xA9 (2 bytes)
        // Button 1 press, y=5 (y+33=38, single byte)
        let x_val: u32 = 200 + 33; // 233
        let x_bytes = [(0xC0 | (x_val >> 6)) as u8, (0x80 | (x_val & 0x3F)) as u8];
        let data = [32u8, x_bytes[0], x_bytes[1], 38]; // button, x(2 bytes), y(1 byte)
        let result = parse_utf8_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 200);
        assert_eq!(result.y, 5);
        assert_eq!(result.consumed, 4);
    }

    #[test]
    fn utf8_mouse_large_xy() {
        // x=300, y=150
        let x_val: u32 = 300 + 33; // 333
        let y_val: u32 = 150 + 33; // 183
        let x_bytes = [(0xC0 | (x_val >> 6)) as u8, (0x80 | (x_val & 0x3F)) as u8];
        let y_bytes = [(0xC0 | (y_val >> 6)) as u8, (0x80 | (y_val & 0x3F)) as u8];
        let data = [32u8, x_bytes[0], x_bytes[1], y_bytes[0], y_bytes[1]];
        let result = parse_utf8_mouse(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 300);
        assert_eq!(result.y, 150);
        assert_eq!(result.consumed, 5);
    }

    #[test]
    fn utf8_mouse_empty() {
        assert!(parse_utf8_mouse(&[]).is_none());
    }

    #[test]
    fn utf8_mouse_incomplete() {
        // Button byte only, no coordinates
        assert!(parse_utf8_mouse(&[32]).is_none());
        // Button + one coord, missing second
        assert!(parse_utf8_mouse(&[32, 43]).is_none());
    }

    #[test]
    fn utf8_mouse_public_api() {
        let data = [32u8, 43, 38];
        let result = try_parse_mouse_utf8(&data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    // ============================================================
    // urxvt mouse (mode 1015) tests
    // ============================================================

    #[test]
    fn urxvt_mouse_click() {
        // Button 1 press: cb=0, sent as 0+32=32; x=11 (1-based), y=6 (1-based)
        let data = b"32;11;6M";
        let result = parse_urxvt_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
        assert_eq!(result.consumed, 8);
    }

    #[test]
    fn urxvt_mouse_button2() {
        // Button 2: cb=1, sent as 33
        let data = b"33;5;10M";
        let result = parse_urxvt_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN2);
        assert_eq!(result.x, 4);
        assert_eq!(result.y, 9);
    }

    #[test]
    fn urxvt_mouse_release() {
        // Release in urxvt: cb=3 (X10-style), sent as 35
        let data = b"35;11;6M";
        let result = parse_urxvt_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEUP1);
    }

    #[test]
    fn urxvt_mouse_wheel_up() {
        // Wheel up: cb=64, sent as 96
        let data = b"96;11;6M";
        let result = parse_urxvt_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_WHEELUP);
    }

    #[test]
    fn urxvt_mouse_large_coords() {
        // Large coordinates (> 223, beyond X10 range)
        let data = b"32;500;300M";
        let result = parse_urxvt_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 499);
        assert_eq!(result.y, 299);
    }

    #[test]
    fn urxvt_mouse_drag() {
        // Drag button 1: cb=32, sent as 64
        let data = b"64;15;20M";
        let result = parse_urxvt_mouse(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDRAG1);
    }

    #[test]
    fn urxvt_mouse_incomplete() {
        assert!(parse_urxvt_mouse(b"32;11;").is_none());
        assert!(parse_urxvt_mouse(b"").is_none());
    }

    #[test]
    fn urxvt_mouse_invalid_char() {
        assert!(parse_urxvt_mouse(b"32;11;6X").is_none());
    }

    #[test]
    fn urxvt_mouse_public_api() {
        let data = b"32;11;6M";
        let result = try_parse_mouse_urxvt(data).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 10);
        assert_eq!(result.y, 5);
    }

    // ============================================================
    // decode_utf8_char tests
    // ============================================================

    #[test]
    fn decode_utf8_ascii() {
        let (cp, len) = decode_utf8_char(&[0x41]).unwrap();
        assert_eq!(cp, 0x41);
        assert_eq!(len, 1);
    }

    #[test]
    fn decode_utf8_two_byte() {
        // U+00E9 = 0xC3 0xA9
        let (cp, len) = decode_utf8_char(&[0xC3, 0xA9]).unwrap();
        assert_eq!(cp, 0xE9);
        assert_eq!(len, 2);
    }

    #[test]
    fn decode_utf8_three_byte() {
        // U+0800 = 0xE0 0xA0 0x80
        let (cp, len) = decode_utf8_char(&[0xE0, 0xA0, 0x80]).unwrap();
        assert_eq!(cp, 0x800);
        assert_eq!(len, 3);
    }

    #[test]
    fn decode_utf8_empty() {
        assert!(decode_utf8_char(&[]).is_none());
    }

    #[test]
    fn decode_utf8_incomplete_two_byte() {
        assert!(decode_utf8_char(&[0xC3]).is_none());
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
            fn parse_utf8_mouse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..64)) {
                let _ = parse_utf8_mouse(&data);
            }

            #[test]
            fn parse_urxvt_mouse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..64)) {
                let _ = parse_urxvt_mouse(&data);
            }

            #[test]
            fn decode_utf8_char_never_panics(data in proptest::collection::vec(any::<u8>(), 0..8)) {
                let _ = decode_utf8_char(&data);
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
