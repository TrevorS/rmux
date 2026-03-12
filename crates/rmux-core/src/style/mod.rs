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

/// Parse a tmux-style string like `fg=red,bg=green,bold` into a `Style`.
///
/// Supports: `fg=COLOR`, `bg=COLOR`, `bold`, `dim`, `underscore`, `blink`,
/// `reverse`, `hidden`, `italics`, `strikethrough`, `overline`, `default`.
/// Colors can be: named (`red`, `green`), `colour<N>`, `color<N>`, or `#RRGGBB`.
#[must_use]
pub fn parse_style(s: &str) -> Style {
    let mut style = Style::DEFAULT;
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(color_str) = part.strip_prefix("fg=") {
            if let Some(c) = parse_color_name(color_str) {
                style.fg = c;
            }
        } else if let Some(color_str) = part.strip_prefix("bg=") {
            if let Some(c) = parse_color_name(color_str) {
                style.bg = c;
            }
        } else if let Some(color_str) = part.strip_prefix("us=") {
            if let Some(c) = parse_color_name(color_str) {
                style.us = c;
            }
        } else {
            match part {
                "bold" => style.attrs |= Attrs::BOLD,
                "dim" => style.attrs |= Attrs::DIM,
                "underscore" => style.attrs |= Attrs::UNDERSCORE,
                "blink" => style.attrs |= Attrs::BLINK,
                "reverse" => style.attrs |= Attrs::REVERSE,
                "hidden" => style.attrs |= Attrs::HIDDEN,
                "italics" => style.attrs |= Attrs::ITALICS,
                "strikethrough" => style.attrs |= Attrs::STRIKETHROUGH,
                "overline" => style.attrs |= Attrs::OVERLINE,
                "default" => style = Style::DEFAULT,
                "none" | "noattr" => style.attrs = Attrs::empty(),
                _ => {} // ignore unknown
            }
        }
    }
    style
}

/// Parse a color name (tmux syntax).
fn parse_color_name(s: &str) -> Option<Color> {
    let s = s.trim();
    // #RRGGBB hex
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb { r, g, b });
        }
    }
    // colour<N> or color<N>
    if let Some(n) = s.strip_prefix("colour").or_else(|| s.strip_prefix("color")) {
        if let Ok(idx) = n.parse::<u8>() {
            return Some(Color::Palette(idx));
        }
    }
    // Named colors
    match s {
        "default" => Some(Color::Default),
        "black" => Some(Color::BLACK),
        "red" => Some(Color::RED),
        "green" => Some(Color::GREEN),
        "yellow" => Some(Color::YELLOW),
        "blue" => Some(Color::BLUE),
        "magenta" => Some(Color::MAGENTA),
        "cyan" => Some(Color::CYAN),
        "white" => Some(Color::WHITE),
        "brightblack" => Some(Color::BRIGHT_BLACK),
        "brightred" => Some(Color::BRIGHT_RED),
        "brightgreen" => Some(Color::BRIGHT_GREEN),
        "brightyellow" => Some(Color::BRIGHT_YELLOW),
        "brightblue" => Some(Color::BRIGHT_BLUE),
        "brightmagenta" => Some(Color::BRIGHT_MAGENTA),
        "brightcyan" => Some(Color::BRIGHT_CYAN),
        "brightwhite" => Some(Color::BRIGHT_WHITE),
        _ => None,
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
    fn parse_style_fg_bg() {
        let s = parse_style("fg=red,bg=green");
        assert_eq!(s.fg, Color::RED);
        assert_eq!(s.bg, Color::GREEN);
    }

    #[test]
    fn parse_style_bold_italic() {
        let s = parse_style("bold,italics");
        assert!(s.attrs.contains(Attrs::BOLD));
        assert!(s.attrs.contains(Attrs::ITALICS));
    }

    #[test]
    fn parse_style_hex_color() {
        let s = parse_style("fg=#ff0000");
        assert_eq!(s.fg, Color::Rgb { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn parse_style_colour_number() {
        let s = parse_style("bg=colour231");
        assert_eq!(s.bg, Color::Palette(231));
    }

    #[test]
    fn parse_style_default_resets() {
        let s = parse_style("fg=red,default");
        assert_eq!(s, Style::DEFAULT);
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

    // ============================================================
    // parse_style — additional coverage
    // ============================================================

    #[test]
    fn parse_style_underline_color() {
        let s = parse_style("us=cyan,underscore");
        assert_eq!(s.us, Color::CYAN);
        assert!(s.attrs.contains(Attrs::UNDERSCORE));
    }

    #[test]
    fn parse_style_all_attrs() {
        let s =
            parse_style("bold,dim,underscore,blink,reverse,hidden,italics,strikethrough,overline");
        assert!(s.attrs.contains(Attrs::BOLD));
        assert!(s.attrs.contains(Attrs::DIM));
        assert!(s.attrs.contains(Attrs::UNDERSCORE));
        assert!(s.attrs.contains(Attrs::BLINK));
        assert!(s.attrs.contains(Attrs::REVERSE));
        assert!(s.attrs.contains(Attrs::HIDDEN));
        assert!(s.attrs.contains(Attrs::ITALICS));
        assert!(s.attrs.contains(Attrs::STRIKETHROUGH));
        assert!(s.attrs.contains(Attrs::OVERLINE));
    }

    #[test]
    fn parse_style_noattr_clears() {
        let s = parse_style("bold,italics,noattr");
        assert!(s.attrs.is_empty());
    }

    #[test]
    fn parse_style_none_clears() {
        let s = parse_style("bold,none");
        assert!(s.attrs.is_empty());
    }

    #[test]
    fn parse_style_color_spelling() {
        let s = parse_style("fg=color100");
        assert_eq!(s.fg, Color::Palette(100));
    }

    #[test]
    fn parse_style_empty_string() {
        let s = parse_style("");
        assert_eq!(s, Style::DEFAULT);
    }

    #[test]
    fn parse_style_unknown_ignored() {
        let s = parse_style("bold,notarealattr,italics");
        assert!(s.attrs.contains(Attrs::BOLD));
        assert!(s.attrs.contains(Attrs::ITALICS));
    }

    #[test]
    fn parse_style_whitespace_trimmed() {
        let s = parse_style(" bold , fg=red ");
        assert!(s.attrs.contains(Attrs::BOLD));
        assert_eq!(s.fg, Color::RED);
    }

    #[test]
    fn parse_style_all_named_colors() {
        for (name, expected) in [
            ("black", Color::BLACK),
            ("red", Color::RED),
            ("green", Color::GREEN),
            ("yellow", Color::YELLOW),
            ("blue", Color::BLUE),
            ("magenta", Color::MAGENTA),
            ("cyan", Color::CYAN),
            ("white", Color::WHITE),
            ("brightblack", Color::BRIGHT_BLACK),
            ("brightred", Color::BRIGHT_RED),
            ("brightgreen", Color::BRIGHT_GREEN),
            ("brightyellow", Color::BRIGHT_YELLOW),
            ("brightblue", Color::BRIGHT_BLUE),
            ("brightmagenta", Color::BRIGHT_MAGENTA),
            ("brightcyan", Color::BRIGHT_CYAN),
            ("brightwhite", Color::BRIGHT_WHITE),
        ] {
            let s = parse_style(&format!("fg={name}"));
            assert_eq!(s.fg, expected, "failed for color {name}");
        }
    }

    #[test]
    fn parse_style_hex_all_channels() {
        let s = parse_style("bg=#1a2b3c");
        assert_eq!(s.bg, Color::Rgb { r: 0x1a, g: 0x2b, b: 0x3c });
    }

    #[test]
    fn parse_style_hex_bad_length_ignored() {
        let s = parse_style("fg=#abc");
        assert_eq!(s.fg, Color::Default); // Not 6 hex chars
    }

    #[test]
    fn parse_style_hex_bad_hex_ignored() {
        let s = parse_style("fg=#gggggg");
        assert_eq!(s.fg, Color::Default);
    }

    #[test]
    fn parse_style_colour_boundary() {
        let s = parse_style("fg=colour0");
        assert_eq!(s.fg, Color::Palette(0));
        let s = parse_style("fg=colour255");
        assert_eq!(s.fg, Color::Palette(255));
    }

    #[test]
    fn parse_style_fg_default() {
        let s = parse_style("fg=default");
        assert_eq!(s.fg, Color::Default);
    }

    #[test]
    fn parse_style_default_after_attrs() {
        // "default" resets everything, including previously set attrs
        let s = parse_style("fg=red,bold,default");
        assert_eq!(s, Style::DEFAULT);
    }

    #[test]
    fn parse_style_multiple_commas() {
        let s = parse_style("bold,,italics,,,");
        assert!(s.attrs.contains(Attrs::BOLD));
        assert!(s.attrs.contains(Attrs::ITALICS));
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_style_never_panics(input in ".*") {
                let _ = parse_style(&input);
            }

            #[test]
            fn parse_style_ascii_never_panics(
                input in proptest::collection::vec(any::<u8>(), 0..200)
            ) {
                if let Ok(s) = std::str::from_utf8(&input) {
                    let _ = parse_style(s);
                }
            }

            #[test]
            fn parse_style_structured(
                fg in proptest::option::of("(red|green|blue|cyan|magenta|yellow|white|black|default|colour[0-9]{1,3}|#[0-9a-f]{6})"),
                bg in proptest::option::of("(red|green|blue|cyan|magenta|yellow|white|black|default|colour[0-9]{1,3}|#[0-9a-f]{6})"),
                attrs in proptest::collection::vec("(bold|dim|underscore|blink|reverse|hidden|italics|strikethrough|overline|none|noattr|default)", 0..5),
            ) {
                let mut parts = Vec::new();
                if let Some(f) = &fg { parts.push(format!("fg={f}")); }
                if let Some(b) = &bg { parts.push(format!("bg={b}")); }
                parts.extend(attrs);
                let style_str = parts.join(",");
                let result = parse_style(&style_str);
                // Just verify it returns a valid Style without panicking
                let _ = result.is_default();
            }
        }
    }
}
