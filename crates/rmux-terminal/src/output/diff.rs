//! Screen diff engine.
//!
//! Computes the minimal set of terminal operations needed to update the
//! display from one screen state to another.

use crate::output::writer::TermWriter;
use rmux_core::grid::cell::GridCell;
use rmux_core::screen::Screen;
use rmux_core::style::{Color, Style};

/// Render a full screen (no previous state to diff against).
pub fn render_full(screen: &Screen, writer: &mut TermWriter) {
    writer.hide_cursor();
    writer.begin_sync();
    writer.reset_state();
    writer.write_raw(b"\x1b[0m"); // Reset attributes

    for y in 0..screen.height() {
        writer.cursor_position(0, y);
        let mut x = 0;
        while x < screen.width() {
            let cell = screen.grid.get_cell(x, y);
            if cell.is_padding() {
                x += 1;
                continue;
            }
            writer.set_style(&cell.style);
            let bytes = cell.data.as_bytes();
            if bytes.is_empty() || bytes == [b' '] {
                writer.write_raw(b" ");
            } else {
                writer.write_raw(bytes);
            }
            x += 1;
        }
    }

    // Position cursor
    if screen.cursor.x < screen.width() && screen.cursor.y < screen.height() {
        writer.cursor_position(screen.cursor.x, screen.cursor.y);
    }
    writer.show_cursor();
    writer.end_sync();
}

/// Render only the differences between old and new screen states.
pub fn diff_screens(old: &Screen, new: &Screen, writer: &mut TermWriter) {
    writer.hide_cursor();
    writer.begin_sync();

    let height = new.height();
    let width = new.width();

    for y in 0..height {
        let mut changed_in_line = false;
        let mut x = 0;
        while x < width {
            let old_cell = if y < old.height() && x < old.width() {
                old.grid.get_cell(x, y)
            } else {
                GridCell::CLEARED
            };
            let new_cell = new.grid.get_cell(x, y);

            if old_cell != new_cell {
                if !changed_in_line {
                    changed_in_line = true;
                }
                writer.cursor_position(x, y);
                if new_cell.is_padding() {
                    x += 1;
                    continue;
                }
                writer.set_style(&new_cell.style);
                let bytes = new_cell.data.as_bytes();
                if bytes.is_empty() || bytes == [b' '] {
                    writer.write_raw(b" ");
                } else {
                    writer.write_raw(bytes);
                }
            }
            x += 1;
        }
    }

    // If the screen shrank, clear the extra lines
    if old.height() > new.height() {
        for y in new.height()..old.height() {
            writer.cursor_position(0, y);
            writer.clear_to_eol();
        }
    }

    // Position cursor
    if new.cursor.x < width && new.cursor.y < height {
        writer.cursor_position(new.cursor.x, new.cursor.y);
    }
    writer.show_cursor();
    writer.end_sync();
}

/// Render a simple status line at the given row.
pub fn render_status_line(
    writer: &mut TermWriter,
    session_name: &str,
    window_idx: u32,
    pane_count: usize,
    width: u32,
    y: u32,
) {
    writer.cursor_position(0, y);
    // Green background, black text for status line
    let status_style = Style {
        fg: Color::BLACK,
        bg: Color::GREEN,
        us: Color::Default,
        attrs: rmux_core::style::Attrs::empty(),
    };
    writer.set_style(&status_style);

    let status = if pane_count > 1 {
        format!("[{session_name}] {window_idx}:* ({pane_count} panes)")
    } else {
        format!("[{session_name}] {window_idx}:*")
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

    #[test]
    fn render_empty_screen() {
        let screen = Screen::new(10, 5, 0);
        let mut writer = TermWriter::new(4096);
        render_full(&screen, &mut writer);
        let output = writer.buffer();
        // Should contain cursor positioning and sync sequences
        assert!(!output.is_empty());
        // Should contain hide cursor
        assert!(output.windows(6).any(|w| w == b"\x1b[?25l"));
        // Should contain show cursor
        assert!(output.windows(6).any(|w| w == b"\x1b[?25h"));
    }

    #[test]
    fn diff_identical_screens() {
        let screen = Screen::new(10, 5, 0);
        let mut writer = TermWriter::new(4096);
        diff_screens(&screen, &screen, &mut writer);
        let output = writer.buffer();
        // Sync wrapper + cursor positioning, but no cell updates
        assert!(!output.is_empty());
    }

    #[test]
    fn status_line_rendering() {
        let mut writer = TermWriter::new(4096);
        render_status_line(&mut writer, "main", 0, 1, 40, 24);
        let output = writer.buffer();
        assert!(!output.is_empty());
        // Should contain the session name
        assert!(output.windows(4).any(|w| w == b"main"));
    }
}
