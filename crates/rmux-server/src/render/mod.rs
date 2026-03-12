//! Rendering subsystem: redraw, borders, status line.
//!
//! Renders window contents (panes, borders, status line) into terminal
//! output bytes that are sent to clients.

use crate::format::{FormatContext, format_expand, strftime_expand};
use crate::window::Window;
use rmux_core::layout::{LayoutCell, LayoutType};
use rmux_core::style::{Attrs, Color, Style};
use rmux_terminal::output::writer::TermWriter;

use bitflags::bitflags;

bitflags! {
    /// Flags for a window in the status line.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct WindowFlags: u8 {
        /// Current (active) window.
        const ACTIVE   = 0x01;
        /// Last active window.
        const LAST     = 0x02;
        /// Window is zoomed.
        const ZOOMED   = 0x04;
        /// Window has bell alert.
        const BELL     = 0x08;
        /// Window has activity alert.
        const ACTIVITY = 0x10;
    }
}

impl WindowFlags {
    /// Build the tmux-compatible window flags string.
    ///
    /// Flags: `*` (current), `-` (last), `Z` (zoomed), `#` (bell), `!` (activity).
    /// Multiple flags can combine (e.g. `*Z`). If no flags apply, returns empty string.
    pub fn to_flag_string(self) -> String {
        let mut s = String::new();
        if self.contains(Self::ACTIVE) {
            s.push('*');
        }
        if self.contains(Self::LAST) {
            s.push('-');
        }
        if self.contains(Self::ZOOMED) {
            s.push('Z');
        }
        if self.contains(Self::BELL) {
            s.push('#');
        }
        if self.contains(Self::ACTIVITY) {
            s.push('!');
        }
        s
    }
}

/// Info about each window in the session, for rendering the status line.
#[derive(Default)]
pub struct WindowInfo {
    pub idx: u32,
    pub name: String,
    pub flags: WindowFlags,
}

/// Status line and border configuration from options.
pub struct StatusConfig {
    pub left: String,
    pub right: String,
    pub window_status_format: String,
    pub window_status_current_format: String,
    pub status_style: Style,
    pub pane_border_style: Style,
    pub pane_active_border_style: Style,
    /// Whether to show the status line at the top instead of bottom.
    pub status_position_top: bool,
    /// Whether the status line is enabled.
    pub status_enabled: bool,
    /// Justification of the window list: "left", "centre", or "right".
    pub status_justify: String,
    /// Maximum character width for status-left.
    pub status_left_length: usize,
    /// Maximum character width for status-right.
    pub status_right_length: usize,
    /// Separator between window entries in the status line.
    pub window_status_separator: String,
    /// Style applied to inactive window entries.
    pub window_status_style: Style,
    /// Style applied to the active window entry.
    pub window_status_current_style: Style,
    /// Whether to send xterm title escape sequences to clients.
    pub set_titles: bool,
    /// Format string for the xterm title.
    pub set_titles_string: String,
}

/// Render a window's contents to raw terminal output bytes.
///
/// Returns the bytes that should be written to the client's terminal.
pub fn render_window(
    window: &Window,
    session_name: &str,
    sx: u32,
    sy: u32,
    window_list: &[WindowInfo],
    prompt: Option<&str>,
    status_config: Option<&StatusConfig>,
) -> Vec<u8> {
    let mut writer = TermWriter::new(sx as usize * sy as usize * 4);
    let show_status = status_config.is_none_or(|c| c.status_enabled);
    let status_top = show_status && status_config.is_some_and(|c| c.status_position_top);
    let status_row = if status_top { 0 } else { sy.saturating_sub(1) };
    let pane_y_offset = u32::from(status_top);
    let pane_area_height = if show_status { sy.saturating_sub(1) } else { sy };

    // Set xterm window title if enabled
    if let Some(cfg) = status_config {
        if cfg.set_titles {
            let ctx = build_status_context(session_name, window, window_list);
            let title = format_expand(&cfg.set_titles_string, &ctx);
            writer.write_raw(b"\x1b]2;");
            writer.write_raw(title.as_bytes());
            writer.write_raw(b"\x1b\\");
        }
    }

    writer.hide_cursor();
    writer.begin_sync();
    writer.reset_state();
    writer.write_raw(b"\x1b[0m");

    if window.pane_count() <= 1 {
        // Single pane: render directly
        if let Some(pane) = window.active_pane() {
            render_pane_at(&mut writer, pane, 0, pane_y_offset, sx, pane_area_height);
        } else {
            writer.clear_screen();
        }
    } else {
        // Multi-pane: render each pane at its offset, then draw borders
        render_panes_with_borders(&mut writer, window, sx, pane_area_height, status_config);
    }

    // Status line (or command prompt) — only if status is enabled
    if show_status {
        if let Some(prompt_buf) = prompt {
            render_prompt_line(&mut writer, prompt_buf, sx, status_row);
        } else {
            render_status_line(
                &mut writer,
                session_name,
                window,
                window_list,
                sx,
                status_row,
                status_config,
            );
        }
    }

    // Position cursor at active pane (copy mode cursor if in copy mode)
    if let Some(pane) = window.active_pane() {
        if let Some(cm) = &pane.copy_mode {
            let cx = pane.xoff + cm.cx;
            let cy = pane.yoff + cm.cy + pane_y_offset;
            if cx < sx && cy < sy {
                writer.cursor_position(cx, cy);
            }
        } else {
            let cx = pane.xoff + pane.screen.cursor.x;
            let cy = pane.yoff + pane.screen.cursor.y + pane_y_offset;
            if cx < sx && cy < sy {
                writer.cursor_position(cx, cy);
            }
        }
    }

    // Set cursor style from active pane
    if let Some(pane) = window.active_pane() {
        if pane.copy_mode.is_none() {
            writer.set_cursor_style(pane.screen.cursor.cursor_style);
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

    // Build selection for hit testing if in copy mode
    let selection =
        pane.copy_mode.as_ref().and_then(|cm| cm.current_selection(screen.grid.history_size()));

    let oy = pane.copy_mode.as_ref().map_or(0, |cm| cm.oy);

    for y in 0..pane_h {
        writer.cursor_position(xoff, yoff + y);

        // In copy mode with scroll offset, read from history
        let abs_y = if oy > 0 {
            let hs = screen.grid.history_size();
            hs.saturating_sub(oy) + y
        } else {
            screen.grid.history_size() + y
        };

        for x in 0..pane_w {
            let cell = if oy > 0 {
                // Reading from absolute position (may be history)
                if let Some(line) = screen.grid.get_line_absolute(abs_y) {
                    if x < line.cell_count() {
                        line.get_cell(x)
                    } else {
                        rmux_core::grid::cell::GridCell::CLEARED
                    }
                } else {
                    rmux_core::grid::cell::GridCell::CLEARED
                }
            } else {
                screen.grid.get_cell(x, y)
            };

            if cell.is_padding() {
                continue;
            }

            // Check if this cell is in the selection (reverse video)
            let in_selection = selection.as_ref().is_some_and(|sel| sel.contains(x, abs_y));

            if in_selection {
                let mut style = cell.style;
                style.attrs ^= Attrs::REVERSE;
                writer.set_style(&style);
            } else {
                writer.set_style(&cell.style);
            }

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
fn render_panes_with_borders(
    writer: &mut TermWriter,
    window: &Window,
    sx: u32,
    max_height: u32,
    status_config: Option<&StatusConfig>,
) {
    // First, render each pane at its offset
    for pane in window.panes.values() {
        render_pane_at(writer, pane, pane.xoff, pane.yoff, sx, max_height);
    }

    // Then draw borders from the layout tree
    if let Some(layout) = &window.layout {
        let (border, active_border) = if let Some(cfg) = status_config {
            (cfg.pane_border_style, cfg.pane_active_border_style)
        } else {
            (Style::DEFAULT, Style { fg: Color::GREEN, ..Style::DEFAULT })
        };
        draw_borders(writer, layout, window.active_pane, max_height, &border, &active_border);
    }
}

/// Recursively draw borders for split layout nodes.
fn draw_borders(
    writer: &mut TermWriter,
    cell: &LayoutCell,
    active_pane: u32,
    max_height: u32,
    border_style: &Style,
    active_border_style: &Style,
) {
    if cell.is_pane() {
        return;
    }

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

                writer.set_style(if is_active { active_border_style } else { border_style });

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

                writer.set_style(if is_active { active_border_style } else { border_style });

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
        draw_borders(writer, child, active_pane, max_height, border_style, active_border_style);
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
    window: &Window,
    window_list: &[WindowInfo],
    width: u32,
    y: u32,
    status_config: Option<&StatusConfig>,
) {
    use std::fmt::Write;

    writer.cursor_position(0, y);
    let status_style = if let Some(cfg) = status_config {
        cfg.status_style
    } else {
        Style { fg: Color::BLACK, bg: Color::GREEN, us: Color::Default, attrs: Attrs::empty() }
    };
    writer.set_style(&status_style);

    // Build format context for variable expansion
    let ctx = build_status_context(session_name, window, window_list);

    // Expand status-left (format variables + strftime)
    let left = if let Some(cfg) = status_config {
        strftime_expand(&format_expand(&cfg.left, &ctx))
    } else {
        format!("[{session_name}] ")
    };

    // Build window list in the center using format expansion
    let separator = status_config.map_or(" ", |cfg| cfg.window_status_separator.as_str());
    let mut center_parts: Vec<(String, bool)> = Vec::new();
    for winfo in window_list {
        let (text, is_active) = if let Some(cfg) = status_config {
            let mut wctx = FormatContext::new();
            wctx.set("window_index", winfo.idx.to_string());
            wctx.set("window_name", &winfo.name);
            wctx.set("window_flags", winfo.flags.to_flag_string());
            let is_active = winfo.flags.contains(WindowFlags::ACTIVE);
            wctx.set("window_active", if is_active { "1" } else { "0" });
            wctx.set("session_name", session_name);
            let fmt = if is_active {
                &cfg.window_status_current_format
            } else {
                &cfg.window_status_format
            };
            (format_expand(fmt, &wctx), is_active)
        } else {
            let is_active = winfo.flags.contains(WindowFlags::ACTIVE);
            let mut s = format!("{}:{}", winfo.idx, winfo.name);
            let flag_str = winfo.flags.to_flag_string();
            s.push_str(&flag_str);
            (s, is_active)
        };
        center_parts.push((text, is_active));
    }
    let center_joined: String =
        center_parts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join(separator);

    // Build suffix for pane count and copy mode indicator
    let mut suffix = String::new();
    let pane_count = window.pane_count();
    if pane_count > 1 {
        write!(suffix, " ({pane_count} panes)").ok();
    }
    if let Some(pane) = window.active_pane() {
        if let Some(cm) = &pane.copy_mode {
            let hs = pane.screen.grid.history_size();
            write!(suffix, " [Copy mode - {}/{hs}]", cm.oy).ok();
        }
    }
    let center_len = center_joined.len() + suffix.len();

    // Expand status-right (format variables + strftime)
    let right = if let Some(cfg) = status_config {
        strftime_expand(&format_expand(&cfg.right, &ctx))
    } else {
        String::new()
    };

    // Truncate left/right to configured max lengths
    let (left, right) = if let Some(cfg) = status_config {
        let l: String = left.chars().take(cfg.status_left_length).collect();
        let r: String = right.chars().take(cfg.status_right_length).collect();
        (l, r)
    } else {
        (left, right)
    };

    writer.write_raw(left.as_bytes());
    write_justified_center(
        writer,
        &center_parts,
        &suffix,
        separator,
        width as usize,
        left.len(),
        right.len(),
        center_len,
        status_config,
        &status_style,
    );
    writer.set_style(&status_style);
    writer.write_raw(right.as_bytes());
    writer.set_style(&Style::DEFAULT);
}

/// Write the center section of the status line with justification.
#[allow(clippy::too_many_arguments)]
fn write_justified_center(
    writer: &mut TermWriter,
    center_parts: &[(String, bool)],
    suffix: &str,
    separator: &str,
    width: usize,
    left_len: usize,
    right_len: usize,
    center_len: usize,
    status_config: Option<&StatusConfig>,
    status_style: &Style,
) {
    let justify = status_config.map_or("left", |cfg| cfg.status_justify.as_str());
    let avail = width.saturating_sub(left_len + right_len);
    let write_center = |w: &mut TermWriter| {
        write_window_entries(w, center_parts, separator, status_config, status_style);
        w.write_raw(suffix.as_bytes());
    };

    match justify {
        "centre" => {
            let pad_before = avail.saturating_sub(center_len) / 2;
            let pad_after = avail.saturating_sub(center_len).saturating_sub(pad_before);
            write_padding(writer, pad_before);
            write_center(writer);
            write_padding(writer, pad_after);
        }
        "right" => {
            write_padding(writer, avail.saturating_sub(center_len));
            write_center(writer);
        }
        _ => {
            write_center(writer);
            write_padding(writer, avail.saturating_sub(center_len));
        }
    }
}

/// Write N spaces.
fn write_padding(writer: &mut TermWriter, n: usize) {
    for _ in 0..n {
        writer.write_raw(b" ");
    }
}

/// Write window entries to the status line with per-entry styles.
fn write_window_entries(
    writer: &mut TermWriter,
    parts: &[(String, bool)],
    separator: &str,
    status_config: Option<&StatusConfig>,
    status_style: &Style,
) {
    for (i, (text, is_active)) in parts.iter().enumerate() {
        if i > 0 {
            writer.set_style(status_style);
            writer.write_raw(separator.as_bytes());
        }
        if let Some(cfg) = status_config {
            let entry_style = if *is_active {
                &cfg.window_status_current_style
            } else {
                &cfg.window_status_style
            };
            if !entry_style.is_default() {
                writer.set_style(entry_style);
            }
        }
        writer.write_raw(text.as_bytes());
        if status_config.is_some() {
            writer.set_style(status_style);
        }
    }
}

/// Build a `FormatContext` for status line expansion.
fn build_status_context(
    session_name: &str,
    window: &Window,
    window_list: &[WindowInfo],
) -> FormatContext {
    let mut ctx = FormatContext::new();
    ctx.set("session_name", session_name);
    ctx.set("window_name", &window.name);

    // Find active window index
    if let Some(active) = window_list.iter().find(|w| w.flags.contains(WindowFlags::ACTIVE)) {
        ctx.set("window_index", active.idx.to_string());
    }

    // Pane info
    if let Some(pane) = window.active_pane() {
        ctx.set("pane_index", pane.id.to_string());
        ctx.set("pane_id", format!("%{}", pane.id));
        ctx.set("pane_title", &*pane.screen.title);
        ctx.set("pane_active", "1");
    }

    ctx.set("pane_count", window.pane_count().to_string());
    ctx.set("window_panes", window.pane_count().to_string());

    // Hostname
    if let Ok(hostname) = nix::unistd::gethostname() {
        let h = hostname.to_string_lossy().to_string();
        if let Some(short) = h.split('.').next() {
            ctx.set("host_short", short);
        }
        ctx.set("host", h);
    }

    ctx
}

/// Render the command prompt line (replaces the status line when in prompt mode).
///
/// `prompt_str` is the full prompt string including prefix (`:`, `/`, or `?`).
fn render_prompt_line(writer: &mut TermWriter, prompt_str: &str, width: u32, y: u32) {
    writer.cursor_position(0, y);
    writer.set_style(&Style::DEFAULT);

    writer.write_raw(prompt_str.as_bytes());

    // Fill rest with spaces
    let remaining = (width as usize).saturating_sub(prompt_str.len());
    for _ in 0..remaining {
        writer.write_raw(b" ");
    }

    // Position cursor right after the typed text
    let cursor_x = prompt_str.len().min(width as usize);
    writer.cursor_position(cursor_x as u32, y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::Pane;
    use rmux_core::layout::layout_even_horizontal;

    fn single_window_list(idx: u32, name: &str) -> Vec<WindowInfo> {
        vec![WindowInfo { idx, name: name.to_string(), flags: WindowFlags::ACTIVE }]
    }

    #[test]
    fn render_single_pane() {
        let mut window = Window::new("0".into(), 80, 24);
        let pane = Pane::new(80, 24, 0);
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let wl = single_window_list(0, "0");
        let output = render_window(&window, "main", 80, 25, &wl, None, None);
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

        let wl = single_window_list(0, "0");
        let output = render_window(&window, "main", 80, 24, &wl, None, None);
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

        let wl = single_window_list(0, "test");
        let output = render_window(&window, "main", 80, 24, &wl, None, None);
        // Should contain "test" (the window name) in the status line
        assert!(output.windows(4).any(|w| w == b"test"));
    }

    #[test]
    fn render_empty_window() {
        let window = Window::new("empty".into(), 80, 24);
        let wl = single_window_list(0, "empty");
        let output = render_window(&window, "sess", 80, 25, &wl, None, None);
        // Even with no panes, the status line should produce output
        assert!(!output.is_empty());
    }

    #[test]
    fn render_multi_pane_with_content() {
        let mut window = Window::new("multi".into(), 80, 23);
        let mut pane1 = Pane::new(39, 23, 0);
        let mut pane2 = Pane::new(40, 23, 0);
        pane1.process_input(b"Hello");
        pane2.process_input(b"World");
        let pid1 = pane1.id;
        let pid2 = pane2.id;
        pane1.xoff = 0;
        pane1.yoff = 0;
        pane2.xoff = 40;
        pane2.yoff = 0;
        window.active_pane = pid1;
        window.panes.insert(pid1, pane1);
        window.panes.insert(pid2, pane2);
        window.layout = Some(layout_even_horizontal(80, 23, &[pid1, pid2]));

        let wl = single_window_list(0, "multi");
        let output = render_window(&window, "sess", 80, 24, &wl, None, None);
        assert!(output.windows(5).any(|w| w == b"Hello"));
        assert!(output.windows(5).any(|w| w == b"World"));
    }

    #[test]
    fn status_line_shows_copy_mode() {
        let mut window = Window::new("cp".into(), 80, 23);
        let mut pane = Pane::new(80, 23, 0);
        pane.enter_copy_mode("vi");
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let wl = single_window_list(0, "cp");
        let output = render_window(&window, "sess", 80, 24, &wl, None, None);
        assert!(output.windows(9).any(|w| w == b"Copy mode"));
    }

    #[test]
    fn status_line_shows_pane_count() {
        let mut window = Window::new("cnt".into(), 80, 23);
        let mut pane1 = Pane::new(39, 23, 0);
        let mut pane2 = Pane::new(40, 23, 0);
        let pid1 = pane1.id;
        let pid2 = pane2.id;
        pane1.xoff = 0;
        pane1.yoff = 0;
        pane2.xoff = 40;
        pane2.yoff = 0;
        window.active_pane = pid1;
        window.panes.insert(pid1, pane1);
        window.panes.insert(pid2, pane2);
        window.layout = Some(layout_even_horizontal(80, 23, &[pid1, pid2]));

        let wl = single_window_list(0, "cnt");
        let output = render_window(&window, "sess", 80, 24, &wl, None, None);
        assert!(output.windows(7).any(|w| w == b"2 panes"));
    }

    #[test]
    fn status_line_shows_multiple_windows() {
        let mut window = Window::new("bash".into(), 80, 23);
        let pane = Pane::new(80, 23, 0);
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let wl = vec![
            WindowInfo { idx: 0, name: "bash".to_string(), flags: WindowFlags::ACTIVE },
            WindowInfo { idx: 1, name: "vim".to_string(), flags: WindowFlags::LAST },
            WindowInfo { idx: 2, name: "logs".to_string(), flags: WindowFlags::empty() },
        ];
        let output = render_window(&window, "sess", 80, 24, &wl, None, None);
        // Should contain all window names
        assert!(output.windows(6).any(|w| w == b"0:bash"));
        assert!(output.windows(5).any(|w| w == b"1:vim"));
        assert!(output.windows(6).any(|w| w == b"2:logs"));
        // Active window should have *
        assert!(output.windows(7).any(|w| w == b"0:bash*"));
        // Last window should have -
        assert!(output.windows(6).any(|w| w == b"1:vim-"));
    }

    #[test]
    fn status_line_with_format_expansion() {
        let mut window = Window::new("bash".into(), 80, 23);
        let pane = Pane::new(80, 23, 0);
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let wl = single_window_list(0, "bash");
        let cfg = StatusConfig {
            left: "[#{session_name}] ".to_string(),
            right: "RIGHT".to_string(),
            window_status_format: "#I:#W#F".to_string(),
            window_status_current_format: "#I:#W#F".to_string(),
            status_style: Style {
                fg: Color::BLACK,
                bg: Color::GREEN,
                us: Color::Default,
                attrs: Attrs::empty(),
            },
            pane_border_style: Style::DEFAULT,
            pane_active_border_style: Style { fg: Color::GREEN, ..Style::DEFAULT },
            status_position_top: false,
            status_enabled: true,
            status_justify: "left".to_string(),
            status_left_length: 10,
            status_right_length: 40,
            window_status_separator: " ".to_string(),
            window_status_style: Style::DEFAULT,
            window_status_current_style: Style::DEFAULT,
            set_titles: false,
            set_titles_string: String::new(),
        };
        let output = render_window(&window, "dev", 80, 24, &wl, None, Some(&cfg));
        // Status line should contain expanded session name
        assert!(output.windows(5).any(|w| w == b"[dev]"));
        // And the right side
        assert!(output.windows(5).any(|w| w == b"RIGHT"));
    }

    #[test]
    fn build_status_context_sets_vars() {
        let mut window = Window::new("vim".into(), 80, 23);
        let pane = Pane::new(80, 23, 0);
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let wl = vec![WindowInfo { idx: 3, name: "vim".to_string(), flags: WindowFlags::ACTIVE }];
        let ctx = build_status_context("work", &window, &wl);
        assert_eq!(ctx.get("session_name"), Some("work"));
        assert_eq!(ctx.get("window_name"), Some("vim"));
        assert_eq!(ctx.get("window_index"), Some("3"));
        assert_eq!(ctx.get("pane_active"), Some("1"));
    }

    #[test]
    fn custom_window_status_format() {
        let mut window = Window::new("bash".into(), 80, 23);
        let pane = Pane::new(80, 23, 0);
        let pid = pane.id;
        window.active_pane = pid;
        window.panes.insert(pid, pane);

        let wl = vec![
            WindowInfo { idx: 0, name: "bash".to_string(), flags: WindowFlags::ACTIVE },
            WindowInfo { idx: 1, name: "vim".to_string(), flags: WindowFlags::empty() },
        ];
        let cfg = StatusConfig {
            left: "[#S] ".to_string(),
            right: String::new(),
            window_status_format: "[#I]#W".to_string(),
            window_status_current_format: "[#I]#W*".to_string(),
            status_style: Style::DEFAULT,
            pane_border_style: Style::DEFAULT,
            pane_active_border_style: Style { fg: Color::GREEN, ..Style::DEFAULT },
            status_position_top: false,
            status_enabled: true,
            status_justify: "left".to_string(),
            status_left_length: 10,
            status_right_length: 40,
            window_status_separator: " ".to_string(),
            window_status_style: Style::DEFAULT,
            window_status_current_style: Style::DEFAULT,
            set_titles: false,
            set_titles_string: String::new(),
        };
        let output = render_window(&window, "main", 80, 24, &wl, None, Some(&cfg));
        // Active window uses current format
        assert!(output.windows(8).any(|w| w == b"[0]bash*"));
        // Inactive window uses normal format (no *)
        assert!(output.windows(6).any(|w| w == b"[1]vim"));
    }

    #[test]
    fn window_flags_active() {
        assert_eq!(WindowFlags::ACTIVE.to_flag_string(), "*");
    }

    #[test]
    fn window_flags_last() {
        assert_eq!(WindowFlags::LAST.to_flag_string(), "-");
    }

    #[test]
    fn window_flags_zoomed_active() {
        assert_eq!((WindowFlags::ACTIVE | WindowFlags::ZOOMED).to_flag_string(), "*Z");
    }

    #[test]
    fn window_flags_bell() {
        assert_eq!(WindowFlags::BELL.to_flag_string(), "#");
    }

    #[test]
    fn window_flags_activity() {
        assert_eq!(WindowFlags::ACTIVITY.to_flag_string(), "!");
    }

    #[test]
    fn window_flags_none() {
        assert_eq!(WindowFlags::empty().to_flag_string(), "");
    }

    #[test]
    fn window_flags_combined() {
        let f = WindowFlags::ACTIVE | WindowFlags::ZOOMED | WindowFlags::BELL;
        assert_eq!(f.to_flag_string(), "*Z#");
    }
}
