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
}
