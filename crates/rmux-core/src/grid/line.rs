//! Grid line: a row of cells with compact and extended storage.

use super::cell::{CellFlags, CompactCell, ExtendedCell, GridCell};
use bitflags::bitflags;

bitflags! {
    /// Flags on a grid line.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct LineFlags: u8 {
        /// Line was wrapped from previous line.
        const WRAPPED       = 0x01;
        /// Line is dead (to be collected).
        const DEAD          = 0x02;
        /// Start of shell prompt (OSC 133).
        const START_PROMPT  = 0x04;
        /// Start of command output (OSC 133).
        const START_OUTPUT  = 0x08;
    }
}

/// A single line in the grid, containing cells and their extended data.
#[derive(Debug, Clone)]
pub struct GridLine {
    /// Compact cell storage. Length is the number of cells that have been written.
    cells: Vec<CompactCell>,
    /// Extended cell storage for non-ASCII/RGB cells. Indexed by CompactCell.data
    /// when the EXTENDED flag is set.
    extended: Vec<ExtendedCell>,
    /// Line flags.
    pub flags: LineFlags,
    /// Timestamp when this line was created/modified (Unix time).
    pub time: i64,
}

impl GridLine {
    /// Create a new empty line.
    #[must_use]
    pub fn new() -> Self {
        Self { cells: Vec::new(), extended: Vec::new(), flags: LineFlags::empty(), time: 0 }
    }

    /// Create a new line with the given capacity.
    #[must_use]
    pub fn with_capacity(cap: u32) -> Self {
        Self {
            cells: Vec::with_capacity(cap as usize),
            extended: Vec::new(),
            flags: LineFlags::empty(),
            time: 0,
        }
    }

    /// Number of cells that have been written to this line.
    #[must_use]
    pub fn cell_count(&self) -> u32 {
        self.cells.len() as u32
    }

    /// Get a resolved cell at the given position.
    ///
    /// Returns the cleared cell if `x` is beyond the line's cell count.
    #[must_use]
    pub fn get_cell(&self, x: u32) -> GridCell {
        let x = x as usize;
        if x >= self.cells.len() {
            return GridCell::CLEARED;
        }
        let compact = &self.cells[x];
        if compact.is_extended() {
            let idx = compact.extended_index();
            let ext = self.extended.get(idx);
            GridCell::unpack(compact, ext)
        } else {
            GridCell::unpack(compact, None)
        }
    }

    /// Set a cell at the given position.
    ///
    /// Extends the line with cleared cells if `x` is beyond current length.
    pub fn set_cell(&mut self, x: u32, cell: &GridCell) {
        let x = x as usize;
        // Extend the line if needed
        if x >= self.cells.len() {
            self.cells.resize(x + 1, CompactCell::CLEARED);
        }

        let (mut compact, ext) = cell.pack();

        if let Some(ext_cell) = ext {
            // Need extended storage
            let ext_idx = self.extended.len();
            self.extended.push(ext_cell);
            compact.data = ext_idx as u8;
            self.cells[x] = compact;
        } else {
            // Remove EXTENDED flag in case this cell was previously extended
            self.cells[x] = compact;
        }
    }

    /// Clear cells from `start` to `end` (exclusive) with the given background color.
    pub fn clear_range(&mut self, start: u32, end: u32, bg: crate::style::Color) {
        let start = start as usize;
        let end = end.min(self.cells.len() as u32) as usize;

        let mut cleared = CompactCell::CLEARED;
        if !bg.is_default() {
            match bg {
                crate::style::Color::Palette(idx) => {
                    cleared.bg = idx;
                    cleared.flags |= CellFlags::BG256;
                }
                crate::style::Color::Rgb { .. } => {
                    // Need extended cell for RGB background on cleared cells
                    // For simplicity, just use default for now
                }
                crate::style::Color::Default => {}
            }
        }

        for cell in &mut self.cells[start..end] {
            *cell = cleared;
        }
    }

    /// Truncate the line to the given number of cells.
    pub fn truncate(&mut self, len: u32) {
        self.cells.truncate(len as usize);
    }

    /// Fill the line to the given width with cleared cells.
    pub fn fill_to(&mut self, width: u32) {
        if (self.cells.len() as u32) < width {
            self.cells.resize(width as usize, CompactCell::CLEARED);
        }
    }

    /// Compact extended storage by removing unreferenced entries.
    ///
    /// Call this periodically to reclaim memory from deleted extended cells.
    pub fn compact_extended(&mut self) {
        if self.extended.is_empty() {
            return;
        }

        // Build a mapping from old indices to new indices
        let mut used = vec![false; self.extended.len()];
        for cell in &self.cells {
            if cell.is_extended() {
                let idx = cell.extended_index();
                if idx < used.len() {
                    used[idx] = true;
                }
            }
        }

        // Rebuild extended array, updating references
        let mut new_extended = Vec::new();
        let mut old_to_new = vec![0usize; self.extended.len()];

        for (old_idx, is_used) in used.iter().enumerate() {
            if *is_used {
                old_to_new[old_idx] = new_extended.len();
                new_extended.push(self.extended[old_idx].clone());
            }
        }

        // Update cell references
        for cell in &mut self.cells {
            if cell.is_extended() {
                let old_idx = cell.extended_index();
                if old_idx < old_to_new.len() {
                    cell.data = old_to_new[old_idx] as u8;
                }
            }
        }

        self.extended = new_extended;
    }

    /// Returns true if the line has no meaningful content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells
            .iter()
            .all(|c| !c.is_extended() && c.data == b' ' && c.attrs == 0 && c.fg == 8 && c.bg == 8)
    }

    /// Get raw compact cells slice (for benchmarking/testing).
    #[must_use]
    pub fn compact_cells(&self) -> &[CompactCell] {
        &self.cells
    }
}

impl Default for GridLine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Style};
    use crate::utf8::Utf8Char;

    #[test]
    fn new_line_is_empty() {
        let line = GridLine::new();
        assert_eq!(line.cell_count(), 0);
        assert!(line.is_empty());
    }

    #[test]
    fn set_and_get_ascii_cell() {
        let mut line = GridLine::new();
        let cell = GridCell {
            data: Utf8Char::from_ascii(b'H'),
            style: Style { fg: Color::RED, ..Style::DEFAULT },
            link: 0,
            flags: CellFlags::empty(),
        };
        line.set_cell(0, &cell);
        let got = line.get_cell(0);
        assert_eq!(got.data, cell.data);
        assert_eq!(got.style.fg, Color::RED);
    }

    #[test]
    fn get_beyond_end_returns_cleared() {
        let line = GridLine::new();
        let cell = line.get_cell(100);
        assert_eq!(cell, GridCell::CLEARED);
    }

    #[test]
    fn set_cell_extends_line() {
        let mut line = GridLine::new();
        let cell = GridCell { data: Utf8Char::from_ascii(b'X'), ..GridCell::CLEARED };
        line.set_cell(5, &cell);
        assert_eq!(line.cell_count(), 6);
        // Cells 0-4 should be cleared
        assert_eq!(line.get_cell(0), GridCell::CLEARED);
        assert_eq!(line.get_cell(5).data, Utf8Char::from_ascii(b'X'));
    }

    #[test]
    fn unicode_cell_uses_extended() {
        let mut line = GridLine::new();
        let cell = GridCell {
            data: Utf8Char::from_char('世'),
            style: Style::DEFAULT,
            link: 0,
            flags: CellFlags::empty(),
        };
        line.set_cell(0, &cell);
        let got = line.get_cell(0);
        assert_eq!(got.data, Utf8Char::from_char('世'));
    }

    #[test]
    fn clear_range() {
        let mut line = GridLine::with_capacity(10);
        for i in 0..10u32 {
            let cell = GridCell {
                data: Utf8Char::from_ascii(b'A' + i as u8),
                style: Style { fg: Color::RED, ..Style::DEFAULT },
                link: 0,
                flags: CellFlags::empty(),
            };
            line.set_cell(i, &cell);
        }
        line.clear_range(3, 7, Color::Default);
        assert_eq!(line.get_cell(2).data, Utf8Char::from_ascii(b'C'));
        assert!(line.get_cell(3).flags.contains(CellFlags::CLEARED));
        assert!(line.get_cell(6).flags.contains(CellFlags::CLEARED));
        assert_eq!(line.get_cell(7).data, Utf8Char::from_ascii(b'H'));
    }

    #[test]
    fn fill_to_width() {
        let mut line = GridLine::new();
        line.set_cell(0, &GridCell { data: Utf8Char::from_ascii(b'A'), ..GridCell::CLEARED });
        line.fill_to(80);
        assert_eq!(line.cell_count(), 80);
    }

    #[test]
    fn line_flags() {
        let mut line = GridLine::new();
        line.flags = LineFlags::WRAPPED | LineFlags::START_PROMPT;
        assert!(line.flags.contains(LineFlags::WRAPPED));
        assert!(line.flags.contains(LineFlags::START_PROMPT));
    }
}
