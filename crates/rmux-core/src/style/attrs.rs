//! Terminal text attributes.
//!
//! Matches tmux's attribute bitflags from `tmux.h`.

use bitflags::bitflags;

bitflags! {
    /// Text attributes (bold, dim, italic, etc.).
    ///
    /// These match tmux's `GRID_ATTR_*` constants and the SGR parameter codes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Attrs: u16 {
        /// Bold (SGR 1).
        const BOLD           = 0x0001;
        /// Dim/faint (SGR 2).
        const DIM            = 0x0002;
        /// Underline (SGR 4).
        const UNDERSCORE     = 0x0004;
        /// Blink (SGR 5).
        const BLINK          = 0x0008;
        /// Reverse video (SGR 7).
        const REVERSE        = 0x0010;
        /// Hidden/invisible (SGR 8).
        const HIDDEN         = 0x0020;
        /// Italic (SGR 3).
        const ITALICS        = 0x0040;
        /// Strikethrough (SGR 9).
        const STRIKETHROUGH  = 0x0080;
        /// Double underline (SGR 21).
        const DOUBLE_UNDERSCORE = 0x0100;
        /// Curly underline (SGR 4:3).
        const CURLY_UNDERSCORE  = 0x0200;
        /// Dotted underline (SGR 4:4).
        const DOTTED_UNDERSCORE = 0x0400;
        /// Dashed underline (SGR 4:5).
        const DASHED_UNDERSCORE = 0x0800;
        /// Overline (SGR 53).
        const OVERLINE       = 0x1000;
        /// Character set flag (for ACS drawing characters).
        const CHARSET        = 0x2000;
    }
}

impl Attrs {
    /// All underline styles combined.
    pub const ALL_UNDERLINES: Self = Self::from_bits_truncate(
        Self::UNDERSCORE.bits()
            | Self::DOUBLE_UNDERSCORE.bits()
            | Self::CURLY_UNDERSCORE.bits()
            | Self::DOTTED_UNDERSCORE.bits()
            | Self::DASHED_UNDERSCORE.bits(),
    );

    /// Check if any underline style is set.
    #[must_use]
    pub fn has_any_underline(self) -> bool {
        self.intersects(Self::ALL_UNDERLINES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert!(Attrs::default().is_empty());
    }

    #[test]
    fn combine_attrs() {
        let bold_italic = Attrs::BOLD | Attrs::ITALICS;
        assert!(bold_italic.contains(Attrs::BOLD));
        assert!(bold_italic.contains(Attrs::ITALICS));
        assert!(!bold_italic.contains(Attrs::DIM));
    }

    #[test]
    fn underline_detection() {
        assert!(Attrs::UNDERSCORE.has_any_underline());
        assert!(Attrs::CURLY_UNDERSCORE.has_any_underline());
        assert!(!Attrs::BOLD.has_any_underline());
    }

    #[test]
    fn all_individual_attrs() {
        // Test each of the 14 attribute flags individually.
        let all_attrs = [
            Attrs::BOLD,
            Attrs::DIM,
            Attrs::UNDERSCORE,
            Attrs::BLINK,
            Attrs::REVERSE,
            Attrs::HIDDEN,
            Attrs::ITALICS,
            Attrs::STRIKETHROUGH,
            Attrs::DOUBLE_UNDERSCORE,
            Attrs::CURLY_UNDERSCORE,
            Attrs::DOTTED_UNDERSCORE,
            Attrs::DASHED_UNDERSCORE,
            Attrs::OVERLINE,
            Attrs::CHARSET,
        ];

        for (i, &attr) in all_attrs.iter().enumerate() {
            // Each flag should not be empty.
            assert!(!attr.is_empty(), "Attr at index {i} should not be empty");

            // Each flag should be distinct from all others.
            for (j, &other) in all_attrs.iter().enumerate() {
                if i != j {
                    assert_ne!(attr, other, "Attr at index {i} should differ from index {j}");
                    // They should not overlap (no shared bits).
                    assert!(
                        (attr & other).is_empty(),
                        "Attr at index {i} should not share bits with index {j}"
                    );
                }
            }

            // Setting and checking the flag works.
            let combined = Attrs::empty() | attr;
            assert!(combined.contains(attr));
        }
    }

    #[test]
    fn all_underlines_combined() {
        // ALL_UNDERLINES should contain all underline variants.
        assert!(Attrs::ALL_UNDERLINES.contains(Attrs::UNDERSCORE));
        assert!(Attrs::ALL_UNDERLINES.contains(Attrs::DOUBLE_UNDERSCORE));
        assert!(Attrs::ALL_UNDERLINES.contains(Attrs::CURLY_UNDERSCORE));
        assert!(Attrs::ALL_UNDERLINES.contains(Attrs::DOTTED_UNDERSCORE));
        assert!(Attrs::ALL_UNDERLINES.contains(Attrs::DASHED_UNDERSCORE));

        // ALL_UNDERLINES should NOT contain non-underline attrs.
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::BOLD));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::DIM));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::BLINK));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::REVERSE));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::HIDDEN));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::ITALICS));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::STRIKETHROUGH));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::OVERLINE));
        assert!(!Attrs::ALL_UNDERLINES.contains(Attrs::CHARSET));
    }

    #[test]
    fn has_any_underline_detection() {
        // Each underline variant should trigger has_any_underline.
        let underline_variants = [
            Attrs::UNDERSCORE,
            Attrs::DOUBLE_UNDERSCORE,
            Attrs::CURLY_UNDERSCORE,
            Attrs::DOTTED_UNDERSCORE,
            Attrs::DASHED_UNDERSCORE,
        ];
        for &ul in &underline_variants {
            assert!(ul.has_any_underline(), "{ul:?} should have underline");
            // Combined with other attrs should still detect underline.
            let combined = ul | Attrs::BOLD | Attrs::ITALICS;
            assert!(combined.has_any_underline(), "{ul:?} | BOLD | ITALICS should have underline");
        }

        // Non-underline attrs should NOT trigger has_any_underline.
        let non_underline = [
            Attrs::BOLD,
            Attrs::DIM,
            Attrs::BLINK,
            Attrs::REVERSE,
            Attrs::HIDDEN,
            Attrs::ITALICS,
            Attrs::STRIKETHROUGH,
            Attrs::OVERLINE,
            Attrs::CHARSET,
        ];
        for &attr in &non_underline {
            assert!(!attr.has_any_underline(), "{attr:?} should not have underline");
        }

        // Empty attrs should not have underline.
        assert!(!Attrs::empty().has_any_underline());

        // All non-underline combined should not have underline.
        let mut combined = Attrs::empty();
        for &attr in &non_underline {
            combined |= attr;
        }
        assert!(!combined.has_any_underline());
    }
}
