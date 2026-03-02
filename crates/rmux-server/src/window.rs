//! Window management.

use crate::pane::Pane;
use rmux_core::layout::LayoutCell;
use rmux_core::options::{Options, default_window_options};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_WINDOW_ID: AtomicU32 = AtomicU32::new(0);

/// A tmux window (contains one or more panes).
#[derive(Debug)]
pub struct Window {
    /// Unique window ID.
    pub id: u32,
    /// Window name.
    pub name: String,
    /// Panes in this window, keyed by pane ID.
    pub panes: HashMap<u32, Pane>,
    /// Active pane ID.
    pub active_pane: u32,
    /// Layout tree.
    pub layout: Option<LayoutCell>,
    /// Window width.
    pub sx: u32,
    /// Window height.
    pub sy: u32,
    /// Window options.
    pub options: Options,
}

impl Window {
    /// Create a new window.
    #[must_use]
    pub fn new(name: String, sx: u32, sy: u32) -> Self {
        Self {
            id: NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed),
            name,
            panes: HashMap::new(),
            active_pane: 0,
            layout: None,
            sx,
            sy,
            options: default_window_options(),
        }
    }

    /// Get the active pane.
    #[must_use]
    pub fn active_pane(&self) -> Option<&Pane> {
        self.panes.get(&self.active_pane)
    }

    /// Get the active pane mutably.
    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(&self.active_pane)
    }

    /// Number of panes.
    #[must_use]
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_window() {
        let w = Window::new("0".into(), 80, 24);
        assert_eq!(w.name, "0");
        assert_eq!(w.pane_count(), 0);
    }
}
