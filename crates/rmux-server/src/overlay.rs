//! Interactive overlay system for choose-tree, choose-buffer, choose-client,
//! display-menu, and display-popup.
//!
//! Overlays are client-level state that take over the client's input and render
//! on top of pane content. This mirrors tmux's overlay behavior.

/// A single item in a list overlay (choose-tree, choose-buffer, choose-client).
#[derive(Debug, Clone)]
pub struct ListItem {
    /// Display text (already format-expanded).
    pub display: String,
    /// Command to execute on selection.
    pub command: Vec<String>,
    /// Indentation level (for tree views).
    pub indent: u32,
    /// Whether this tree node is collapsed (has hidden children).
    pub collapsed: bool,
    /// Number of hidden children when collapsed.
    pub hidden_children: usize,
    /// Whether this item supports the 'd' (delete/detach) action.
    pub deletable: bool,
    /// Delete command (e.g., detach-client, delete-buffer).
    pub delete_command: Vec<String>,
}

/// State for list-style overlays (choose-tree, choose-buffer, choose-client).
pub struct ListOverlay {
    /// All visible items.
    pub items: Vec<ListItem>,
    /// Currently selected index.
    pub selected: usize,
    /// Scroll offset (index of first visible item).
    pub scroll_offset: usize,
    /// Search/filter string.
    pub filter: String,
    /// Whether the filter input is active.
    pub filtering: bool,
    /// Title displayed at the top of the overlay.
    pub title: String,
    /// The kind of list, for rebuild-on-delete behavior.
    pub kind: ListKind,
}

/// What kind of list overlay this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Tree,
    Buffer,
    Client,
}

/// A menu item for display-menu.
#[derive(Debug, Clone)]
pub struct MenuItem {
    /// Display name (empty = separator line).
    pub name: String,
    /// Key shortcut (if any).
    pub key: Option<char>,
    /// Command to run on selection.
    pub command: Vec<String>,
}

/// State for display-menu.
pub struct MenuOverlay {
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub title: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
}

/// The overlay currently active on a client.
pub enum OverlayState {
    /// choose-tree, choose-buffer, choose-client.
    List(ListOverlay),
    /// display-menu.
    Menu(MenuOverlay),
}

/// Action returned from overlay input processing.
#[derive(Debug)]
pub enum OverlayAction {
    /// Input consumed, overlay state updated, needs redraw.
    Handled,
    /// Overlay dismissed with no selection.
    Cancel,
    /// Item selected — execute the associated command.
    Select { command: Vec<String> },
    /// Delete action on current item.
    Delete { command: Vec<String> },
    /// Tree node toggled — server should rebuild the tree overlay.
    RebuildTree,
    /// Input not consumed by overlay.
    Unhandled,
}

impl ListOverlay {
    /// Ensure `selected` is within bounds and adjust scroll.
    pub fn clamp(&mut self, visible_height: usize) {
        if self.items.is_empty() {
            self.selected = 0;
            self.scroll_offset = 0;
            return;
        }
        if self.selected >= self.items.len() {
            self.selected = self.items.len() - 1;
        }
        // Keep selected within the visible viewport
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
        if visible_height > 0 && self.selected >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected - visible_height + 1;
        }
    }

    /// The number of visible rows for the list area (reserves 1 row for title/filter).
    pub fn visible_height(&self, terminal_height: u32) -> usize {
        // Reserve 1 row for status bar, 1 row for title/filter
        terminal_height.saturating_sub(2) as usize
    }
}

/// Process input for a list overlay. Returns `(action, bytes_consumed)`.
pub fn process_list_input(state: &mut ListOverlay, data: &[u8]) -> (OverlayAction, usize) {
    if data.is_empty() {
        return (OverlayAction::Handled, 0);
    }

    // If filter mode is active, handle filter input
    if state.filtering {
        return process_filter_input(state, data);
    }

    match data[0] {
        // Enter — select current item
        0x0D | 0x0A => {
            if let Some(item) = state.items.get(state.selected) {
                let cmd = item.command.clone();
                (OverlayAction::Select { command: cmd }, 1)
            } else {
                (OverlayAction::Cancel, 1)
            }
        }
        // Arrow keys (CSI sequences) — must be checked before bare escape
        0x1B if data.len() >= 3 && data[1] == b'[' => match data[2] {
            b'A' => {
                state.selected = state.selected.saturating_sub(1);
                (OverlayAction::Handled, 3)
            }
            b'B' => {
                if state.selected + 1 < state.items.len() {
                    state.selected += 1;
                }
                (OverlayAction::Handled, 3)
            }
            // Right arrow — expand collapsed tree node
            b'C' => {
                if toggle_tree_node(state, true) {
                    (OverlayAction::RebuildTree, 3)
                } else {
                    (OverlayAction::Handled, 3)
                }
            }
            // Left arrow — collapse expanded tree node
            b'D' => {
                toggle_tree_node(state, false);
                (OverlayAction::Handled, 3)
            }
            _ => (OverlayAction::Handled, 3),
        },
        // Bare escape / q — cancel
        0x1B | b'q' => (OverlayAction::Cancel, 1),
        // j / Ctrl-N — move down
        b'j' | 0x0E => {
            if state.selected + 1 < state.items.len() {
                state.selected += 1;
            }
            (OverlayAction::Handled, 1)
        }
        // k / Ctrl-P — move up
        b'k' | 0x10 => {
            state.selected = state.selected.saturating_sub(1);
            (OverlayAction::Handled, 1)
        }
        // g — go to top
        b'g' => {
            state.selected = 0;
            (OverlayAction::Handled, 1)
        }
        // G — go to bottom
        b'G' => {
            if !state.items.is_empty() {
                state.selected = state.items.len() - 1;
            }
            (OverlayAction::Handled, 1)
        }
        // / — enter filter mode
        b'/' => {
            state.filtering = true;
            state.filter.clear();
            (OverlayAction::Handled, 1)
        }
        // d — delete/detach action
        b'd' => {
            if let Some(item) = state.items.get(state.selected) {
                if item.deletable {
                    let cmd = item.delete_command.clone();
                    return (OverlayAction::Delete { command: cmd }, 1);
                }
            }
            (OverlayAction::Handled, 1)
        }
        _ => (OverlayAction::Handled, 1),
    }
}

/// Toggle expand/collapse on a tree node. Returns true if a rebuild is needed.
///
/// Collapse removes child items from the vec directly.
/// Expand marks the node as expanded and returns true so the server can rebuild.
fn toggle_tree_node(state: &mut ListOverlay, expand: bool) -> bool {
    if state.kind != ListKind::Tree {
        return false;
    }
    let idx = state.selected;
    if idx >= state.items.len() {
        return false;
    }

    if expand {
        // Only expand indent=0 items that are collapsed
        if state.items[idx].indent != 0 || !state.items[idx].collapsed {
            return false;
        }
        state.items[idx].collapsed = false;
        true // server must rebuild to insert children
    } else {
        // Collapse: remove children (indent > 0 items following this indent=0 item)
        if state.items[idx].indent != 0 || state.items[idx].collapsed {
            return false;
        }
        let mut children = 0;
        while idx + 1 + children < state.items.len()
            && state.items[idx + 1 + children].indent > state.items[idx].indent
        {
            children += 1;
        }
        if children > 0 {
            state.items.drain((idx + 1)..(idx + 1 + children));
            state.items[idx].collapsed = true;
            state.items[idx].hidden_children = children;
        }
        false
    }
}

/// Process input while in filter mode.
fn process_filter_input(state: &mut ListOverlay, data: &[u8]) -> (OverlayAction, usize) {
    if data.is_empty() {
        return (OverlayAction::Handled, 0);
    }
    match data[0] {
        // Enter — confirm filter, return to navigation
        0x0D | 0x0A => {
            state.filtering = false;
            state.selected = 0;
            state.scroll_offset = 0;
            (OverlayAction::Handled, 1)
        }
        // Escape — cancel filter
        0x1B => {
            state.filtering = false;
            state.filter.clear();
            state.selected = 0;
            state.scroll_offset = 0;
            (OverlayAction::Handled, 1)
        }
        // Backspace
        0x7F | 0x08 => {
            state.filter.pop();
            state.selected = 0;
            state.scroll_offset = 0;
            (OverlayAction::Handled, 1)
        }
        // Ctrl-U — clear filter
        0x15 => {
            state.filter.clear();
            state.selected = 0;
            state.scroll_offset = 0;
            (OverlayAction::Handled, 1)
        }
        // Printable ASCII
        0x20..=0x7E => {
            state.filter.push(data[0] as char);
            state.selected = 0;
            state.scroll_offset = 0;
            (OverlayAction::Handled, 1)
        }
        _ => (OverlayAction::Handled, 1),
    }
}

/// Process input for a menu overlay. Returns `(action, bytes_consumed)`.
pub fn process_menu_input(state: &mut MenuOverlay, data: &[u8]) -> (OverlayAction, usize) {
    if data.is_empty() {
        return (OverlayAction::Handled, 0);
    }

    // Check for key shortcut match first
    if data[0] >= 0x20 && data[0] <= 0x7E {
        let ch = data[0] as char;
        for item in &state.items {
            if item.key == Some(ch) && !item.name.is_empty() {
                return (OverlayAction::Select { command: item.command.clone() }, 1);
            }
        }
    }

    match data[0] {
        // Enter — select current item
        0x0D | 0x0A => {
            if let Some(item) = state.items.get(state.selected) {
                if !item.name.is_empty() {
                    let cmd = item.command.clone();
                    return (OverlayAction::Select { command: cmd }, 1);
                }
            }
            (OverlayAction::Handled, 1)
        }
        // Arrow keys — must be checked before bare escape
        0x1B if data.len() >= 3 && data[1] == b'[' => match data[2] {
            b'A' => {
                move_menu_up(state);
                (OverlayAction::Handled, 3)
            }
            b'B' => {
                move_menu_down(state);
                (OverlayAction::Handled, 3)
            }
            _ => (OverlayAction::Handled, 3),
        },
        // Bare escape / q — cancel
        0x1B | b'q' => (OverlayAction::Cancel, 1),
        // j — down
        b'j' => {
            move_menu_down(state);
            (OverlayAction::Handled, 1)
        }
        // k — up
        b'k' => {
            move_menu_up(state);
            (OverlayAction::Handled, 1)
        }
        _ => (OverlayAction::Handled, 1),
    }
}

/// Move menu selection down, skipping separators.
fn move_menu_down(state: &mut MenuOverlay) {
    let mut next = state.selected + 1;
    while next < state.items.len() {
        if !state.items[next].name.is_empty() {
            state.selected = next;
            return;
        }
        next += 1;
    }
}

/// Move menu selection up, skipping separators.
fn move_menu_up(state: &mut MenuOverlay) {
    let mut prev = state.selected;
    while prev > 0 {
        prev -= 1;
        if !state.items[prev].name.is_empty() {
            state.selected = prev;
            return;
        }
    }
}

/// Get the filtered items for a list overlay.
pub fn filtered_items(state: &ListOverlay) -> Vec<(usize, &ListItem)> {
    if state.filter.is_empty() {
        state.items.iter().enumerate().collect()
    } else {
        let filter_lower = state.filter.to_lowercase();
        state
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.display.to_lowercase().contains(&filter_lower))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_list_items() -> Vec<ListItem> {
        vec![
            ListItem {
                display: "session-0: 2 windows".into(),
                command: vec!["switch-client".into(), "-t".into(), "session-0".into()],
                indent: 0,
                collapsed: false,
                hidden_children: 0,
                deletable: false,
                delete_command: vec![],
            },
            ListItem {
                display: "session-1: 1 windows".into(),
                command: vec!["switch-client".into(), "-t".into(), "session-1".into()],
                indent: 0,
                collapsed: false,
                hidden_children: 0,
                deletable: false,
                delete_command: vec![],
            },
            ListItem {
                display: "session-2: 3 windows".into(),
                command: vec!["switch-client".into(), "-t".into(), "session-2".into()],
                indent: 0,
                collapsed: false,
                hidden_children: 0,
                deletable: false,
                delete_command: vec![],
            },
        ]
    }

    fn test_list_overlay() -> ListOverlay {
        ListOverlay {
            items: test_list_items(),
            selected: 0,
            scroll_offset: 0,
            filter: String::new(),
            filtering: false,
            title: "choose-tree".into(),
            kind: ListKind::Tree,
        }
    }

    #[test]
    fn list_navigate_down() {
        let mut state = test_list_overlay();
        let (action, consumed) = process_list_input(&mut state, b"j");
        assert!(matches!(action, OverlayAction::Handled));
        assert_eq!(consumed, 1);
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn list_navigate_up() {
        let mut state = test_list_overlay();
        state.selected = 2;
        let (action, _) = process_list_input(&mut state, b"k");
        assert!(matches!(action, OverlayAction::Handled));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn list_navigate_up_at_top_stays() {
        let mut state = test_list_overlay();
        let (_, _) = process_list_input(&mut state, b"k");
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn list_navigate_down_at_bottom_stays() {
        let mut state = test_list_overlay();
        state.selected = 2;
        let (_, _) = process_list_input(&mut state, b"j");
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn list_select_returns_command() {
        let mut state = test_list_overlay();
        state.selected = 1;
        let (action, _) = process_list_input(&mut state, b"\r");
        match action {
            OverlayAction::Select { command } => {
                assert_eq!(command, vec!["switch-client", "-t", "session-1"]);
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn list_cancel_on_escape() {
        let mut state = test_list_overlay();
        let (action, _) = process_list_input(&mut state, b"\x1b");
        assert!(matches!(action, OverlayAction::Cancel));
    }

    #[test]
    fn list_cancel_on_q() {
        let mut state = test_list_overlay();
        let (action, _) = process_list_input(&mut state, b"q");
        assert!(matches!(action, OverlayAction::Cancel));
    }

    #[test]
    fn list_go_to_top() {
        let mut state = test_list_overlay();
        state.selected = 2;
        let (_, _) = process_list_input(&mut state, b"g");
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn list_go_to_bottom() {
        let mut state = test_list_overlay();
        let (_, _) = process_list_input(&mut state, b"G");
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn list_arrow_keys() {
        let mut state = test_list_overlay();
        // Down arrow
        let (_, consumed) = process_list_input(&mut state, b"\x1b[B");
        assert_eq!(consumed, 3);
        assert_eq!(state.selected, 1);
        // Up arrow
        let (_, consumed) = process_list_input(&mut state, b"\x1b[A");
        assert_eq!(consumed, 3);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn list_emacs_nav() {
        let mut state = test_list_overlay();
        // Ctrl-N (down)
        let (_, _) = process_list_input(&mut state, b"\x0e");
        assert_eq!(state.selected, 1);
        // Ctrl-P (up)
        let (_, _) = process_list_input(&mut state, b"\x10");
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn filter_mode_enter_and_exit() {
        let mut state = test_list_overlay();
        // Enter filter mode
        let (_, _) = process_list_input(&mut state, b"/");
        assert!(state.filtering);
        // Type "session-1"
        for ch in b"session-1" {
            let (_, _) = process_list_input(&mut state, std::slice::from_ref(ch));
        }
        assert_eq!(state.filter, "session-1");
        // Confirm filter
        let (_, _) = process_list_input(&mut state, b"\r");
        assert!(!state.filtering);
        assert_eq!(state.filter, "session-1");
    }

    #[test]
    fn filter_mode_escape_clears() {
        let mut state = test_list_overlay();
        let (_, _) = process_list_input(&mut state, b"/");
        for ch in b"test" {
            let (_, _) = process_list_input(&mut state, std::slice::from_ref(ch));
        }
        let (_, _) = process_list_input(&mut state, b"\x1b");
        assert!(!state.filtering);
        assert!(state.filter.is_empty());
    }

    #[test]
    fn filter_backspace() {
        let mut state = test_list_overlay();
        let (_, _) = process_list_input(&mut state, b"/");
        for ch in b"abc" {
            let (_, _) = process_list_input(&mut state, std::slice::from_ref(ch));
        }
        let (_, _) = process_list_input(&mut state, b"\x7f");
        assert_eq!(state.filter, "ab");
    }

    #[test]
    fn filter_ctrl_u_clears() {
        let mut state = test_list_overlay();
        let (_, _) = process_list_input(&mut state, b"/");
        for ch in b"abc" {
            let (_, _) = process_list_input(&mut state, std::slice::from_ref(ch));
        }
        let (_, _) = process_list_input(&mut state, b"\x15");
        assert!(state.filter.is_empty());
    }

    #[test]
    fn filtered_items_filters_by_display() {
        let state = test_list_overlay();
        let items = filtered_items(&state);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn filtered_items_with_filter() {
        let mut state = test_list_overlay();
        state.filter = "session-1".into();
        let items = filtered_items(&state);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1.display, "session-1: 1 windows");
    }

    #[test]
    fn filtered_items_case_insensitive() {
        let mut state = test_list_overlay();
        state.filter = "SESSION-2".into();
        let items = filtered_items(&state);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn delete_action_on_deletable_item() {
        let mut state = test_list_overlay();
        state.items[0].deletable = true;
        state.items[0].delete_command = vec!["kill-session".into(), "session-0".into()];

        let (action, _) = process_list_input(&mut state, b"d");
        match action {
            OverlayAction::Delete { command } => {
                assert_eq!(command, vec!["kill-session", "session-0"]);
            }
            other => panic!("expected Delete, got {other:?}"),
        }
    }

    #[test]
    fn delete_noop_on_non_deletable_item() {
        let mut state = test_list_overlay();
        let (action, _) = process_list_input(&mut state, b"d");
        assert!(matches!(action, OverlayAction::Handled));
    }

    #[test]
    fn clamp_keeps_selected_in_bounds() {
        let mut state = test_list_overlay();
        state.selected = 100;
        state.clamp(10);
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn clamp_adjusts_scroll_offset() {
        let mut state = test_list_overlay();
        state.selected = 2;
        state.scroll_offset = 0;
        state.clamp(2); // only 2 rows visible
        assert_eq!(state.scroll_offset, 1); // scroll so selected is visible
    }

    #[test]
    fn clamp_empty_items() {
        let mut state = test_list_overlay();
        state.items.clear();
        state.clamp(10);
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    // ============================================================
    // Menu overlay tests
    // ============================================================

    fn test_menu_items() -> Vec<MenuItem> {
        vec![
            MenuItem {
                name: "New Window".into(),
                key: Some('c'),
                command: vec!["new-window".into()],
            },
            MenuItem { name: String::new(), key: None, command: vec![] }, // separator
            MenuItem {
                name: "Kill Window".into(),
                key: Some('&'),
                command: vec!["kill-window".into()],
            },
        ]
    }

    fn test_menu_overlay() -> MenuOverlay {
        MenuOverlay {
            items: test_menu_items(),
            selected: 0,
            title: "Window".into(),
            x: 5,
            y: 5,
            width: 20,
        }
    }

    #[test]
    fn menu_select_by_key() {
        let mut state = test_menu_overlay();
        let (action, _) = process_menu_input(&mut state, b"c");
        match action {
            OverlayAction::Select { command } => {
                assert_eq!(command, vec!["new-window"]);
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn menu_select_by_enter() {
        let mut state = test_menu_overlay();
        let (action, _) = process_menu_input(&mut state, b"\r");
        match action {
            OverlayAction::Select { command } => {
                assert_eq!(command, vec!["new-window"]);
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn menu_navigate_skips_separator() {
        let mut state = test_menu_overlay();
        // Move down — should skip separator (index 1) and land on index 2
        let (_, _) = process_menu_input(&mut state, b"j");
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn menu_cancel() {
        let mut state = test_menu_overlay();
        let (action, _) = process_menu_input(&mut state, b"\x1b");
        assert!(matches!(action, OverlayAction::Cancel));
    }

    #[test]
    fn menu_arrow_keys() {
        let mut state = test_menu_overlay();
        // Down
        let (_, consumed) = process_menu_input(&mut state, b"\x1b[B");
        assert_eq!(consumed, 3);
        assert_eq!(state.selected, 2);
        // Up
        let (_, _) = process_menu_input(&mut state, b"\x1b[A");
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn list_visible_height() {
        let state = test_list_overlay();
        assert_eq!(state.visible_height(24), 22);
        assert_eq!(state.visible_height(2), 0);
        assert_eq!(state.visible_height(1), 0);
    }

    #[test]
    fn select_on_empty_list_cancels() {
        let mut state = test_list_overlay();
        state.items.clear();
        let (action, _) = process_list_input(&mut state, b"\r");
        assert!(matches!(action, OverlayAction::Cancel));
    }

    // ============================================================
    // Tree expand/collapse tests
    // ============================================================

    fn test_tree_overlay() -> ListOverlay {
        ListOverlay {
            items: vec![
                ListItem {
                    display: "sess-0: 2 windows".into(),
                    command: vec!["switch-client".into(), "-t".into(), "sess-0".into()],
                    indent: 0,
                    collapsed: false,
                    hidden_children: 0,
                    deletable: true,
                    delete_command: vec!["kill-session".into(), "-t".into(), "sess-0".into()],
                },
                ListItem {
                    display: "0: bash*".into(),
                    command: vec!["select-window".into(), "-t".into(), "sess-0:0".into()],
                    indent: 1,
                    collapsed: false,
                    hidden_children: 0,
                    deletable: true,
                    delete_command: vec!["kill-window".into(), "-t".into(), "sess-0:0".into()],
                },
                ListItem {
                    display: "1: vim".into(),
                    command: vec!["select-window".into(), "-t".into(), "sess-0:1".into()],
                    indent: 1,
                    collapsed: false,
                    hidden_children: 0,
                    deletable: true,
                    delete_command: vec!["kill-window".into(), "-t".into(), "sess-0:1".into()],
                },
                ListItem {
                    display: "sess-1: 1 windows".into(),
                    command: vec!["switch-client".into(), "-t".into(), "sess-1".into()],
                    indent: 0,
                    collapsed: false,
                    hidden_children: 0,
                    deletable: true,
                    delete_command: vec!["kill-session".into(), "-t".into(), "sess-1".into()],
                },
                ListItem {
                    display: "0: zsh*".into(),
                    command: vec!["select-window".into(), "-t".into(), "sess-1:0".into()],
                    indent: 1,
                    collapsed: false,
                    hidden_children: 0,
                    deletable: true,
                    delete_command: vec!["kill-window".into(), "-t".into(), "sess-1:0".into()],
                },
            ],
            selected: 0,
            scroll_offset: 0,
            filter: String::new(),
            filtering: false,
            title: "choose-tree".into(),
            kind: ListKind::Tree,
        }
    }

    #[test]
    fn tree_collapse_removes_children() {
        let mut state = test_tree_overlay();
        assert_eq!(state.items.len(), 5);

        // Left arrow collapses sess-0 (selected=0)
        let (action, consumed) = process_list_input(&mut state, b"\x1b[D");
        assert!(matches!(action, OverlayAction::Handled));
        assert_eq!(consumed, 3);
        assert!(state.items[0].collapsed);
        assert_eq!(state.items[0].hidden_children, 2);
        // Children removed: sess-0's 2 windows gone
        assert_eq!(state.items.len(), 3);
        // Next item is sess-1
        assert!(state.items[1].display.contains("sess-1"));
    }

    #[test]
    fn tree_collapse_already_collapsed_is_noop() {
        let mut state = test_tree_overlay();
        state.items[0].collapsed = true;
        let original_len = state.items.len();
        let (action, _) = process_list_input(&mut state, b"\x1b[D");
        assert!(matches!(action, OverlayAction::Handled));
        assert_eq!(state.items.len(), original_len);
    }

    #[test]
    fn tree_expand_returns_rebuild() {
        let mut state = test_tree_overlay();
        // First collapse
        let (_, _) = process_list_input(&mut state, b"\x1b[D");
        assert!(state.items[0].collapsed);

        // Right arrow expands — returns RebuildTree
        let (action, consumed) = process_list_input(&mut state, b"\x1b[C");
        assert!(matches!(action, OverlayAction::RebuildTree));
        assert_eq!(consumed, 3);
        assert!(!state.items[0].collapsed);
    }

    #[test]
    fn tree_expand_already_expanded_is_noop() {
        let mut state = test_tree_overlay();
        let (action, _) = process_list_input(&mut state, b"\x1b[C");
        assert!(matches!(action, OverlayAction::Handled));
    }

    #[test]
    fn tree_collapse_on_child_item_is_noop() {
        let mut state = test_tree_overlay();
        state.selected = 1; // a window item (indent=1)
        let original_len = state.items.len();
        let (action, _) = process_list_input(&mut state, b"\x1b[D");
        assert!(matches!(action, OverlayAction::Handled));
        assert_eq!(state.items.len(), original_len);
    }

    #[test]
    fn tree_toggle_non_tree_kind_is_noop() {
        let mut state = test_tree_overlay();
        state.kind = ListKind::Buffer;
        let (action, _) = process_list_input(&mut state, b"\x1b[D");
        assert!(matches!(action, OverlayAction::Handled));
        assert!(!state.items[0].collapsed);
    }
}
