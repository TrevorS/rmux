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

    /// Resize a pane in the given direction by the given amount.
    ///
    /// Returns `true` if the resize succeeded. The caller must update pane
    /// screen sizes after this call.
    pub fn resize_pane(&mut self, pane_id: u32, direction: ResizeDirection, amount: u32) -> bool {
        // Find the path to the pane
        let mut path = Vec::new();
        if !self.find_path(pane_id, &mut path) {
            return false;
        }

        // Walk up the path to find the nearest split that can accommodate this resize
        self.apply_resize(&path, 0, direction, amount)
    }

    /// Find the path of child indices to reach a pane.
    fn find_path(&self, pane_id: u32, path: &mut Vec<usize>) -> bool {
        if self.is_pane() {
            return self.pane_id == Some(pane_id);
        }
        for (i, child) in self.children.iter().enumerate() {
            path.push(i);
            if child.find_path(pane_id, path) {
                return true;
            }
            path.pop();
        }
        false
    }

    /// Apply resize along the path.
    fn apply_resize(
        &mut self,
        path: &[usize],
        depth: usize,
        direction: ResizeDirection,
        amount: u32,
    ) -> bool {
        if depth >= path.len() || self.is_pane() {
            return false;
        }

        let child_idx = path[depth];
        let n = self.children.len();

        // Check if this split level can handle the resize direction
        let can_handle = matches!(
            (self.cell_type, direction),
            (LayoutType::LeftRight, ResizeDirection::Left | ResizeDirection::Right)
                | (LayoutType::TopBottom, ResizeDirection::Up | ResizeDirection::Down)
        );

        if can_handle {
            // Find the sibling to steal/give space from
            let (shrink_idx, grow_idx) = match direction {
                ResizeDirection::Left | ResizeDirection::Up => {
                    if child_idx == 0 {
                        return false;
                    }
                    (child_idx - 1, child_idx)
                }
                ResizeDirection::Right | ResizeDirection::Down => {
                    if child_idx + 1 >= n {
                        return false;
                    }
                    (child_idx + 1, child_idx)
                }
            };

            let is_horizontal = matches!(self.cell_type, LayoutType::LeftRight);

            let (shrink_size, _grow_size, min_size) = if is_horizontal {
                (self.children[shrink_idx].sx, self.children[grow_idx].sx, PANE_MINIMUM_WIDTH)
            } else {
                (self.children[shrink_idx].sy, self.children[grow_idx].sy, PANE_MINIMUM_HEIGHT)
            };

            let actual_amount = amount.min(shrink_size.saturating_sub(min_size));
            if actual_amount == 0 {
                return false;
            }

            if is_horizontal {
                // Adjust widths
                self.children[shrink_idx].sx -= actual_amount;
                self.children[grow_idx].sx += actual_amount;
                // Fix offsets: recalculate x_off for all children
                let mut x = self.x_off;
                for child in &mut self.children {
                    child.x_off = x;
                    resize_subtree_width(child);
                    x += child.sx + 1; // +1 for separator
                }
            } else {
                // Adjust heights
                self.children[shrink_idx].sy -= actual_amount;
                self.children[grow_idx].sy += actual_amount;
                // Fix offsets
                let mut y = self.y_off;
                for child in &mut self.children {
                    child.y_off = y;
                    resize_subtree_height(child);
                    y += child.sy + 1;
                }
            }

            return true;
        }

        // This split level doesn't handle this direction — recurse deeper
        self.children[child_idx].apply_resize(path, depth + 1, direction, amount)
    }

    /// Resize this layout tree to new dimensions, redistributing space.
    pub fn resize_layout(&mut self, new_sx: u32, new_sy: u32) {
        let old_sx = self.sx;
        let old_sy = self.sy;
        self.sx = new_sx;
        self.sy = new_sy;

        if self.is_pane() {
            return;
        }

        match self.cell_type {
            LayoutType::LeftRight => {
                // Redistribute width proportionally
                let n = self.children.len() as u32;
                let separators = n.saturating_sub(1);
                let old_avail = old_sx.saturating_sub(separators).max(n);
                let new_avail = new_sx.saturating_sub(separators).max(n);

                let mut x = self.x_off;
                let mut remaining = new_avail;
                for (i, child) in self.children.iter_mut().enumerate() {
                    let new_w = if i + 1 == n as usize {
                        remaining
                    } else {
                        let proportion = (child.sx as u64 * new_avail as u64) / old_avail as u64;
                        (proportion as u32)
                            .max(PANE_MINIMUM_WIDTH)
                            .min(remaining - (n - 1 - i as u32))
                    };
                    child.x_off = x;
                    child.resize_layout(new_w, new_sy);
                    x += new_w + 1;
                    remaining = remaining.saturating_sub(new_w);
                }
            }
            LayoutType::TopBottom => {
                let n = self.children.len() as u32;
                let separators = n.saturating_sub(1);
                let old_avail = old_sy.saturating_sub(separators).max(n);
                let new_avail = new_sy.saturating_sub(separators).max(n);

                let mut y = self.y_off;
                let mut remaining = new_avail;
                for (i, child) in self.children.iter_mut().enumerate() {
                    let new_h = if i + 1 == n as usize {
                        remaining
                    } else {
                        let proportion = (child.sy as u64 * new_avail as u64) / old_avail as u64;
                        (proportion as u32)
                            .max(PANE_MINIMUM_HEIGHT)
                            .min(remaining - (n - 1 - i as u32))
                    };
                    child.y_off = y;
                    child.resize_layout(new_sx, new_h);
                    y += new_h + 1;
                    remaining = remaining.saturating_sub(new_h);
                }
            }
            LayoutType::Pane => unreachable!(),
        }
    }
}

/// Direction for resize operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeDirection {
    Up,
    Down,
    Left,
    Right,
}

/// After resizing a child's width, propagate to its subtree.
fn resize_subtree_width(cell: &mut LayoutCell) {
    if cell.is_pane() {
        return;
    }
    match cell.cell_type {
        LayoutType::LeftRight => {
            // Children share the width — redistribute proportionally
            // For simplicity, just leave them (the resize_layout handles full redistributions)
        }
        LayoutType::TopBottom => {
            // All children get the same width
            for child in &mut cell.children {
                child.x_off = cell.x_off;
                child.sx = cell.sx;
                resize_subtree_width(child);
            }
        }
        LayoutType::Pane => {}
    }
}

/// After resizing a child's height, propagate to its subtree.
fn resize_subtree_height(cell: &mut LayoutCell) {
    if cell.is_pane() {
        return;
    }
    match cell.cell_type {
        LayoutType::TopBottom => {}
        LayoutType::LeftRight => {
            for child in &mut cell.children {
                child.y_off = cell.y_off;
                child.sy = cell.sy;
                resize_subtree_height(child);
            }
        }
        LayoutType::Pane => {}
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

/// Create a main-horizontal layout: one large pane on top, others split below.
#[must_use]
pub fn layout_main_horizontal(sx: u32, sy: u32, pane_ids: &[u32]) -> LayoutCell {
    if pane_ids.len() <= 1 {
        return LayoutCell::new_pane(0, 0, sx, sy, pane_ids.first().copied().unwrap_or(0));
    }

    // Main pane gets roughly 2/3 of the height
    let main_height = (sy * 2 / 3).max(PANE_MINIMUM_HEIGHT);
    let bottom_height = sy.saturating_sub(main_height + 1); // -1 for separator

    let mut root = LayoutCell::new_split(LayoutType::TopBottom, 0, 0, sx, sy);
    root.children.push(LayoutCell::new_pane(0, 0, sx, main_height, pane_ids[0]));

    if pane_ids.len() == 2 {
        root.children.push(LayoutCell::new_pane(
            0,
            main_height + 1,
            sx,
            bottom_height,
            pane_ids[1],
        ));
    } else {
        // Multiple panes split horizontally on the bottom
        let bottom_ids = &pane_ids[1..];
        let mut bottom = layout_even_horizontal(sx, bottom_height, bottom_ids);
        bottom.y_off = main_height + 1;
        set_y_offset_recursive(&mut bottom, main_height + 1);
        root.children.push(bottom);
    }

    root
}

/// Create a main-vertical layout: one large pane on the left, others split right.
#[must_use]
pub fn layout_main_vertical(sx: u32, sy: u32, pane_ids: &[u32]) -> LayoutCell {
    if pane_ids.len() <= 1 {
        return LayoutCell::new_pane(0, 0, sx, sy, pane_ids.first().copied().unwrap_or(0));
    }

    // Main pane gets roughly 2/3 of the width
    let main_width = (sx * 2 / 3).max(PANE_MINIMUM_WIDTH);
    let right_width = sx.saturating_sub(main_width + 1);

    let mut root = LayoutCell::new_split(LayoutType::LeftRight, 0, 0, sx, sy);
    root.children.push(LayoutCell::new_pane(0, 0, main_width, sy, pane_ids[0]));

    if pane_ids.len() == 2 {
        root.children.push(LayoutCell::new_pane(main_width + 1, 0, right_width, sy, pane_ids[1]));
    } else {
        let right_ids = &pane_ids[1..];
        let mut right = layout_even_vertical(right_width, sy, right_ids);
        right.x_off = main_width + 1;
        set_x_offset_recursive(&mut right, main_width + 1);
        root.children.push(right);
    }

    root
}

/// Create a tiled layout: fill a grid as evenly as possible.
#[must_use]
pub fn layout_tiled(sx: u32, sy: u32, pane_ids: &[u32]) -> LayoutCell {
    if pane_ids.len() <= 1 {
        return LayoutCell::new_pane(0, 0, sx, sy, pane_ids.first().copied().unwrap_or(0));
    }

    let num_panes = pane_ids.len() as u32;
    // Determine grid dimensions: grid_cols x grid_rows
    let grid_cols = (num_panes as f64).sqrt().ceil() as u32;
    let grid_rows = num_panes.div_ceil(grid_cols);

    let row_seps = grid_rows.saturating_sub(1);
    let avail_h = sy.saturating_sub(row_seps);
    let base_h = avail_h / grid_rows;
    let extra_h = (avail_h % grid_rows) as usize;

    let mut root = LayoutCell::new_split(LayoutType::TopBottom, 0, 0, sx, sy);
    let mut cur_y = 0u32;
    let mut idx = 0usize;

    for row in 0..grid_rows {
        let row_height = base_h + if (row as usize) < extra_h { 1 } else { 0 };
        let panes_in_row = if row < grid_rows - 1 {
            grid_cols.min(num_panes - idx as u32)
        } else {
            num_panes - idx as u32
        };

        if panes_in_row == 1 {
            root.children.push(LayoutCell::new_pane(0, cur_y, sx, row_height, pane_ids[idx]));
            idx += 1;
        } else {
            let col_seps = panes_in_row.saturating_sub(1);
            let avail_w = sx.saturating_sub(col_seps);
            let base_w = avail_w / panes_in_row;
            let extra_w = (avail_w % panes_in_row) as usize;

            let mut row_cell =
                LayoutCell::new_split(LayoutType::LeftRight, 0, cur_y, sx, row_height);
            let mut cur_x = 0u32;
            for col in 0..panes_in_row {
                let col_width = base_w + if (col as usize) < extra_w { 1 } else { 0 };
                row_cell.children.push(LayoutCell::new_pane(
                    cur_x,
                    cur_y,
                    col_width,
                    row_height,
                    pane_ids[idx],
                ));
                cur_x += col_width + 1;
                idx += 1;
            }
            root.children.push(row_cell);
        }
        cur_y += row_height + 1;
    }

    // If there's only one row, unwrap the unnecessary TopBottom wrapper
    if root.children.len() == 1 {
        return root.children.remove(0);
    }

    root
}

/// Set x_off recursively for all pane children.
fn set_x_offset_recursive(cell: &mut LayoutCell, base_x: u32) {
    if cell.is_pane() {
        cell.x_off = base_x;
        return;
    }
    match cell.cell_type {
        LayoutType::LeftRight => {
            let mut x = base_x;
            for child in &mut cell.children {
                child.x_off = x;
                set_x_offset_recursive(child, x);
                x += child.sx + 1;
            }
        }
        LayoutType::TopBottom => {
            for child in &mut cell.children {
                child.x_off = base_x;
                set_x_offset_recursive(child, base_x);
            }
        }
        LayoutType::Pane => {}
    }
    cell.x_off = base_x;
}

/// Set y_off recursively for all pane children.
fn set_y_offset_recursive(cell: &mut LayoutCell, base_y: u32) {
    if cell.is_pane() {
        cell.y_off = base_y;
        return;
    }
    match cell.cell_type {
        LayoutType::TopBottom => {
            let mut y = base_y;
            for child in &mut cell.children {
                child.y_off = y;
                set_y_offset_recursive(child, y);
                y += child.sy + 1;
            }
        }
        LayoutType::LeftRight => {
            for child in &mut cell.children {
                child.y_off = base_y;
                set_y_offset_recursive(child, base_y);
            }
        }
        LayoutType::Pane => {}
    }
    cell.y_off = base_y;
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

    #[test]
    fn main_horizontal_single_pane() {
        let layout = layout_main_horizontal(80, 24, &[0]);
        assert!(layout.is_pane());
        assert_eq!(layout.pane_id, Some(0));
    }

    #[test]
    fn main_horizontal_two_panes() {
        let layout = layout_main_horizontal(80, 24, &[0, 1]);
        assert_eq!(layout.cell_type, LayoutType::TopBottom);
        assert_eq!(layout.pane_count(), 2);
        let p0 = layout.find_pane(0).unwrap();
        let p1 = layout.find_pane(1).unwrap();
        // Main pane should be taller
        assert!(p0.sy > p1.sy);
        // Heights plus separator should equal total
        assert_eq!(p0.sy + 1 + p1.sy, 24);
    }

    #[test]
    fn main_horizontal_three_panes() {
        let layout = layout_main_horizontal(80, 24, &[0, 1, 2]);
        assert_eq!(layout.pane_count(), 3);
        let p0 = layout.find_pane(0).unwrap();
        // Main pane is on top
        assert_eq!(p0.y_off, 0);
        assert_eq!(p0.sx, 80);
        // Bottom panes should be side by side
        let p1 = layout.find_pane(1).unwrap();
        let p2 = layout.find_pane(2).unwrap();
        assert_eq!(p1.y_off, p2.y_off); // same row
    }

    #[test]
    fn main_vertical_single_pane() {
        let layout = layout_main_vertical(80, 24, &[0]);
        assert!(layout.is_pane());
    }

    #[test]
    fn main_vertical_two_panes() {
        let layout = layout_main_vertical(80, 24, &[0, 1]);
        assert_eq!(layout.cell_type, LayoutType::LeftRight);
        assert_eq!(layout.pane_count(), 2);
        let p0 = layout.find_pane(0).unwrap();
        let p1 = layout.find_pane(1).unwrap();
        // Main pane should be wider
        assert!(p0.sx > p1.sx);
        assert_eq!(p0.sx + 1 + p1.sx, 80);
    }

    #[test]
    fn main_vertical_three_panes() {
        let layout = layout_main_vertical(80, 24, &[0, 1, 2]);
        assert_eq!(layout.pane_count(), 3);
        let p0 = layout.find_pane(0).unwrap();
        assert_eq!(p0.x_off, 0);
        assert_eq!(p0.sy, 24);
        // Right panes should be stacked vertically
        let p1 = layout.find_pane(1).unwrap();
        let p2 = layout.find_pane(2).unwrap();
        assert_eq!(p1.x_off, p2.x_off); // same column
        assert!(p1.y_off < p2.y_off); // p1 above p2
    }

    #[test]
    fn tiled_single_pane() {
        let layout = layout_tiled(80, 24, &[0]);
        assert!(layout.is_pane());
    }

    #[test]
    fn tiled_two_panes() {
        let layout = layout_tiled(80, 24, &[0, 1]);
        assert_eq!(layout.pane_count(), 2);
        let ids = layout.pane_ids();
        assert!(ids.contains(&0));
        assert!(ids.contains(&1));
    }

    #[test]
    fn tiled_four_panes_grid() {
        let layout = layout_tiled(80, 24, &[0, 1, 2, 3]);
        assert_eq!(layout.pane_count(), 4);
        // 4 panes should form a 2x2 grid
        let p0 = layout.find_pane(0).unwrap();
        let p3 = layout.find_pane(3).unwrap();
        // Diagonal corners should differ in both x and y
        assert!(p3.x_off > p0.x_off || p3.y_off > p0.y_off);
    }

    #[test]
    fn tiled_five_panes() {
        let layout = layout_tiled(80, 24, &[0, 1, 2, 3, 4]);
        assert_eq!(layout.pane_count(), 5);
    }

    #[test]
    fn resize_pane_horizontal() {
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        cell.split_horizontal(1);

        let left_width_before = cell.children[0].sx;
        let right_width_before = cell.children[1].sx;

        // Resize pane 0 to the right (grow pane 0, shrink pane 1)
        assert!(cell.resize_pane(0, ResizeDirection::Right, 5));

        assert_eq!(cell.children[0].sx, left_width_before + 5);
        assert_eq!(cell.children[1].sx, right_width_before - 5);
    }

    #[test]
    fn resize_pane_vertical() {
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        cell.split_vertical(1);

        let top_height_before = cell.children[0].sy;
        let bottom_height_before = cell.children[1].sy;

        assert!(cell.resize_pane(0, ResizeDirection::Down, 3));

        assert_eq!(cell.children[0].sy, top_height_before + 3);
        assert_eq!(cell.children[1].sy, bottom_height_before - 3);
    }

    #[test]
    fn resize_pane_beyond_minimum_clamps() {
        let mut cell = LayoutCell::new_pane(0, 0, 10, 24, 0);
        cell.split_horizontal(1);

        // Try to resize way beyond available space
        let right_width = cell.children[1].sx;
        assert!(cell.resize_pane(0, ResizeDirection::Right, 1000));
        // Right pane should be at minimum width
        assert_eq!(cell.children[1].sx, PANE_MINIMUM_WIDTH);
        // Left pane gained what right pane lost
        let gained = right_width - PANE_MINIMUM_WIDTH;
        assert!(gained > 0);
    }

    #[test]
    fn resize_pane_nonexistent_returns_false() {
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        cell.split_horizontal(1);
        assert!(!cell.resize_pane(999, ResizeDirection::Right, 5));
    }

    #[test]
    fn resize_pane_at_edge_returns_false() {
        let mut cell = LayoutCell::new_pane(0, 0, 80, 24, 0);
        cell.split_horizontal(1);
        // Pane 0 is the leftmost — can't resize left (no sibling to the left)
        assert!(!cell.resize_pane(0, ResizeDirection::Left, 5));
        // Pane 1 is the rightmost — can't resize right
        assert!(!cell.resize_pane(1, ResizeDirection::Right, 5));
    }

    #[test]
    fn resize_layout_proportional() {
        let mut layout = layout_even_horizontal(80, 24, &[0, 1]);
        let p0_width_before = layout.children[0].sx;
        let p1_width_before = layout.children[1].sx;

        // Resize the whole layout to 160 wide
        layout.resize_layout(160, 24);

        // Widths should roughly double (proportional)
        assert!(layout.children[0].sx > p0_width_before);
        assert!(layout.children[1].sx > p1_width_before);
        // Total should equal new width (minus separator)
        assert_eq!(layout.children[0].sx + 1 + layout.children[1].sx, 160);
    }

    #[test]
    fn resize_layout_vertical() {
        let mut layout = layout_even_vertical(80, 24, &[0, 1]);

        layout.resize_layout(80, 48);

        // Heights should roughly double
        assert_eq!(layout.children[0].sy + 1 + layout.children[1].sy, 48);
    }

    #[test]
    fn resize_layout_single_pane() {
        let mut layout = LayoutCell::new_pane(0, 0, 80, 24, 0);
        layout.resize_layout(160, 48);
        assert_eq!(layout.sx, 160);
        assert_eq!(layout.sy, 48);
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
