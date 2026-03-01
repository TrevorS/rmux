//! Selection state for copy mode.

/// Selection type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    /// Normal character-wise selection.
    Normal,
    /// Line-wise selection.
    Line,
    /// Block (rectangular) selection.
    Block,
}

/// A selection range on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Selection type.
    pub sel_type: SelectionType,
    /// Start column.
    pub start_x: u32,
    /// Start row (absolute, including history offset).
    pub start_y: u32,
    /// End column.
    pub end_x: u32,
    /// End row.
    pub end_y: u32,
    /// Whether the selection is active (being extended).
    pub active: bool,
}

impl Selection {
    /// Create a new selection starting at the given position.
    #[must_use]
    pub fn new(x: u32, y: u32, sel_type: SelectionType) -> Self {
        Self {
            sel_type,
            start_x: x,
            start_y: y,
            end_x: x,
            end_y: y,
            active: true,
        }
    }

    /// Normalize the selection so start <= end.
    #[must_use]
    pub fn normalized(&self) -> (u32, u32, u32, u32) {
        if self.start_y < self.end_y
            || (self.start_y == self.end_y && self.start_x <= self.end_x)
        {
            (self.start_x, self.start_y, self.end_x, self.end_y)
        } else {
            (self.end_x, self.end_y, self.start_x, self.start_y)
        }
    }

    /// Check if a cell at (x, y) is within this selection.
    #[must_use]
    pub fn contains(&self, x: u32, y: u32) -> bool {
        let (sx, sy, ex, ey) = self.normalized();
        match self.sel_type {
            SelectionType::Normal => {
                if y < sy || y > ey {
                    return false;
                }
                if y == sy && y == ey {
                    return x >= sx && x <= ex;
                }
                if y == sy {
                    return x >= sx;
                }
                if y == ey {
                    return x <= ex;
                }
                true
            }
            SelectionType::Line => y >= sy && y <= ey,
            SelectionType::Block => x >= sx && x <= ex && y >= sy && y <= ey,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_selection_contains() {
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 5,
            start_y: 2,
            end_x: 10,
            end_y: 4,
            active: true,
        };
        assert!(sel.contains(5, 2));
        assert!(sel.contains(80, 3)); // middle line
        assert!(sel.contains(10, 4));
        assert!(!sel.contains(4, 2));
        assert!(!sel.contains(11, 4));
        assert!(!sel.contains(0, 5));
    }

    #[test]
    fn block_selection_contains() {
        let sel = Selection {
            sel_type: SelectionType::Block,
            start_x: 3,
            start_y: 1,
            end_x: 8,
            end_y: 5,
            active: true,
        };
        assert!(sel.contains(5, 3));
        assert!(!sel.contains(2, 3));
        assert!(!sel.contains(9, 3));
    }

    #[test]
    fn line_selection_contains() {
        let sel = Selection {
            sel_type: SelectionType::Line,
            start_x: 5,
            start_y: 2,
            end_x: 10,
            end_y: 4,
            active: true,
        };
        assert!(sel.contains(0, 2)); // entire line 2
        assert!(sel.contains(80, 3)); // entire line 3
        assert!(!sel.contains(0, 5));
    }
}
