//! Terminal style types: colors, attributes, and combined styles.
//!
//! A `Style` combines foreground/background colors, underline color, and text attributes.
//! This module also re-exports [`Color`] and [`Attrs`] for convenience.

pub mod attrs;
pub mod color;

pub use attrs::Attrs;
pub use color::Color;

/// A complete terminal style combining colors and attributes.
///
/// Matches the information stored in tmux's `struct grid_cell` minus the character data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Style {
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Underline color (separate from underline attribute).
    pub us: Color,
    /// Text attributes (bold, italic, etc.).
    pub attrs: Attrs,
}

impl Style {
    /// A completely default style (default colors, no attributes).
    pub const DEFAULT: Self = Self {
        fg: Color::Default,
        bg: Color::Default,
        us: Color::Default,
        attrs: Attrs::empty(),
    };

    /// Returns true if this style has no colors set and no attributes.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.fg.is_default()
            && self.bg.is_default()
            && self.us.is_default()
            && self.attrs.is_empty()
    }

    /// Returns true if two styles look the same visually (ignoring non-visual flags).
    #[must_use]
    pub fn looks_equal(&self, other: &Self) -> bool {
        self.fg == other.fg && self.bg == other.bg && self.us == other.us && self.attrs == other.attrs
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_style_is_default() {
        assert!(Style::default().is_default());
    }

    #[test]
    fn style_with_fg_not_default() {
        let s = Style {
            fg: Color::RED,
            ..Style::DEFAULT
        };
        assert!(!s.is_default());
    }

    #[test]
    fn looks_equal_works() {
        let s1 = Style {
            fg: Color::Rgb { r: 1, g: 2, b: 3 },
            attrs: Attrs::BOLD,
            ..Style::DEFAULT
        };
        let s2 = s1;
        assert!(s1.looks_equal(&s2));
    }
}
