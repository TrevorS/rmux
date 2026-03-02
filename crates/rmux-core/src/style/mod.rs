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
    pub const DEFAULT: Self =
        Self { fg: Color::Default, bg: Color::Default, us: Color::Default, attrs: Attrs::empty() };

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
        self.fg == other.fg
            && self.bg == other.bg
            && self.us == other.us
            && self.attrs == other.attrs
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
        let s = Style { fg: Color::RED, ..Style::DEFAULT };
        assert!(!s.is_default());
    }

    #[test]
    fn looks_equal_works() {
        let s1 =
            Style { fg: Color::Rgb { r: 1, g: 2, b: 3 }, attrs: Attrs::BOLD, ..Style::DEFAULT };
        let s2 = s1;
        assert!(s1.looks_equal(&s2));
    }

    #[test]
    fn style_with_bg_not_default() {
        let s = Style { bg: Color::BLUE, ..Style::DEFAULT };
        assert!(!s.is_default());
    }

    #[test]
    fn style_with_underline_color() {
        let s = Style { us: Color::RED, ..Style::DEFAULT };
        assert!(!s.is_default());
    }

    #[test]
    fn style_with_attrs_not_default() {
        let s = Style { attrs: Attrs::BOLD, ..Style::DEFAULT };
        assert!(!s.is_default());

        let s2 = Style { attrs: Attrs::ITALICS, ..Style::DEFAULT };
        assert!(!s2.is_default());

        let s3 = Style { attrs: Attrs::UNDERSCORE, ..Style::DEFAULT };
        assert!(!s3.is_default());
    }

    #[test]
    fn looks_equal_ignores_underline_color_when_no_underline() {
        // Two styles that differ only in underline color but have no underline attr.
        // The current looks_equal compares all fields including us, so they will
        // compare as not-equal even without underline. This documents the current behavior.
        let s1 = Style { us: Color::RED, ..Style::DEFAULT };
        let s2 = Style { us: Color::BLUE, ..Style::DEFAULT };
        // They differ in us, and looks_equal checks us unconditionally.
        assert!(!s1.looks_equal(&s2));
        // But if the underline colors are the same, they should look equal.
        let s3 = Style { us: Color::RED, ..Style::DEFAULT };
        assert!(s1.looks_equal(&s3));
    }

    #[test]
    fn looks_equal_detects_underline_color_diff() {
        // Two styles with UNDERSCORE attr and different underline colors should not look equal.
        let s1 = Style { attrs: Attrs::UNDERSCORE, us: Color::RED, ..Style::DEFAULT };
        let s2 = Style { attrs: Attrs::UNDERSCORE, us: Color::GREEN, ..Style::DEFAULT };
        assert!(!s1.looks_equal(&s2));

        // Same underline color with same attrs should look equal.
        let s3 = Style { attrs: Attrs::UNDERSCORE, us: Color::RED, ..Style::DEFAULT };
        assert!(s1.looks_equal(&s3));
    }
}
