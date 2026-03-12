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
    /// Last active pane ID (for last-pane command).
    pub last_active_pane: Option<u32>,
    /// Layout tree.
    pub layout: Option<LayoutCell>,
    /// Window width.
    pub sx: u32,
    /// Window height.
    pub sy: u32,
    /// Window options.
    pub options: Options,
    /// Bell alert flag (set when BEL received in a non-active window).
    pub has_bell: bool,
    /// Activity alert flag (set when output received in a non-active window with monitor-activity).
    pub has_activity: bool,
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
            last_active_pane: None,
            layout: None,
            sx,
            sy,
            options: default_window_options(),
            has_bell: false,
            has_activity: false,
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

    #[test]
    fn active_pane_empty() {
        let w = Window::new("empty".into(), 80, 24);
        assert!(w.active_pane().is_none());
    }

    #[test]
    fn active_pane_found() {
        use crate::pane::Pane;
        let mut w = Window::new("found".into(), 80, 24);
        let p = Pane::new(80, 24, 0);
        let pid = p.id;
        w.panes.insert(pid, p);
        w.active_pane = pid;
        assert!(w.active_pane().is_some());
        assert_eq!(w.active_pane().unwrap().id, pid);
    }

    #[test]
    fn active_pane_mut_found() {
        use crate::pane::Pane;
        let mut w = Window::new("mutfound".into(), 80, 24);
        let p = Pane::new(80, 24, 0);
        let pid = p.id;
        w.panes.insert(pid, p);
        w.active_pane = pid;
        let pane = w.active_pane_mut().unwrap();
        assert_eq!(pane.id, pid);
        pane.sx = 100;
        assert_eq!(w.panes.get(&pid).unwrap().sx, 100);
    }

    #[test]
    fn pane_count_multiple() {
        use crate::pane::Pane;
        let mut w = Window::new("multi".into(), 80, 24);
        let p1 = Pane::new(80, 24, 0);
        let p2 = Pane::new(80, 24, 0);
        let p3 = Pane::new(80, 24, 0);
        w.panes.insert(p1.id, p1);
        w.panes.insert(p2.id, p2);
        w.panes.insert(p3.id, p3);
        assert_eq!(w.pane_count(), 3);
    }
}
