//! Layout tree for arranging panes within a window.
//!
//! The layout is a tree of cells. Internal nodes are either horizontal (left-right)
//! or vertical (top-bottom) splits. Leaf nodes correspond to panes.
//! This matches tmux's `struct layout_cell`.

/// Layout cell type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutType {
    /// Left-right split (children are arranged horizontally).
    LeftRight,
    /// Top-bottom split (children are arranged vertically).
    TopBottom,
    /// Leaf node (a pane).
    Pane,
}

/// A node in the layout tree.
#[derive(Debug, Clone)]
pub struct LayoutCell {
    /// Cell type.
    pub cell_type: LayoutType,
    /// Position (column offset within parent).
    pub x_off: u32,
    /// Position (row offset within parent).
    pub y_off: u32,
    /// Width (columns).
    pub sx: u32,
    /// Height (rows).
    pub sy: u32,
    /// Pane ID (only valid for LayoutType::Pane).
    pub pane_id: Option<u32>,
    /// Child cells (for LeftRight and TopBottom).
    pub children: Vec<LayoutCell>,
}

/// Minimum pane size.
pub const PANE_MINIMUM_WIDTH: u32 = 1;
pub const PANE_MINIMUM_HEIGHT: u32 = 1;

impl LayoutCell {
    /// Create a new pane leaf cell.
    #[must_use]
    pub fn new_pane(x: u32, y: u32, sx: u32, sy: u32, pane_id: u32) -> Self {
        Self {
            cell_type: LayoutType::Pane,
            x_off: x,
            y_off: y,
            sx,
            sy,
            pane_id: Some(pane_id),
            children: Vec::new(),
        }
    }

    /// Create a new split node.
    #[must_use]
    pub fn new_split(cell_type: LayoutType, x: u32, y: u32, sx: u32, sy: u32) -> Self {
        debug_assert!(cell_type != LayoutType::Pane);
        Self { cell_type, x_off: x, y_off: y, sx, sy, pane_id: None, children: Vec::new() }
    }

    /// Whether this is a leaf (pane) node.
    #[must_use]
    pub fn is_pane(&self) -> bool {
        self.cell_type == LayoutType::Pane
    }

    /// Split this pane horizontally (creating left and right).
    ///
    /// Returns the new pane cell. The original pane becomes the left child.
    pub fn split_horizontal(&mut self, new_pane_id: u32) -> Option<&LayoutCell> {
        if !self.is_pane() || self.sx < PANE_MINIMUM_WIDTH * 2 + 1 {
            return None;
        }

        let old_pane_id = self.pane_id.take();
        let half = self.sx / 2;

        let left =
            LayoutCell::new_pane(self.x_off, self.y_off, half, self.sy, old_pane_id.unwrap_or(0));
        let right = LayoutCell::new_pane(
            self.x_off + half + 1, // +1 for separator
            self.y_off,
            self.sx - half - 1,
            self.sy,
            new_pane_id,
        );

        self.cell_type = LayoutType::LeftRight;
        self.children = vec![left, right];
        self.children.last()
    }

    /// Split this pane vertically (creating top and bottom).
    pub fn split_vertical(&mut self, new_pane_id: u32) -> Option<&LayoutCell> {
        if !self.is_pane() || self.sy < PANE_MINIMUM_HEIGHT * 2 + 1 {
            return None;
        }

        let old_pane_id = self.pane_id.take();
        let half = self.sy / 2;

        let top =
            LayoutCell::new_pane(self.x_off, self.y_off, self.sx, half, old_pane_id.unwrap_or(0));
        let bottom = LayoutCell::new_pane(
            self.x_off,
            self.y_off + half + 1, // +1 for separator
            self.sx,
            self.sy - half - 1,
            new_pane_id,
        );

        self.cell_type = LayoutType::TopBottom;
        self.children = vec![top, bottom];
        self.children.last()
    }

    /// Find the cell for a given pane ID.
    #[must_use]
    pub fn find_pane(&self, pane_id: u32) -> Option<&LayoutCell> {
        if self.is_pane() && self.pane_id == Some(pane_id) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_pane(pane_id) {
                return Some(found);
            }
        }
        None
    }

    /// Find the pane ID at screen coordinates (x, y).
    ///
    /// Returns the pane ID of the leaf cell whose region contains (x, y), or `None`.
    #[must_use]
    pub fn pane_at(&self, x: u32, y: u32) -> Option<u32> {
        if self.is_pane() {
            if x >= self.x_off
                && x < self.x_off + self.sx
                && y >= self.y_off
                && y < self.y_off + self.sy
            {
                return self.pane_id;
            }
            return None;
        }
        for child in &self.children {
            if let Some(pid) = child.pane_at(x, y) {
                return Some(pid);
            }
        }
        None
    }

    /// Count the number of panes in this layout subtree.
    #[must_use]
    pub fn pane_count(&self) -> usize {
        if self.is_pane() { 1 } else { self.children.iter().map(LayoutCell::pane_count).sum() }
    }

    /// Collect all pane IDs in this layout.
    #[must_use]
    pub fn pane_ids(&self) -> Vec<u32> {
        let mut ids = Vec::new();
        self.collect_pane_ids(&mut ids);
        ids
    }

    fn collect_pane_ids(&self, ids: &mut Vec<u32>) {
        if let Some(id) = self.pane_id {
            ids.push(id);
        }
        for child in &self.children {
            child.collect_pane_ids(ids);
        }
    }
}

/// Create an even-horizontal layout for the given panes.
#[must_use]
pub fn layout_even_horizontal(sx: u32, sy: u32, pane_ids: &[u32]) -> LayoutCell {
    if pane_ids.len() <= 1 {
        return LayoutCell::new_pane(0, 0, sx, sy, pane_ids.first().copied().unwrap_or(0));
    }

    let n = pane_ids.len() as u32;
    let separators = n - 1;
    let available = sx.saturating_sub(separators);
    let base_width = available / n;
    let extra = (available % n) as usize;

    let mut root = LayoutCell::new_split(LayoutType::LeftRight, 0, 0, sx, sy);
    let mut x = 0;
    for (i, &pane_id) in pane_ids.iter().enumerate() {
        let w = base_width + if i < extra { 1 } else { 0 };
        root.children.push(LayoutCell::new_pane(x, 0, w, sy, pane_id));
        x += w + 1; // +1 for separator
    }
    root
}

/// Create an even-vertical layout for the given panes.
#[must_use]
pub fn layout_even_vertical(sx: u32, sy: u32, pane_ids: &[u32]) -> LayoutCell {
    if pane_ids.len() <= 1 {
        return LayoutCell::new_pane(0, 0, sx, sy, pane_ids.first().copied().unwrap_or(0));
    }

    let n = pane_ids.len() as u32;
    let separators = n - 1;
    let available = sy.saturating_sub(separators);
    let base_height = available / n;
    let extra = (available % n) as usize;

    let mut root = LayoutCell::new_split(LayoutType::TopBottom, 0, 0, sx, sy);
    let mut y = 0;
    for (i, &pane_id) in pane_ids.iter().enumerate() {
        let h = base_height + if i < extra { 1 } else { 0 };
        root.children.push(LayoutCell::new_pane(0, y, sx, h, pane_id));
        y += h + 1;
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_pane_layout() {
        let cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        assert!(cell.is_pane());
        assert_eq!(cell.pane_count(), 1);
    }

    #[test]
    fn horizontal_split() {
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        assert!(cell.split_horizontal(1).is_some());
        assert_eq!(cell.cell_type, LayoutType::LeftRight);
        assert_eq!(cell.pane_count(), 2);
        assert!(cell.find_pane(0).is_some());
        assert!(cell.find_pane(1).is_some());
    }

    #[test]
    fn vertical_split() {
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        assert!(cell.split_vertical(1).is_some());
        assert_eq!(cell.cell_type, LayoutType::TopBottom);
        assert_eq!(cell.pane_count(), 2);
    }

    #[test]
    fn even_horizontal() {
        let layout = layout_even_horizontal(80, 24, &[0, 1, 2]);
        assert_eq!(layout.pane_count(), 3);
        let ids = layout.pane_ids();
        assert!(ids.contains(&0));
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[test]
    fn even_vertical() {
        let layout = layout_even_vertical(80, 24, &[0, 1]);
        assert_eq!(layout.pane_count(), 2);
    }

    #[test]
    fn split_too_small_fails() {
        let mut cell = LayoutCell::new_pane(0, 0, 2, 2, 0);
        assert!(cell.split_horizontal(1).is_none());
    }

    #[test]
    fn triple_horizontal_split() {
        // Split horizontally 3 times to get 3 panes.
        // Start with a wide enough pane (80 columns).
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        assert!(cell.split_horizontal(1).is_some());
        // Now cell is LeftRight with children [pane0, pane1].
        assert_eq!(cell.pane_count(), 2);

        // Split the second child (pane1) horizontally again.
        let result = cell.children[1].split_horizontal(2);
        assert!(result.is_some());
        assert_eq!(cell.pane_count(), 3);

        // Verify all three panes exist.
        assert!(cell.find_pane(0).is_some());
        assert!(cell.find_pane(1).is_some());
        assert!(cell.find_pane(2).is_some());

        // All panes should have the same height as the original.
        let p0 = cell.find_pane(0).unwrap();
        let p1 = cell.find_pane(1).unwrap();
        let p2 = cell.find_pane(2).unwrap();
        assert_eq!(p0.sy, 24);
        assert_eq!(p1.sy, 24);
        assert_eq!(p2.sy, 24);

        // Widths should add up (with separators) to the original width.
        // p0.sx + 1 + p1.sx + 1 + p2.sx = 80
        assert_eq!(p0.sx + 1 + p1.sx + 1 + p2.sx, 80);
    }

    #[test]
    fn nested_split() {
        // Split horizontally then split one pane vertically to get 3 panes.
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        assert!(cell.split_horizontal(1).is_some());
        assert_eq!(cell.pane_count(), 2);

        // Split the right pane (pane1) vertically.
        let result = cell.children[1].split_vertical(2);
        assert!(result.is_some());
        assert_eq!(cell.pane_count(), 3);

        // Verify all panes exist.
        assert!(cell.find_pane(0).is_some());
        assert!(cell.find_pane(1).is_some());
        assert!(cell.find_pane(2).is_some());

        // Left pane should be full height.
        let p0 = cell.find_pane(0).unwrap();
        assert_eq!(p0.sy, 24);

        // Right-side panes (top and bottom) should share the height.
        let p1 = cell.find_pane(1).unwrap();
        let p2 = cell.find_pane(2).unwrap();
        assert_eq!(p1.sx, p2.sx); // Same width (they're stacked vertically).
        // Heights should add up with separator.
        assert_eq!(p1.sy + 1 + p2.sy, 24);
    }

    #[test]
    fn find_pane_nonexistent() {
        let cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        assert!(cell.find_pane(999).is_none());

        // Also test in a split layout.
        let mut split = LayoutCell::new_pane(0, 0, 80, 24, 0);
        split.split_horizontal(1);
        assert!(split.find_pane(999).is_none());
    }

    #[test]
    fn pane_at_boundary() {
        // Create a horizontal split and test the boundary between panes.
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        cell.split_horizontal(1);

        let left = cell.find_pane(0).unwrap();
        let right = cell.find_pane(1).unwrap();

        // The last column of the left pane should still be left pane.
        let left_last_col = left.x_off + left.sx - 1;
        assert_eq!(cell.pane_at(left_last_col, 0), Some(0));

        // The first column of the right pane should be right pane.
        assert_eq!(cell.pane_at(right.x_off, 0), Some(1));

        // The separator column (between left and right) should return None.
        let separator_col = left.x_off + left.sx;
        assert!(
            separator_col < right.x_off,
            "There should be a gap (separator) between left and right"
        );
        assert_eq!(cell.pane_at(separator_col, 0), None);
    }

    #[test]
    fn pane_ids_returns_all() {
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        cell.split_horizontal(1);
        cell.children[1].split_vertical(2);

        let ids = cell.pane_ids();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&0));
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[test]
    fn pane_count_matches_ids() {
        // Single pane.
        let cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        assert_eq!(cell.pane_count(), cell.pane_ids().len());

        // After splits.
        let mut cell2 = LayoutCell::new_pane(0, 0, 80, 24, 0);
        cell2.split_horizontal(1);
        assert_eq!(cell2.pane_count(), cell2.pane_ids().len());

        cell2.children[0].split_vertical(2);
        assert_eq!(cell2.pane_count(), cell2.pane_ids().len());
    }

    #[test]
    fn split_minimum_size() {
        // A pane needs at least PANE_MINIMUM_WIDTH * 2 + 1 = 3 columns for horizontal split.
        let mut cell_exact_min = LayoutCell::new_pane(0, 0, 3, 10, 0);
        assert!(cell_exact_min.split_horizontal(1).is_some());

        // At exactly PANE_MINIMUM_WIDTH * 2 = 2, it should fail.
        let mut cell_too_small = LayoutCell::new_pane(0, 0, 2, 10, 0);
        assert!(cell_too_small.split_horizontal(1).is_none());

        // Width 1 should also fail.
        let mut cell_tiny = LayoutCell::new_pane(0, 0, 1, 10, 0);
        assert!(cell_tiny.split_horizontal(1).is_none());

        // Vertical split: needs PANE_MINIMUM_HEIGHT * 2 + 1 = 3 rows.
        let mut cell_vert_ok = LayoutCell::new_pane(0, 0, 10, 3, 0);
        assert!(cell_vert_ok.split_vertical(1).is_some());

        let mut cell_vert_fail = LayoutCell::new_pane(0, 0, 10, 2, 0);
        assert!(cell_vert_fail.split_vertical(1).is_none());
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn pane_count_matches_pane_ids_len(
                width in 20u32..200, height in 10u32..100,
                pane_id in 0u32..1000,
            ) {
                let layout = LayoutCell::new_pane(0, 0, width, height, pane_id);
                prop_assert_eq!(layout.pane_count(), layout.pane_ids().len());
            }

            #[test]
            fn pane_at_within_bounds_returns_some(
                width in 20u32..200, height in 10u32..100,
                pane_id in 0u32..1000,
            ) {
                let layout = LayoutCell::new_pane(0, 0, width, height, pane_id);
                // Any coordinate strictly within the pane bounds should return Some
                let x = width / 2;
                let y = height / 2;
                prop_assert_eq!(layout.pane_at(x, y), Some(pane_id));
            }

            #[test]
            fn pane_at_outside_bounds_returns_none(
                width in 20u32..200, height in 10u32..100,
                pane_id in 0u32..1000,
            ) {
                let layout = LayoutCell::new_pane(0, 0, width, height, pane_id);
                // Coordinates outside bounds should return None
                prop_assert!(layout.pane_at(width, 0).is_none());
                prop_assert!(layout.pane_at(0, height).is_none());
            }

            #[test]
            fn horizontal_split_preserves_pane_count(
                width in 20u32..200, height in 10u32..100,
            ) {
                let mut layout = LayoutCell::new_pane(0, 0, width, height, 0);
                if width > PANE_MINIMUM_WIDTH * 2 {
                    layout.split_horizontal(1);
                    prop_assert_eq!(layout.pane_count(), 2);
                    prop_assert_eq!(layout.pane_ids().len(), 2);
                }
            }

            #[test]
            fn vertical_split_preserves_pane_count(
                width in 20u32..200, height in 10u32..100,
            ) {
                let mut layout = LayoutCell::new_pane(0, 0, width, height, 0);
                if height > PANE_MINIMUM_HEIGHT * 2 {
                    layout.split_vertical(1);
                    prop_assert_eq!(layout.pane_count(), 2);
                    prop_assert_eq!(layout.pane_ids().len(), 2);
                }
            }

            #[test]
            fn even_horizontal_pane_count(
                width in 20u32..200, height in 10u32..100,
                n_panes in 1u32..6,
            ) {
                let pane_ids: Vec<u32> = (0..n_panes).collect();
                let layout = layout_even_horizontal(width, height, &pane_ids);
                prop_assert_eq!(layout.pane_count(), n_panes as usize);
            }

            #[test]
            fn even_vertical_pane_count(
                width in 20u32..200, height in 10u32..100,
                n_panes in 1u32..6,
            ) {
                let pane_ids: Vec<u32> = (0..n_panes).collect();
                let layout = layout_even_vertical(width, height, &pane_ids);
                prop_assert_eq!(layout.pane_count(), n_panes as usize);
            }

            #[test]
            fn find_pane_returns_correct_id(
                width in 20u32..200, height in 10u32..100,
                pane_id in 0u32..1000,
            ) {
                let layout = LayoutCell::new_pane(0, 0, width, height, pane_id);
                let found = layout.find_pane(pane_id);
                prop_assert!(found.is_some());
                prop_assert_eq!(found.unwrap().pane_id, Some(pane_id));
            }
        }
    }
}
