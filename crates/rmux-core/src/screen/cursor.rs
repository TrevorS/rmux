//! Cursor position and state.

use crate::style::Style;

/// Cursor style (shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CursorStyle {
    /// Default terminal cursor.
    #[default]
    Default,
    /// Blinking block.
    BlinkingBlock,
    /// Steady block.
    SteadyBlock,
    /// Blinking underline.
    BlinkingUnderline,
    /// Steady underline.
    SteadyUnderline,
    /// Blinking bar.
    BlinkingBar,
    /// Steady bar.
    SteadyBar,
}

/// Cursor position and associated state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Column position (0-based).
    pub x: u32,
    /// Row position (0-based).
    pub y: u32,
    /// Current style applied to new characters.
    pub style: Style,
    /// Cursor shape.
    pub cursor_style: CursorStyle,
    /// Origin mode: cursor movement is relative to scroll region.
    pub origin_mode: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            style: Style::DEFAULT,
            cursor_style: CursorStyle::Default,
            origin_mode: false,
        }
    }
}

/// Saved cursor state (for DECSC/DECRC).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedCursor {
    /// Saved column.
    pub x: u32,
    /// Saved row.
    pub y: u32,
    /// Saved style.
    pub style: Style,
    /// Saved origin mode.
    pub origin_mode: bool,
}

impl From<&Cursor> for SavedCursor {
    fn from(c: &Cursor) -> Self {
        Self { x: c.x, y: c.y, style: c.style, origin_mode: c.origin_mode }
    }
}

impl SavedCursor {
    /// Restore this saved state into a cursor.
    pub fn restore_into(&self, cursor: &mut Cursor) {
        cursor.x = self.x;
        cursor.y = self.y;
        cursor.style = self.style;
        cursor.origin_mode = self.origin_mode;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Attrs, Color, Style};

    #[test]
    fn default_cursor() {
        let c = Cursor::default();
        assert_eq!(c.x, 0);
        assert_eq!(c.y, 0);
        assert_eq!(c.cursor_style, CursorStyle::Default);
        assert!(!c.origin_mode);
    }

    #[test]
    fn saved_cursor_roundtrip() {
        let mut cursor = Cursor::default();
        cursor.x = 10;
        cursor.y = 20;
        cursor.style = Style { fg: Color::RED, ..Style::DEFAULT };
        cursor.origin_mode = true;

        let saved = SavedCursor::from(&cursor);
        let mut restored = Cursor::default();
        saved.restore_into(&mut restored);

        assert_eq!(restored.x, cursor.x);
        assert_eq!(restored.y, cursor.y);
        assert_eq!(restored.style, cursor.style);
        assert_eq!(restored.origin_mode, cursor.origin_mode);
    }

    #[test]
    fn saved_cursor_preserves_fields() {
        let mut cursor = Cursor::default();
        cursor.x = 5;
        cursor.y = 15;
        cursor.style = Style { bg: Color::Palette(42), attrs: Attrs::BOLD, ..Style::DEFAULT };
        cursor.origin_mode = true;

        let saved = SavedCursor::from(&cursor);

        assert_eq!(saved.x, 5);
        assert_eq!(saved.y, 15);
        assert_eq!(saved.style, cursor.style);
        assert!(saved.origin_mode);
    }

    #[test]
    fn cursor_style_default() {
        assert_eq!(CursorStyle::default(), CursorStyle::Default);
    }

    #[test]
    fn restore_into_overwrites() {
        let mut cursor = Cursor {
            x: 100,
            y: 200,
            style: Style { fg: Color::RED, ..Style::DEFAULT },
            cursor_style: CursorStyle::BlinkingBar,
            origin_mode: true,
        };

        let saved = SavedCursor {
            x: 3,
            y: 7,
            style: Style::DEFAULT,
            origin_mode: false,
        };

        saved.restore_into(&mut cursor);

        assert_eq!(cursor.x, 3);
        assert_eq!(cursor.y, 7);
        assert_eq!(cursor.style, Style::DEFAULT);
        assert!(!cursor.origin_mode);
        // cursor_style is not restored by SavedCursor
        assert_eq!(cursor.cursor_style, CursorStyle::BlinkingBar);
    }
}
