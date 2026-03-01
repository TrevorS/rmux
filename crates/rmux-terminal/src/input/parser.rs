//! VT100/xterm escape sequence parser.
//!
//! Based on Paul Williams' state machine with the ASCII fast path optimization.
//! This is the hottest code path in rmux - all PTY output flows through here.

use super::params::Params;
use rmux_core::grid::cell::{CellFlags, GridCell};
use rmux_core::screen::Screen;
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
    CsiDispatch {
        params: Params,
        intermediates: SmallVec<[u8; 4]>,
        final_byte: u8,
    },
    /// ESC sequence dispatched.
    EscDispatch {
        intermediates: SmallVec<[u8; 4]>,
        final_byte: u8,
    },
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
                    // ESC - always transitions to Escape state
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
            State::EscapeIntermediate => {
                match byte {
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
                }
            }
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
            State::CsiParam => {
                match byte {
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
                }
            }
            State::CsiIntermediate => {
                match byte {
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
                }
            }
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
            State::DcsEntry | State::DcsParam | State::DcsIntermediate => {
                match byte {
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
                }
            }
            State::DcsPassthrough => {
                match byte {
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
                }
            }
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
        for &byte in data {
            let cell = GridCell {
                data: Utf8Char::from_ascii(byte),
                style: screen.cursor.style,
                link: 0,
                flags: CellFlags::empty(),
            };
            write_cell(screen, &cell);
        }
    }

    /// Handle a single printable byte.
    fn handle_print_byte(&self, byte: u8, screen: &mut Screen) {
        let cell = GridCell {
            data: Utf8Char::from_ascii(byte),
            style: screen.cursor.style,
            link: 0,
            flags: CellFlags::empty(),
        };
        write_cell(screen, &cell);
    }

    /// Handle completed UTF-8 sequence.
    fn handle_print_utf8(&mut self, screen: &mut Screen) {
        let bytes = &self.utf8_buf[..self.utf8_len as usize];
        if let Some(data) = Utf8Char::from_bytes(bytes) {
            let width = data.width();
            let cell = GridCell {
                data,
                style: screen.cursor.style,
                link: 0,
                flags: CellFlags::empty(),
            };
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
    fn handle_execute(&self, byte: u8, screen: &mut Screen) {
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
            0x0E => {} // SO - shift out (alternate charset)
            0x0F => {} // SI - shift in (normal charset)
            _ => {}    // Other C0 controls ignored
        }
    }

    /// Handle ESC sequence dispatch.
    fn handle_esc_dispatch(&self, final_byte: u8, screen: &mut Screen) {
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
                    screen.grid.scroll_region_down(
                        screen.scroll_region.top,
                        screen.scroll_region.bottom,
                    );
                } else if screen.cursor.y > 0 {
                    screen.cursor.y -= 1;
                }
            }
            b'c' => screen.reset(), // RIS - full reset
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
                        screen
                            .grid
                            .clear_region(0, 0, screen.cursor.x, screen.cursor.y);
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
                        screen.grid.clear_region(
                            screen.cursor.x,
                            y,
                            screen.width() - 1,
                            y,
                        );
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
            (b'L', None) => {
                // IL - insert lines
                let n = self.params.get_u32(0, 1).max(1);
                for _ in 0..n {
                    screen.grid.scroll_region_down(
                        screen.cursor.y,
                        screen.scroll_region.bottom,
                    );
                }
            }
            (b'M', None) => {
                // DL - delete lines
                let n = self.params.get_u32(0, 1).max(1);
                for _ in 0..n {
                    screen.grid.scroll_region_up(
                        screen.cursor.y,
                        screen.scroll_region.bottom,
                    );
                }
            }
            (b'm', None) => {
                // SGR - select graphic rendition
                self.handle_sgr(screen);
            }
            (b'r', None) => {
                // DECSTBM - set scroll region
                let top = self.params.get_u32(0, 1).max(1) - 1;
                let bottom = self
                    .params
                    .get_u32(1, screen.height())
                    .min(screen.height())
                    - 1;
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
                    screen.cursor.style.attrs -= screen.cursor.style.attrs & (Attrs::BOLD | Attrs::DIM);
                }
                23 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::ITALICS,
                24 => screen.cursor.style.attrs -= screen.cursor.style.attrs & Attrs::ALL_UNDERLINES,
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

        // Parse OSC number
        let sep = self.osc_data.iter().position(|&b| b == b';');
        let (num_bytes, data) = if let Some(pos) = sep {
            (&self.osc_data[..pos], &self.osc_data[pos + 1..])
        } else {
            (&self.osc_data[..], &[][..])
        };

        let num: i32 = std::str::from_utf8(num_bytes)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(-1);

        match num {
            0 | 2 => {
                // Set window title
                if let Ok(title) = std::str::from_utf8(data) {
                    screen.title = title.to_string();
                }
            }
            7 => {
                // Set working directory
                if let Ok(path) = std::str::from_utf8(data) {
                    screen.path = Some(path.to_string());
                }
            }
            _ => {} // Other OSC sequences
        }
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
        screen.grid.scroll_region_up(
            screen.scroll_region.top,
            screen.scroll_region.bottom,
        );
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
        assert_eq!(
            cell.style.fg,
            Color::Rgb {
                r: 100,
                g: 200,
                b: 50
            }
        );
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
}
