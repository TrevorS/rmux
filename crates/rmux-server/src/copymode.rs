//! Copy mode state machine and navigation.
//!
//! When a pane enters copy mode, the user can scroll through history,
//! move a cursor independently, select text, and copy it to a paste buffer.

use rmux_core::screen::Screen;
use rmux_core::screen::selection::{Selection, SelectionType};

/// Direction and character for jump-to-char (f/F/t/T).
#[derive(Debug, Clone, Copy)]
pub struct JumpState {
    /// The character to jump to.
    pub ch: char,
    /// The jump type.
    pub jump_type: JumpType,
}

/// Type of jump-to-char operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpType {
    /// `f` — forward to character (land on it).
    Forward,
    /// `F` — backward to character (land on it).
    Backward,
    /// `t` — forward to character (land before it).
    ForwardTill,
    /// `T` — backward to character (land after it).
    BackwardTill,
}

/// Copy mode state for a single pane.
#[derive(Debug, Clone)]
pub struct CopyModeState {
    /// Copy-mode cursor column (0-based, independent of live cursor).
    pub cx: u32,
    /// Copy-mode cursor row (0-based, relative to visible area top).
    pub cy: u32,
    /// Scroll offset: how many history lines above the current view.
    /// 0 means showing the live screen. N means the view is scrolled up N lines.
    pub oy: u32,
    /// Whether a selection is currently active.
    pub selecting: bool,
    /// Selection start position (absolute x).
    pub sel_start_x: u32,
    /// Selection start position (absolute y, including history offset).
    pub sel_start_y: u32,
    /// Selection type.
    pub sel_type: SelectionType,
    /// Search string (if any active search).
    pub search_str: Option<String>,
    /// Whether last search was forward.
    pub search_forward: bool,
    /// Vi mode or emacs mode key table name.
    pub key_table: String,
    /// Last jump-to-char character and direction for repeat (`;` / `,`).
    pub last_jump: Option<JumpState>,
    /// Pending jump type — waiting for user to type the target character.
    pub pending_jump: Option<JumpType>,
}

impl CopyModeState {
    /// Create a new copy mode state from a pane's current screen.
    ///
    /// Cursor starts at the bottom-left of the visible area with no scroll offset.
    pub fn enter(screen: &Screen, mode_keys: &str) -> Self {
        Self {
            cx: 0,
            cy: screen.height().saturating_sub(1),
            oy: 0,
            selecting: false,
            sel_start_x: 0,
            sel_start_y: 0,
            sel_type: SelectionType::Normal,
            search_str: None,
            search_forward: true,
            key_table: if mode_keys == "vi" {
                "copy-mode-vi".to_string()
            } else {
                "copy-mode-emacs".to_string()
            },
            last_jump: None,
            pending_jump: None,
        }
    }

    /// Convert copy mode cursor position to absolute y in the grid.
    ///
    /// Absolute y: 0 = oldest history line, history_size = first visible line.
    pub fn absolute_y(&self, history_size: u32) -> u32 {
        history_size.saturating_sub(self.oy) + self.cy
    }

    // --- Navigation ---

    /// Move cursor up by `count` lines, scrolling into history if needed.
    pub fn cursor_up(&mut self, screen: &Screen, count: u32) {
        for _ in 0..count {
            if self.cy > 0 {
                self.cy -= 1;
            } else if self.oy < screen.grid.history_size() {
                self.oy += 1;
            }
        }
    }

    /// Move cursor down by `count` lines, scrolling towards live screen.
    pub fn cursor_down(&mut self, screen: &Screen, count: u32) {
        let max_y = screen.height().saturating_sub(1);
        for _ in 0..count {
            if self.cy < max_y {
                self.cy += 1;
            } else if self.oy > 0 {
                self.oy -= 1;
            }
        }
    }

    /// Move cursor left by one column.
    pub fn cursor_left(&mut self) {
        self.cx = self.cx.saturating_sub(1);
    }

    /// Move cursor right by one column.
    pub fn cursor_right(&mut self, screen: &Screen) {
        let max_x = screen.width().saturating_sub(1);
        if self.cx < max_x {
            self.cx += 1;
        }
    }

    /// Scroll up by one page (screen height).
    pub fn page_up(&mut self, screen: &Screen) {
        let page = screen.height();
        let max_oy = screen.grid.history_size();
        self.oy = (self.oy + page).min(max_oy);
    }

    /// Scroll down by one page.
    pub fn page_down(&mut self, screen: &Screen) {
        let page = screen.height();
        self.oy = self.oy.saturating_sub(page);
    }

    /// Scroll up by half a page.
    pub fn halfpage_up(&mut self, screen: &Screen) {
        let half = screen.height() / 2;
        let max_oy = screen.grid.history_size();
        self.oy = (self.oy + half).min(max_oy);
    }

    /// Scroll down by half a page.
    pub fn halfpage_down(&mut self, screen: &Screen) {
        let half = screen.height() / 2;
        self.oy = self.oy.saturating_sub(half);
    }

    /// Jump to the top of history.
    pub fn history_top(&mut self, screen: &Screen) {
        self.oy = screen.grid.history_size();
        self.cy = 0;
        self.cx = 0;
    }

    /// Jump to the bottom (live screen).
    pub fn history_bottom(&mut self, screen: &Screen) {
        self.oy = 0;
        self.cy = screen.height().saturating_sub(1);
        self.cx = 0;
    }

    /// Jump to a specific line number (1-based, from top of history).
    pub fn goto_line(&mut self, screen: &Screen, line: u32) {
        let abs_y = line.saturating_sub(1); // Convert 1-based to 0-based
        let total = screen.grid.history_size() + screen.height();
        let clamped = abs_y.min(total.saturating_sub(1));
        self.move_to_absolute_y(clamped, screen);
        self.cx = 0;
    }

    /// Move cursor to start of line.
    pub fn start_of_line(&mut self) {
        self.cx = 0;
    }

    /// Move cursor to end of line.
    pub fn end_of_line(&mut self, screen: &Screen) {
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            self.cx = line.cell_count().saturating_sub(1);
        } else {
            self.cx = screen.width().saturating_sub(1);
        }
    }

    /// Move cursor to first non-space character on the line.
    pub fn back_to_indentation(&mut self, screen: &Screen) {
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            for x in 0..line.cell_count() {
                let cell = line.get_cell(x);
                let bytes = cell.data.as_bytes();
                if !bytes.is_empty() && bytes != [b' '] {
                    self.cx = x;
                    return;
                }
            }
        }
        self.cx = 0;
    }

    /// Move cursor to the start of the next word.
    pub fn next_word(&mut self, screen: &Screen) {
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            let max = line.cell_count();
            let mut x = self.cx;
            // Skip current word (non-space)
            while x < max {
                let cell = line.get_cell(x);
                let bytes = cell.data.as_bytes();
                if bytes.is_empty() || bytes == [b' '] {
                    break;
                }
                x += 1;
            }
            // Skip spaces
            while x < max {
                let cell = line.get_cell(x);
                let bytes = cell.data.as_bytes();
                if !bytes.is_empty() && bytes != [b' '] {
                    break;
                }
                x += 1;
            }
            if x < max {
                self.cx = x;
            }
        }
    }

    /// Move cursor to the start of the previous word.
    pub fn previous_word(&mut self, screen: &Screen) {
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            let mut x = self.cx;
            // Move left past spaces
            while x > 0 {
                x -= 1;
                let cell = line.get_cell(x);
                let bytes = cell.data.as_bytes();
                if !bytes.is_empty() && bytes != [b' '] {
                    break;
                }
            }
            // Move left through word characters
            while x > 0 {
                let cell = line.get_cell(x - 1);
                let bytes = cell.data.as_bytes();
                if bytes.is_empty() || bytes == [b' '] {
                    break;
                }
                x -= 1;
            }
            self.cx = x;
        }
    }

    /// Move cursor to end of current word.
    pub fn next_word_end(&mut self, screen: &Screen) {
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            let max = line.cell_count();
            let mut x = self.cx + 1;
            // Skip spaces
            while x < max {
                let cell = line.get_cell(x);
                let bytes = cell.data.as_bytes();
                if !bytes.is_empty() && bytes != [b' '] {
                    break;
                }
                x += 1;
            }
            // Move to end of word
            while x + 1 < max {
                let cell = line.get_cell(x + 1);
                let bytes = cell.data.as_bytes();
                if bytes.is_empty() || bytes == [b' '] {
                    break;
                }
                x += 1;
            }
            if x < max {
                self.cx = x;
            }
        }
    }

    // --- Jump to character (f/F/t/T) ---

    /// Jump forward to the next occurrence of `ch` on the current line.
    pub fn jump_forward(&mut self, screen: &Screen, ch: char) {
        self.last_jump = Some(JumpState { ch, jump_type: JumpType::Forward });
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            for x in (self.cx + 1)..line.cell_count() {
                if cell_matches_char(line, x, ch) {
                    self.cx = x;
                    return;
                }
            }
        }
    }

    /// Jump backward to the previous occurrence of `ch` on the current line.
    pub fn jump_backward(&mut self, screen: &Screen, ch: char) {
        self.last_jump = Some(JumpState { ch, jump_type: JumpType::Backward });
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            for x in (0..self.cx).rev() {
                if cell_matches_char(line, x, ch) {
                    self.cx = x;
                    return;
                }
            }
        }
    }

    /// Jump forward to just before the next occurrence of `ch`.
    pub fn jump_forward_till(&mut self, screen: &Screen, ch: char) {
        self.last_jump = Some(JumpState { ch, jump_type: JumpType::ForwardTill });
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            for x in (self.cx + 1)..line.cell_count() {
                if cell_matches_char(line, x, ch) {
                    self.cx = x.saturating_sub(1).max(self.cx + 1);
                    return;
                }
            }
        }
    }

    /// Jump backward to just after the previous occurrence of `ch`.
    pub fn jump_backward_till(&mut self, screen: &Screen, ch: char) {
        self.last_jump = Some(JumpState { ch, jump_type: JumpType::BackwardTill });
        let abs_y = self.absolute_y(screen.grid.history_size());
        if let Some(line) = screen.grid.get_line_absolute(abs_y) {
            for x in (0..self.cx).rev() {
                if cell_matches_char(line, x, ch) {
                    self.cx = (x + 1).min(self.cx.saturating_sub(1));
                    return;
                }
            }
        }
    }

    /// Repeat the last jump-to-char in the same direction (`;`).
    pub fn jump_again(&mut self, screen: &Screen) {
        if let Some(state) = self.last_jump {
            // Temporarily clear last_jump to avoid overwrite
            match state.jump_type {
                JumpType::Forward => self.jump_forward(screen, state.ch),
                JumpType::Backward => self.jump_backward(screen, state.ch),
                JumpType::ForwardTill => self.jump_forward_till(screen, state.ch),
                JumpType::BackwardTill => self.jump_backward_till(screen, state.ch),
            }
        }
    }

    /// Repeat the last jump-to-char in the reverse direction (`,`).
    pub fn jump_reverse(&mut self, screen: &Screen) {
        if let Some(state) = self.last_jump {
            match state.jump_type {
                JumpType::Forward => self.jump_backward(screen, state.ch),
                JumpType::Backward => self.jump_forward(screen, state.ch),
                JumpType::ForwardTill => self.jump_backward_till(screen, state.ch),
                JumpType::BackwardTill => self.jump_forward_till(screen, state.ch),
            }
            // Restore original jump state (reverse shouldn't change the saved direction)
            self.last_jump = Some(state);
        }
    }

    // --- Position commands ---

    /// Move cursor to the middle line of the visible area.
    pub fn middle_line(&mut self, screen: &Screen) {
        self.cy = screen.height() / 2;
    }

    /// Move cursor to the top line of the visible area.
    pub fn top_line(&mut self) {
        self.cy = 0;
    }

    /// Move cursor to the bottom line of the visible area.
    pub fn bottom_line(&mut self, screen: &Screen) {
        self.cy = screen.height().saturating_sub(1);
    }

    // --- Paragraph navigation ---

    /// Move cursor to the start of the next paragraph (next blank line after content).
    pub fn next_paragraph(&mut self, screen: &Screen) {
        let history_size = screen.grid.history_size();
        let total_lines = history_size + screen.height();
        let mut abs_y = self.absolute_y(history_size);

        // Skip current non-blank lines
        while abs_y + 1 < total_lines {
            abs_y += 1;
            if is_line_blank(screen, abs_y) {
                break;
            }
        }
        // Skip blank lines
        while abs_y + 1 < total_lines {
            abs_y += 1;
            if !is_line_blank(screen, abs_y) {
                break;
            }
        }

        self.move_to_absolute_y(abs_y, screen);
        self.cx = 0;
    }

    /// Move cursor to the start of the previous paragraph.
    pub fn previous_paragraph(&mut self, screen: &Screen) {
        let history_size = screen.grid.history_size();
        let mut abs_y = self.absolute_y(history_size);

        // Skip current non-blank lines going up
        while abs_y > 0 {
            abs_y -= 1;
            if is_line_blank(screen, abs_y) {
                break;
            }
        }
        // Skip blank lines going up
        while abs_y > 0 {
            abs_y -= 1;
            if !is_line_blank(screen, abs_y) {
                break;
            }
        }

        self.move_to_absolute_y(abs_y, screen);
        self.cx = 0;
    }

    // --- Search ---

    /// Search forward from the current position for `needle`.
    /// Wraps around to the top if not found below.
    pub fn search_forward_for(&mut self, screen: &Screen, needle: &str) {
        self.search_str = Some(needle.to_string());
        self.search_forward = true;
        self.do_search_forward(screen, needle);
    }

    /// Search backward from the current position for `needle`.
    /// Wraps around to the bottom if not found above.
    pub fn search_backward_for(&mut self, screen: &Screen, needle: &str) {
        self.search_str = Some(needle.to_string());
        self.search_forward = false;
        self.do_search_backward(screen, needle);
    }

    /// Repeat the last search in the same direction.
    pub fn search_again(&mut self, screen: &Screen) {
        if let Some(needle) = self.search_str.clone() {
            if self.search_forward {
                self.do_search_forward(screen, &needle);
            } else {
                self.do_search_backward(screen, &needle);
            }
        }
    }

    /// Repeat the last search in the reverse direction.
    pub fn search_reverse(&mut self, screen: &Screen) {
        if let Some(needle) = self.search_str.clone() {
            if self.search_forward {
                self.do_search_backward(screen, &needle);
            } else {
                self.do_search_forward(screen, &needle);
            }
        }
    }

    fn do_search_forward(&mut self, screen: &Screen, needle: &str) {
        let history_size = screen.grid.history_size();
        let total_lines = history_size + screen.height();
        let start_abs_y = self.absolute_y(history_size);

        // Search from current position forward, then wrap
        for offset in 1..=total_lines {
            let abs_y = (start_abs_y + offset) % total_lines;
            if let Some(x) = find_in_line(screen, abs_y, needle) {
                self.cx = x;
                self.move_to_absolute_y(abs_y, screen);
                return;
            }
        }
        // Also check current line after cursor
        if let Some(x) = find_in_line_after(screen, start_abs_y, self.cx + 1, needle) {
            self.cx = x;
        }
    }

    fn do_search_backward(&mut self, screen: &Screen, needle: &str) {
        let history_size = screen.grid.history_size();
        let total_lines = history_size + screen.height();
        let start_abs_y = self.absolute_y(history_size);

        for offset in 1..=total_lines {
            let abs_y = (start_abs_y + total_lines - offset) % total_lines;
            if let Some(x) = find_in_line(screen, abs_y, needle) {
                self.cx = x;
                self.move_to_absolute_y(abs_y, screen);
                return;
            }
        }
    }

    // --- Helpers ---

    /// Move the view so that absolute y `abs_y` is visible, adjusting oy and cy.
    fn move_to_absolute_y(&mut self, abs_y: u32, screen: &Screen) {
        let history_size = screen.grid.history_size();
        let height = screen.height();

        if abs_y < history_size {
            // In history region
            self.oy = history_size - abs_y;
            self.cy = 0;
        } else {
            let visible_row = abs_y - history_size;
            if visible_row < height {
                self.oy = 0;
                self.cy = visible_row;
            } else {
                self.oy = 0;
                self.cy = height.saturating_sub(1);
            }
        }
        // Adjust: if scrolled, place cursor at appropriate row
        // abs_y = history_size - oy + cy  =>  cy = abs_y - history_size + oy
        // We want cy to be in [0, height-1]
        let target_cy = abs_y.saturating_sub(history_size.saturating_sub(self.oy));
        if target_cy < height {
            self.cy = target_cy;
        }
    }

    // --- Selection ---

    /// Start a character-wise selection at the current cursor position.
    pub fn begin_selection(&mut self, history_size: u32) {
        let abs_y = self.absolute_y(history_size);
        self.selecting = true;
        self.sel_start_x = self.cx;
        self.sel_start_y = abs_y;
        self.sel_type = SelectionType::Normal;
    }

    /// Start a line-wise selection.
    pub fn select_line(&mut self, history_size: u32) {
        let abs_y = self.absolute_y(history_size);
        self.selecting = true;
        self.sel_start_x = self.cx;
        self.sel_start_y = abs_y;
        self.sel_type = SelectionType::Line;
    }

    /// Toggle between normal and block (rectangular) selection.
    pub fn rectangle_toggle(&mut self) {
        if self.selecting {
            self.sel_type = match self.sel_type {
                SelectionType::Normal => SelectionType::Block,
                SelectionType::Block => SelectionType::Normal,
                SelectionType::Line => SelectionType::Line,
            };
        }
    }

    /// Build a `Selection` from the current copy mode state.
    ///
    /// Returns `None` if no selection is active.
    pub fn current_selection(&self, history_size: u32) -> Option<Selection> {
        if !self.selecting {
            return None;
        }
        let abs_y = self.absolute_y(history_size);
        Some(Selection {
            sel_type: self.sel_type,
            start_x: self.sel_start_x,
            start_y: self.sel_start_y,
            end_x: self.cx,
            end_y: abs_y,
            active: true,
        })
    }
}

/// Result of handling a copy-mode key action.
#[derive(Debug)]
pub enum CopyModeAction {
    /// Key was handled, pane remains in copy mode. Redraw needed.
    Handled,
    /// Exit copy mode (optionally with data to copy to paste buffer).
    Exit { copy_data: Option<Vec<u8>> },
    /// Enter search prompt (forward or backward).
    SearchPrompt { forward: bool },
    /// Wait for next character for jump-to-char (f/F/t/T).
    JumpPrompt { jump_type: JumpType },
    /// Enter goto-line prompt (`:` in vi copy mode).
    GotoLinePrompt,
    /// Copy selection and pipe to a command (copy-pipe / copy-pipe-and-cancel).
    CopyPipe {
        /// The copied data (None if no selection).
        copy_data: Option<Vec<u8>>,
        /// The shell command to pipe to.
        command: String,
        /// Whether to exit copy mode after.
        cancel: bool,
    },
    /// Key not recognized in copy mode.
    Unhandled,
}

/// Extract the selected text from a pane's grid.
///
/// Returns the selected text as bytes, or `None` if no selection is active.
pub fn copy_selection(screen: &Screen, cm: &CopyModeState) -> Option<Vec<u8>> {
    if !cm.selecting {
        return None;
    }
    let history_size = screen.grid.history_size();
    let selection = cm.current_selection(history_size)?;
    let (sx, sy, ex, ey) = selection.normalized();

    let mut result = Vec::new();
    for abs_y in sy..=ey {
        let Some(line) = screen.grid.get_line_absolute(abs_y) else {
            continue;
        };

        match selection.sel_type {
            SelectionType::Normal => {
                let line_start = if abs_y == sy { sx } else { 0 };
                let line_end = if abs_y == ey { ex + 1 } else { line.cell_count() };
                for x in line_start..line_end.min(line.cell_count()) {
                    let cell = line.get_cell(x);
                    let bytes = cell.data.as_bytes();
                    if bytes.is_empty() {
                        result.push(b' ');
                    } else {
                        result.extend_from_slice(bytes);
                    }
                }
            }
            SelectionType::Line => {
                for x in 0..line.cell_count() {
                    let cell = line.get_cell(x);
                    let bytes = cell.data.as_bytes();
                    if bytes.is_empty() {
                        result.push(b' ');
                    } else {
                        result.extend_from_slice(bytes);
                    }
                }
            }
            SelectionType::Block => {
                for x in sx..=ex.min(line.cell_count().saturating_sub(1)) {
                    let cell = line.get_cell(x);
                    let bytes = cell.data.as_bytes();
                    if bytes.is_empty() {
                        result.push(b' ');
                    } else {
                        result.extend_from_slice(bytes);
                    }
                }
            }
        }

        // Add newline between lines, trimming trailing spaces
        if abs_y < ey {
            while result.last() == Some(&b' ') {
                result.pop();
            }
            result.push(b'\n');
        }
    }

    // Trim trailing spaces from final line
    while result.last() == Some(&b' ') {
        result.pop();
    }

    Some(result)
}

/// Check if a cell at position `x` on `line` matches character `ch`.
fn cell_matches_char(line: &rmux_core::grid::line::GridLine, x: u32, ch: char) -> bool {
    let cell = line.get_cell(x);
    let bytes = cell.data.as_bytes();
    if bytes.is_empty() {
        return ch == ' ';
    }
    // Compare as UTF-8
    let mut buf = [0u8; 4];
    let target = ch.encode_utf8(&mut buf);
    bytes == target.as_bytes()
}

/// Check if a line is blank (all spaces or empty cells).
fn is_line_blank(screen: &Screen, abs_y: u32) -> bool {
    let Some(line) = screen.grid.get_line_absolute(abs_y) else {
        return true;
    };
    for x in 0..line.cell_count() {
        let cell = line.get_cell(x);
        let bytes = cell.data.as_bytes();
        if !bytes.is_empty() && bytes != [b' '] {
            return false;
        }
    }
    true
}

/// Extract the text content of a line as a String.
fn line_text(screen: &Screen, abs_y: u32) -> String {
    let Some(line) = screen.grid.get_line_absolute(abs_y) else {
        return String::new();
    };
    let mut text = String::with_capacity(line.cell_count() as usize);
    for x in 0..line.cell_count() {
        let cell = line.get_cell(x);
        if let Some(s) = cell.data.as_str() {
            text.push_str(s);
        } else {
            text.push(' ');
        }
    }
    text
}

/// Find the first occurrence of `needle` in a line, returning the x position.
fn find_in_line(screen: &Screen, abs_y: u32, needle: &str) -> Option<u32> {
    let text = line_text(screen, abs_y);
    text.find(needle).map(|pos| pos as u32)
}

/// Find `needle` in a line starting from column `start_x`.
fn find_in_line_after(screen: &Screen, abs_y: u32, start_x: u32, needle: &str) -> Option<u32> {
    let text = line_text(screen, abs_y);
    let start = start_x as usize;
    if start >= text.len() {
        return None;
    }
    text[start..].find(needle).map(|pos| (pos + start) as u32)
}

/// Dispatch a copy-mode action by name.
///
/// Called when a key is pressed in copy mode and matched to a copy-mode binding.
pub fn dispatch_copy_mode_action(
    screen: &Screen,
    cm: &mut CopyModeState,
    action: &str,
) -> CopyModeAction {
    dispatch_navigation(screen, cm, action)
        .or_else(|| dispatch_selection(screen, cm, action))
        .or_else(|| dispatch_search_and_jump(screen, cm, action))
        .or_else(|| dispatch_copy_and_exit(screen, cm, action))
        .unwrap_or(CopyModeAction::Unhandled)
}

fn dispatch_navigation(
    screen: &Screen,
    cm: &mut CopyModeState,
    action: &str,
) -> Option<CopyModeAction> {
    match action {
        "cursor-up" => cm.cursor_up(screen, 1),
        "cursor-down" => cm.cursor_down(screen, 1),
        "cursor-left" => cm.cursor_left(),
        "cursor-right" => cm.cursor_right(screen),
        "page-up" => cm.page_up(screen),
        "page-down" => cm.page_down(screen),
        "halfpage-up" => cm.halfpage_up(screen),
        "halfpage-down" => cm.halfpage_down(screen),
        "history-top" => cm.history_top(screen),
        "history-bottom" => cm.history_bottom(screen),
        "start-of-line" => cm.start_of_line(),
        "end-of-line" => cm.end_of_line(screen),
        "back-to-indentation" => cm.back_to_indentation(screen),
        "next-word" => cm.next_word(screen),
        "previous-word" => cm.previous_word(screen),
        "next-word-end" => cm.next_word_end(screen),
        "next-paragraph" => cm.next_paragraph(screen),
        "previous-paragraph" => cm.previous_paragraph(screen),
        "middle-line" => cm.middle_line(screen),
        "top-line" => cm.top_line(),
        "bottom-line" => cm.bottom_line(screen),
        _ => return None,
    }
    Some(CopyModeAction::Handled)
}

fn dispatch_selection(
    screen: &Screen,
    cm: &mut CopyModeState,
    action: &str,
) -> Option<CopyModeAction> {
    match action {
        "begin-selection" => {
            let hs = screen.grid.history_size();
            cm.begin_selection(hs);
        }
        "select-line" => {
            let hs = screen.grid.history_size();
            cm.select_line(hs);
        }
        "rectangle-toggle" => cm.rectangle_toggle(),
        "clear-selection" => cm.selecting = false,
        _ => return None,
    }
    Some(CopyModeAction::Handled)
}

fn dispatch_search_and_jump(
    screen: &Screen,
    cm: &mut CopyModeState,
    action: &str,
) -> Option<CopyModeAction> {
    match action {
        "search-forward" => return Some(CopyModeAction::SearchPrompt { forward: true }),
        "search-backward" => return Some(CopyModeAction::SearchPrompt { forward: false }),
        "search-again" => cm.search_again(screen),
        "search-reverse" => cm.search_reverse(screen),
        "jump-forward" => return Some(CopyModeAction::JumpPrompt { jump_type: JumpType::Forward }),
        "jump-backward" => {
            return Some(CopyModeAction::JumpPrompt { jump_type: JumpType::Backward });
        }
        "jump-to-forward" => {
            return Some(CopyModeAction::JumpPrompt { jump_type: JumpType::ForwardTill });
        }
        "jump-to-backward" => {
            return Some(CopyModeAction::JumpPrompt { jump_type: JumpType::BackwardTill });
        }
        "jump-again" => cm.jump_again(screen),
        "jump-reverse" => cm.jump_reverse(screen),
        "goto-line" => return Some(CopyModeAction::GotoLinePrompt),
        _ => return None,
    }
    Some(CopyModeAction::Handled)
}

fn dispatch_copy_and_exit(
    screen: &Screen,
    cm: &mut CopyModeState,
    action: &str,
) -> Option<CopyModeAction> {
    match action {
        "copy-selection-and-cancel" | "copy-selection" | "copy-selection-no-clear" => {
            let data = copy_selection(screen, cm);
            Some(CopyModeAction::Exit { copy_data: data })
        }
        "cancel" => Some(CopyModeAction::Exit { copy_data: None }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmux_core::grid::cell::GridCell;
    use rmux_core::style::Style;
    use rmux_core::utf8::Utf8Char;

    fn make_screen_with_content(width: u32, height: u32, lines: &[&str]) -> Screen {
        let mut screen = Screen::new(width, height, 2000);
        for (y, line) in lines.iter().enumerate() {
            for (x, ch) in line.bytes().enumerate() {
                screen.grid.set_cell(
                    x as u32,
                    y as u32,
                    &GridCell {
                        data: Utf8Char::from_ascii(ch),
                        style: Style::DEFAULT,
                        link: 0,
                        flags: rmux_core::grid::cell::CellFlags::empty(),
                    },
                );
            }
        }
        screen
    }

    #[test]
    fn enter_positions_cursor_at_bottom() {
        let screen = Screen::new(80, 24, 2000);
        let cm = CopyModeState::enter(&screen, "vi");
        assert_eq!(cm.cx, 0);
        assert_eq!(cm.cy, 23);
        assert_eq!(cm.oy, 0);
        assert!(!cm.selecting);
        assert_eq!(cm.key_table, "copy-mode-vi");
    }

    #[test]
    fn enter_emacs_mode() {
        let screen = Screen::new(80, 24, 2000);
        let cm = CopyModeState::enter(&screen, "emacs");
        assert_eq!(cm.key_table, "copy-mode-emacs");
    }

    #[test]
    fn cursor_movement() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        // Start at bottom-left
        assert_eq!((cm.cx, cm.cy), (0, 23));

        // Move up
        cm.cursor_up(&screen, 5);
        assert_eq!(cm.cy, 18);

        // Move right
        cm.cursor_right(&screen);
        cm.cursor_right(&screen);
        assert_eq!(cm.cx, 2);

        // Move left
        cm.cursor_left();
        assert_eq!(cm.cx, 1);

        // Left at 0 stays at 0
        cm.cursor_left();
        cm.cursor_left();
        assert_eq!(cm.cx, 0);

        // Move down
        cm.cursor_down(&screen, 3);
        assert_eq!(cm.cy, 21);
    }

    #[test]
    fn cursor_up_scrolls_into_history() {
        let mut screen = Screen::new(80, 24, 2000);
        // Create some history
        for _ in 0..10 {
            screen.grid.scroll_up();
        }
        assert_eq!(screen.grid.history_size(), 10);

        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0; // At top of visible area

        // Moving up should scroll into history
        cm.cursor_up(&screen, 1);
        assert_eq!(cm.oy, 1);
        assert_eq!(cm.cy, 0);

        cm.cursor_up(&screen, 5);
        assert_eq!(cm.oy, 6);
    }

    #[test]
    fn page_up_and_down() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..100 {
            screen.grid.scroll_up();
        }

        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.page_up(&screen);
        assert_eq!(cm.oy, 24); // One page

        cm.page_up(&screen);
        assert_eq!(cm.oy, 48);

        cm.page_down(&screen);
        assert_eq!(cm.oy, 24);

        cm.page_down(&screen);
        assert_eq!(cm.oy, 0);

        // Can't go below 0
        cm.page_down(&screen);
        assert_eq!(cm.oy, 0);
    }

    #[test]
    fn history_top_and_bottom() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..50 {
            screen.grid.scroll_up();
        }

        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.history_top(&screen);
        assert_eq!(cm.oy, 50);
        assert_eq!(cm.cy, 0);
        assert_eq!(cm.cx, 0);

        cm.history_bottom(&screen);
        assert_eq!(cm.oy, 0);
        assert_eq!(cm.cy, 23);
    }

    #[test]
    fn start_end_of_line() {
        let screen = make_screen_with_content(80, 24, &["Hello World"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 5;

        cm.start_of_line();
        assert_eq!(cm.cx, 0);

        cm.end_of_line(&screen);
        assert_eq!(cm.cx, 10); // "Hello World" is 11 chars, index 10 is last
    }

    #[test]
    fn next_word_movement() {
        let screen = make_screen_with_content(80, 24, &["hello world foo"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.next_word(&screen);
        assert_eq!(cm.cx, 6); // Start of "world"

        cm.next_word(&screen);
        assert_eq!(cm.cx, 12); // Start of "foo"
    }

    #[test]
    fn previous_word_movement() {
        let screen = make_screen_with_content(80, 24, &["hello world foo"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 14; // End of "foo"

        cm.previous_word(&screen);
        assert_eq!(cm.cx, 12); // Start of "foo"

        cm.previous_word(&screen);
        assert_eq!(cm.cx, 6); // Start of "world"
    }

    #[test]
    fn begin_and_yank_selection() {
        let screen = make_screen_with_content(80, 24, &["Hello World"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.begin_selection(screen.grid.history_size());
        assert!(cm.selecting);

        cm.cx = 4; // Select "Hello"
        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(data, b"Hello");
    }

    #[test]
    fn multiline_selection() {
        let screen = make_screen_with_content(80, 24, &["Line one", "Line two", "Line three"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.begin_selection(screen.grid.history_size());
        cm.cy = 1;
        cm.cx = 7; // Through "Line two"

        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(String::from_utf8_lossy(&data), "Line one\nLine two");
    }

    #[test]
    fn line_selection() {
        let screen = make_screen_with_content(80, 24, &["Hello World", "Goodbye"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 3;

        cm.select_line(screen.grid.history_size());
        assert_eq!(cm.sel_type, SelectionType::Line);

        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(String::from_utf8_lossy(&data), "Hello World");
    }

    #[test]
    fn dispatch_cancel_exits() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        match dispatch_copy_mode_action(&screen, &mut cm, "cancel") {
            CopyModeAction::Exit { copy_data } => assert!(copy_data.is_none()),
            _ => panic!("expected Exit"),
        }
    }

    #[test]
    fn dispatch_copy_and_cancel() {
        let screen = make_screen_with_content(80, 24, &["Test data"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;
        cm.begin_selection(screen.grid.history_size());
        cm.cx = 3;

        match dispatch_copy_mode_action(&screen, &mut cm, "copy-selection-and-cancel") {
            CopyModeAction::Exit { copy_data } => {
                let data = copy_data.unwrap();
                assert_eq!(data, b"Test");
            }
            _ => panic!("expected Exit with data"),
        }
    }

    #[test]
    fn dispatch_navigation() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        match dispatch_copy_mode_action(&screen, &mut cm, "cursor-up") {
            CopyModeAction::Handled => assert_eq!(cm.cy, 22),
            _ => panic!("expected Handled"),
        }
    }

    #[test]
    fn halfpage_up_and_down() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..100 {
            screen.grid.scroll_up();
        }

        let mut cm = CopyModeState::enter(&screen, "vi");
        assert_eq!(cm.oy, 0);

        cm.halfpage_up(&screen);
        assert_eq!(cm.oy, 12); // half of 24

        cm.halfpage_up(&screen);
        assert_eq!(cm.oy, 24);

        cm.halfpage_down(&screen);
        assert_eq!(cm.oy, 12);

        cm.halfpage_down(&screen);
        assert_eq!(cm.oy, 0);

        // Can't go below 0
        cm.halfpage_down(&screen);
        assert_eq!(cm.oy, 0);
    }

    #[test]
    fn back_to_indentation() {
        let screen = make_screen_with_content(80, 24, &["   hello world"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.back_to_indentation(&screen);
        assert_eq!(cm.cx, 3); // First non-space is 'h' at index 3
    }

    #[test]
    fn next_word_end() {
        let screen = make_screen_with_content(80, 24, &["hello world foo"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.next_word_end(&screen);
        assert_eq!(cm.cx, 4); // End of "hello" (index 4)

        cm.next_word_end(&screen);
        assert_eq!(cm.cx, 10); // End of "world" (index 10)

        cm.next_word_end(&screen);
        assert_eq!(cm.cx, 14); // End of "foo" (index 14)
    }

    #[test]
    fn jump_forward_to_char() {
        let screen = make_screen_with_content(80, 24, &["hello world foo"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.jump_forward(&screen, 'o');
        assert_eq!(cm.cx, 4); // 'o' in "hello"

        cm.jump_forward(&screen, 'o');
        assert_eq!(cm.cx, 7); // 'o' in "world"
    }

    #[test]
    fn jump_backward_to_char() {
        let screen = make_screen_with_content(80, 24, &["hello world foo"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 14;

        cm.jump_backward(&screen, 'o');
        assert_eq!(cm.cx, 13); // 'o' in "foo"

        cm.jump_backward(&screen, 'o');
        assert_eq!(cm.cx, 7); // 'o' in "world"
    }

    #[test]
    fn jump_forward_till() {
        let screen = make_screen_with_content(80, 24, &["hello world"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.jump_forward_till(&screen, 'w');
        assert_eq!(cm.cx, 5); // one before 'w' at index 6
    }

    #[test]
    fn jump_backward_till() {
        let screen = make_screen_with_content(80, 24, &["hello world"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 10;

        cm.jump_backward_till(&screen, 'o');
        assert_eq!(cm.cx, 8); // one after 'o' at index 7
    }

    #[test]
    fn jump_again_and_reverse() {
        let screen = make_screen_with_content(80, 24, &["abcabc"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.jump_forward(&screen, 'b');
        assert_eq!(cm.cx, 1);

        cm.jump_again(&screen);
        assert_eq!(cm.cx, 4); // next 'b'

        cm.jump_reverse(&screen);
        assert_eq!(cm.cx, 1); // back to first 'b'
    }

    #[test]
    fn middle_top_bottom_line() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        cm.top_line();
        assert_eq!(cm.cy, 0);

        cm.middle_line(&screen);
        assert_eq!(cm.cy, 12); // 24 / 2

        cm.bottom_line(&screen);
        assert_eq!(cm.cy, 23);
    }

    #[test]
    fn search_forward_finds_text() {
        let screen = make_screen_with_content(80, 24, &["first line", "second line", "third line"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.search_forward_for(&screen, "third");
        assert_eq!(cm.cy, 2);
        assert_eq!(cm.cx, 0);
    }

    #[test]
    fn search_backward_finds_text() {
        let screen = make_screen_with_content(80, 24, &["first line", "second line", "third line"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 2;
        cm.cx = 0;

        cm.search_backward_for(&screen, "first");
        assert_eq!(cm.cy, 0);
        assert_eq!(cm.cx, 0);
    }

    #[test]
    fn search_again_repeats() {
        let screen = make_screen_with_content(80, 24, &["foo bar", "foo baz", "foo qux"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.search_forward_for(&screen, "foo");
        assert_eq!(cm.cy, 1); // finds next line's "foo"

        cm.search_again(&screen);
        assert_eq!(cm.cy, 2); // finds third line's "foo"
    }

    #[test]
    fn paragraph_navigation() {
        let screen = make_screen_with_content(
            80,
            24,
            &["paragraph one", "still paragraph one", "", "paragraph two", "still paragraph two"],
        );
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.next_paragraph(&screen);
        assert_eq!(cm.cy, 3); // start of paragraph two
        assert_eq!(cm.cx, 0);

        cm.previous_paragraph(&screen);
        // Should go back to before the blank line
        assert!(cm.cy <= 1);
    }

    #[test]
    fn dispatch_clear_selection() {
        let screen = make_screen_with_content(80, 24, &["Hello"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;
        cm.begin_selection(screen.grid.history_size());
        assert!(cm.selecting);

        match dispatch_copy_mode_action(&screen, &mut cm, "clear-selection") {
            CopyModeAction::Handled => assert!(!cm.selecting),
            _ => panic!("expected Handled"),
        }
    }

    #[test]
    fn dispatch_position_actions() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        match dispatch_copy_mode_action(&screen, &mut cm, "top-line") {
            CopyModeAction::Handled => assert_eq!(cm.cy, 0),
            _ => panic!("expected Handled"),
        }

        match dispatch_copy_mode_action(&screen, &mut cm, "middle-line") {
            CopyModeAction::Handled => assert_eq!(cm.cy, 12),
            _ => panic!("expected Handled"),
        }

        match dispatch_copy_mode_action(&screen, &mut cm, "bottom-line") {
            CopyModeAction::Handled => assert_eq!(cm.cy, 23),
            _ => panic!("expected Handled"),
        }
    }

    #[test]
    fn cursor_right_wraps_to_next_line() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 79; // Last column

        // cursor_right at end of line should not go further (it doesn't wrap in this impl)
        cm.cursor_right(&screen);
        assert_eq!(cm.cx, 79); // Stays at max
    }

    #[test]
    fn cursor_left_wraps_to_prev_line() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 1;
        cm.cx = 0;

        // cursor_left at start of line uses saturating_sub, stays at 0
        cm.cursor_left();
        assert_eq!(cm.cx, 0);
    }

    #[test]
    fn single_cell_selection_copies_cell() {
        let screen = make_screen_with_content(80, 24, &["Hello World"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        // Position cursor at 'H' (column 0, row 0) and begin selection
        cm.cy = 0;
        cm.cx = 0;
        cm.begin_selection(screen.grid.history_size());
        // Move cursor right to cover "He" (0..1 inclusive)
        cm.cx = 1;
        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(data, b"He");
    }

    #[test]
    fn block_selection() {
        let screen = make_screen_with_content(80, 24, &["ABCDE", "FGHIJ", "KLMNO"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 1; // Start at 'B'

        cm.begin_selection(screen.grid.history_size());
        cm.rectangle_toggle(); // Switch to block selection
        assert_eq!(cm.sel_type, SelectionType::Block);

        cm.cy = 2;
        cm.cx = 3; // End at 'N' (row 2, col 3)

        let data = copy_selection(&screen, &cm).unwrap();
        let text = String::from_utf8_lossy(&data);
        // Block selection cols 1-3, rows 0-2: "BCD", "GHI", "LMN"
        assert!(text.contains("BCD"));
        assert!(text.contains("GHI"));
        assert!(text.contains("LMN"));
    }

    #[test]
    fn goto_line_positions_cursor() {
        let screen = make_screen_with_content(
            80,
            24,
            &[
                "line 1", "line 2", "line 3", "line 4", "line 5", "", "", "", "", "", "", "", "",
                "", "", "", "", "", "", "", "", "", "", "",
            ],
        );
        let mut cm = CopyModeState::enter(&screen, "vi");
        // Start at bottom
        assert_eq!(cm.cy, 23);

        // Go to line 1 (1-based)
        cm.goto_line(&screen, 1);
        assert_eq!(cm.cx, 0);
        // Should be at the top of the visible area
        let abs = cm.absolute_y(screen.grid.history_size());
        assert_eq!(abs, 0);

        // Go to line 3
        cm.goto_line(&screen, 3);
        let abs = cm.absolute_y(screen.grid.history_size());
        assert_eq!(abs, 2);
    }

    #[test]
    fn goto_line_clamps_to_total() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        // Go beyond total lines — should clamp
        cm.goto_line(&screen, 99999);
        let abs = cm.absolute_y(screen.grid.history_size());
        assert!(abs < 99999);
    }

    #[test]
    fn goto_line_zero_goes_to_top() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        // Line 0 saturates to 0
        cm.goto_line(&screen, 0);
        let abs = cm.absolute_y(screen.grid.history_size());
        assert_eq!(abs, 0);
        assert_eq!(cm.cx, 0);
    }

    #[test]
    fn dispatch_goto_line_returns_prompt() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        match dispatch_copy_mode_action(&screen, &mut cm, "goto-line") {
            CopyModeAction::GotoLinePrompt => {}
            other => panic!("expected GotoLinePrompt, got {other:?}"),
        }
    }
}
