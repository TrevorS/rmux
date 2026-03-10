//! Screen: a virtual terminal screen with grid, cursor, and mode state.

pub mod cursor;
pub mod selection;

use crate::grid::Grid;
use bitflags::bitflags;
use cursor::{Cursor, SavedCursor};
use selection::Selection;

bitflags! {
    /// Screen mode flags (matching tmux's MODE_* constants).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct ModeFlags: u32 {
        /// Cursor keys mode (DECCKM).
        const CURSOR_KEYS     = 0x0001;
        /// Insert mode (IRM).
        const INSERT          = 0x0002;
        /// Keypad application mode (DECNKM).
        const KEYPAD          = 0x0004;
        /// Auto-wrap mode (DECAWM).
        const WRAP            = 0x0008;
        /// Mouse standard mode.
        const MOUSE_STANDARD  = 0x0010;
        /// Mouse button mode.
        const MOUSE_BUTTON    = 0x0020;
        /// Mouse any-event mode.
        const MOUSE_ANY       = 0x0040;
        /// Mouse SGR mode.
        const MOUSE_SGR       = 0x0080;
        /// Application cursor keys.
        const APP_CURSOR      = 0x0100;
        /// Application keypad.
        const APP_KEYPAD      = 0x0200;
        /// Bracketed paste mode.
        const BRACKETPASTE    = 0x0400;
        /// Focus events.
        const FOCUSON         = 0x0800;
        /// Cursor visible.
        const CURSOR_VISIBLE  = 0x1000;
        /// Origin mode (DECOM).
        const ORIGIN          = 0x2000;
    }
}

/// Scroll region bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollRegion {
    /// Top row of scroll region (inclusive).
    pub top: u32,
    /// Bottom row of scroll region (inclusive).
    pub bottom: u32,
}

impl ScrollRegion {
    /// Create a scroll region covering the full screen height.
    #[must_use]
    pub fn full(height: u32) -> Self {
        Self { top: 0, bottom: height.saturating_sub(1) }
    }

    /// Whether this region covers the full visible height.
    #[must_use]
    pub fn is_full(&self, height: u32) -> bool {
        self.top == 0 && self.bottom == height.saturating_sub(1)
    }
}

/// Alternate screen state (saved when switching to alt screen).
#[derive(Debug, Clone)]
pub struct AlternateScreen {
    /// Saved grid from the normal screen.
    pub grid: Grid,
    /// Saved cursor.
    pub cursor: SavedCursor,
}

/// A virtual terminal screen.
///
/// This is the primary abstraction for terminal state. Each pane has one screen
/// (plus optionally an alternate screen for full-screen applications).
#[derive(Debug, Clone)]
pub struct Screen {
    /// Window title (set via OSC 0/2).
    pub title: String,
    /// Current working directory (set via OSC 7).
    pub path: Option<String>,
    /// The grid (character storage with scrollback).
    pub grid: Grid,
    /// Cursor position and style.
    pub cursor: Cursor,
    /// Saved cursor (DECSC/DECRC).
    pub saved_cursor: Option<SavedCursor>,
    /// Scroll region.
    pub scroll_region: ScrollRegion,
    /// Screen mode flags.
    pub mode: ModeFlags,
    /// Default mode flags (restored on reset).
    pub default_mode: ModeFlags,
    /// Tab stops (bit per column).
    pub tabs: Vec<bool>,
    /// Alternate screen (if active, this holds the normal screen's state).
    pub alternate: Option<AlternateScreen>,
    /// Current selection (copy mode).
    pub selection: Option<Selection>,
}

impl Screen {
    /// Create a new screen with the given dimensions.
    #[must_use]
    pub fn new(width: u32, height: u32, history_limit: u32) -> Self {
        let default_mode = ModeFlags::WRAP | ModeFlags::CURSOR_VISIBLE;
        let mut tabs = vec![false; width as usize];
        // Set tab stops every 8 columns
        for i in (0..width).step_by(8) {
            tabs[i as usize] = true;
        }

        Self {
            title: String::new(),
            path: None,
            grid: Grid::new(width, height, history_limit),
            cursor: Cursor::default(),
            saved_cursor: None,
            scroll_region: ScrollRegion::full(height),
            mode: default_mode,
            default_mode,
            tabs,
            alternate: None,
            selection: None,
        }
    }

    /// Screen width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.grid.width()
    }

    /// Screen height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.grid.height()
    }

    /// Switch to the alternate screen.
    pub fn enter_alternate(&mut self) {
        if self.alternate.is_some() {
            return; // Already in alternate
        }
        let saved_grid = self.grid.clone();
        let saved_cursor = SavedCursor::from(&self.cursor);
        self.alternate = Some(AlternateScreen { grid: saved_grid, cursor: saved_cursor });
        // Create a fresh grid for the alternate screen (no history)
        self.grid = Grid::new(self.width(), self.height(), 0);
        self.cursor = Cursor::default();
    }

    /// Switch back to the normal screen.
    pub fn exit_alternate(&mut self) {
        if let Some(alt) = self.alternate.take() {
            self.grid = alt.grid;
            alt.cursor.restore_into(&mut self.cursor);
        }
    }

    /// Save cursor position (DECSC).
    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some(SavedCursor::from(&self.cursor));
    }

    /// Restore cursor position (DECRC).
    pub fn restore_cursor(&mut self) {
        if let Some(saved) = &self.saved_cursor {
            saved.restore_into(&mut self.cursor);
        }
    }

    /// Resize the screen.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.grid.resize(width, height);
        self.scroll_region = ScrollRegion::full(height);
        // Resize tab stops
        self.tabs.resize(width as usize, false);
        for i in (0..width).step_by(8) {
            self.tabs[i as usize] = true;
        }
        // Clamp cursor
        self.cursor.x = self.cursor.x.min(width.saturating_sub(1));
        self.cursor.y = self.cursor.y.min(height.saturating_sub(1));
    }

    /// Reset the screen to initial state.
    pub fn reset(&mut self) {
        let width = self.width();
        let height = self.height();
        let limit = self.grid.history_limit();
        *self = Self::new(width, height, limit);
    }

    /// Get the next tab stop after the given column.
    #[must_use]
    pub fn next_tab_stop(&self, x: u32) -> u32 {
        let width = self.width();
        for col in (x + 1)..width {
            if self.tabs.get(col as usize).copied().unwrap_or(false) {
                return col;
            }
        }
        width.saturating_sub(1)
    }

    /// Get the previous tab stop before the given column.
    #[must_use]
    pub fn prev_tab_stop(&self, x: u32) -> u32 {
        if x == 0 {
            return 0;
        }
        for col in (0..x).rev() {
            if self.tabs.get(col as usize).copied().unwrap_or(false) {
                return col;
            }
        }
        0
    }

    /// Set a tab stop at the given column.
    pub fn set_tab_stop(&mut self, x: u32) {
        if let Some(tab) = self.tabs.get_mut(x as usize) {
            *tab = true;
        }
    }

    /// Clear the tab stop at the given column.
    pub fn clear_tab_stop(&mut self, x: u32) {
        if let Some(tab) = self.tabs.get_mut(x as usize) {
            *tab = false;
        }
    }

    /// Clear all tab stops.
    pub fn clear_all_tab_stops(&mut self) {
        self.tabs.fill(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::cell::{CellFlags, GridCell};
    use crate::style::Style;
    use crate::utf8::Utf8Char;

    #[test]
    fn new_screen() {
        let s = Screen::new(80, 24, 2000);
        assert_eq!(s.width(), 80);
        assert_eq!(s.height(), 24);
        assert!(s.mode.contains(ModeFlags::WRAP));
        assert!(s.mode.contains(ModeFlags::CURSOR_VISIBLE));
    }

    #[test]
    fn tab_stops() {
        let s = Screen::new(80, 24, 0);
        assert!(s.tabs[0]);
        assert!(s.tabs[8]);
        assert!(s.tabs[16]);
        assert!(!s.tabs[1]);
    }

    #[test]
    fn next_tab_stop() {
        let s = Screen::new(80, 24, 0);
        assert_eq!(s.next_tab_stop(0), 8);
        assert_eq!(s.next_tab_stop(7), 8);
        assert_eq!(s.next_tab_stop(8), 16);
    }

    #[test]
    fn alternate_screen() {
        let mut s = Screen::new(80, 24, 2000);
        s.cursor.x = 10;
        s.cursor.y = 5;
        s.enter_alternate();
        assert!(s.alternate.is_some());
        assert_eq!(s.cursor.x, 0);
        assert_eq!(s.cursor.y, 0);
        s.exit_alternate();
        assert!(s.alternate.is_none());
        assert_eq!(s.cursor.x, 10);
        assert_eq!(s.cursor.y, 5);
    }

    #[test]
    fn save_restore_cursor() {
        let mut s = Screen::new(80, 24, 0);
        s.cursor.x = 15;
        s.cursor.y = 7;
        s.save_cursor();
        s.cursor.x = 0;
        s.cursor.y = 0;
        s.restore_cursor();
        assert_eq!(s.cursor.x, 15);
        assert_eq!(s.cursor.y, 7);
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut s = Screen::new(80, 24, 0);
        s.cursor.x = 75;
        s.cursor.y = 20;
        s.resize(40, 10);
        assert_eq!(s.cursor.x, 39);
        assert_eq!(s.cursor.y, 9);
    }

    #[test]
    fn screen_reset_clears_state() {
        let mut s = Screen::new(80, 24, 2000);
        // Modify various pieces of state
        s.cursor.x = 30;
        s.cursor.y = 10;
        s.title = "modified".to_string();
        s.path = Some("/tmp".to_string());
        s.mode.insert(ModeFlags::INSERT);
        s.mode.remove(ModeFlags::WRAP);
        s.save_cursor();
        // Write a cell into the grid
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'X'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        s.grid.set_cell(0, 0, &cell);

        s.reset();

        // After reset, screen should be back to initial state
        assert_eq!(s.cursor.x, 0);
        assert_eq!(s.cursor.y, 0);
        assert_eq!(s.title, "");
        assert_eq!(s.path, None);
        assert!(s.mode.contains(ModeFlags::WRAP));
        assert!(s.mode.contains(ModeFlags::CURSOR_VISIBLE));
        assert!(!s.mode.contains(ModeFlags::INSERT));
        assert!(s.saved_cursor.is_none());
        assert!(s.alternate.is_none());
        assert!(s.selection.is_none());
        assert_eq!(s.width(), 80);
        assert_eq!(s.height(), 24);
    }

    #[test]
    fn tab_stop_at_boundaries() {
        let s = Screen::new(80, 24, 0);
        // At column 0, next tab stop should be 8
        assert_eq!(s.next_tab_stop(0), 8);
        // At a tab position (column 8), next stop is 16
        assert_eq!(s.next_tab_stop(8), 16);
        // Just before a tab stop (column 7), next stop is 8
        assert_eq!(s.next_tab_stop(7), 8);
        // At last column (79), should return width-1 = 79 (no more tab stops)
        assert_eq!(s.next_tab_stop(79), 79);
        // At the last tab stop (column 72), next should be 79 (width-1, no more stops)
        assert_eq!(s.next_tab_stop(72), 79);
    }

    #[test]
    fn save_restore_cursor_multiple() {
        let mut s = Screen::new(80, 24, 0);
        // Set cursor to first position and save
        s.cursor.x = 10;
        s.cursor.y = 5;
        s.save_cursor();

        // Move cursor to a new position
        s.cursor.x = 40;
        s.cursor.y = 15;
        // Save again (this should overwrite the saved state)
        s.save_cursor();

        // Move cursor again
        s.cursor.x = 0;
        s.cursor.y = 0;

        // Restore should bring back the second saved position (40, 15)
        s.restore_cursor();
        assert_eq!(s.cursor.x, 40);
        assert_eq!(s.cursor.y, 15);
    }

    #[test]
    fn alternate_screen_enter_exit() {
        use crate::grid::cell::{CellFlags, GridCell};
        use crate::style::Style;
        use crate::utf8::Utf8Char;

        let mut s = Screen::new(80, 24, 2000);
        // Write content to the normal screen
        let cell_a = GridCell {
            data: Utf8Char::from_ascii(b'A'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        s.grid.set_cell(0, 0, &cell_a);
        s.cursor.x = 10;
        s.cursor.y = 5;

        // Enter alternate screen
        s.enter_alternate();
        assert!(s.alternate.is_some());
        // Cursor should be reset in alternate screen
        assert_eq!(s.cursor.x, 0);
        assert_eq!(s.cursor.y, 0);
        // Original content should not be visible (fresh grid)
        assert_ne!(s.grid.get_cell(0, 0).data, Utf8Char::from_ascii(b'A'));

        // Write something on the alternate screen
        let cell_b = GridCell {
            data: Utf8Char::from_ascii(b'B'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        s.grid.set_cell(5, 5, &cell_b);
        s.cursor.x = 20;
        s.cursor.y = 12;

        // Exit alternate screen
        s.exit_alternate();
        assert!(s.alternate.is_none());
        // Original content should be restored
        assert_eq!(s.grid.get_cell(0, 0).data, Utf8Char::from_ascii(b'A'));
        // Original cursor should be restored
        assert_eq!(s.cursor.x, 10);
        assert_eq!(s.cursor.y, 5);
        // Alternate screen content should not be present
        assert_ne!(s.grid.get_cell(5, 5).data, Utf8Char::from_ascii(b'B'));
    }

    #[test]
    fn screen_resize_preserves_content() {
        use crate::grid::cell::{CellFlags, GridCell};
        use crate::style::Style;
        use crate::utf8::Utf8Char;

        let mut s = Screen::new(80, 24, 0);
        // Write content at (0,0)
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'H'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        s.grid.set_cell(0, 0, &cell);
        s.grid.set_cell(
            1,
            0,
            &GridCell {
                data: Utf8Char::from_ascii(b'i'),
                style: Style::DEFAULT,
                link: 0,
                flags: CellFlags::empty(),
            },
        );

        // Resize to larger
        s.resize(120, 40);
        assert_eq!(s.width(), 120);
        assert_eq!(s.height(), 40);
        // Content should still be present
        assert_eq!(s.grid.get_cell(0, 0).data, Utf8Char::from_ascii(b'H'));
        assert_eq!(s.grid.get_cell(1, 0).data, Utf8Char::from_ascii(b'i'));
    }

    #[test]
    fn mode_flags_toggle() {
        let mut s = Screen::new(80, 24, 0);
        // Initially WRAP and CURSOR_VISIBLE are set
        assert!(s.mode.contains(ModeFlags::WRAP));
        assert!(s.mode.contains(ModeFlags::CURSOR_VISIBLE));
        assert!(!s.mode.contains(ModeFlags::INSERT));
        assert!(!s.mode.contains(ModeFlags::BRACKETPASTE));

        // Set INSERT and BRACKETPASTE
        s.mode.insert(ModeFlags::INSERT);
        s.mode.insert(ModeFlags::BRACKETPASTE);
        assert!(s.mode.contains(ModeFlags::INSERT));
        assert!(s.mode.contains(ModeFlags::BRACKETPASTE));

        // Remove WRAP
        s.mode.remove(ModeFlags::WRAP);
        assert!(!s.mode.contains(ModeFlags::WRAP));

        // Toggle CURSOR_VISIBLE
        s.mode.toggle(ModeFlags::CURSOR_VISIBLE);
        assert!(!s.mode.contains(ModeFlags::CURSOR_VISIBLE));
        s.mode.toggle(ModeFlags::CURSOR_VISIBLE);
        assert!(s.mode.contains(ModeFlags::CURSOR_VISIBLE));

        // Set multiple mouse modes
        s.mode.insert(ModeFlags::MOUSE_STANDARD | ModeFlags::MOUSE_SGR);
        assert!(s.mode.contains(ModeFlags::MOUSE_STANDARD));
        assert!(s.mode.contains(ModeFlags::MOUSE_SGR));
        // Remove them
        s.mode.remove(ModeFlags::MOUSE_STANDARD | ModeFlags::MOUSE_SGR);
        assert!(!s.mode.contains(ModeFlags::MOUSE_STANDARD));
        assert!(!s.mode.contains(ModeFlags::MOUSE_SGR));
    }

    #[test]
    fn default_screen_dimensions() {
        let s = Screen::new(132, 43, 5000);
        assert_eq!(s.width(), 132);
        assert_eq!(s.height(), 43);
        // Verify scroll region covers full screen
        assert_eq!(s.scroll_region.top, 0);
        assert_eq!(s.scroll_region.bottom, 42);
        assert!(s.scroll_region.is_full(43));
    }
}
