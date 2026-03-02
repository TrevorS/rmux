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
