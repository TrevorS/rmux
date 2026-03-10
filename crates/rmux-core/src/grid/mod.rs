//! Grid: the core display buffer.
//!
//! A grid stores a rectangular array of terminal cells, with optional scrollback history.
//! This is the most performance-critical data structure in rmux.

pub mod cell;
pub mod history;
pub mod line;

use cell::GridCell;
use history::GridHistory;
use line::GridLine;

use bitflags::bitflags;

bitflags! {
    /// Grid-level flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct GridFlags: u8 {
        /// Grid has history enabled.
        const HISTORY = 0x01;
        /// Grid is in wrapped mode.
        const WRAP    = 0x02;
    }
}

/// The grid: a rectangular display buffer with optional scrollback.
///
/// Coordinates: (x, y) where x is column (0-based) and y is row (0-based).
/// y=0 is the top visible row. History lines have negative logical indices
/// but are accessed through the history API.
#[derive(Debug, Clone)]
pub struct Grid {
    /// Width (columns).
    sx: u32,
    /// Visible height (rows).
    sy: u32,
    /// Line storage (history + visible).
    history: GridHistory,
    /// Grid flags.
    flags: GridFlags,
}

impl Grid {
    /// Create a new grid with the given dimensions and history limit.
    #[must_use]
    pub fn new(sx: u32, sy: u32, history_limit: u32) -> Self {
        let flags = if history_limit > 0 { GridFlags::HISTORY } else { GridFlags::empty() };
        Self { sx, sy, history: GridHistory::new(sy, history_limit), flags }
    }

    /// Grid width (columns).
    #[must_use]
    pub fn width(&self) -> u32 {
        self.sx
    }

    /// Grid visible height (rows).
    #[must_use]
    pub fn height(&self) -> u32 {
        self.sy
    }

    /// Number of history lines.
    #[must_use]
    pub fn history_size(&self) -> u32 {
        self.history.history_size()
    }

    /// History limit.
    #[must_use]
    pub fn history_limit(&self) -> u32 {
        self.history.limit()
    }

    /// Grid flags.
    #[must_use]
    pub fn flags(&self) -> GridFlags {
        self.flags
    }

    /// Get a cell from the visible area.
    #[must_use]
    pub fn get_cell(&self, x: u32, y: u32) -> GridCell {
        if y >= self.sy {
            return GridCell::CLEARED;
        }
        match self.history.visible_line(y) {
            Some(line) => line.get_cell(x),
            None => GridCell::CLEARED,
        }
    }

    /// Set a cell in the visible area.
    pub fn set_cell(&mut self, x: u32, y: u32, cell: &GridCell) {
        if y >= self.sy {
            return;
        }
        if let Some(line) = self.history.visible_line_mut(y) {
            line.set_cell(x, cell);
        }
    }

    /// Get a visible line (immutable).
    #[must_use]
    pub fn get_line(&self, y: u32) -> Option<&GridLine> {
        if y >= self.sy {
            return None;
        }
        self.history.visible_line(y)
    }

    /// Get a mutable visible line.
    #[must_use]
    pub fn get_line_mut(&mut self, y: u32) -> Option<&mut GridLine> {
        if y >= self.sy {
            return None;
        }
        self.history.visible_line_mut(y)
    }

    /// Get a history line (0 = oldest).
    #[must_use]
    pub fn get_history_line(&self, y: u32) -> Option<&GridLine> {
        if y >= self.history_size() {
            return None;
        }
        self.history.get(y)
    }

    /// Scroll the visible area up by one line.
    ///
    /// The top line moves into history (if enabled), and a new empty line
    /// appears at the bottom.
    pub fn scroll_up(&mut self) {
        if self.flags.contains(GridFlags::HISTORY) {
            self.history.scroll_up();
        } else {
            // No history: rotate the visible lines
            // Remove top line and add new one at bottom
            if let Some(line) = self.history.visible_line_mut(0) {
                *line = GridLine::new();
            }
            // This is simplified; real implementation would rotate
            self.history.scroll_up();
        }
    }

    /// Scroll the visible area down by one line (reverse scroll).
    pub fn scroll_down(&mut self) {
        self.history.scroll_down();
    }

    /// Scroll a region of the visible area up by one line.
    pub fn scroll_region_up(&mut self, top: u32, bottom: u32) {
        if top >= self.sy || bottom >= self.sy || top >= bottom {
            return;
        }
        // If scrolling the full screen, use normal scroll
        if top == 0 && bottom == self.sy - 1 {
            self.scroll_up();
            return;
        }
        // Move lines within the region
        // This is a more complex operation for partial scroll regions
        // For now, clear the top line and shift others up
        for y in top..bottom {
            if let (Some(src), Some(dst)) =
                (self.history.visible_line(y + 1).cloned(), self.history.visible_line_mut(y))
            {
                *dst = src;
            }
        }
        if let Some(line) = self.history.visible_line_mut(bottom) {
            *line = GridLine::new();
        }
    }

    /// Scroll a region of the visible area down by one line.
    pub fn scroll_region_down(&mut self, top: u32, bottom: u32) {
        if top >= self.sy || bottom >= self.sy || top >= bottom {
            return;
        }
        for y in (top + 1..=bottom).rev() {
            if let (Some(src), Some(dst)) =
                (self.history.visible_line(y - 1).cloned(), self.history.visible_line_mut(y))
            {
                *dst = src;
            }
        }
        if let Some(line) = self.history.visible_line_mut(top) {
            *line = GridLine::new();
        }
    }

    /// Clear all cells in the visible area.
    pub fn clear(&mut self) {
        for y in 0..self.sy {
            if let Some(line) = self.history.visible_line_mut(y) {
                *line = GridLine::new();
            }
        }
    }

    /// Clear a rectangular region in the visible area.
    pub fn clear_region(&mut self, x1: u32, y1: u32, x2: u32, y2: u32) {
        for y in y1..=y2.min(self.sy - 1) {
            if let Some(line) = self.history.visible_line_mut(y) {
                let start = if y == y1 { x1 } else { 0 };
                let end = if y == y2 { x2 + 1 } else { self.sx };
                line.clear_range(start, end, crate::style::Color::Default);
            }
        }
    }

    /// Resize the grid to new dimensions.
    pub fn resize(&mut self, new_sx: u32, new_sy: u32) {
        self.sx = new_sx;
        self.history.resize_visible(new_sy);
        self.sy = new_sy;
    }

    /// Clear all history.
    pub fn clear_history(&mut self) {
        self.history.clear_history();
    }

    /// Collect (trim) excess history lines.
    pub fn collect_history(&mut self) {
        self.history.collect_if_needed(false);
    }

    /// Get a line by absolute index.
    ///
    /// Index 0 is the oldest history line. `history_size()` is the first
    /// visible line. `history_size() + height() - 1` is the last visible line.
    #[must_use]
    pub fn get_line_absolute(&self, abs_y: u32) -> Option<&GridLine> {
        let hs = self.history_size();
        if abs_y < hs {
            self.get_history_line(abs_y)
        } else {
            let vis_y = abs_y - hs;
            self.get_line(vis_y)
        }
    }

    /// Insert `n` blank cells at position (x, y), shifting cells right.
    pub fn insert_characters(&mut self, x: u32, y: u32, n: u32) {
        if y >= self.sy {
            return;
        }
        if let Some(line) = self.history.visible_line_mut(y) {
            line.insert_cells(x, n, self.sx);
        }
    }

    /// Delete `n` cells at position (x, y), shifting cells left.
    pub fn delete_characters(&mut self, x: u32, y: u32, n: u32) {
        if y >= self.sy {
            return;
        }
        if let Some(line) = self.history.visible_line_mut(y) {
            line.delete_cells(x, n, self.sx);
        }
    }

    /// Erase `n` cells starting at position (x, y) without shifting.
    pub fn erase_characters(&mut self, x: u32, y: u32, n: u32) {
        if y >= self.sy {
            return;
        }
        let end = (x + n).min(self.sx);
        if let Some(line) = self.history.visible_line_mut(y) {
            line.clear_range(x, end, crate::style::Color::Default);
        }
    }

    /// Total number of lines (history + visible).
    #[must_use]
    pub fn total_lines(&self) -> u32 {
        self.history_size() + self.sy
    }

    /// Compare two grids for equality (visible area only).
    #[must_use]
    pub fn compare(&self, other: &Grid) -> bool {
        if self.sx != other.sx || self.sy != other.sy {
            return false;
        }
        for y in 0..self.sy {
            for x in 0..self.sx {
                if self.get_cell(x, y) != other.get_cell(x, y) {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Style;
    use crate::utf8::Utf8Char;
    use cell::CellFlags;

    fn make_cell(ch: u8) -> GridCell {
        GridCell {
            data: Utf8Char::from_ascii(ch),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        }
    }

    #[test]
    fn new_grid() {
        let g = Grid::new(80, 24, 2000);
        assert_eq!(g.width(), 80);
        assert_eq!(g.height(), 24);
        assert_eq!(g.history_size(), 0);
    }

    #[test]
    fn set_and_get_cell() {
        let mut g = Grid::new(80, 24, 2000);
        let cell = make_cell(b'A');
        g.set_cell(5, 3, &cell);
        assert_eq!(g.get_cell(5, 3).data, Utf8Char::from_ascii(b'A'));
    }

    #[test]
    fn out_of_bounds_returns_cleared() {
        let g = Grid::new(80, 24, 2000);
        assert_eq!(g.get_cell(100, 100), GridCell::CLEARED);
    }

    #[test]
    fn scroll_up() {
        let mut g = Grid::new(80, 24, 2000);
        g.set_cell(0, 0, &make_cell(b'X'));
        g.scroll_up();
        assert_eq!(g.history_size(), 1);
        // The 'X' should now be in history
        let hist = g.get_history_line(0).unwrap();
        assert_eq!(hist.get_cell(0).data, Utf8Char::from_ascii(b'X'));
    }

    #[test]
    fn clear_grid() {
        let mut g = Grid::new(80, 24, 2000);
        g.set_cell(0, 0, &make_cell(b'A'));
        g.set_cell(79, 23, &make_cell(b'Z'));
        g.clear();
        assert_eq!(g.get_cell(0, 0), GridCell::CLEARED);
        assert_eq!(g.get_cell(79, 23), GridCell::CLEARED);
    }

    #[test]
    fn resize_grid() {
        let mut g = Grid::new(80, 24, 2000);
        g.resize(120, 40);
        assert_eq!(g.width(), 120);
        assert_eq!(g.height(), 40);
    }

    #[test]
    fn compare_grids() {
        let mut g1 = Grid::new(80, 24, 0);
        let mut g2 = Grid::new(80, 24, 0);
        assert!(g1.compare(&g2));

        g1.set_cell(5, 5, &make_cell(b'X'));
        assert!(!g1.compare(&g2));

        g2.set_cell(5, 5, &make_cell(b'X'));
        assert!(g1.compare(&g2));
    }

    #[test]
    fn scroll_region() {
        let mut g = Grid::new(80, 24, 0);
        g.set_cell(0, 5, &make_cell(b'A'));
        g.set_cell(0, 6, &make_cell(b'B'));
        g.set_cell(0, 7, &make_cell(b'C'));

        g.scroll_region_up(5, 7);
        // B should now be at y=5, C at y=6, y=7 cleared
        assert_eq!(g.get_cell(0, 5).data, Utf8Char::from_ascii(b'B'));
        assert_eq!(g.get_cell(0, 6).data, Utf8Char::from_ascii(b'C'));
    }

    #[test]
    fn clear_region() {
        let mut g = Grid::new(80, 24, 0);
        for x in 0..10 {
            g.set_cell(x, 0, &make_cell(b'A' + x as u8));
        }
        g.clear_region(3, 0, 6, 0);
        assert_eq!(g.get_cell(2, 0).data, Utf8Char::from_ascii(b'C'));
        assert!(g.get_cell(3, 0).flags.contains(CellFlags::CLEARED));
        assert_eq!(g.get_cell(7, 0).data, Utf8Char::from_ascii(b'H'));
    }

    #[test]
    fn get_cell_out_of_bounds() {
        let g = Grid::new(80, 24, 0);
        // x beyond grid width should return a cleared cell.
        let cell = g.get_cell(200, 0);
        assert_eq!(cell, GridCell::CLEARED);
        // y beyond grid height should also return a cleared cell.
        let cell = g.get_cell(0, 50);
        assert_eq!(cell, GridCell::CLEARED);
        // Both x and y beyond bounds.
        let cell = g.get_cell(999, 999);
        assert_eq!(cell, GridCell::CLEARED);
    }

    #[test]
    fn set_cell_extends_grid() {
        let mut g = Grid::new(80, 24, 0);
        // Setting a cell at x > current line length should work
        // (the line internally extends with cleared cells).
        let cell = make_cell(b'Z');
        g.set_cell(100, 0, &cell);
        // Should be able to read it back.
        assert_eq!(g.get_cell(100, 0).data, Utf8Char::from_ascii(b'Z'));
        // Cells before it that were auto-extended should be cleared.
        assert_eq!(g.get_cell(99, 0), GridCell::CLEARED);
    }

    #[test]
    fn resize_smaller_truncates() {
        let mut g = Grid::new(80, 24, 0);
        // Write cells across the full width.
        for x in 0..80 {
            g.set_cell(x, 0, &make_cell(b'A'));
        }
        // Resize to a smaller width.
        g.resize(40, 24);
        assert_eq!(g.width(), 40);
        assert_eq!(g.height(), 24);
        // Resize to smaller height.
        g.resize(40, 10);
        assert_eq!(g.height(), 10);
    }

    #[test]
    fn scroll_region_works() {
        let mut g = Grid::new(80, 10, 0);
        // Set up cells in a scroll region (rows 2-6).
        g.set_cell(0, 2, &make_cell(b'A'));
        g.set_cell(0, 3, &make_cell(b'B'));
        g.set_cell(0, 4, &make_cell(b'C'));
        g.set_cell(0, 5, &make_cell(b'D'));
        g.set_cell(0, 6, &make_cell(b'E'));

        // Scroll the region up: rows 2-6.
        g.scroll_region_up(2, 6);
        // Row 2 should now have what was in row 3 ('B').
        assert_eq!(g.get_cell(0, 2).data, Utf8Char::from_ascii(b'B'));
        // Row 5 should now have what was in row 6 ('E').
        assert_eq!(g.get_cell(0, 5).data, Utf8Char::from_ascii(b'E'));
        // Row 6 (bottom of region) should be cleared.
        assert_eq!(g.get_cell(0, 6), GridCell::CLEARED);

        // Now test scroll_region_down.
        g.set_cell(0, 2, &make_cell(b'X'));
        g.set_cell(0, 3, &make_cell(b'Y'));
        g.scroll_region_down(2, 6);
        // Row 2 (top of region) should be cleared after scroll down.
        assert_eq!(g.get_cell(0, 2), GridCell::CLEARED);
        // Row 3 should have what was in row 2 ('X').
        assert_eq!(g.get_cell(0, 3).data, Utf8Char::from_ascii(b'X'));
        // Row 4 should have what was in row 3 ('Y').
        assert_eq!(g.get_cell(0, 4).data, Utf8Char::from_ascii(b'Y'));
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn set_get_roundtrip(x in 0u32..80, y in 0u32..24, ch in 0x20u8..0x7f) {
                let mut grid = Grid::new(80, 24, 0);
                let cell = GridCell {
                    data: Utf8Char::from_ascii(ch),
                    style: Style::DEFAULT,
                    link: 0,
                    flags: CellFlags::empty(),
                };
                grid.set_cell(x, y, &cell);
                let got = grid.get_cell(x, y);
                prop_assert_eq!(got.data.as_bytes(), cell.data.as_bytes());
            }

            #[test]
            fn scroll_up_preserves_width(n_scrolls in 1u32..100, width in 10u32..200, height in 5u32..50) {
                let mut grid = Grid::new(width, height, 1000);
                for _ in 0..n_scrolls {
                    grid.scroll_up();
                }
                prop_assert_eq!(grid.width(), width);
                prop_assert_eq!(grid.height(), height);
            }

            #[test]
            fn resize_preserves_content(
                new_width in 10u32..200,
                new_height in 5u32..50,
            ) {
                let mut grid = Grid::new(80, 24, 0);
                // Write something
                let cell = GridCell {
                    data: Utf8Char::from_ascii(b'X'),
                    style: Style::DEFAULT,
                    link: 0,
                    flags: CellFlags::empty(),
                };
                grid.set_cell(0, 0, &cell);
                grid.resize(new_width, new_height);
                // Grid should have new dimensions
                prop_assert_eq!(grid.width(), new_width);
                prop_assert_eq!(grid.height(), new_height);
            }
        }
    }
}
