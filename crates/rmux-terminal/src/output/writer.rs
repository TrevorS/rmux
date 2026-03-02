//! Escape sequence generation for terminal output.

use bytes::BytesMut;
use rmux_core::style::{Attrs, Color, Style};

/// Terminal output writer.
///
/// Generates escape sequences to update the client terminal.
/// Tracks the current terminal state to minimize output.
pub struct TermWriter {
    buf: BytesMut,
    current_style: Style,
}

impl TermWriter {
    /// Create a new writer with a buffer of the given capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self { buf: BytesMut::with_capacity(capacity), current_style: Style::DEFAULT }
    }

    /// Get the output buffer.
    #[must_use]
    pub fn buffer(&self) -> &[u8] {
        &self.buf
    }

    /// Take the buffer contents, leaving the buffer empty.
    pub fn take(&mut self) -> BytesMut {
        self.buf.split()
    }

    /// Reset the tracked terminal state.
    pub fn reset_state(&mut self) {
        self.current_style = Style::DEFAULT;
    }

    /// Write raw bytes to the buffer.
    pub fn write_raw(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Move cursor to the given position.
    pub fn cursor_position(&mut self, x: u32, y: u32) {
        use std::fmt::Write;
        write!(self.buf, "\x1b[{};{}H", y + 1, x + 1).ok();
    }

    /// Set style, emitting only the necessary SGR sequences.
    pub fn set_style(&mut self, style: &Style) {
        if *style == self.current_style {
            return;
        }

        // Check if we need a full reset
        let removed_attrs = self.current_style.attrs & !style.attrs;
        if !removed_attrs.is_empty() {
            self.write_raw(b"\x1b[0m");
            self.current_style = Style::DEFAULT;
        }

        // Apply new attributes
        let new_attrs = style.attrs & !self.current_style.attrs;
        if new_attrs.contains(Attrs::BOLD) {
            self.write_raw(b"\x1b[1m");
        }
        if new_attrs.contains(Attrs::DIM) {
            self.write_raw(b"\x1b[2m");
        }
        if new_attrs.contains(Attrs::ITALICS) {
            self.write_raw(b"\x1b[3m");
        }
        if new_attrs.contains(Attrs::UNDERSCORE) {
            self.write_raw(b"\x1b[4m");
        }
        if new_attrs.contains(Attrs::BLINK) {
            self.write_raw(b"\x1b[5m");
        }
        if new_attrs.contains(Attrs::REVERSE) {
            self.write_raw(b"\x1b[7m");
        }
        if new_attrs.contains(Attrs::HIDDEN) {
            self.write_raw(b"\x1b[8m");
        }
        if new_attrs.contains(Attrs::STRIKETHROUGH) {
            self.write_raw(b"\x1b[9m");
        }

        // Apply colors
        if style.fg != self.current_style.fg {
            self.write_fg(style.fg);
        }
        if style.bg != self.current_style.bg {
            self.write_bg(style.bg);
        }

        self.current_style = *style;
    }

    fn write_fg(&mut self, color: Color) {
        use std::fmt::Write;
        match color {
            Color::Default => self.write_raw(b"\x1b[39m"),
            Color::Palette(n) if n < 8 => {
                write!(self.buf, "\x1b[{}m", 30 + n).ok();
            }
            Color::Palette(n) if n < 16 => {
                write!(self.buf, "\x1b[{}m", 90 + n - 8).ok();
            }
            Color::Palette(n) => {
                write!(self.buf, "\x1b[38;5;{n}m").ok();
            }
            Color::Rgb { r, g, b } => {
                write!(self.buf, "\x1b[38;2;{r};{g};{b}m").ok();
            }
        }
    }

    fn write_bg(&mut self, color: Color) {
        use std::fmt::Write;
        match color {
            Color::Default => self.write_raw(b"\x1b[49m"),
            Color::Palette(n) if n < 8 => {
                write!(self.buf, "\x1b[{}m", 40 + n).ok();
            }
            Color::Palette(n) if n < 16 => {
                write!(self.buf, "\x1b[{}m", 100 + n - 8).ok();
            }
            Color::Palette(n) => {
                write!(self.buf, "\x1b[48;5;{n}m").ok();
            }
            Color::Rgb { r, g, b } => {
                write!(self.buf, "\x1b[48;2;{r};{g};{b}m").ok();
            }
        }
    }

    /// Clear the screen.
    pub fn clear_screen(&mut self) {
        self.write_raw(b"\x1b[2J");
    }

    /// Clear to end of line.
    pub fn clear_to_eol(&mut self) {
        self.write_raw(b"\x1b[K");
    }

    /// Hide cursor.
    pub fn hide_cursor(&mut self) {
        self.write_raw(b"\x1b[?25l");
    }

    /// Show cursor.
    pub fn show_cursor(&mut self) {
        self.write_raw(b"\x1b[?25h");
    }

    /// Enable synchronized output (begin batch).
    pub fn begin_sync(&mut self) {
        self.write_raw(b"\x1b[?2026h");
    }

    /// Disable synchronized output (end batch).
    pub fn end_sync(&mut self) {
        self.write_raw(b"\x1b[?2026l");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_position() {
        let mut w = TermWriter::new(256);
        w.cursor_position(0, 0);
        assert_eq!(w.buffer(), b"\x1b[1;1H");
        w.take();
        w.cursor_position(9, 4);
        assert_eq!(w.buffer(), b"\x1b[5;10H");
    }

    #[test]
    fn style_bold() {
        let mut w = TermWriter::new(256);
        let style = Style { attrs: Attrs::BOLD, ..Style::DEFAULT };
        w.set_style(&style);
        assert!(w.buffer().windows(4).any(|w| w == b"\x1b[1m"));
    }

    #[test]
    fn style_no_change() {
        let mut w = TermWriter::new(256);
        w.set_style(&Style::DEFAULT);
        assert!(w.buffer().is_empty()); // No output needed
    }

    #[test]
    fn fg_color() {
        let mut w = TermWriter::new(256);
        let style = Style { fg: Color::RED, ..Style::DEFAULT };
        w.set_style(&style);
        assert!(w.buffer().windows(5).any(|w| w == b"\x1b[31m"));
    }
}
