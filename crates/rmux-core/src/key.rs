//! Key codes, modifiers, and mouse events.
//!
//! Matches tmux's key_code type and modifier flags.

use bitflags::bitflags;

/// A key code representing a keyboard or mouse input event.
///
/// This is a u64 matching tmux's `key_code` type, with the actual key value
/// in the lower bits and modifier flags in the upper bits.
pub type KeyCode = u64;

// Key code ranges matching tmux
/// First special key code.
pub const KEYC_UNKNOWN: KeyCode = 0x000ff000;

// Named keys (matching tmux's enum)
pub const KEYC_NONE: KeyCode = KEYC_UNKNOWN;
pub const KEYC_BACKSPACE: KeyCode = 0x00100000;
pub const KEYC_TAB: KeyCode = 0x00100001;
pub const KEYC_RETURN: KeyCode = 0x00100002;
pub const KEYC_ESCAPE: KeyCode = 0x00100003;
pub const KEYC_SPACE: KeyCode = 0x00100004;
pub const KEYC_DELETE: KeyCode = 0x00100005;

pub const KEYC_UP: KeyCode = 0x00100010;
pub const KEYC_DOWN: KeyCode = 0x00100011;
pub const KEYC_LEFT: KeyCode = 0x00100012;
pub const KEYC_RIGHT: KeyCode = 0x00100013;
pub const KEYC_HOME: KeyCode = 0x00100014;
pub const KEYC_END: KeyCode = 0x00100015;
pub const KEYC_INSERT: KeyCode = 0x00100016;
pub const KEYC_PPAGE: KeyCode = 0x00100017;
pub const KEYC_NPAGE: KeyCode = 0x00100018;

pub const KEYC_F1: KeyCode = 0x00100020;
pub const KEYC_F2: KeyCode = 0x00100021;
pub const KEYC_F3: KeyCode = 0x00100022;
pub const KEYC_F4: KeyCode = 0x00100023;
pub const KEYC_F5: KeyCode = 0x00100024;
pub const KEYC_F6: KeyCode = 0x00100025;
pub const KEYC_F7: KeyCode = 0x00100026;
pub const KEYC_F8: KeyCode = 0x00100027;
pub const KEYC_F9: KeyCode = 0x00100028;
pub const KEYC_F10: KeyCode = 0x00100029;
pub const KEYC_F11: KeyCode = 0x0010002a;
pub const KEYC_F12: KeyCode = 0x0010002b;

// Mouse button keys
pub const KEYC_MOUSE: KeyCode = 0x00100040;
pub const KEYC_MOUSEDOWN1: KeyCode = 0x00100041;
pub const KEYC_MOUSEDOWN2: KeyCode = 0x00100042;
pub const KEYC_MOUSEDOWN3: KeyCode = 0x00100043;
pub const KEYC_MOUSEUP1: KeyCode = 0x00100044;
pub const KEYC_MOUSEUP2: KeyCode = 0x00100045;
pub const KEYC_MOUSEUP3: KeyCode = 0x00100046;
pub const KEYC_MOUSEDRAG1: KeyCode = 0x00100047;
pub const KEYC_MOUSEDRAG2: KeyCode = 0x00100048;
pub const KEYC_MOUSEDRAG3: KeyCode = 0x00100049;
pub const KEYC_WHEELUP: KeyCode = 0x0010004a;
pub const KEYC_WHEELDOWN: KeyCode = 0x0010004b;

bitflags! {
    /// Key modifier flags (stored in upper bits of key_code).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct KeyModifiers: u64 {
        /// Shift modifier.
        const SHIFT   = 0x0200_0000_0000_0000;
        /// Meta/Alt modifier.
        const META    = 0x0400_0000_0000_0000;
        /// Control modifier.
        const CTRL    = 0x0800_0000_0000_0000;
        /// Extended key (kitty keyboard protocol).
        const XTERM   = 0x1000_0000_0000_0000;
    }
}

/// Mask to extract the base key from a key code.
pub const KEYC_MASK_KEY: KeyCode = 0x000f_ffff_ffff_ffff;
/// Mask to extract modifiers from a key code.
pub const KEYC_MASK_MODIFIERS: KeyCode = 0xff00_0000_0000_0000;

/// Extract the base key (without modifiers) from a key code.
#[must_use]
pub fn keyc_base(key: KeyCode) -> KeyCode {
    key & KEYC_MASK_KEY
}

/// Extract the modifiers from a key code.
#[must_use]
pub fn keyc_modifiers(key: KeyCode) -> KeyModifiers {
    KeyModifiers::from_bits_truncate(key & KEYC_MASK_MODIFIERS)
}

/// Build a key code from a base key and modifiers.
#[must_use]
pub fn keyc_build(base: KeyCode, modifiers: KeyModifiers) -> KeyCode {
    (base & KEYC_MASK_KEY) | modifiers.bits()
}

/// Returns true if the key code represents a mouse event.
#[must_use]
pub fn keyc_is_mouse(key: KeyCode) -> bool {
    let base = keyc_base(key);
    (KEYC_MOUSE..=KEYC_WHEELDOWN).contains(&base)
}

/// Mouse event data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseEvent {
    /// Whether this is a valid mouse event.
    pub valid: bool,
    /// Mouse position (column).
    pub x: u32,
    /// Mouse position (row).
    pub y: u32,
    /// Button state.
    pub button: u32,
    /// Previous mouse position (column).
    pub last_x: u32,
    /// Previous mouse position (row).
    pub last_y: u32,
    /// Previous button state.
    pub last_button: u32,
    /// SGR mode type.
    pub sgr_type: u32,
    /// SGR button.
    pub sgr_button: u32,
}

/// Key event combining a key code with optional mouse data and raw bytes.
#[derive(Debug, Clone, Default)]
pub struct KeyEvent {
    /// The key code (with modifiers).
    pub key: KeyCode,
    /// Mouse event data (if this is a mouse event).
    pub mouse: MouseEvent,
    /// Raw bytes that produced this key event.
    pub raw: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_code_roundtrip() {
        let key = keyc_build(KEYC_F5, KeyModifiers::CTRL | KeyModifiers::SHIFT);
        assert_eq!(keyc_base(key), KEYC_F5);
        assert_eq!(keyc_modifiers(key), KeyModifiers::CTRL | KeyModifiers::SHIFT);
    }

    #[test]
    fn mouse_detection() {
        assert!(keyc_is_mouse(KEYC_MOUSEDOWN1));
        assert!(keyc_is_mouse(KEYC_WHEELUP));
        assert!(!keyc_is_mouse(KEYC_F1));
        assert!(!keyc_is_mouse(b'a'.into()));
    }

    #[test]
    fn ascii_key() {
        let key: KeyCode = b'a'.into();
        assert_eq!(keyc_base(key), b'a'.into());
        assert_eq!(keyc_modifiers(key), KeyModifiers::empty());
    }

    #[test]
    fn all_modifier_combinations() {
        // Test each modifier individually.
        let base = KEYC_F1;
        for modifier in
            [KeyModifiers::SHIFT, KeyModifiers::META, KeyModifiers::CTRL, KeyModifiers::XTERM]
        {
            let key = keyc_build(base, modifier);
            assert_eq!(keyc_base(key), base);
            assert_eq!(keyc_modifiers(key), modifier);
        }

        // Test combined modifiers.
        let all =
            KeyModifiers::SHIFT | KeyModifiers::META | KeyModifiers::CTRL | KeyModifiers::XTERM;
        let key = keyc_build(base, all);
        assert_eq!(keyc_base(key), base);
        assert_eq!(keyc_modifiers(key), all);

        // Test a pair.
        let pair = KeyModifiers::CTRL | KeyModifiers::SHIFT;
        let key = keyc_build(base, pair);
        assert_eq!(keyc_modifiers(key), pair);
        assert!(keyc_modifiers(key).contains(KeyModifiers::CTRL));
        assert!(keyc_modifiers(key).contains(KeyModifiers::SHIFT));
        assert!(!keyc_modifiers(key).contains(KeyModifiers::META));
    }

    #[test]
    fn function_keys_defined() {
        let fkeys = [
            KEYC_F1, KEYC_F2, KEYC_F3, KEYC_F4, KEYC_F5, KEYC_F6, KEYC_F7, KEYC_F8, KEYC_F9,
            KEYC_F10, KEYC_F11, KEYC_F12,
        ];
        // All function keys should be distinct.
        for i in 0..fkeys.len() {
            for j in (i + 1)..fkeys.len() {
                assert_ne!(fkeys[i], fkeys[j], "F{} and F{} should be distinct", i + 1, j + 1);
            }
        }
        // They should all be in the special key range.
        for (i, &fk) in fkeys.iter().enumerate() {
            assert!(fk > KEYC_UNKNOWN, "F{} should be above KEYC_UNKNOWN", i + 1);
        }
    }

    #[test]
    fn arrow_keys_defined() {
        let arrows = [KEYC_UP, KEYC_DOWN, KEYC_LEFT, KEYC_RIGHT];
        // All arrow keys should be distinct.
        for i in 0..arrows.len() {
            for j in (i + 1)..arrows.len() {
                assert_ne!(arrows[i], arrows[j], "Arrow keys at indices {i} and {j} should differ");
            }
        }
        // They should be different from each other by specific constant values.
        assert_eq!(KEYC_UP, 0x00100010);
        assert_eq!(KEYC_DOWN, 0x00100011);
        assert_eq!(KEYC_LEFT, 0x00100012);
        assert_eq!(KEYC_RIGHT, 0x00100013);
    }

    #[test]
    fn mouse_key_detection_comprehensive() {
        // All mouse keys should be detected as mouse events.
        let mouse_keys = [
            KEYC_MOUSEDOWN1,
            KEYC_MOUSEDOWN2,
            KEYC_MOUSEDOWN3,
            KEYC_MOUSEUP1,
            KEYC_MOUSEUP2,
            KEYC_MOUSEUP3,
            KEYC_MOUSEDRAG1,
            KEYC_MOUSEDRAG2,
            KEYC_MOUSEDRAG3,
            KEYC_WHEELUP,
            KEYC_WHEELDOWN,
        ];
        for &mk in &mouse_keys {
            assert!(keyc_is_mouse(mk), "Mouse key {mk:#x} should be detected as mouse");
        }
        // Also test KEYC_MOUSE itself.
        assert!(keyc_is_mouse(KEYC_MOUSE));

        // Mouse keys with modifiers should still be detected.
        let with_ctrl = keyc_build(KEYC_MOUSEDOWN1, KeyModifiers::CTRL);
        assert!(keyc_is_mouse(with_ctrl));
        let with_shift_meta = keyc_build(KEYC_WHEELUP, KeyModifiers::SHIFT | KeyModifiers::META);
        assert!(keyc_is_mouse(with_shift_meta));
    }

    #[test]
    fn non_mouse_keys_not_mouse() {
        // Regular ASCII keys.
        assert!(!keyc_is_mouse(b'a'.into()));
        assert!(!keyc_is_mouse(b'Z'.into()));
        assert!(!keyc_is_mouse(b' '.into()));

        // Function keys.
        assert!(!keyc_is_mouse(KEYC_F1));
        assert!(!keyc_is_mouse(KEYC_F12));

        // Navigation keys.
        assert!(!keyc_is_mouse(KEYC_UP));
        assert!(!keyc_is_mouse(KEYC_DOWN));
        assert!(!keyc_is_mouse(KEYC_LEFT));
        assert!(!keyc_is_mouse(KEYC_RIGHT));
        assert!(!keyc_is_mouse(KEYC_HOME));
        assert!(!keyc_is_mouse(KEYC_END));
        assert!(!keyc_is_mouse(KEYC_PPAGE));
        assert!(!keyc_is_mouse(KEYC_NPAGE));

        // Special keys.
        assert!(!keyc_is_mouse(KEYC_BACKSPACE));
        assert!(!keyc_is_mouse(KEYC_TAB));
        assert!(!keyc_is_mouse(KEYC_RETURN));
        assert!(!keyc_is_mouse(KEYC_ESCAPE));
        assert!(!keyc_is_mouse(KEYC_DELETE));
        assert!(!keyc_is_mouse(KEYC_INSERT));
    }

    #[test]
    fn key_event_fields() {
        let event = KeyEvent {
            key: keyc_build(KEYC_F5, KeyModifiers::CTRL),
            mouse: MouseEvent {
                valid: false,
                x: 10,
                y: 20,
                button: 0,
                last_x: 0,
                last_y: 0,
                last_button: 0,
                sgr_type: 0,
                sgr_button: 0,
            },
            raw: vec![0x1b, 0x5b, 0x31, 0x35, 0x7e],
        };
        assert_eq!(keyc_base(event.key), KEYC_F5);
        assert_eq!(keyc_modifiers(event.key), KeyModifiers::CTRL);
        assert!(!event.mouse.valid);
        assert_eq!(event.mouse.x, 10);
        assert_eq!(event.mouse.y, 20);
        assert_eq!(event.raw.len(), 5);

        // Test Default construction.
        let default_event = KeyEvent::default();
        assert_eq!(default_event.key, 0);
        assert!(default_event.raw.is_empty());
        assert_eq!(default_event.mouse, MouseEvent::default());
    }
}
