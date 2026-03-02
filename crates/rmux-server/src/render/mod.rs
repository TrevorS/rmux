//! Rendering subsystem: redraw, borders, status line.
//!
//! Renders window contents (panes, borders, status line) into terminal
//! output bytes that are sent to clients.

use crate::window::Window;
use rmux_core::layout::{LayoutCell, LayoutType};
use rmux_core::style::{Attrs, Color, Style};
use rmux_terminal::output::writer::TermWriter;

/// Render a window's contents to raw terminal output bytes.
///
/// Returns the bytes that should be written to the client's terminal.
pub fn render_window(
    window: &Window,
    session_name: &str,
    window_idx: u32,
    sx: u32,
    sy: u32,
) -> Vec<u8> {
    let mut writer = TermWriter::new(sx as usize * sy as usize * 4);
    let status_row = sy.saturating_sub(1);

    writer.hide_cursor();
    writer.begin_sync();
    writer.reset_state();
    writer.write_raw(b"\x1b[0m");

    if window.pane_count() <= 1 {
        // Single pane: render directly
        if let Some(pane) = window.active_pane() {
            render_pane_at(&mut writer, pane, 0, 0, sx, status_row);
        } else {
            writer.clear_screen();
        }
    } else {
        // Multi-pane: render each pane at its offset, then draw borders
        render_panes_with_borders(&mut writer, window, sx, status_row);
    }

    // Status line
    render_status_line(&mut writer, session_name, window_idx, window, sx, status_row);

    // Position cursor at active pane
    if let Some(pane) = window.active_pane() {
        let cx = pane.xoff + pane.screen.cursor.x;
        let cy = pane.yoff + pane.screen.cursor.y;
        if cx < sx && cy < status_row {
            writer.cursor_position(cx, cy);
        }
    }

    writer.show_cursor();
    writer.end_sync();
    writer.take().to_vec()
}

/// Render a pane's screen content at a given offset.
fn render_pane_at(
    writer: &mut TermWriter,
    pane: &crate::pane::Pane,
    xoff: u32,
    yoff: u32,
    max_width: u32,
    max_height: u32,
) {
    let screen = &pane.screen;
    let pane_w = pane.sx.min(max_width.saturating_sub(xoff));
    let pane_h = pane.sy.min(max_height.saturating_sub(yoff));

    for y in 0..pane_h {
        writer.cursor_position(xoff, yoff + y);
        for x in 0..pane_w {
            let cell = screen.grid.get_cell(x, y);
            if cell.is_padding() {
                continue;
            }
            writer.set_style(&cell.style);
            let bytes = cell.data.as_bytes();
            if bytes.is_empty() || bytes == [b' '] {
                writer.write_raw(b" ");
            } else {
                writer.write_raw(bytes);
            }
        }
    }
}

/// Render all panes with borders between them.
fn render_panes_with_borders(writer: &mut TermWriter, window: &Window, sx: u32, max_height: u32) {
    // First, render each pane at its offset
    for pane in window.panes.values() {
        render_pane_at(writer, pane, pane.xoff, pane.yoff, sx, max_height);
    }

    // Then draw borders from the layout tree
    if let Some(layout) = &window.layout {
        draw_borders(writer, layout, window.active_pane, max_height);
    }
}

/// Recursively draw borders for split layout nodes.
fn draw_borders(writer: &mut TermWriter, cell: &LayoutCell, active_pane: u32, max_height: u32) {
    if cell.is_pane() {
        return;
    }

    let border_style =
        Style { fg: Color::Default, bg: Color::Default, us: Color::Default, attrs: Attrs::empty() };

    let active_border_style =
        Style { fg: Color::GREEN, bg: Color::Default, us: Color::Default, attrs: Attrs::empty() };

    match cell.cell_type {
        LayoutType::LeftRight => {
            // Draw vertical borders between children
            for i in 0..cell.children.len().saturating_sub(1) {
                let left_child = &cell.children[i];
                let border_x = left_child.x_off + left_child.sx;
                let border_y = left_child.y_off;
                let border_h = left_child.sy.min(max_height.saturating_sub(border_y));

                // Check if the active pane is adjacent to this border
                let right_child = &cell.children[i + 1];
                let is_active = is_pane_in_subtree(left_child, active_pane)
                    || is_pane_in_subtree(right_child, active_pane);

                writer.set_style(if is_active { &active_border_style } else { &border_style });

                for y in 0..border_h {
                    writer.cursor_position(border_x, border_y + y);
                    writer.write_raw("\u{2502}".as_bytes()); // │
                }
            }
        }
        LayoutType::TopBottom => {
            // Draw horizontal borders between children
            for i in 0..cell.children.len().saturating_sub(1) {
                let top_child = &cell.children[i];
                let border_x = top_child.x_off;
                let border_y = top_child.y_off + top_child.sy;
                let border_w = top_child.sx;

                if border_y >= max_height {
                    continue;
                }

                let bottom_child = &cell.children[i + 1];
                let is_active = is_pane_in_subtree(top_child, active_pane)
                    || is_pane_in_subtree(bottom_child, active_pane);

                writer.set_style(if is_active { &active_border_style } else { &border_style });

                writer.cursor_position(border_x, border_y);
                for _ in 0..border_w {
                    writer.write_raw("\u{2500}".as_bytes()); // ─
                }
            }
        }
        LayoutType::Pane => {}
    }

    // Recurse into children
    for child in &cell.children {
        draw_borders(writer, child, active_pane, max_height);
    }
}

/// Check if a layout subtree contains a specific pane.
fn is_pane_in_subtree(cell: &LayoutCell, pane_id: u32) -> bool {
    if cell.is_pane() {
        return cell.pane_id == Some(pane_id);
    }
    cell.children.iter().any(|c| is_pane_in_subtree(c, pane_id))
}

/// Render the status line at the bottom.
fn render_status_line(
    writer: &mut TermWriter,
    session_name: &str,
    window_idx: u32,
    window: &Window,
    width: u32,
    y: u32,
) {
    writer.cursor_position(0, y);
    let status_style =
        Style { fg: Color::BLACK, bg: Color::GREEN, us: Color::Default, attrs: Attrs::empty() };
    writer.set_style(&status_style);

    let pane_count = window.pane_count();
    let status = if pane_count > 1 {
        format!("[{session_name}] {window_idx}:{} ({pane_count} panes)", window.name)
    } else {
        format!("[{session_name}] {window_idx}:{}", window.name)
    };
    writer.write_raw(status.as_bytes());

    // Fill rest with spaces
    let remaining = (width as usize).saturating_sub(status.len());
    for _ in 0..remaining {
        writer.write_raw(b" ");
    }
    writer.set_style(&Style::DEFAULT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::Pane;
    use rmux_core::layout::layout_even_horizontal;

    #[test]
    fn render_single_pane() {
        let mut window = Window::new("0".into(), 80, 24);
        let pane = Pane::new(80, 24, 0);
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let output = render_window(&window, "main", 0, 80, 25);
        assert!(!output.is_empty());
    }

    #[test]
    fn render_two_panes_with_border() {
        let mut window = Window::new("0".into(), 80, 23);
        let pane1 = Pane::new(39, 23, 0);
        let pane2 = Pane::new(40, 23, 0);
        let pid1 = pane1.id;
        let pid2 = pane2.id;

        let mut p1 = pane1;
        p1.xoff = 0;
        p1.yoff = 0;
        let mut p2 = pane2;
        p2.xoff = 40;
        p2.yoff = 0;

        window.active_pane = pid1;
        window.panes.insert(pid1, p1);
        window.panes.insert(pid2, p2);
        window.layout = Some(layout_even_horizontal(80, 23, &[pid1, pid2]));

        let output = render_window(&window, "main", 0, 80, 24);
        // Should contain the vertical border character (│ = 0xe2 0x94 0x82 in UTF-8)
        assert!(output.windows(3).any(|w| w == [0xe2, 0x94, 0x82]));
    }

    #[test]
    fn status_line_shows_window_name() {
        let mut window = Window::new("test".into(), 80, 23);
        let pane = Pane::new(80, 23, 0);
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let output = render_window(&window, "main", 0, 80, 24);
        // Should contain "test" (the window name) in the status line
        assert!(output.windows(4).any(|w| w == b"test"));
    }
}
