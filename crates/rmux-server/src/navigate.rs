//! Directional pane navigation.
//!
//! Finds the pane in a given direction relative to the current pane
//! within a layout tree.

use rmux_core::layout::LayoutCell;

/// Direction for pane navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Find the pane in the given direction from the current pane.
///
/// Returns the pane ID of the closest pane whose center is in the
/// specified direction from the current pane's center.
pub fn find_pane_in_direction(
    layout: &LayoutCell,
    current_pane_id: u32,
    direction: Direction,
) -> Option<u32> {
    // Collect all pane positions (id, center_x, center_y)
    let mut panes = Vec::new();
    collect_pane_centers(layout, &mut panes);

    let (cur_cx, cur_cy) =
        panes.iter().find(|(id, _, _)| *id == current_pane_id).map(|(_, cx, cy)| (*cx, *cy))?;

    let mut best: Option<(u32, i64)> = None;

    for &(id, cx, cy) in &panes {
        if id == current_pane_id {
            continue;
        }

        let valid = match direction {
            Direction::Up => cy < cur_cy,
            Direction::Down => cy > cur_cy,
            Direction::Left => cx < cur_cx,
            Direction::Right => cx > cur_cx,
        };

        if !valid {
            continue;
        }

        let dx = (cx - cur_cx) as i64;
        let dy = (cy - cur_cy) as i64;
        let dist = dx * dx + dy * dy;

        if best.is_none_or(|(_, d)| dist < d) {
            best = Some((id, dist));
        }
    }

    best.map(|(id, _)| id)
}

/// Collect all pane centers from a layout tree.
fn collect_pane_centers(cell: &LayoutCell, out: &mut Vec<(u32, i32, i32)>) {
    if cell.is_pane() {
        if let Some(id) = cell.pane_id {
            #[allow(clippy::cast_possible_wrap)]
            let cx = cell.x_off as i32 + cell.sx as i32 / 2;
            #[allow(clippy::cast_possible_wrap)]
            let cy = cell.y_off as i32 + cell.sy as i32 / 2;
            out.push((id, cx, cy));
        }
    } else {
        for child in &cell.children {
            collect_pane_centers(child, out);
        }
    }
}

/// Get the next pane in the layout (for `select-pane -t +`).
pub fn next_pane(layout: &LayoutCell, current_pane_id: u32) -> Option<u32> {
    let ids = layout.pane_ids();
    if ids.len() < 2 {
        return None;
    }
    let pos = ids.iter().position(|&id| id == current_pane_id)?;
    let next = (pos + 1) % ids.len();
    Some(ids[next])
}

/// Get the previous pane in the layout.
pub fn previous_pane(layout: &LayoutCell, current_pane_id: u32) -> Option<u32> {
    let ids = layout.pane_ids();
    if ids.len() < 2 {
        return None;
    }
    let pos = ids.iter().position(|&id| id == current_pane_id)?;
    let prev = if pos == 0 { ids.len() - 1 } else { pos - 1 };
    Some(ids[prev])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmux_core::layout::{LayoutType, layout_even_horizontal, layout_even_vertical};

    #[test]
    fn navigate_horizontal_split() {
        // Two panes side by side: [0 | 1]
        let layout = layout_even_horizontal(80, 24, &[10, 11]);
        assert_eq!(find_pane_in_direction(&layout, 10, Direction::Right), Some(11));
        assert_eq!(find_pane_in_direction(&layout, 11, Direction::Left), Some(10));
        assert_eq!(find_pane_in_direction(&layout, 10, Direction::Left), None);
        assert_eq!(find_pane_in_direction(&layout, 11, Direction::Right), None);
    }

    #[test]
    fn navigate_vertical_split() {
        // Two panes top/bottom: [0] / [1]
        let layout = layout_even_vertical(80, 24, &[10, 11]);
        assert_eq!(find_pane_in_direction(&layout, 10, Direction::Down), Some(11));
        assert_eq!(find_pane_in_direction(&layout, 11, Direction::Up), Some(10));
        assert_eq!(find_pane_in_direction(&layout, 10, Direction::Up), None);
        assert_eq!(find_pane_in_direction(&layout, 11, Direction::Down), None);
    }

    #[test]
    fn navigate_four_pane_grid() {
        // Create a 2x2 grid: [0 | 1] / [2 | 3]
        let mut root = LayoutCell::new_split(LayoutType::TopBottom, 0, 0, 80, 24);
        let mut top = LayoutCell::new_split(LayoutType::LeftRight, 0, 0, 80, 11);
        top.children.push(LayoutCell::new_pane(0, 0, 39, 11, 10));
        top.children.push(LayoutCell::new_pane(40, 0, 39, 11, 11));
        let mut bottom = LayoutCell::new_split(LayoutType::LeftRight, 0, 12, 80, 11);
        bottom.children.push(LayoutCell::new_pane(0, 12, 39, 11, 12));
        bottom.children.push(LayoutCell::new_pane(40, 12, 39, 11, 13));
        root.children.push(top);
        root.children.push(bottom);

        // From top-left (10): right→11, down→12
        assert_eq!(find_pane_in_direction(&root, 10, Direction::Right), Some(11));
        assert_eq!(find_pane_in_direction(&root, 10, Direction::Down), Some(12));

        // From bottom-right (13): left→12, up→11
        assert_eq!(find_pane_in_direction(&root, 13, Direction::Left), Some(12));
        assert_eq!(find_pane_in_direction(&root, 13, Direction::Up), Some(11));
    }

    #[test]
    fn next_pane_wraps() {
        let layout = layout_even_horizontal(80, 24, &[10, 11, 12]);
        assert_eq!(next_pane(&layout, 10), Some(11));
        assert_eq!(next_pane(&layout, 11), Some(12));
        assert_eq!(next_pane(&layout, 12), Some(10)); // wraps
    }

    #[test]
    fn previous_pane_wraps() {
        let layout = layout_even_horizontal(80, 24, &[10, 11, 12]);
        assert_eq!(previous_pane(&layout, 10), Some(12)); // wraps
        assert_eq!(previous_pane(&layout, 11), Some(10));
        assert_eq!(previous_pane(&layout, 12), Some(11));
    }

    #[test]
    fn single_pane_no_navigation() {
        let layout = LayoutCell::new_pane(0, 0, 80, 24, 10);
        assert_eq!(find_pane_in_direction(&layout, 10, Direction::Up), None);
        assert_eq!(next_pane(&layout, 10), None);
        assert_eq!(previous_pane(&layout, 10), None);
    }
}
