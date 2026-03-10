//! UTF-8 character representation with display width.
//!
//! `Utf8Char` stores a single grapheme cluster's UTF-8 bytes inline (up to 32 bytes)
//! along with its display width and byte length. This avoids heap allocation for the
//! vast majority of characters.
//!
//! Matches tmux's `struct utf8_data` but with a safer interface.

use unicode_width::UnicodeWidthChar;

/// Maximum inline storage for a UTF-8 grapheme cluster.
/// Handles combining characters and complex emoji sequences.
const UTF8_MAX_BYTES: usize = 32;

/// A single displayed character (grapheme cluster) with its UTF-8 bytes stored inline.
///
/// This is the character data stored in grid cells. Most cells contain a single ASCII
/// byte, so the common case uses only 1 byte of the inline storage.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Utf8Char {
    /// UTF-8 encoded bytes, stored inline.
    data: [u8; UTF8_MAX_BYTES],
    /// Number of valid bytes in `data`.
    size: u8,
    /// Display width in terminal columns (0, 1, or 2).
    width: u8,
}

impl Utf8Char {
    /// An empty character (zero width, zero bytes).
    pub const EMPTY: Self = Self { data: [0; UTF8_MAX_BYTES], size: 0, width: 0 };

    /// A space character.
    pub const SPACE: Self = Self {
        data: {
            let mut d = [0u8; UTF8_MAX_BYTES];
            d[0] = b' ';
            d
        },
        size: 1,
        width: 1,
    };

    /// Create from a single byte, assumed to be ASCII.
    ///
    /// Non-ASCII bytes (≥0x80) are stored as-is with width 0 (non-printable).
    /// This avoids panics when the VT100 parser encounters unexpected bytes.
    #[must_use]
    pub fn from_ascii(byte: u8) -> Self {
        let mut data = [0u8; UTF8_MAX_BYTES];
        data[0] = byte;
        let width = if (0x20..0x7f).contains(&byte) { 1 } else { 0 };
        Self { data, size: 1, width }
    }

    /// Create from a Unicode character.
    #[must_use]
    pub fn from_char(ch: char) -> Self {
        let mut data = [0u8; UTF8_MAX_BYTES];
        let s = ch.encode_utf8(&mut data);
        let size = s.len() as u8;
        let width = UnicodeWidthChar::width(ch).unwrap_or(0) as u8;
        Self { data, size, width }
    }

    /// Create from a UTF-8 byte slice. Returns `None` if the slice is too long.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > UTF8_MAX_BYTES {
            return None;
        }
        let mut data = [0u8; UTF8_MAX_BYTES];
        data[..bytes.len()].copy_from_slice(bytes);
        // Compute width from the first character
        let width = std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| s.chars().next())
            .and_then(UnicodeWidthChar::width)
            .unwrap_or(0) as u8;
        Some(Self { data, size: bytes.len() as u8, width })
    }

    /// Set the display width explicitly (for tab characters, combining chars, etc.).
    #[must_use]
    pub fn with_width(mut self, width: u8) -> Self {
        self.width = width;
        self
    }

    /// The UTF-8 bytes of this character.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data[..self.size as usize]
    }

    /// The display width in terminal columns.
    #[must_use]
    pub fn width(&self) -> u8 {
        self.width
    }

    /// The number of UTF-8 bytes.
    #[must_use]
    pub fn len(&self) -> u8 {
        self.size
    }

    /// Whether this character has zero bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Convert to a string slice, if the bytes are valid UTF-8.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }
}

impl Default for Utf8Char {
    fn default() -> Self {
        Self::SPACE
    }
}

impl std::fmt::Debug for Utf8Char {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.as_str() {
            Some(s) => write!(f, "Utf8Char({s:?}, w={})", self.width),
            None => write!(f, "Utf8Char({:?}, w={})", self.as_bytes(), self.width),
        }
    }
}

impl std::fmt::Display for Utf8Char {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.as_str() {
            Some(s) => f.write_str(s),
            None => write!(f, "\u{FFFD}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_char() {
        let c = Utf8Char::from_ascii(b'A');
        assert_eq!(c.as_bytes(), b"A");
        assert_eq!(c.width(), 1);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn space_char() {
        assert_eq!(Utf8Char::SPACE.width(), 1);
        assert_eq!(Utf8Char::SPACE.as_bytes(), b" ");
    }

    #[test]
    fn unicode_cjk() {
        // CJK character (width 2)
        let c = Utf8Char::from_char('\u{4E16}'); // 世
        assert_eq!(c.width(), 2);
        assert_eq!(c.len(), 3); // 3 UTF-8 bytes
        assert_eq!(c.as_str(), Some("世"));
    }

    #[test]
    fn unicode_emoji() {
        let c = Utf8Char::from_char('\u{1F600}'); // 😀
        assert_eq!(c.width(), 2);
        assert_eq!(c.len(), 4); // 4 UTF-8 bytes
    }

    #[test]
    fn from_bytes_valid() {
        let c = Utf8Char::from_bytes("é".as_bytes()).unwrap();
        assert_eq!(c.width(), 1);
        assert_eq!(c.as_str(), Some("é"));
    }

    #[test]
    fn from_bytes_too_long() {
        let long = [0x41u8; 33];
        assert!(Utf8Char::from_bytes(&long).is_none());
    }

    #[test]
    fn with_width_override() {
        let tab = Utf8Char::from_ascii(b' ').with_width(8);
        assert_eq!(tab.width(), 8);
    }

    #[test]
    fn display_trait() {
        let c = Utf8Char::from_char('R');
        assert_eq!(format!("{c}"), "R");
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn ascii_roundtrip(ch in 0x20u8..0x7f) {
                let u = Utf8Char::from_ascii(ch);
                let bytes = u.as_bytes();
                prop_assert_eq!(bytes.len(), 1);
                prop_assert_eq!(bytes[0], ch);
            }

            #[test]
            fn width_is_nonnegative(ch in proptest::char::any()) {
                let u = Utf8Char::from_char(ch);
                prop_assert!(u.width() <= 2);
            }

            #[test]
            fn from_char_roundtrip(ch in proptest::char::any().prop_filter("not null", |c| *c != '\0' && !c.is_control())) {
                let u = Utf8Char::from_char(ch);
                let bytes = u.as_bytes();
                let s = std::str::from_utf8(bytes);
                prop_assert!(s.is_ok());
            }
        }
    }
}
