//! Copy mode state machine and navigation.
//!
//! When a pane enters copy mode, the user can scroll through history,
//! move a cursor independently, select text, and copy it to a paste buffer.

use rmux_core::screen::selection::{Selection, SelectionType};
use rmux_core::screen::Screen;

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

/// Dispatch a copy-mode action by name.
///
/// Called when a key is pressed in copy mode and matched to a copy-mode binding.
pub fn dispatch_copy_mode_action(
    screen: &Screen,
    cm: &mut CopyModeState,
    action: &str,
) -> CopyModeAction {
    match action {
        // Navigation
        "cursor-up" => { cm.cursor_up(screen, 1); CopyModeAction::Handled }
        "cursor-down" => { cm.cursor_down(screen, 1); CopyModeAction::Handled }
        "cursor-left" => { cm.cursor_left(); CopyModeAction::Handled }
        "cursor-right" => { cm.cursor_right(screen); CopyModeAction::Handled }
        "page-up" => { cm.page_up(screen); CopyModeAction::Handled }
        "page-down" => { cm.page_down(screen); CopyModeAction::Handled }
        "halfpage-up" => { cm.halfpage_up(screen); CopyModeAction::Handled }
        "halfpage-down" => { cm.halfpage_down(screen); CopyModeAction::Handled }
        "history-top" => { cm.history_top(screen); CopyModeAction::Handled }
        "history-bottom" => { cm.history_bottom(screen); CopyModeAction::Handled }
        "start-of-line" => { cm.start_of_line(); CopyModeAction::Handled }
        "end-of-line" => { cm.end_of_line(screen); CopyModeAction::Handled }
        "back-to-indentation" => { cm.back_to_indentation(screen); CopyModeAction::Handled }
        "next-word" => { cm.next_word(screen); CopyModeAction::Handled }
        "previous-word" => { cm.previous_word(screen); CopyModeAction::Handled }
        "next-word-end" => { cm.next_word_end(screen); CopyModeAction::Handled }

        // Selection
        "begin-selection" => {
            let hs = screen.grid.history_size();
            cm.begin_selection(hs);
            CopyModeAction::Handled
        }
        "select-line" => {
            let hs = screen.grid.history_size();
            cm.select_line(hs);
            CopyModeAction::Handled
        }
        "rectangle-toggle" => {
            cm.rectangle_toggle();
            CopyModeAction::Handled
        }

        // Copy and exit
        "copy-selection-and-cancel" => {
            let data = copy_selection(screen, cm);
            CopyModeAction::Exit { copy_data: data }
        }

        // Cancel (exit without copying)
        "cancel" => CopyModeAction::Exit { copy_data: None },

        _ => CopyModeAction::Unhandled,
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
}
