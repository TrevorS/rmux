//! Scrollback history management.
//!
//! Uses `VecDeque<GridLine>` for O(1) push/pop operations, compared to tmux's
//! `memmove`-based approach which is O(n) when trimming history.

use super::line::GridLine;
use std::collections::VecDeque;

/// Manages the history and visible portions of grid lines.
///
/// The grid stores all lines (history + visible) in a single `VecDeque`.
/// History lines are at the front, visible lines at the back.
///
/// When the history exceeds `limit`, the oldest 10% is trimmed (matching
/// tmux's `grid_collect_history` behavior).
#[derive(Debug, Clone)]
pub struct GridHistory {
    /// All lines: history (front) + visible (back).
    lines: VecDeque<GridLine>,
    /// Number of history lines (lines before the visible area).
    history_size: u32,
    /// Number of visible lines.
    visible_size: u32,
    /// Maximum number of history lines before trimming.
    limit: u32,
    /// Number of history lines that have been scrolled off (for reporting).
    scrolled: u32,
}

impl GridHistory {
    /// Create a new history with the given visible size and history limit.
    #[must_use]
    pub fn new(visible: u32, limit: u32) -> Self {
        let mut lines = VecDeque::with_capacity((visible + limit.min(1000)) as usize);
        for _ in 0..visible {
            lines.push_back(GridLine::new());
        }
        Self {
            lines,
            history_size: 0,
            visible_size: visible,
            limit,
            scrolled: 0,
        }
    }

    /// Total number of lines (history + visible).
    #[must_use]
    pub fn total_lines(&self) -> u32 {
        self.history_size + self.visible_size
    }

    /// Number of history lines.
    #[must_use]
    pub fn history_size(&self) -> u32 {
        self.history_size
    }

    /// Number of visible lines.
    #[must_use]
    pub fn visible_size(&self) -> u32 {
        self.visible_size
    }

    /// History limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// Number of lines scrolled off the top (for reporting).
    #[must_use]
    pub fn scrolled(&self) -> u32 {
        self.scrolled
    }

    /// Get a line by absolute index (0 = oldest history line).
    #[must_use]
    pub fn get(&self, index: u32) -> Option<&GridLine> {
        self.lines.get(index as usize)
    }

    /// Get a mutable line by absolute index.
    #[must_use]
    pub fn get_mut(&mut self, index: u32) -> Option<&mut GridLine> {
        self.lines.get_mut(index as usize)
    }

    /// Get a visible line (0 = top of visible area).
    #[must_use]
    pub fn visible_line(&self, y: u32) -> Option<&GridLine> {
        self.lines.get((self.history_size + y) as usize)
    }

    /// Get a mutable visible line.
    #[must_use]
    pub fn visible_line_mut(&mut self, y: u32) -> Option<&mut GridLine> {
        let idx = (self.history_size + y) as usize;
        self.lines.get_mut(idx)
    }

    /// Scroll the visible area up by one line.
    ///
    /// The top visible line moves into history, and a new empty line
    /// is added at the bottom of the visible area.
    pub fn scroll_up(&mut self) {
        self.history_size += 1;
        self.lines.push_back(GridLine::new());
        self.collect_if_needed(false);
    }

    /// Scroll the visible area down by one line (reverse scroll).
    ///
    /// The bottom visible line is removed, and a new empty line is
    /// inserted at the top of the visible area.
    pub fn scroll_down(&mut self) {
        if self.lines.len() as u32 > self.history_size {
            // Remove last visible line
            self.lines.pop_back();
        }
        // Insert new line at the top of visible area
        let insert_pos = self.history_size as usize;
        self.lines.insert(insert_pos, GridLine::new());
    }

    /// Trim history if it exceeds the limit.
    ///
    /// Removes the oldest 10% of history lines (matching tmux's behavior).
    /// This is O(n) for the trim but amortized O(1) per scroll operation
    /// since it only triggers every `limit/10` scrolls.
    pub fn collect_if_needed(&mut self, collect_all: bool) {
        if self.history_size == 0 || self.history_size < self.limit {
            return;
        }

        let trim_count = if collect_all {
            self.history_size - self.limit
        } else {
            (self.limit / 10).max(1).min(self.history_size)
        };

        // O(1) per element removed from the front of VecDeque
        for _ in 0..trim_count {
            self.lines.pop_front();
        }
        self.history_size -= trim_count;
        self.scrolled += trim_count;
    }

    /// Resize the visible area.
    pub fn resize_visible(&mut self, new_height: u32) {
        if new_height > self.visible_size {
            // Growing: add new lines at the bottom
            let extra = new_height - self.visible_size;
            for _ in 0..extra {
                self.lines.push_back(GridLine::new());
            }
        } else if new_height < self.visible_size {
            // Shrinking: move excess visible lines into history
            let excess = self.visible_size - new_height;
            self.history_size += excess;
        }
        self.visible_size = new_height;
        self.collect_if_needed(true);
    }

    /// Clear all history.
    pub fn clear_history(&mut self) {
        for _ in 0..self.history_size {
            self.lines.pop_front();
        }
        self.scrolled += self.history_size;
        self.history_size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_history() {
        let h = GridHistory::new(24, 2000);
        assert_eq!(h.history_size(), 0);
        assert_eq!(h.visible_size(), 24);
        assert_eq!(h.total_lines(), 24);
    }

    #[test]
    fn scroll_up_adds_history() {
        let mut h = GridHistory::new(24, 2000);
        h.scroll_up();
        assert_eq!(h.history_size(), 1);
        assert_eq!(h.visible_size(), 24);
        assert_eq!(h.total_lines(), 25);
    }

    #[test]
    fn scroll_up_trims_at_limit() {
        let mut h = GridHistory::new(24, 100);
        for _ in 0..110 {
            h.scroll_up();
        }
        // Should have trimmed: 100 limit, trim 10%=10 lines
        assert!(h.history_size() <= 100);
    }

    #[test]
    fn visible_line_access() {
        let h = GridHistory::new(24, 2000);
        assert!(h.visible_line(0).is_some());
        assert!(h.visible_line(23).is_some());
        assert!(h.visible_line(24).is_none());
    }

    #[test]
    fn resize_grow() {
        let mut h = GridHistory::new(24, 2000);
        h.resize_visible(30);
        assert_eq!(h.visible_size(), 30);
        assert_eq!(h.total_lines(), 30);
    }

    #[test]
    fn resize_shrink() {
        let mut h = GridHistory::new(24, 2000);
        h.resize_visible(20);
        assert_eq!(h.visible_size(), 20);
        // 4 lines moved to history
        assert_eq!(h.history_size(), 4);
    }

    #[test]
    fn clear_history() {
        let mut h = GridHistory::new(24, 2000);
        for _ in 0..50 {
            h.scroll_up();
        }
        assert_eq!(h.history_size(), 50);
        h.clear_history();
        assert_eq!(h.history_size(), 0);
        assert_eq!(h.visible_size(), 24);
    }

    #[test]
    fn scroll_down() {
        let mut h = GridHistory::new(24, 2000);
        h.scroll_down();
        assert_eq!(h.history_size(), 0);
        assert_eq!(h.total_lines(), 24); // VecDeque might have same total
    }
}
