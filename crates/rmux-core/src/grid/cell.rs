//! Grid cell types: compact and extended storage.
//!
//! Most terminal cells contain ASCII with default or simple 256-color styling.
//! We use a compact 8-byte representation for the common case and spill to an
//! extended representation for non-ASCII characters, RGB colors, or hyperlinks.
//!
//! This matches tmux's `grid_cell_entry` (compact) / `grid_extd_entry` (extended) split.

use crate::style::{Attrs, Color, Style};
use crate::utf8::Utf8Char;
use bitflags::bitflags;

bitflags! {
    /// Flags on a grid cell entry.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct CellFlags: u16 {
        /// Cell uses extended storage.
        const EXTENDED       = 0x0001;
        /// Cell is padding for a wide character (right half).
        const PADDING        = 0x0002;
        /// Cell is selected (copy mode).
        const SELECTED       = 0x0004;
        /// Cell has been explicitly cleared.
        const CLEARED        = 0x0008;
        /// FG is a 256-color index (in compact cell).
        const FG256          = 0x0010;
        /// BG is a 256-color index (in compact cell).
        const BG256          = 0x0020;
        /// Cell is a tab.
        const TAB            = 0x0040;
    }
}

/// Compact cell entry - 8 bytes.
///
/// Used for ASCII characters with 256-color or default styling.
/// When `flags` has `EXTENDED` set, `data` is an index into the line's extended cell array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct CompactCell {
    /// ASCII byte, or index into extended array when EXTENDED flag is set.
    pub data: u8,
    /// Packed attributes (maps to lower 8 bits of Attrs).
    pub attrs: u8,
    /// Foreground color byte.
    pub fg: u8,
    /// Background color byte.
    pub bg: u8,
    /// Cell flags.
    pub flags: CellFlags,
    /// Padding for alignment.
    _pad: [u8; 2],
}

impl CompactCell {
    /// Create a cleared/empty compact cell.
    pub const CLEARED: Self = Self {
        data: b' ',
        attrs: 0,
        fg: 8, // default
        bg: 8, // default
        flags: CellFlags::CLEARED,
        _pad: [0; 2],
    };

    /// Whether this cell uses extended storage.
    #[must_use]
    pub fn is_extended(&self) -> bool {
        self.flags.contains(CellFlags::EXTENDED)
    }

    /// Get the extended cell index (only valid when `is_extended()` is true).
    #[must_use]
    pub fn extended_index(&self) -> usize {
        debug_assert!(self.is_extended());
        self.data as usize
    }
}

impl Default for CompactCell {
    fn default() -> Self {
        Self::CLEARED
    }
}

/// Extended cell entry for non-ASCII, RGB colors, or hyperlinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedCell {
    /// UTF-8 character data.
    pub data: Utf8Char,
    /// Full style (fg, bg, underline color, attrs).
    pub style: Style,
    /// Hyperlink ID (0 = no hyperlink).
    pub link: u32,
    /// Cell flags.
    pub flags: CellFlags,
}

impl Default for ExtendedCell {
    fn default() -> Self {
        Self { data: Utf8Char::SPACE, style: Style::DEFAULT, link: 0, flags: CellFlags::empty() }
    }
}

/// A resolved grid cell - the public interface for reading cell contents.
///
/// This is constructed by resolving a `CompactCell` (possibly with its `ExtendedCell`).
/// It's analogous to tmux's `struct grid_cell`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCell {
    /// Character data.
    pub data: Utf8Char,
    /// Full style.
    pub style: Style,
    /// Hyperlink ID.
    pub link: u32,
    /// Cell flags.
    pub flags: CellFlags,
}

impl GridCell {
    /// A cleared cell (space with default style).
    pub const CLEARED: Self =
        Self { data: Utf8Char::SPACE, style: Style::DEFAULT, link: 0, flags: CellFlags::CLEARED };

    /// Whether this cell is a padding cell (right half of a wide character).
    #[must_use]
    pub fn is_padding(&self) -> bool {
        self.flags.contains(CellFlags::PADDING)
    }

    /// The display width of this cell (0, 1, or 2).
    #[must_use]
    pub fn width(&self) -> u8 {
        if self.is_padding() { 0 } else { self.data.width() }
    }

    /// Pack this cell into compact form if possible, or return extended.
    ///
    /// Returns `(compact, Option<extended>)`. If `extended` is `Some`, the compact cell's
    /// `data` field should be set to the index where the extended cell is stored.
    #[must_use]
    pub fn pack(&self) -> (CompactCell, Option<ExtendedCell>) {
        let needs_extended = self.data.len() > 1
            || self.link != 0
            || matches!(self.style.fg, Color::Rgb { .. })
            || matches!(self.style.bg, Color::Rgb { .. })
            || !self.style.us.is_default();

        if needs_extended {
            let compact = CompactCell {
                data: 0, // Will be set to extended index by caller
                attrs: 0,
                fg: 0,
                bg: 0,
                flags: self.flags | CellFlags::EXTENDED,
                _pad: [0; 2],
            };
            let extended = ExtendedCell {
                data: self.data,
                style: self.style,
                link: self.link,
                flags: self.flags,
            };
            (compact, Some(extended))
        } else {
            let byte = if self.data.is_empty() { b' ' } else { self.data.as_bytes()[0] };

            let mut flags = self.flags;
            let fg = match self.style.fg {
                Color::Default => 8,
                Color::Palette(idx) => {
                    flags |= CellFlags::FG256;
                    idx
                }
                Color::Rgb { .. } => unreachable!(),
            };
            let bg = match self.style.bg {
                Color::Default => 8,
                Color::Palette(idx) => {
                    flags |= CellFlags::BG256;
                    idx
                }
                Color::Rgb { .. } => unreachable!(),
            };

            let compact = CompactCell {
                data: byte,
                attrs: self.style.attrs.bits() as u8,
                fg,
                bg,
                flags,
                _pad: [0; 2],
            };
            (compact, None)
        }
    }

    /// Unpack a compact cell (with optional extended data) into a GridCell.
    #[must_use]
    pub fn unpack(compact: &CompactCell, extended: Option<&ExtendedCell>) -> Self {
        if let Some(ext) = extended {
            GridCell { data: ext.data, style: ext.style, link: ext.link, flags: ext.flags }
        } else {
            let fg = if compact.flags.contains(CellFlags::FG256) {
                Color::Palette(compact.fg)
            } else if compact.fg == 8 {
                Color::Default
            } else {
                Color::Palette(compact.fg)
            };
            let bg = if compact.flags.contains(CellFlags::BG256) {
                Color::Palette(compact.bg)
            } else if compact.bg == 8 {
                Color::Default
            } else {
                Color::Palette(compact.bg)
            };
            let attrs = Attrs::from_bits_truncate(compact.attrs as u16);

            GridCell {
                data: Utf8Char::from_ascii(compact.data),
                style: Style { fg, bg, us: Color::Default, attrs },
                link: 0,
                flags: compact.flags & !CellFlags::FG256 & !CellFlags::BG256,
            }
        }
    }
}

impl Default for GridCell {
    fn default() -> Self {
        Self::CLEARED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_cell_size() {
        assert_eq!(std::mem::size_of::<CompactCell>(), 8);
    }

    #[test]
    fn pack_ascii_default() {
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'A'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        let (compact, ext) = cell.pack();
        assert!(ext.is_none());
        assert!(!compact.is_extended());
        assert_eq!(compact.data, b'A');
    }

    #[test]
    fn pack_ascii_colored() {
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'X'),
            style: Style {
                fg: Color::RED,
                bg: Color::BLUE,
                us: Color::Default,
                attrs: Attrs::BOLD,
            },
            link: 0,
            flags: CellFlags::empty(),
        };
        let (compact, ext) = cell.pack();
        assert!(ext.is_none());
        assert!(!compact.is_extended());
    }

    #[test]
    fn pack_unicode_needs_extended() {
        let cell = GridCell {
            data: Utf8Char::from_char('世'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        let (compact, ext) = cell.pack();
        assert!(compact.is_extended());
        assert!(ext.is_some());
    }

    #[test]
    fn pack_rgb_needs_extended() {
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'A'),
            style: Style { fg: Color::Rgb { r: 255, g: 0, b: 0 }, ..Style::DEFAULT },
            link: 0,
            flags: CellFlags::empty(),
        };
        let (_compact, ext) = cell.pack();
        assert!(ext.is_some());
    }

    #[test]
    fn unpack_roundtrip_compact() {
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'Z'),
            style: Style {
                fg: Color::Palette(196),
                bg: Color::Default,
                us: Color::Default,
                attrs: Attrs::BOLD | Attrs::UNDERSCORE,
            },
            link: 0,
            flags: CellFlags::empty(),
        };
        let (compact, ext) = cell.pack();
        let unpacked = GridCell::unpack(&compact, ext.as_ref());
        assert_eq!(unpacked.data, cell.data);
        assert_eq!(unpacked.style.fg, cell.style.fg);
        assert_eq!(unpacked.style.attrs, cell.style.attrs);
    }

    #[test]
    fn unpack_roundtrip_extended() {
        let cell = GridCell {
            data: Utf8Char::from_char('世'),
            style: Style {
                fg: Color::Rgb { r: 100, g: 200, b: 50 },
                bg: Color::Palette(42),
                us: Color::Rgb { r: 255, g: 0, b: 0 },
                attrs: Attrs::ITALICS,
            },
            link: 42,
            flags: CellFlags::empty(),
        };
        let (compact, ext) = cell.pack();
        let unpacked = GridCell::unpack(&compact, ext.as_ref());
        assert_eq!(unpacked.data, cell.data);
        assert_eq!(unpacked.style, cell.style);
        assert_eq!(unpacked.link, cell.link);
    }
}
