//! VT100/xterm escape sequence parser.
//!
//! Based on Paul Williams' state machine with the ASCII fast path optimization.
//! This is the hottest code path in rmux - all PTY output flows through here.

use super::params::Params;
use rmux_core::grid::cell::{CellFlags, GridCell};
use rmux_core::screen::{Notification, Screen};
use rmux_core::style::{Attrs, Color, Style};
use rmux_core::utf8::Utf8Char;
use smallvec::SmallVec;

/// Parser state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    OscString,
    SosPmApcString,
}

/// An action produced by the parser for the screen to process.
#[derive(Debug)]
pub enum Action {
    /// Print a string of characters to the screen.
    Print(Vec<u8>),
    /// Execute a C0 control character.
    Execute(u8),
    /// CSI sequence dispatched.
    CsiDispatch { params: Params, intermediates: SmallVec<[u8; 4]>, final_byte: u8 },
    /// ESC sequence dispatched.
    EscDispatch { intermediates: SmallVec<[u8; 4]>, final_byte: u8 },
    /// OSC string completed.
    OscDispatch(Vec<u8>),
    /// DCS string completed.
    DcsDispatch(Vec<u8>),
}

/// The VT100 input parser.
///
/// Processes raw bytes from a PTY and produces actions for the screen.
/// Uses an ASCII fast path: runs of printable ASCII (0x20..=0x7e) in Ground state
/// are batched into a single Print action, bypassing the state machine per-byte.
#[derive(Debug)]
pub struct InputParser {
    state: State,
    params: Params,
    intermediates: SmallVec<[u8; 4]>,
    osc_data: Vec<u8>,
    dcs_data: Vec<u8>,
    /// Buffer for collecting parameter bytes in CSI sequences.
    param_buf: Vec<u8>,
    /// UTF-8 accumulation buffer.
    utf8_buf: [u8; 4],
    utf8_len: u8,
    utf8_needed: u8,
    /// Current hyperlink ID (0 = no hyperlink, set by OSC 8).
    current_hyperlink: u32,
    /// Counter for generating unique hyperlink IDs.
    hyperlink_counter: u32,
    /// G0 charset: false = ASCII (B), true = DEC line drawing (0).
    g0_line_drawing: bool,
    /// G1 charset: false = ASCII (B), true = DEC line drawing (0).
    g1_line_drawing: bool,
    /// Active charset: false = G0 (SI), true = G1 (SO).
    use_g1: bool,
}

impl InputParser {
    /// Create a new parser in the ground state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: Params::new(),
            intermediates: SmallVec::new(),
            osc_data: Vec::new(),
            dcs_data: Vec::new(),
            param_buf: Vec::new(),
            utf8_buf: [0; 4],
            utf8_len: 0,
            utf8_needed: 0,
            current_hyperlink: 0,
            hyperlink_counter: 0,
            g0_line_drawing: false,
            g1_line_drawing: false,
            use_g1: false,
        }
    }

    /// Parse a chunk of bytes and apply actions to the screen.
    ///
    /// This is the main entry point. It contains the ASCII fast path optimization.
    pub fn parse(&mut self, data: &[u8], screen: &mut Screen) {
        let mut i = 0;
        while i < data.len() {
            // FAST PATH: Batch printable ASCII in Ground state.
            // This skips the per-byte state machine for the common case of plain text.
            if self.state == State::Ground && data[i] >= 0x20 && data[i] <= 0x7e {
                let start = i;
                while i < data.len() && data[i] >= 0x20 && data[i] <= 0x7e {
                    i += 1;
                }
                self.handle_print_ascii(&data[start..i], screen);
                continue;
            }

            let byte = data[i];
            i += 1;

            // Handle UTF-8 continuation bytes
            if self.utf8_needed > 0 {
                if byte & 0xC0 == 0x80 {
                    self.utf8_buf[self.utf8_len as usize] = byte;
                    self.utf8_len += 1;
                    if self.utf8_len == self.utf8_needed {
                        self.handle_print_utf8(screen);
                    }
                    continue;
                }
                // Invalid continuation: reset and process this byte normally
                self.utf8_len = 0;
                self.utf8_needed = 0;
            }

            // UTF-8 start bytes in Ground state
            if self.state == State::Ground {
                let needed = match byte {
                    0xC0..=0xDF => 2,
                    0xE0..=0xEF => 3,
                    0xF0..=0xF7 => 4,
                    _ => 0,
                };
                if needed > 0 {
                    self.utf8_buf[0] = byte;
                    self.utf8_len = 1;
                    self.utf8_needed = needed;
                    continue;
                }
            }

            self.process_byte(byte, screen);
        }
    }

    fn process_byte(&mut self, byte: u8, screen: &mut Screen) {
        // C0 control characters (0x00-0x1F) are handled in most states
        if byte < 0x20 {
            match byte {
                0x1B => {
                    // ESC - transitions to Escape state.
                    // If we're in OscString, dispatch the OSC first (handles ESC \ as ST).
                    if self.state == State::OscString {
                        self.handle_osc_dispatch(screen);
                    }
                    self.transition(State::Escape);
                    return;
                }
                0x18 | 0x1A => {
                    // CAN, SUB - abort sequence, return to ground
                    self.transition(State::Ground);
                    return;
                }
                _ => {
                    if self.state == State::Ground
                        || self.state == State::Escape
                        || self.state == State::CsiEntry
                        || self.state == State::CsiParam
                    {
                        self.handle_execute(byte, screen);
                        return;
                    }
                }
            }
        }

        match self.state {
            State::Ground => {
                // Non-ASCII, non-control: shouldn't reach here due to fast path
                // but handle as a print for robustness
                self.handle_print_byte(byte, screen);
            }
            State::Escape => {
                match byte {
                    0x20..=0x2F => {
                        self.intermediates.push(byte);
                        self.transition(State::EscapeIntermediate);
                    }
                    0x5B => {
                        // '[' -> CSI
                        self.transition(State::CsiEntry);
                    }
                    0x5D => {
                        // ']' -> OSC
                        self.osc_data.clear();
                        self.transition(State::OscString);
                    }
                    0x50 => {
                        // 'P' -> DCS
                        self.dcs_data.clear();
                        self.transition(State::DcsEntry);
                    }
                    0x58 | 0x5E | 0x5F => {
                        // 'X', '^', '_' -> SOS/PM/APC
                        self.transition(State::SosPmApcString);
                    }
                    0x30..=0x4F | 0x51..=0x57 | 0x59..=0x5A | 0x5C | 0x60..=0x7E => {
                        self.handle_esc_dispatch(byte, screen);
                        self.transition(State::Ground);
                    }
                    _ => {
                        self.transition(State::Ground);
                    }
                }
            }
            State::EscapeIntermediate => match byte {
                0x20..=0x2F => {
                    self.intermediates.push(byte);
                }
                0x30..=0x7E => {
                    self.handle_esc_dispatch(byte, screen);
                    self.transition(State::Ground);
                }
                _ => {
                    self.transition(State::Ground);
                }
            },
            State::CsiEntry => {
                match byte {
                    0x30..=0x3B => {
                        self.param_buf.push(byte);
                        self.transition(State::CsiParam);
                    }
                    0x3C..=0x3F => {
                        // Private mode indicator
                        self.intermediates.push(byte);
                        self.transition(State::CsiParam);
                    }
                    0x20..=0x2F => {
                        self.intermediates.push(byte);
                        self.transition(State::CsiIntermediate);
                    }
                    0x40..=0x7E => {
                        self.params.parse(&self.param_buf);
                        self.handle_csi_dispatch(byte, screen);
                        self.transition(State::Ground);
                    }
                    _ => {
                        self.transition(State::CsiIgnore);
                    }
                }
            }
            State::CsiParam => match byte {
                0x30..=0x3B => {
                    self.param_buf.push(byte);
                }
                0x20..=0x2F => {
                    self.intermediates.push(byte);
                    self.transition(State::CsiIntermediate);
                }
                0x40..=0x7E => {
                    self.params.parse(&self.param_buf);
                    self.handle_csi_dispatch(byte, screen);
                    self.transition(State::Ground);
                }
                0x3C..=0x3F => {
                    self.transition(State::CsiIgnore);
                }
                _ => {
                    self.transition(State::CsiIgnore);
                }
            },
            State::CsiIntermediate => match byte {
                0x20..=0x2F => {
                    self.intermediates.push(byte);
                }
                0x40..=0x7E => {
                    self.params.parse(&self.param_buf);
                    self.handle_csi_dispatch(byte, screen);
                    self.transition(State::Ground);
                }
                _ => {
                    self.transition(State::CsiIgnore);
                }
            },
            State::CsiIgnore => {
                if (0x40..=0x7E).contains(&byte) {
                    self.transition(State::Ground);
                }
            }
            State::OscString => {
                match byte {
                    0x07 => {
                        // BEL terminates OSC
                        self.handle_osc_dispatch(screen);
                        self.transition(State::Ground);
                    }
                    0x9C => {
                        // ST terminates OSC
                        self.handle_osc_dispatch(screen);
                        self.transition(State::Ground);
                    }
                    0x1B => {
                        // Check for ESC \ (ST)
                        // We'll handle this by transitioning to Escape
                        // and the '\' will complete the OSC
                        self.handle_osc_dispatch(screen);
                        self.transition(State::Escape);
                    }
                    _ => {
                        self.osc_data.push(byte);
                    }
                }
            }
            State::DcsEntry | State::DcsParam | State::DcsIntermediate => match byte {
                0x40..=0x7E => self.transition(State::DcsPassthrough),
                0x30..=0x3F => {
                    self.param_buf.push(byte);
                    self.state = State::DcsParam;
                }
                0x20..=0x2F => {
                    self.intermediates.push(byte);
                    self.state = State::DcsIntermediate;
                }
                _ => self.transition(State::DcsIgnore),
            },
            State::DcsPassthrough => match byte {
                0x9C => {
                    self.handle_dcs_dispatch(screen);
                    self.transition(State::Ground);
                }
                0x1B => {
                    self.handle_dcs_dispatch(screen);
                    self.transition(State::Escape);
                }
                _ => {
                    self.dcs_data.push(byte);
                }
            },
            State::DcsIgnore => {
                if byte == 0x9C || byte == 0x1B {
                    self.transition(State::Ground);
                }
            }
            State::SosPmApcString => {
                if byte == 0x9C || byte == 0x1B || byte == 0x07 {
                    self.transition(State::Ground);
                }
            }
        }
    }

    fn transition(&mut self, new_state: State) {
        // Clear state when entering new sequence states
        match new_state {
            State::Escape | State::CsiEntry | State::DcsEntry => {
                self.params.clear();
                self.intermediates.clear();
                self.param_buf.clear();
            }
            State::Ground => {
                self.utf8_len = 0;
                self.utf8_needed = 0;
            }
            _ => {}
        }
        self.state = new_state;
    }

    /// Handle a run of printable ASCII bytes (fast path).
    fn handle_print_ascii(&self, data: &[u8], screen: &mut Screen) {
        let line_drawing = self.is_line_drawing_active();
        for &byte in data {
            let data = if line_drawing {
                translate_line_drawing(byte)
            } else {
                Utf8Char::from_ascii(byte)
            };
            let cell =
                GridCell { data, style: screen.cursor.style, link: 0, flags: CellFlags::empty() };
            write_cell(screen, &cell);
        }
    }

    /// Handle a single printable byte.
    fn handle_print_byte(&self, byte: u8, screen: &mut Screen) {
        let data = if self.is_line_drawing_active() {
            translate_line_drawing(byte)
        } else {
            Utf8Char::from_ascii(byte)
        };
        let cell =
            GridCell { data, style: screen.cursor.style, link: 0, flags: CellFlags::empty() };
        write_cell(screen, &cell);
    }

    /// Check if the active charset is DEC line drawing.
    fn is_line_drawing_active(&self) -> bool {
        if self.use_g1 { self.g1_line_drawing } else { self.g0_line_drawing }
    }

    /// Handle completed UTF-8 sequence.
    fn handle_print_utf8(&mut self, screen: &mut Screen) {
        let bytes = &self.utf8_buf[..self.utf8_len as usize];
        if let Some(data) = Utf8Char::from_bytes(bytes) {
            let width = data.width();
            let cell =
                GridCell { data, style: screen.cursor.style, link: 0, flags: CellFlags::empty() };
            write_cell(screen, &cell);

            // For wide characters, add a padding cell
            if width > 1 {
                let padding = GridCell {
                    data: Utf8Char::EMPTY,
                    style: screen.cursor.style,
                    link: 0,
                    flags: CellFlags::PADDING,
                };
                write_cell(screen, &padding);
            }
        }
        self.utf8_len = 0;
        self.utf8_needed = 0;
    }

    /// Handle C0 control character execution.
    fn handle_execute(&mut self, byte: u8, screen: &mut Screen) {
        match byte {
            0x07 => {} // BEL - ring bell (handled by client)
            0x08 => {
                // BS - backspace
                if screen.cursor.x > 0 {
                    screen.cursor.x -= 1;
                }
            }
            0x09 => {
                // HT - horizontal tab
                screen.cursor.x = screen.next_tab_stop(screen.cursor.x);
            }
            0x0A..=0x0C => {
                // LF, VT, FF - line feed
                handle_linefeed(screen);
            }
            0x0D => {
                // CR - carriage return
                screen.cursor.x = 0;
            }
            0x0E => self.use_g1 = true, // SO - shift out (use G1 charset)
            0x0F => self.use_g1 = false, // SI - shift in (use G0 charset)
            _ => {}                     // Other C0 controls ignored
        }
    }

    /// Handle ESC sequence dispatch.
    fn handle_esc_dispatch(&mut self, final_byte: u8, screen: &mut Screen) {
        // Check for charset designation: ESC ( X or ESC ) X
        if let Some(&intermediate) = self.intermediates.first() {
            match intermediate {
                0x28 => {
                    // ESC ( X — designate G0 charset
                    self.g0_line_drawing = final_byte == b'0';
                    return;
                }
                0x29 => {
                    // ESC ) X — designate G1 charset
                    self.g1_line_drawing = final_byte == b'0';
                    return;
                }
                _ => {}
            }
        }

        match final_byte {
            b'7' => screen.save_cursor(),    // DECSC
            b'8' => screen.restore_cursor(), // DECRC
            b'D' => handle_linefeed(screen), // IND
            b'E' => {
                // NEL
                screen.cursor.x = 0;
                handle_linefeed(screen);
            }
            b'M' => {
                // RI - reverse index
                if screen.cursor.y == screen.scroll_region.top {
                    screen
                        .grid
                        .scroll_region_down(screen.scroll_region.top, screen.scroll_region.bottom);
                } else if screen.cursor.y > 0 {
                    screen.cursor.y -= 1;
                }
            }
            b'H' => screen.set_tab_stop(screen.cursor.x), // HTS - set tab stop
            b'c' => {
                // RIS - full reset
                self.g0_line_drawing = false;
                self.g1_line_drawing = false;
                self.use_g1 = false;
                screen.reset();
            }
            _ => {}
        }
    }

    /// Handle CSI sequence dispatch.
    fn handle_csi_dispatch(&self, final_byte: u8, screen: &mut Screen) {
        let private = self.intermediates.first().copied();

        match (final_byte, private) {
            // Cursor movement
            (b'A', None) => {
                // CUU - cursor up
                let n = self.params.get_u32(0, 1).max(1);
                screen.cursor.y = screen.cursor.y.saturating_sub(n);
            }
            (b'B', None) => {
                // CUD - cursor down
                let n = self.params.get_u32(0, 1).max(1);
                screen.cursor.y = (screen.cursor.y + n).min(screen.height() - 1);
            }
            (b'C', None) => {
                // CUF - cursor forward
                let n = self.params.get_u32(0, 1).max(1);
                screen.cursor.x = (screen.cursor.x + n).min(screen.width() - 1);
            }
            (b'D', None) => {
                // CUB - cursor backward
                let n = self.params.get_u32(0, 1).max(1);
                screen.cursor.x = screen.cursor.x.saturating_sub(n);
            }
            (b'H' | b'f', None) => {
                // CUP / HVP - cursor position
                let row = self.params.get_u32(0, 1).max(1) - 1;
                let col = self.params.get_u32(1, 1).max(1) - 1;
                screen.cursor.y = row.min(screen.height() - 1);
                screen.cursor.x = col.min(screen.width() - 1);
            }
            (b'J', None) => {
                // ED - erase in display
                let mode = self.params.get_u32(0, 0);
                match mode {
                    0 => {
                        // Erase from cursor to end of display
                        screen.grid.clear_region(
                            screen.cursor.x,
                            screen.cursor.y,
                            screen.width() - 1,
                            screen.height() - 1,
                        );
                    }
                    1 => {
                        // Erase from start to cursor
                        screen.grid.clear_region(0, 0, screen.cursor.x, screen.cursor.y);
                    }
                    2 | 3 => {
                        // Erase entire display
                        screen.grid.clear();
                    }
                    _ => {}
                }
            }
            (b'K', None) => {
                // EL - erase in line
                let mode = self.params.get_u32(0, 0);
                let y = screen.cursor.y;
                match mode {
                    0 => {
                        screen.grid.clear_region(screen.cursor.x, y, screen.width() - 1, y);
                    }
                    1 => {
                        screen.grid.clear_region(0, y, screen.cursor.x, y);
                    }
                    2 => {
                        screen.grid.clear_region(0, y, screen.width() - 1, y);
                    }
                    _ => {}
                }
            }
            (b'E', None) => {
                // CNL - cursor next line
                let n = self.params.get_u32(0, 1).max(1);
                screen.cursor.x = 0;
                screen.cursor.y = (screen.cursor.y + n).min(screen.height() - 1);
            }
            (b'F', None) => {
                // CPL - cursor previous line
                let n = self.params.get_u32(0, 1).max(1);
                screen.cursor.x = 0;
                screen.cursor.y = screen.cursor.y.saturating_sub(n);
            }
            (b'G', None) => {
                // CHA - cursor character absolute (1-based column)
                let col = self.params.get_u32(0, 1).max(1) - 1;
                screen.cursor.x = col.min(screen.width() - 1);
            }
            (b'L', None) => {
                // IL - insert lines
                let n = self.params.get_u32(0, 1).max(1);
                for _ in 0..n {
                    screen.grid.scroll_region_down(screen.cursor.y, screen.scroll_region.bottom);
                }
            }
            (b'M', None) => {
                // DL - delete lines
                let n = self.params.get_u32(0, 1).max(1);
                for _ in 0..n {
                    screen.grid.scroll_region_up(screen.cursor.y, screen.scroll_region.bottom);
                }
            }
            (b'P', None) => {
                // DCH - delete characters
                let n = self.params.get_u32(0, 1).max(1);
                screen.grid.delete_characters(screen.cursor.x, screen.cursor.y, n);
            }
            (b'S', None) => {
                // SU - scroll up
                let n = self.params.get_u32(0, 1).max(1);
                for _ in 0..n {
                    screen
                        .grid
                        .scroll_region_up(screen.scroll_region.top, screen.scroll_region.bottom);
                }
            }
            (b'T', None) => {
                // SD - scroll down
                let n = self.params.get_u32(0, 1).max(1);
                for _ in 0..n {
                    screen
                        .grid
                        .scroll_region_down(screen.scroll_region.top, screen.scroll_region.bottom);
                }
            }
            (b'X', None) => {
                // ECH - erase characters
                let n = self.params.get_u32(0, 1).max(1);
                screen.grid.erase_characters(screen.cursor.x, screen.cursor.y, n);
            }
            (b'Z', None) => {
                // CBT - cursor backward tab
                let n = self.params.get_u32(0, 1).max(1);
                for _ in 0..n {
                    screen.cursor.x = screen.prev_tab_stop(screen.cursor.x);
                }
            }
            (b'@', None) => {
                // ICH - insert characters
                let n = self.params.get_u32(0, 1).max(1);
                screen.grid.insert_characters(screen.cursor.x, screen.cursor.y, n);
            }
            (b'`', None) => {
                // HPA - horizontal position absolute (1-based)
                let col = self.params.get_u32(0, 1).max(1) - 1;
                screen.cursor.x = col.min(screen.width() - 1);
            }
            (b'b', None) => {
                // REP - repeat preceding graphic character
                let n = self.params.get_u32(0, 1).max(1);
                if screen.cursor.x > 0 {
                    let prev_cell = screen.grid.get_cell(screen.cursor.x - 1, screen.cursor.y);
                    if !prev_cell.flags.contains(CellFlags::CLEARED) {
                        for _ in 0..n {
                            write_cell(screen, &prev_cell);
                        }
                    }
                }
            }
            (b'd', None) => {
                // VPA - vertical position absolute (1-based)
                let row = self.params.get_u32(0, 1).max(1) - 1;
                screen.cursor.y = row.min(screen.height() - 1);
            }
            (b'g', None) => {
                // TBC - tab clear
                let mode = self.params.get_u32(0, 0);
                match mode {
                    0 => screen.clear_tab_stop(screen.cursor.x),
                    3 => screen.clear_all_tab_stops(),
                    _ => {}
                }
            }
            (b'm', None) => {
                // SGR - select graphic rendition
                self.handle_sgr(screen);
            }
            (b'r', None) => {
                // DECSTBM - set scroll region
                let top = self.params.get_u32(0, 1).max(1) - 1;
                let bottom = self.params.get_u32(1, screen.height()).min(screen.height()) - 1;
                if top < bottom {
                    screen.scroll_region.top = top;
                    screen.scroll_region.bottom = bottom;
                }
                screen.cursor.x = 0;
                screen.cursor.y = 0;
            }
            (b'h', Some(b'?')) => {
                // DECSET - private mode set
                self.handle_decset(screen, true);
            }
            (b'l', Some(b'?')) => {
                // DECRST - private mode reset
                self.handle_decset(screen, false);
            }
            (b'q', Some(b' ')) => {
                // DECSCUSR - set cursor style
                use rmux_core::screen::cursor::CursorStyle;
                screen.cursor.cursor_style = match self.params.get_u32(0, 0) {
                    0 | 1 => CursorStyle::BlinkingBlock,
                    2 => CursorStyle::SteadyBlock,
                    3 => CursorStyle::BlinkingUnderline,
                    4 => CursorStyle::SteadyUnderline,
                    5 => CursorStyle::BlinkingBar,
                    6 => CursorStyle::SteadyBar,
                    _ => CursorStyle::Default,
                };
            }
            // DSR — Device Status Report
            (b'n', None) => {
                match self.params.get_u32(0, 0) {
                    // Status report → respond "OK"
                    5 => {
                        screen.replies.extend_from_slice(b"\x1b[0n");
                    }
                    // Cursor position report → respond with position
                    6 => {
                        use std::fmt::Write;
                        let mut reply = String::new();
                        write!(reply, "\x1b[{};{}R", screen.cursor.y + 1, screen.cursor.x + 1).ok();
                        screen.replies.extend_from_slice(reply.as_bytes());
                    }
                    _ => {}
                }
            }
            _ => {} // Unhandled CSI sequences
        }
    }

    /// Handle SGR (Select Graphic Rendition) parameters.
    fn handle_sgr(&self, screen: &mut Screen) {
        let params = self.params.values();
        if params.is_empty() {
            // SGR 0 - reset
            screen.cursor.style = Style::DEFAULT;
            return;
        }

        let mut i = 0;
        while i < params.len() {
            let p = if params[i] < 0 { 0 } else { params[i] };
            match p {
                0 => screen.cursor.style = Style::DEFAULT,
                1 => screen.cursor.style.attrs |= Attrs::BOLD,
                2 => screen.cursor.style.attrs |= Attrs::DIM,
                3 => screen.cursor.style.attrs |= Attrs::ITALICS,
                4 => screen.cursor.style.attrs |= Attrs::UNDERSCORE,
                5 => screen.cursor.style.attrs |= Attrs::BLINK,
                7 => screen.cursor.style.attrs |= Attrs::REVERSE,
                8 => screen.cursor.style.attrs |= Attrs::HIDDEN,
                9 => screen.cursor.style.attrs |= Attrs::STRIKETHROUGH,
                21 => screen.cursor.style.attrs |= Attrs::DOUBLE_UNDERSCORE,
                22 => {
                    screen.cursor.style.attrs -=
                        screen.cursor.style.attrs & (Attrs::BOLD | Attrs::DIM);
                }
                23 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::ITALICS,
                24 => {
                    screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::ALL_UNDERLINES;
                }
                25 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::BLINK,
                27 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::REVERSE,
                28 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::HIDDEN,
                29 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::STRIKETHROUGH,
                30..=37 => screen.cursor.style.fg = Color::Palette((p - 30) as u8),
                38 => {
                    i += 1;
                    if i < params.len() {
                        match params[i] {
                            5 if i + 1 < params.len() => {
                                i += 1;
                                screen.cursor.style.fg = Color::Palette(params[i] as u8);
                            }
                            2 if i + 3 < params.len() => {
                                let r = params[i + 1] as u8;
                                let g = params[i + 2] as u8;
                                let b = params[i + 3] as u8;
                                screen.cursor.style.fg = Color::Rgb { r, g, b };
                                i += 3;
                            }
                            _ => {}
                        }
                    }
                }
                39 => screen.cursor.style.fg = Color::Default,
                40..=47 => screen.cursor.style.bg = Color::Palette((p - 40) as u8),
                48 => {
                    i += 1;
                    if i < params.len() {
                        match params[i] {
                            5 if i + 1 < params.len() => {
                                i += 1;
                                screen.cursor.style.bg = Color::Palette(params[i] as u8);
                            }
                            2 if i + 3 < params.len() => {
                                let r = params[i + 1] as u8;
                                let g = params[i + 2] as u8;
                                let b = params[i + 3] as u8;
                                screen.cursor.style.bg = Color::Rgb { r, g, b };
                                i += 3;
                            }
                            _ => {}
                        }
                    }
                }
                49 => screen.cursor.style.bg = Color::Default,
                53 => screen.cursor.style.attrs |= Attrs::OVERLINE,
                55 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::OVERLINE,
                90..=97 => screen.cursor.style.fg = Color::Palette((p - 90 + 8) as u8),
                100..=107 => screen.cursor.style.bg = Color::Palette((p - 100 + 8) as u8),
                _ => {} // Unknown SGR parameter
            }
            i += 1;
        }
    }

    /// Handle DECSET/DECRST (private mode set/reset).
    fn handle_decset(&self, screen: &mut Screen, set: bool) {
        use rmux_core::screen::ModeFlags;
        for &p in self.params.values() {
            if p < 0 {
                continue;
            }
            let flag = match p {
                1 => ModeFlags::CURSOR_KEYS,
                7 => ModeFlags::WRAP,
                12 | 13 => continue, // Cursor blink - not a mode flag
                25 => ModeFlags::CURSOR_VISIBLE,
                1000 => ModeFlags::MOUSE_STANDARD,
                1002 => ModeFlags::MOUSE_BUTTON,
                1003 => ModeFlags::MOUSE_ANY,
                1004 => ModeFlags::FOCUSON,
                1006 => ModeFlags::MOUSE_SGR,
                1049 => {
                    // Alternate screen with saved cursor
                    if set {
                        screen.enter_alternate();
                    } else {
                        screen.exit_alternate();
                    }
                    continue;
                }
                2004 => ModeFlags::BRACKETPASTE,
                _ => continue,
            };
            if set {
                screen.mode |= flag;
            } else {
                screen.mode -= screen.mode & flag;
            }
        }
    }

    /// Handle OSC dispatch.
    fn handle_osc_dispatch(&mut self, screen: &mut Screen) {
        if self.osc_data.is_empty() {
            return;
        }

        // Clone to avoid borrow conflicts with &mut self methods
        let osc = std::mem::take(&mut self.osc_data);

        // Parse OSC number
        let sep = osc.iter().position(|&b| b == b';');
        let (num_bytes, data) =
            if let Some(pos) = sep { (&osc[..pos], &osc[pos + 1..]) } else { (&osc[..], &[][..]) };

        let num: i32 =
            std::str::from_utf8(num_bytes).ok().and_then(|s| s.parse().ok()).unwrap_or(-1);

        match num {
            0 | 2 => {
                // Set window title
                if let Ok(title) = std::str::from_utf8(data) {
                    screen.title = title.to_string();
                }
            }
            1 => {
                // Set icon name (stored as title in tmux)
                if let Ok(title) = std::str::from_utf8(data) {
                    screen.title = title.to_string();
                }
            }
            4 => {
                // Set/query palette color: OSC 4;index;spec ST
                self.handle_osc_palette_color(data, screen);
            }
            7 => {
                // Set working directory
                if let Ok(path) = std::str::from_utf8(data) {
                    screen.path = Some(path.to_string());
                }
            }
            8 => {
                // Hyperlink: OSC 8;params;uri ST
                self.handle_osc_hyperlink(data, screen);
            }
            10 => {
                // Set/query foreground color
                if let Ok(color) = std::str::from_utf8(data) {
                    if !color.is_empty() && color != "?" {
                        screen
                            .notifications
                            .push_back(Notification::SetForegroundColor(color.to_string()));
                    }
                }
            }
            11 => {
                // Set/query background color
                if let Ok(color) = std::str::from_utf8(data) {
                    if !color.is_empty() && color != "?" {
                        screen
                            .notifications
                            .push_back(Notification::SetBackgroundColor(color.to_string()));
                    }
                }
            }
            52 => {
                // Clipboard: OSC 52;selection;base64data ST
                self.handle_osc_clipboard(data, screen);
            }
            112 => {
                // Reset cursor color
                screen.notifications.push_back(Notification::ResetCursorColor);
            }
            _ => {}
        }
    }

    /// Handle OSC 4: palette color set/query.
    /// Format: OSC 4;index;spec ST where spec is rgb:RR/GG/BB or ?
    fn handle_osc_palette_color(&self, data: &[u8], screen: &mut Screen) {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        // Format: index;rgb:RR/GG/BB
        let Some((idx_str, color_spec)) = text.split_once(';') else {
            return;
        };
        let Ok(idx) = idx_str.parse::<u8>() else {
            return;
        };
        if color_spec == "?" {
            return; // Query — would need response channel
        }
        if let Some(rgb) = color_spec.strip_prefix("rgb:") {
            if let Some((r, g, b)) = parse_rgb_spec(rgb) {
                screen.notifications.push_back(Notification::SetPaletteColor(idx, r, g, b));
            }
        }
    }

    /// Handle OSC 8: hyperlink.
    /// Format: OSC 8;params;uri ST
    fn handle_osc_hyperlink(&mut self, data: &[u8], screen: &mut Screen) {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        // Split into params;uri
        let Some((_params, uri)) = text.split_once(';') else {
            return;
        };
        if uri.is_empty() {
            // Close hyperlink
            self.current_hyperlink = 0;
        } else {
            // Open hyperlink — assign a unique ID
            self.hyperlink_counter += 1;
            self.current_hyperlink = self.hyperlink_counter;
        }
        let _ = screen; // hyperlink ID is stored on cells via current_hyperlink
    }

    /// Handle OSC 52: clipboard.
    /// Format: OSC 52;selection;base64data ST
    fn handle_osc_clipboard(&self, data: &[u8], screen: &mut Screen) {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        // Format: selection;base64data (selection is c/p/s etc., often just "c")
        let base64_data = if let Some((_sel, b64)) = text.split_once(';') { b64 } else { text };
        if base64_data == "?" {
            return; // Query — would need response channel
        }
        screen.notifications.push_back(Notification::SetClipboard(base64_data.to_string()));
    }

    /// Handle DCS dispatch.
    fn handle_dcs_dispatch(&mut self, _screen: &mut Screen) {
        // DCS sequences (like DECRQSS, sixel) will be implemented later
    }

    /// Current parser state (for testing/debugging).
    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }
}

impl Default for InputParser {
    fn default() -> Self {
        Self::new()
    }
}

/// DEC Special Graphics (line drawing) character set translation.
/// Maps ASCII bytes 0x60-0x7E to Unicode box-drawing equivalents.
fn translate_line_drawing(byte: u8) -> Utf8Char {
    match byte {
        b'j' => Utf8Char::from_char('┘'), // lower-right corner
        b'k' => Utf8Char::from_char('┐'), // upper-right corner
        b'l' => Utf8Char::from_char('┌'), // upper-left corner
        b'm' => Utf8Char::from_char('└'), // lower-left corner
        b'n' => Utf8Char::from_char('┼'), // crossing
        b'q' => Utf8Char::from_char('─'), // horizontal line
        b't' => Utf8Char::from_char('├'), // left tee
        b'u' => Utf8Char::from_char('┤'), // right tee
        b'v' => Utf8Char::from_char('┴'), // bottom tee
        b'w' => Utf8Char::from_char('┬'), // top tee
        b'x' => Utf8Char::from_char('│'), // vertical line
        b'`' => Utf8Char::from_char('◆'), // diamond
        b'a' => Utf8Char::from_char('▒'), // checkerboard
        b'f' => Utf8Char::from_char('°'), // degree
        b'g' => Utf8Char::from_char('±'), // plus/minus
        b'o' => Utf8Char::from_char('⎺'), // scan line 1
        b'p' => Utf8Char::from_char('⎻'), // scan line 3
        b'r' => Utf8Char::from_char('⎼'), // scan line 7
        b's' => Utf8Char::from_char('⎽'), // scan line 9
        b'~' => Utf8Char::from_char('·'), // bullet
        b'y' => Utf8Char::from_char('≤'), // less-than-or-equal
        b'z' => Utf8Char::from_char('≥'), // greater-than-or-equal
        b'{' => Utf8Char::from_char('π'), // pi
        b'|' => Utf8Char::from_char('≠'), // not-equal
        b'}' => Utf8Char::from_char('£'), // pound sign
        _ => Utf8Char::from_ascii(byte),  // unmapped: pass through
    }
}

/// Parse an X11 rgb spec like "RR/GG/BB" or "RRRR/GGGG/BBBB" into (r, g, b).
fn parse_rgb_spec(spec: &str) -> Option<(u8, u8, u8)> {
    let parts: Vec<&str> = spec.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let r = u16::from_str_radix(parts[0], 16).ok()?;
    let g = u16::from_str_radix(parts[1], 16).ok()?;
    let b = u16::from_str_radix(parts[2], 16).ok()?;
    // If values are 4-digit hex (0-FFFF), scale down to 8-bit
    if parts[0].len() > 2 {
        Some(((r >> 8) as u8, (g >> 8) as u8, (b >> 8) as u8))
    } else {
        Some((r as u8, g as u8, b as u8))
    }
}

/// Write a cell at the cursor position and advance the cursor.
fn write_cell(screen: &mut Screen, cell: &GridCell) {
    use rmux_core::screen::ModeFlags;

    let width = screen.width();

    // Wrap if at end of line
    if screen.cursor.x >= width {
        if screen.mode.contains(ModeFlags::WRAP) {
            screen.cursor.x = 0;
            handle_linefeed(screen);
        } else {
            screen.cursor.x = width - 1;
        }
    }

    screen.grid.set_cell(screen.cursor.x, screen.cursor.y, cell);
    screen.cursor.x += 1;
}

/// Handle line feed (move cursor down, scroll if at bottom of scroll region).
fn handle_linefeed(screen: &mut Screen) {
    if screen.cursor.y == screen.scroll_region.bottom {
        screen.grid.scroll_region_up(screen.scroll_region.top, screen.scroll_region.bottom);
    } else if screen.cursor.y < screen.height() - 1 {
        screen.cursor.y += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_screen() -> Screen {
        Screen::new(80, 24, 2000)
    }

    #[test]
    fn parse_plain_ascii() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"Hello", &mut screen);
        assert_eq!(screen.cursor.x, 5);
        for (i, ch) in "Hello".chars().enumerate() {
            let cell = screen.grid.get_cell(i as u32, 0);
            assert_eq!(cell.data.as_str(), Some(&ch.to_string()[..]));
        }
    }

    #[test]
    fn parse_newline() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"Line1\r\nLine2", &mut screen);
        assert_eq!(screen.cursor.y, 1);
        assert_eq!(screen.cursor.x, 5);
    }

    #[test]
    fn parse_cursor_movement() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // ESC[10;20H - move cursor to row 10, col 20
        parser.parse(b"\x1b[10;20H", &mut screen);
        assert_eq!(screen.cursor.y, 9); // 1-indexed -> 0-indexed
        assert_eq!(screen.cursor.x, 19);
    }

    #[test]
    fn parse_sgr_bold() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b[1mBold", &mut screen);
        let cell = screen.grid.get_cell(0, 0);
        assert!(cell.style.attrs.contains(Attrs::BOLD));
    }

    #[test]
    fn parse_sgr_fg_color() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b[31mR", &mut screen); // Red
        let cell = screen.grid.get_cell(0, 0);
        assert_eq!(cell.style.fg, Color::Palette(1));
    }

    #[test]
    fn parse_sgr_rgb() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b[38;2;100;200;50mG", &mut screen);
        let cell = screen.grid.get_cell(0, 0);
        assert_eq!(cell.style.fg, Color::Rgb { r: 100, g: 200, b: 50 });
    }

    #[test]
    fn parse_sgr_reset() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b[1;31mR\x1b[0mN", &mut screen);
        let cell0 = screen.grid.get_cell(0, 0);
        assert!(cell0.style.attrs.contains(Attrs::BOLD));
        let cell1 = screen.grid.get_cell(1, 0);
        assert!(!cell1.style.attrs.contains(Attrs::BOLD));
        assert_eq!(cell1.style.fg, Color::Default);
    }

    #[test]
    fn parse_erase_display() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"Hello\x1b[2J", &mut screen);
        // Screen should be cleared
        let cell = screen.grid.get_cell(0, 0);
        assert!(cell.flags.contains(CellFlags::CLEARED));
    }

    #[test]
    fn parse_scroll_region() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b[5;20r", &mut screen);
        assert_eq!(screen.scroll_region.top, 4);
        assert_eq!(screen.scroll_region.bottom, 19);
    }

    #[test]
    fn parse_osc_title() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]0;My Title\x07", &mut screen);
        assert_eq!(screen.title, "My Title");
    }

    #[test]
    fn parse_osc_icon_name() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]1;Icon\x07", &mut screen);
        assert_eq!(screen.title, "Icon");
    }

    #[test]
    fn parse_osc_working_directory() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]7;file:///home/user\x07", &mut screen);
        assert_eq!(screen.path.as_deref(), Some("file:///home/user"));
    }

    #[test]
    fn parse_osc_clipboard() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // OSC 52;c;SGVsbG8= ST (base64 of "Hello")
        parser.parse(b"\x1b]52;c;SGVsbG8=\x07", &mut screen);
        assert_eq!(screen.notifications.len(), 1);
        assert_eq!(screen.notifications[0], Notification::SetClipboard("SGVsbG8=".to_string()));
    }

    #[test]
    fn parse_osc_clipboard_query_ignored() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]52;c;?\x07", &mut screen);
        assert!(screen.notifications.is_empty());
    }

    #[test]
    fn parse_osc_palette_color() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]4;1;rgb:ff/00/00\x07", &mut screen);
        assert_eq!(screen.notifications.len(), 1);
        assert_eq!(screen.notifications[0], Notification::SetPaletteColor(1, 0xff, 0, 0));
    }

    #[test]
    fn parse_osc_palette_color_16bit() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]4;5;rgb:ffff/8080/0000\x07", &mut screen);
        assert_eq!(screen.notifications.len(), 1);
        assert_eq!(screen.notifications[0], Notification::SetPaletteColor(5, 0xff, 0x80, 0));
    }

    #[test]
    fn parse_osc_foreground_color() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]10;rgb:ff/ff/ff\x07", &mut screen);
        assert_eq!(screen.notifications.len(), 1);
        assert_eq!(
            screen.notifications[0],
            Notification::SetForegroundColor("rgb:ff/ff/ff".to_string())
        );
    }

    #[test]
    fn parse_osc_background_color() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]11;rgb:00/00/00\x07", &mut screen);
        assert_eq!(screen.notifications.len(), 1);
        assert_eq!(
            screen.notifications[0],
            Notification::SetBackgroundColor("rgb:00/00/00".to_string())
        );
    }

    #[test]
    fn parse_osc_reset_cursor_color() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]112\x07", &mut screen);
        assert_eq!(screen.notifications.len(), 1);
        assert_eq!(screen.notifications[0], Notification::ResetCursorColor);
    }

    #[test]
    fn parse_osc_hyperlink_open_close() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // Open hyperlink
        parser.parse(b"\x1b]8;;https://example.com\x07", &mut screen);
        assert_eq!(parser.current_hyperlink, 1);
        // Close hyperlink
        parser.parse(b"\x1b]8;;\x07", &mut screen);
        assert_eq!(parser.current_hyperlink, 0);
    }

    #[test]
    fn parse_osc_st_terminator() {
        // OSC terminated with ST (ESC \) instead of BEL
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b]0;Title via ST\x1b\\", &mut screen);
        assert_eq!(screen.title, "Title via ST");
    }

    #[test]
    fn parse_rgb_spec_helper() {
        assert_eq!(parse_rgb_spec("ff/00/80"), Some((0xff, 0, 0x80)));
        assert_eq!(parse_rgb_spec("ffff/8080/0000"), Some((0xff, 0x80, 0)));
        assert_eq!(parse_rgb_spec("invalid"), None);
        assert_eq!(parse_rgb_spec("xx/yy/zz"), None);
    }

    #[test]
    fn dec_line_drawing_charset() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // ESC (0 activates DEC line drawing for G0
        parser.parse(b"\x1b(0", &mut screen);
        assert!(parser.g0_line_drawing);
        // 'q' should become '─' (horizontal line)
        parser.parse(b"lqqk", &mut screen);
        let cell = screen.grid.get_cell(0, 0);
        assert_eq!(cell.data.as_str(), Some("┌"));
        let cell = screen.grid.get_cell(1, 0);
        assert_eq!(cell.data.as_str(), Some("─"));
        let cell = screen.grid.get_cell(3, 0);
        assert_eq!(cell.data.as_str(), Some("┐"));
    }

    #[test]
    fn dec_line_drawing_deactivate() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // Activate, draw, deactivate, draw
        parser.parse(b"\x1b(0q\x1b(Bq", &mut screen);
        let cell0 = screen.grid.get_cell(0, 0);
        assert_eq!(cell0.data.as_str(), Some("─")); // line drawing
        let cell1 = screen.grid.get_cell(1, 0);
        assert_eq!(cell1.data.as_str(), Some("q")); // normal ASCII
    }

    #[test]
    fn so_si_charset_switching() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // Set G1 to line drawing, then SO to activate it
        parser.parse(b"\x1b)0", &mut screen);
        assert!(parser.g1_line_drawing);
        parser.parse(b"\x0Eq", &mut screen); // SO + 'q'
        let cell = screen.grid.get_cell(0, 0);
        assert_eq!(cell.data.as_str(), Some("─"));
        // SI back to G0 (ASCII)
        parser.parse(b"\x0Fq", &mut screen);
        let cell = screen.grid.get_cell(1, 0);
        assert_eq!(cell.data.as_str(), Some("q"));
    }

    #[test]
    fn translate_line_drawing_coverage() {
        // Verify key box-drawing characters
        assert_eq!(translate_line_drawing(b'j').as_str(), Some("┘"));
        assert_eq!(translate_line_drawing(b'k').as_str(), Some("┐"));
        assert_eq!(translate_line_drawing(b'l').as_str(), Some("┌"));
        assert_eq!(translate_line_drawing(b'm').as_str(), Some("└"));
        assert_eq!(translate_line_drawing(b'n').as_str(), Some("┼"));
        assert_eq!(translate_line_drawing(b'x').as_str(), Some("│"));
        assert_eq!(translate_line_drawing(b'`').as_str(), Some("◆"));
        // Unmapped byte passes through
        assert_eq!(translate_line_drawing(b'A').as_str(), Some("A"));
    }

    #[test]
    fn parse_decscusr_cursor_styles() {
        use rmux_core::screen::cursor::CursorStyle;
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // CSI 2 SP q → steady block
        parser.parse(b"\x1b[2 q", &mut screen);
        assert_eq!(screen.cursor.cursor_style, CursorStyle::SteadyBlock);
        // CSI 5 SP q → blinking bar
        parser.parse(b"\x1b[5 q", &mut screen);
        assert_eq!(screen.cursor.cursor_style, CursorStyle::BlinkingBar);
        // CSI 0 SP q → blinking block (default)
        parser.parse(b"\x1b[0 q", &mut screen);
        assert_eq!(screen.cursor.cursor_style, CursorStyle::BlinkingBlock);
    }

    #[test]
    fn parse_alternate_screen() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"Normal\x1b[?1049h", &mut screen);
        assert!(screen.alternate.is_some());
        parser.parse(b"\x1b[?1049l", &mut screen);
        assert!(screen.alternate.is_none());
    }

    #[test]
    fn fast_path_long_ascii() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        let data = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz";
        parser.parse(data, &mut screen);
        assert_eq!(screen.cursor.x, data.len() as u32);
    }

    #[test]
    fn utf8_cjk_character() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse("世".as_bytes(), &mut screen);
        let cell = screen.grid.get_cell(0, 0);
        assert_eq!(cell.data.as_str(), Some("世"));
        assert_eq!(cell.data.width(), 2);
        // Position should advance by 2 (wide char + padding)
        assert_eq!(screen.cursor.x, 2);
    }

    #[test]
    fn wrap_at_end_of_line() {
        let mut screen = Screen::new(5, 3, 0);
        let mut parser = InputParser::new();
        parser.parse(b"12345X", &mut screen);
        // Should wrap to next line
        assert_eq!(screen.cursor.y, 1);
        assert_eq!(screen.cursor.x, 1);
    }

    #[test]
    fn parser_state_ground_after_complete_sequence() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        parser.parse(b"\x1b[1mA", &mut screen);
        assert_eq!(parser.state(), State::Ground);
    }

    #[test]
    fn dsr_status_report() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // CSI 5 n → DSR status report
        parser.parse(b"\x1b[5n", &mut screen);
        assert_eq!(screen.replies, b"\x1b[0n");
    }

    #[test]
    fn dsr_cursor_position_report() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // Move cursor to (5, 3) then query
        screen.cursor.x = 5;
        screen.cursor.y = 3;
        parser.parse(b"\x1b[6n", &mut screen);
        // CPR is 1-based: row 4, col 6
        assert_eq!(screen.replies, b"\x1b[4;6R");
    }

    #[test]
    fn dsr_cursor_position_origin() {
        let mut screen = make_screen();
        let mut parser = InputParser::new();
        // Cursor at origin (0,0) → CPR should be 1;1
        parser.parse(b"\x1b[6n", &mut screen);
        assert_eq!(screen.replies, b"\x1b[1;1R");
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_ascii_printable_never_panics(data in proptest::collection::vec(0x20u8..0x7f, 0..1024)) {
                let mut screen = Screen::new(80, 24, 100);
                let mut parser = InputParser::new();
                parser.parse(&data, &mut screen);
                prop_assert!(screen.cursor.x <= 80);
                prop_assert!(screen.cursor.y < 24);
            }

            #[test]
            fn parse_ascii_ends_in_ground(data in proptest::collection::vec(0x20u8..0x7f, 0..512)) {
                let mut screen = Screen::new(80, 24, 0);
                let mut parser = InputParser::new();
                parser.parse(&data, &mut screen);
                prop_assert_eq!(parser.state(), State::Ground);
            }

            #[test]
            fn parse_preserves_screen_dimensions(
                data in proptest::collection::vec(0x20u8..0x7f, 0..512),
                width in 10u32..200,
                height in 5u32..50,
            ) {
                let mut screen = Screen::new(width, height, 0);
                let mut parser = InputParser::new();
                parser.parse(&data, &mut screen);
                prop_assert_eq!(screen.width(), width);
                prop_assert_eq!(screen.height(), height);
            }

            #[test]
            fn parse_with_escapes(n_lines in 1u32..50, width in 10u32..100) {
                let mut screen = Screen::new(width, 24, 100);
                let mut parser = InputParser::new();
                let mut data = Vec::new();
                for _ in 0..n_lines {
                    data.extend_from_slice(b"\x1b[32m");
                    data.extend(std::iter::repeat_n(b'X', width.min(79) as usize));
                    data.extend_from_slice(b"\x1b[0m\r\n");
                }
                parser.parse(&data, &mut screen);
                prop_assert_eq!(screen.width(), width);
            }
        }
    }
}
