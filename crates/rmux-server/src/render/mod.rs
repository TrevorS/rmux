//! Rendering subsystem: redraw, borders, status line.
//!
//! Renders window contents (panes, borders, status line) into terminal
//! output bytes that are sent to clients.

use crate::window::Window;
use rmux_terminal::output::diff;
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

    // For single-pane windows, render the pane directly
    if let Some(pane) = window.active_pane() {
        // Render the pane's screen
        diff::render_full(&pane.screen, &mut writer);

        // Render status line at the bottom
        diff::render_status_line(
            &mut writer,
            session_name,
            window_idx,
            window.pane_count(),
            sx,
            sy.saturating_sub(1), // Status line at the last row
        );
    } else {
        // No panes - just render an empty screen with status line
        writer.clear_screen();
        diff::render_status_line(
            &mut writer,
            session_name,
            window_idx,
            0,
            sx,
            sy.saturating_sub(1),
        );
    }

    writer.take().to_vec()
}
