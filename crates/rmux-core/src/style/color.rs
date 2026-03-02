//! Terminal color types.
//!
//! Matches tmux's color representation: default, palette (0-255), and 24-bit RGB.
//! The compact encoding uses a single `i32` with flag bits, matching tmux's `colour.c`.

/// A terminal color.
///
/// Colors can be default (terminal's own color), a 256-color palette index,
/// or a 24-bit RGB value. This matches tmux's internal color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// Terminal default color (no explicit color set).
    #[default]
    Default,
    /// 256-color palette index (0-255).
    /// 0-7: standard colors, 8-15: bright colors, 16-231: 6x6x6 cube, 232-255: grayscale.
    Palette(u8),
    /// 24-bit RGB color.
    Rgb { r: u8, g: u8, b: u8 },
}

// Flag bits matching tmux's colour.c
const COLOUR_FLAG_256: i32 = 0x0100_0000;
const COLOUR_FLAG_RGB: i32 = 0x0200_0000;

impl Color {
    /// Encode to tmux's internal i32 representation.
    ///
    /// This encoding is used in the wire protocol and grid cell storage.
    #[must_use]
    pub fn to_tmux_raw(self) -> i32 {
        match self {
            Color::Default => 8, // tmux uses 8 for default
            Color::Palette(idx) => i32::from(idx) | COLOUR_FLAG_256,
            Color::Rgb { r, g, b } => {
                (i32::from(r) << 16) | (i32::from(g) << 8) | i32::from(b) | COLOUR_FLAG_RGB
            }
        }
    }

    /// Decode from tmux's internal i32 representation.
    #[must_use]
    pub fn from_tmux_raw(raw: i32) -> Self {
        if raw & COLOUR_FLAG_RGB != 0 {
            let val = raw & 0x00FF_FFFF;
            Color::Rgb {
                r: ((val >> 16) & 0xFF) as u8,
                g: ((val >> 8) & 0xFF) as u8,
                b: (val & 0xFF) as u8,
            }
        } else if raw & COLOUR_FLAG_256 != 0 {
            Color::Palette((raw & 0xFF) as u8)
        } else if raw == 8 {
            Color::Default
        } else if (0..=7).contains(&raw) {
            // Standard color (0-7) without 256 flag
            Color::Palette(raw as u8)
        } else {
            Color::Default
        }
    }

    /// Returns true if this is the default (unset) color.
    #[must_use]
    pub fn is_default(self) -> bool {
        matches!(self, Color::Default)
    }
}

/// Named standard colors (indices 0-7).
impl Color {
    pub const BLACK: Self = Color::Palette(0);
    pub const RED: Self = Color::Palette(1);
    pub const GREEN: Self = Color::Palette(2);
    pub const YELLOW: Self = Color::Palette(3);
    pub const BLUE: Self = Color::Palette(4);
    pub const MAGENTA: Self = Color::Palette(5);
    pub const CYAN: Self = Color::Palette(6);
    pub const WHITE: Self = Color::Palette(7);
    pub const BRIGHT_BLACK: Self = Color::Palette(8);
    pub const BRIGHT_RED: Self = Color::Palette(9);
    pub const BRIGHT_GREEN: Self = Color::Palette(10);
    pub const BRIGHT_YELLOW: Self = Color::Palette(11);
    pub const BRIGHT_BLUE: Self = Color::Palette(12);
    pub const BRIGHT_MAGENTA: Self = Color::Palette(13);
    pub const BRIGHT_CYAN: Self = Color::Palette(14);
    pub const BRIGHT_WHITE: Self = Color::Palette(15);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_color_roundtrip() {
        let c = Color::Default;
        assert_eq!(Color::from_tmux_raw(c.to_tmux_raw()), c);
    }

    #[test]
    fn palette_color_roundtrip() {
        for i in 0..=255u8 {
            let c = Color::Palette(i);
            let raw = c.to_tmux_raw();
            assert_eq!(Color::from_tmux_raw(raw), c, "palette {i} failed roundtrip");
        }
    }

    #[test]
    fn rgb_color_roundtrip() {
        let c = Color::Rgb { r: 0xAB, g: 0xCD, b: 0xEF };
        assert_eq!(Color::from_tmux_raw(c.to_tmux_raw()), c);
    }

    #[test]
    fn rgb_extremes() {
        let black = Color::Rgb { r: 0, g: 0, b: 0 };
        let white = Color::Rgb { r: 255, g: 255, b: 255 };
        assert_eq!(Color::from_tmux_raw(black.to_tmux_raw()), black);
        assert_eq!(Color::from_tmux_raw(white.to_tmux_raw()), white);
    }
}
