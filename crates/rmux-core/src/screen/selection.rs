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
        Self { sel_type, start_x: x, start_y: y, end_x: x, end_y: y, active: true }
    }

    /// Normalize the selection so start <= end.
    #[must_use]
    pub fn normalized(&self) -> (u32, u32, u32, u32) {
        if self.start_y < self.end_y || (self.start_y == self.end_y && self.start_x <= self.end_x) {
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
            SelectionType::Block => {
                // For block selections, x ranges are independent of y ordering.
                let (min_x, max_x) = (self.start_x.min(self.end_x), self.start_x.max(self.end_x));
                x >= min_x && x <= max_x && y >= sy && y <= ey
            }
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

    #[test]
    fn backward_normal_selection() {
        // Start coordinates are after end coordinates (user selected backwards).
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 10,
            start_y: 4,
            end_x: 5,
            end_y: 2,
            active: true,
        };
        // Should still contain the same cells as the forward selection.
        assert!(sel.contains(5, 2));
        assert!(sel.contains(80, 3)); // middle line, any column
        assert!(sel.contains(10, 4));
        assert!(!sel.contains(4, 2)); // before start on first line
        assert!(!sel.contains(11, 4)); // after end on last line
        assert!(!sel.contains(0, 1)); // above selection
        assert!(!sel.contains(0, 5)); // below selection
    }

    #[test]
    fn single_cell_selection() {
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 7,
            start_y: 3,
            end_x: 7,
            end_y: 3,
            active: true,
        };
        assert!(sel.contains(7, 3));
        assert!(!sel.contains(6, 3));
        assert!(!sel.contains(8, 3));
        assert!(!sel.contains(7, 2));
        assert!(!sel.contains(7, 4));
    }

    #[test]
    fn normalized_backward() {
        // When start is after end, normalized() should flip them.
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 15,
            start_y: 10,
            end_x: 3,
            end_y: 5,
            active: true,
        };
        let (sx, sy, ex, ey) = sel.normalized();
        assert_eq!(sx, 3);
        assert_eq!(sy, 5);
        assert_eq!(ex, 15);
        assert_eq!(ey, 10);

        // Forward selection should remain unchanged.
        let sel2 = Selection {
            sel_type: SelectionType::Normal,
            start_x: 3,
            start_y: 5,
            end_x: 15,
            end_y: 10,
            active: true,
        };
        let (sx2, sy2, ex2, ey2) = sel2.normalized();
        assert_eq!(sx2, 3);
        assert_eq!(sy2, 5);
        assert_eq!(ex2, 15);
        assert_eq!(ey2, 10);
    }

    #[test]
    fn block_selection_edge_columns() {
        // Block selection where start_x == end_x (single-column block).
        let sel = Selection {
            sel_type: SelectionType::Block,
            start_x: 5,
            start_y: 2,
            end_x: 5,
            end_y: 6,
            active: true,
        };
        assert!(sel.contains(5, 2));
        assert!(sel.contains(5, 4));
        assert!(sel.contains(5, 6));
        assert!(!sel.contains(4, 4)); // column to the left
        assert!(!sel.contains(6, 4)); // column to the right
        assert!(!sel.contains(5, 1)); // row above
        assert!(!sel.contains(5, 7)); // row below
    }

    #[test]
    fn line_selection_x_independent() {
        // Line selection should contain any x value on the selected lines.
        let sel = Selection {
            sel_type: SelectionType::Line,
            start_x: 50,
            start_y: 3,
            end_x: 10,
            end_y: 5,
            active: true,
        };
        // Any x on lines 3, 4, 5 should be contained.
        for y in 3..=5 {
            assert!(sel.contains(0, y), "x=0, y={y} should be in line selection");
            assert!(sel.contains(100, y), "x=100, y={y} should be in line selection");
            assert!(sel.contains(u32::MAX, y), "x=MAX, y={y} should be in line selection");
        }
        // Lines outside the range should not be contained.
        assert!(!sel.contains(50, 2));
        assert!(!sel.contains(10, 6));
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn normalized_always_has_start_before_end(
                x1 in 0u32..200, y1 in 0u32..200,
                x2 in 0u32..200, y2 in 0u32..200,
            ) {
                let mut sel = Selection::new(x1, y1, SelectionType::Normal);
                sel.end_x = x2;
                sel.end_y = y2;
                let (sx, sy, ex, ey) = sel.normalized();
                // start_y <= end_y, and if equal, start_x <= end_x
                prop_assert!(
                    sy < ey || (sy == ey && sx <= ex),
                    "normalized violated: start=({sx},{sy}) end=({ex},{ey})"
                );
            }

            #[test]
            fn contains_always_true_for_start_point(
                x in 0u32..200, y in 0u32..200,
            ) {
                // A single-point selection (start == end) always contains its own point
                let sel = Selection::new(x, y, SelectionType::Normal);
                prop_assert!(sel.contains(x, y));
            }

            #[test]
            fn block_contains_all_interior_points(
                x1 in 0u32..100, y1 in 0u32..100,
                x2 in 0u32..100, y2 in 0u32..100,
                dx in 0u32..100, dy in 0u32..100,
            ) {
                let mut sel = Selection::new(x1, y1, SelectionType::Block);
                sel.end_x = x2;
                sel.end_y = y2;
                // For block selections, use actual x/y min/max since
                // normalized() orders by row first, not column.
                let min_x = x1.min(x2);
                let max_x = x1.max(x2);
                let min_y = y1.min(y2);
                let max_y = y1.max(y2);
                let x_range = max_x - min_x + 1;
                let y_range = max_y - min_y + 1;
                let test_x = min_x + dx % x_range;
                let test_y = min_y + dy % y_range;
                prop_assert!(sel.contains(test_x, test_y));
            }

            #[test]
            fn line_selection_ignores_x(
                x1 in 0u32..200, y1 in 0u32..200,
                x2 in 0u32..200, y2 in 0u32..200,
                test_x in 0u32..500,
            ) {
                let mut sel = Selection::new(x1, y1, SelectionType::Line);
                sel.end_x = x2;
                sel.end_y = y2;
                let (_, sy, _, ey) = sel.normalized();
                // For any y in range, any x value should be contained
                if sy <= ey {
                    let test_y = sy;
                    prop_assert!(sel.contains(test_x, test_y));
                }
            }
        }
    }
}
