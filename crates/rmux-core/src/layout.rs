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
}
