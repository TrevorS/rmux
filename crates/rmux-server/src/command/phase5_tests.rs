//! Comprehensive e2e tests for Phase 5: Copy mode, paste buffers, mouse support.
//!
//! Tests exercise:
//! - Paste buffer store operations
//! - Copy mode commands (copy-mode, paste-buffer, list/show/set/delete-buffer)
//! - Copy mode navigation and selection
//! - Key table bindings (copy-mode-vi, copy-mode-emacs)
//! - Mouse event parsing
//! - Layout pane_at coordinate lookup

use super::test_helpers::MockCommandServer;
use crate::command::{CommandResult, execute_command};

fn exec(
    server: &mut MockCommandServer,
    args: &[&str],
) -> Result<CommandResult, crate::server::ServerError> {
    let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    execute_command(&argv, server)
}

fn output_text(result: Result<CommandResult, crate::server::ServerError>) -> String {
    match result.unwrap() {
        CommandResult::Output(text) => text,
        other => panic!("expected Output, got {other:?}"),
    }
}

/// Build a screen pre-populated with ASCII text lines.
/// Shared across copy-mode test modules to avoid duplication.
fn make_screen(width: u32, height: u32, lines: &[&str]) -> rmux_core::screen::Screen {
    use rmux_core::grid::cell::{CellFlags, GridCell};
    use rmux_core::style::Style;
    use rmux_core::utf8::Utf8Char;

    let mut screen = rmux_core::screen::Screen::new(width, height, 2000);
    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.bytes().enumerate() {
            screen.grid.set_cell(
                x as u32,
                y as u32,
                &GridCell {
                    data: Utf8Char::from_ascii(ch),
                    style: Style::DEFAULT,
                    link: 0,
                    flags: CellFlags::empty(),
                },
            );
        }
    }
    screen
}

// ============================================================
// Paste buffer store
// ============================================================

mod paste_buffer_store_tests {
    use crate::paste::PasteBufferStore;

    #[test]
    fn add_and_get_top() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"hello".to_vec());
        let top = store.get_top().unwrap();
        assert_eq!(top.data, b"hello");
    }

    #[test]
    fn add_multiple_most_recent_first() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"first".to_vec());
        store.add(b"second".to_vec());
        assert_eq!(store.get_top().unwrap().data, b"second");
    }

    #[test]
    fn limit_enforcement_removes_oldest() {
        let mut store = PasteBufferStore::new(2);
        store.add(b"one".to_vec());
        store.add(b"two".to_vec());
        store.add(b"three".to_vec());
        assert_eq!(store.len(), 2);
        assert_eq!(store.get_top().unwrap().data, b"three");
        // "one" should have been evicted
        assert!(store.get_by_name("buffer0000").is_none());
    }

    #[test]
    fn named_buffer_set_and_get() {
        let mut store = PasteBufferStore::new(50);
        store.set("myname", b"data".to_vec());
        let buf = store.get_by_name("myname").unwrap();
        assert_eq!(buf.data, b"data");
    }

    #[test]
    fn delete_buffer() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"hello".to_vec());
        let name = store.get_top().unwrap().name.clone();
        assert!(store.delete(&name));
        assert!(store.is_empty());
    }

    #[test]
    fn list_buffers() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"first".to_vec());
        store.add(b"second".to_vec());
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert!(list[0].name.contains("buffer0001")); // Most recent first
        assert!(list[1].name.contains("buffer0000"));
    }

    #[test]
    fn clear_buffers() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"a".to_vec());
        store.add(b"b".to_vec());
        store.clear();
        assert!(store.is_empty());
    }
}

// ============================================================
// Paste buffer commands via MockCommandServer
// ============================================================

mod paste_command_tests {
    use super::*;

    #[test]
    fn set_and_show_buffer() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "test", "hello world"]).unwrap();
        let output = output_text(exec(&mut s, &["show-buffer", "-b", "test"]));
        assert_eq!(output, "hello world");
    }

    #[test]
    fn set_buffer_auto_name() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "auto content"]).unwrap();
        // Should be accessible via list-buffers
        let output = output_text(exec(&mut s, &["list-buffers"]));
        assert!(output.contains("auto content") || output.contains("buffer"));
    }

    #[test]
    fn list_buffers_empty() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["list-buffers"]).unwrap();
        // Empty buffers should return Ok (no output)
        matches!(result, CommandResult::Ok);
    }

    #[test]
    fn delete_buffer_removes_it() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "del", "data"]).unwrap();
        exec(&mut s, &["delete-buffer", "-b", "del"]).unwrap();
        let result = exec(&mut s, &["show-buffer", "-b", "del"]);
        assert!(result.is_err());
    }

    #[test]
    fn copy_mode_enters() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["copy-mode"]).unwrap();
        assert!(s.copy_mode_entered);
    }

    #[test]
    fn paste_buffer_no_buffers_errors() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Paste with no buffers should error
        let result = exec(&mut s, &["paste-buffer"]);
        assert!(result.is_err());
    }

    #[test]
    fn paste_buffer_with_data_succeeds() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Set a buffer, then paste should succeed
        exec(&mut s, &["set-buffer", "hello"]).unwrap();
        let result = exec(&mut s, &["paste-buffer"]);
        assert!(result.is_ok());
    }

    #[test]
    fn show_buffer_nonexistent_errors() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["show-buffer", "-b", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn delete_buffer_requires_name() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["delete-buffer"]);
        assert!(result.is_err());
    }

    #[test]
    fn command_aliases_work() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Test aliases
        exec(&mut s, &["setb", "-b", "a1", "test1"]).unwrap();
        let output = output_text(exec(&mut s, &["showb", "-b", "a1"]));
        assert_eq!(output, "test1");

        let output = output_text(exec(&mut s, &["lsb"]));
        assert!(!output.is_empty());

        exec(&mut s, &["deleteb", "-b", "a1"]).unwrap();
    }
}

// ============================================================
// Copy mode state and navigation
// ============================================================

mod copy_mode_tests {
    use super::make_screen;
    use crate::copymode::{
        CopyModeAction, CopyModeState, copy_selection, dispatch_copy_mode_action,
    };
    use rmux_core::screen::Screen;
    use rmux_core::screen::selection::SelectionType;

    #[test]
    fn enter_vi_mode() {
        let screen = Screen::new(80, 24, 2000);
        let cm = CopyModeState::enter(&screen, "vi");
        assert_eq!(cm.key_table, "copy-mode-vi");
        assert_eq!(cm.cx, 0);
        assert_eq!(cm.cy, 23); // Bottom of screen
        assert_eq!(cm.oy, 0);
        assert!(!cm.selecting);
    }

    #[test]
    fn enter_emacs_mode() {
        let screen = Screen::new(80, 24, 2000);
        let cm = CopyModeState::enter(&screen, "emacs");
        assert_eq!(cm.key_table, "copy-mode-emacs");
    }

    #[test]
    fn cursor_movement_clamps() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        // At bottom, moving down should do nothing
        cm.cursor_down(&screen, 10);
        assert_eq!(cm.cy, 23);
        assert_eq!(cm.oy, 0);

        // Move to top
        cm.cursor_up(&screen, 100);
        assert_eq!(cm.cy, 0);
        assert_eq!(cm.oy, 0); // No history

        // Left at 0 stays at 0
        cm.cursor_left();
        assert_eq!(cm.cx, 0);

        // Right doesn't exceed width
        for _ in 0..100 {
            cm.cursor_right(&screen);
        }
        assert_eq!(cm.cx, 79);
    }

    #[test]
    fn page_movement_with_history() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..100 {
            screen.grid.scroll_up();
        }

        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.page_up(&screen);
        assert_eq!(cm.oy, 24);
        cm.page_up(&screen);
        assert_eq!(cm.oy, 48);
        cm.page_down(&screen);
        assert_eq!(cm.oy, 24);

        // Halfpage
        cm.halfpage_up(&screen);
        assert_eq!(cm.oy, 36);
        cm.halfpage_down(&screen);
        assert_eq!(cm.oy, 24);
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

        cm.history_bottom(&screen);
        assert_eq!(cm.oy, 0);
        assert_eq!(cm.cy, 23);
    }

    #[test]
    fn word_navigation() {
        let screen = make_screen(80, 24, &["hello world foo bar"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.next_word(&screen);
        assert_eq!(cm.cx, 6); // "world"

        cm.next_word(&screen);
        assert_eq!(cm.cx, 12); // "foo"

        cm.previous_word(&screen);
        assert_eq!(cm.cx, 6); // "world"

        cm.next_word_end(&screen);
        assert_eq!(cm.cx, 10); // end of "world"
    }

    #[test]
    fn line_navigation() {
        let screen = make_screen(80, 24, &["  hello world"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 5;

        cm.start_of_line();
        assert_eq!(cm.cx, 0);

        cm.end_of_line(&screen);
        assert_eq!(cm.cx, 12); // "  hello world" is 13 chars, last index 12

        cm.back_to_indentation(&screen);
        assert_eq!(cm.cx, 2); // First non-space
    }

    #[test]
    fn selection_normal() {
        let screen = make_screen(80, 24, &["Hello World"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.begin_selection(screen.grid.history_size());
        assert!(cm.selecting);
        assert_eq!(cm.sel_type, SelectionType::Normal);

        cm.cx = 4;
        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(data, b"Hello");
    }

    #[test]
    fn selection_line() {
        let screen = make_screen(80, 24, &["Full Line Here", "Second Line"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 3;

        cm.select_line(screen.grid.history_size());
        assert_eq!(cm.sel_type, SelectionType::Line);

        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(String::from_utf8_lossy(&data), "Full Line Here");
    }

    #[test]
    fn selection_multiline() {
        let screen = make_screen(80, 24, &["Line A", "Line B", "Line C"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;

        cm.begin_selection(screen.grid.history_size());
        cm.cy = 2;
        cm.cx = 5;

        let data = copy_selection(&screen, &cm).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("Line A"));
        assert!(text.contains("Line B"));
        assert!(text.contains("Line C"));
    }

    #[test]
    fn selection_rectangle() {
        let screen = make_screen(80, 24, &["ABCDE", "FGHIJ", "KLMNO"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 1;

        cm.begin_selection(screen.grid.history_size());
        cm.rectangle_toggle();
        assert_eq!(cm.sel_type, SelectionType::Block);

        cm.cy = 2;
        cm.cx = 3;

        let data = copy_selection(&screen, &cm).unwrap();
        let text = String::from_utf8_lossy(&data);
        // Block selection: columns 1-3 of rows 0-2
        assert!(text.contains("BCD"));
        assert!(text.contains("GHI"));
        assert!(text.contains("LMN"));
    }

    #[test]
    fn dispatch_cancel() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        match dispatch_copy_mode_action(&screen, &mut cm, "cancel") {
            CopyModeAction::Exit { copy_data } => assert!(copy_data.is_none()),
            _ => panic!("expected Exit"),
        }
    }

    #[test]
    fn dispatch_copy_and_cancel_with_selection() {
        let screen = make_screen(80, 24, &["Test"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;
        cm.begin_selection(screen.grid.history_size());
        cm.cx = 3;

        match dispatch_copy_mode_action(&screen, &mut cm, "copy-selection-and-cancel") {
            CopyModeAction::Exit { copy_data } => {
                assert_eq!(copy_data.unwrap(), b"Test");
            }
            _ => panic!("expected Exit with data"),
        }
    }

    #[test]
    fn dispatch_navigation_actions() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        // All navigation actions return Handled
        for action in &[
            "cursor-up",
            "cursor-down",
            "cursor-left",
            "cursor-right",
            "page-up",
            "page-down",
            "halfpage-up",
            "halfpage-down",
            "history-top",
            "history-bottom",
            "start-of-line",
            "end-of-line",
            "back-to-indentation",
            "next-word",
            "previous-word",
            "next-word-end",
        ] {
            match dispatch_copy_mode_action(&screen, &mut cm, action) {
                CopyModeAction::Handled => {}
                other => panic!("expected Handled for {action}, got {other:?}"),
            }
        }
    }

    #[test]
    fn dispatch_selection_actions() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        match dispatch_copy_mode_action(&screen, &mut cm, "begin-selection") {
            CopyModeAction::Handled => assert!(cm.selecting),
            _ => panic!("expected Handled"),
        }

        match dispatch_copy_mode_action(&screen, &mut cm, "rectangle-toggle") {
            CopyModeAction::Handled => assert_eq!(cm.sel_type, SelectionType::Block),
            _ => panic!("expected Handled"),
        }
    }

    #[test]
    fn dispatch_unknown_returns_unhandled() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");

        match dispatch_copy_mode_action(&screen, &mut cm, "nonexistent-action") {
            CopyModeAction::Unhandled => {}
            _ => panic!("expected Unhandled"),
        }
    }

    #[test]
    fn no_selection_returns_none() {
        let screen = Screen::new(80, 24, 2000);
        let cm = CopyModeState::enter(&screen, "vi");
        assert!(copy_selection(&screen, &cm).is_none());
    }
}

// ============================================================
// Key binding tables
// ============================================================

mod key_table_tests {
    use crate::keybind::KeyBindings;
    use rmux_core::key::*;

    #[test]
    fn vi_table_has_hjkl() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'h' as KeyCode),
            Some(&vec!["cursor-left".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'j' as KeyCode),
            Some(&vec!["cursor-down".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'k' as KeyCode),
            Some(&vec!["cursor-up".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'l' as KeyCode),
            Some(&vec!["cursor-right".to_string()])
        );
    }

    #[test]
    fn vi_table_has_selection() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'v' as KeyCode),
            Some(&vec!["begin-selection".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'y' as KeyCode),
            Some(&vec!["copy-selection-and-cancel".to_string()])
        );
    }

    #[test]
    fn vi_table_has_cancel() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'q' as KeyCode),
            Some(&vec!["cancel".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_ESCAPE),
            Some(&vec!["cancel".to_string()])
        );
    }

    #[test]
    fn emacs_table_has_navigation() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_UP),
            Some(&vec!["cursor-up".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_ESCAPE),
            Some(&vec!["cancel".to_string()])
        );
    }

    #[test]
    fn add_and_remove_binding() {
        let mut kb = KeyBindings::default_bindings();
        kb.add_binding("copy-mode-vi", b'z' as KeyCode, vec!["test-action".into()]);
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'z' as KeyCode),
            Some(&vec!["test-action".to_string()])
        );
        assert!(kb.remove_binding("copy-mode-vi", b'z' as KeyCode));
        assert!(kb.lookup_in_table("copy-mode-vi", b'z' as KeyCode).is_none());
    }

    #[test]
    fn prefix_bracket_bindings() {
        let mut kb = KeyBindings::default_bindings();

        // Prefix+[ should enter copy mode
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"[");
        match result {
            Some(crate::keybind::KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["copy-mode"]);
            }
            _ => panic!("expected Command for copy-mode"),
        }

        // Prefix+] should paste buffer
        let _ = kb.process_input(b"\x02");
        let (result, _) = kb.process_input(b"]");
        match result {
            Some(crate::keybind::KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["paste-buffer"]);
            }
            _ => panic!("expected Command for paste-buffer"),
        }
    }

    #[test]
    fn list_bindings_includes_copy_mode() {
        let kb = KeyBindings::default_bindings();
        let bindings = kb.list_bindings();
        let has_vi = bindings.iter().any(|b| b.contains("copy-mode-vi"));
        let has_emacs = bindings.iter().any(|b| b.contains("copy-mode-emacs"));
        assert!(has_vi, "list-keys should include copy-mode-vi bindings");
        assert!(has_emacs, "list-keys should include copy-mode-emacs bindings");
    }
}

// ============================================================
// Mouse event parsing
// ============================================================

mod mouse_parsing_tests {
    use rmux_core::key::*;
    use rmux_terminal::keys::{parse_key, parse_key_event};
    use rmux_terminal::mouse;

    #[test]
    fn x10_click_parsed() {
        // ESC[M + button(0+32) + x(10+33) + y(5+33)
        let data = [0x1b, b'[', b'M', 32, 43, 38];
        let (key, consumed) = parse_key(&data).unwrap();
        assert_eq!(key, KEYC_MOUSEDOWN1);
        assert_eq!(consumed, 6);
    }

    #[test]
    fn sgr_click_parsed() {
        let data = b"\x1b[<0;11;6M";
        let (key, consumed) = parse_key(data).unwrap();
        assert_eq!(key, KEYC_MOUSEDOWN1);
        assert_eq!(consumed, 10);
    }

    #[test]
    fn sgr_release_parsed() {
        let data = b"\x1b[<0;5;3m";
        let (key, _) = parse_key(data).unwrap();
        assert_eq!(key, KEYC_MOUSEUP1);
    }

    #[test]
    fn sgr_wheel_up() {
        let data = b"\x1b[<64;10;20M";
        let (key, _) = parse_key(data).unwrap();
        assert_eq!(key, KEYC_WHEELUP);
    }

    #[test]
    fn sgr_wheel_down() {
        let data = b"\x1b[<65;10;20M";
        let (key, _) = parse_key(data).unwrap();
        assert_eq!(key, KEYC_WHEELDOWN);
    }

    #[test]
    fn sgr_drag() {
        let data = b"\x1b[<32;15;20M";
        let (key, _) = parse_key(data).unwrap();
        assert_eq!(key, KEYC_MOUSEDRAG1);
    }

    #[test]
    fn parse_key_event_returns_coords() {
        let data = b"\x1b[<0;11;6M";
        let event = parse_key_event(data).unwrap();
        assert_eq!(event.key, KEYC_MOUSEDOWN1);
        assert_eq!(event.mouse_x, 10); // 11-1
        assert_eq!(event.mouse_y, 5); // 6-1
    }

    #[test]
    fn parse_key_event_non_mouse_has_zero_coords() {
        let event = parse_key_event(b"a").unwrap();
        assert_eq!(event.key, b'a' as KeyCode);
        assert_eq!(event.mouse_x, 0);
        assert_eq!(event.mouse_y, 0);
    }

    #[test]
    fn encode_sgr_roundtrip() {
        let encoded = mouse::encode_sgr_mouse(KEYC_MOUSEDOWN1, 10, 5);
        let event = parse_key_event(&encoded).unwrap();
        assert_eq!(event.key, KEYC_MOUSEDOWN1);
        assert_eq!(event.mouse_x, 10);
        assert_eq!(event.mouse_y, 5);
    }

    #[test]
    fn encode_sgr_release_roundtrip() {
        let encoded = mouse::encode_sgr_mouse(KEYC_MOUSEUP1, 3, 7);
        let event = parse_key_event(&encoded).unwrap();
        assert_eq!(event.key, KEYC_MOUSEUP1);
        assert_eq!(event.mouse_x, 3);
        assert_eq!(event.mouse_y, 7);
    }

    #[test]
    fn keyc_is_mouse_works() {
        assert!(keyc_is_mouse(KEYC_MOUSEDOWN1));
        assert!(keyc_is_mouse(KEYC_MOUSEUP1));
        assert!(keyc_is_mouse(KEYC_MOUSEDRAG1));
        assert!(keyc_is_mouse(KEYC_WHEELUP));
        assert!(keyc_is_mouse(KEYC_WHEELDOWN));
        assert!(!keyc_is_mouse(b'a'.into()));
        assert!(!keyc_is_mouse(KEYC_UP));
    }
}

// ============================================================
// Layout pane_at
// ============================================================

mod layout_tests {
    use rmux_core::layout::{LayoutCell, layout_even_horizontal, layout_even_vertical};

    #[test]
    fn pane_at_single_pane() {
        let layout = LayoutCell::new_pane(0, 0, 80, 24, 42);
        assert_eq!(layout.pane_at(0, 0), Some(42));
        assert_eq!(layout.pane_at(79, 23), Some(42));
        assert_eq!(layout.pane_at(80, 0), None); // Out of bounds
    }

    #[test]
    fn pane_at_horizontal_split() {
        let layout = layout_even_horizontal(80, 24, &[1, 2]);
        // Left pane
        assert_eq!(layout.pane_at(0, 0), Some(1));
        // Right pane
        assert_eq!(layout.pane_at(79, 0), Some(2));
    }

    #[test]
    fn pane_at_vertical_split() {
        let layout = layout_even_vertical(80, 24, &[1, 2]);
        // Top pane
        assert_eq!(layout.pane_at(0, 0), Some(1));
        // Bottom pane
        assert_eq!(layout.pane_at(0, 23), Some(2));
    }

    #[test]
    fn pane_at_three_panes() {
        let layout = layout_even_horizontal(80, 24, &[1, 2, 3]);
        // Each pane should be roughly 80/3 wide
        assert_eq!(layout.pane_at(0, 0), Some(1));
        assert_eq!(layout.pane_at(79, 0), Some(3));
    }
}

// ============================================================
// Grid absolute line access
// ============================================================

mod grid_absolute_tests {
    use rmux_core::grid::Grid;
    use rmux_core::grid::cell::{CellFlags, GridCell};
    use rmux_core::style::Style;
    use rmux_core::utf8::Utf8Char;

    #[test]
    fn total_lines_no_history() {
        let grid = Grid::new(80, 24, 2000);
        assert_eq!(grid.total_lines(), 24);
    }

    #[test]
    fn total_lines_with_history() {
        let mut grid = Grid::new(80, 24, 2000);
        for _ in 0..10 {
            grid.scroll_up();
        }
        assert_eq!(grid.total_lines(), 34); // 10 + 24
    }

    #[test]
    fn get_line_absolute_visible() {
        let mut grid = Grid::new(80, 24, 2000);
        // Write to visible line 0
        grid.set_cell(
            0,
            0,
            &GridCell {
                data: Utf8Char::from_ascii(b'A'),
                style: Style::DEFAULT,
                link: 0,
                flags: CellFlags::empty(),
            },
        );

        let hs = grid.history_size();
        let line = grid.get_line_absolute(hs).unwrap(); // hs+0 = first visible
        let cell = line.get_cell(0);
        assert_eq!(cell.data.as_bytes(), b"A");
    }

    #[test]
    fn get_line_absolute_history() {
        let mut grid = Grid::new(80, 24, 2000);
        // Write to visible line 0
        grid.set_cell(
            0,
            0,
            &GridCell {
                data: Utf8Char::from_ascii(b'H'),
                style: Style::DEFAULT,
                link: 0,
                flags: CellFlags::empty(),
            },
        );
        // Scroll up to push it to history
        grid.scroll_up();

        let line = grid.get_line_absolute(0).unwrap();
        let cell = line.get_cell(0);
        assert_eq!(cell.data.as_bytes(), b"H");
    }

    #[test]
    fn get_line_absolute_out_of_range() {
        let grid = Grid::new(80, 24, 2000);
        assert!(grid.get_line_absolute(100).is_none());
    }
}

// ============================================================
// Pane copy mode state
// ============================================================

mod pane_copy_mode_tests {
    use crate::pane::Pane;

    #[test]
    fn enter_and_exit_copy_mode() {
        let mut pane = Pane::new(80, 24, 0);
        assert!(!pane.is_in_copy_mode());

        pane.enter_copy_mode("vi");
        assert!(pane.is_in_copy_mode());
        assert_eq!(pane.copy_mode.as_ref().unwrap().key_table, "copy-mode-vi");

        pane.exit_copy_mode();
        assert!(!pane.is_in_copy_mode());
    }

    #[test]
    fn enter_emacs_copy_mode() {
        let mut pane = Pane::new(80, 24, 0);
        pane.enter_copy_mode("emacs");
        assert_eq!(pane.copy_mode.as_ref().unwrap().key_table, "copy-mode-emacs");
    }
}

// ============================================================
// Extended paste buffer store edge cases
// ============================================================

mod paste_buffer_store_extended {
    use crate::paste::PasteBufferStore;

    #[test]
    fn named_buffer_not_evicted_by_limit() {
        let mut store = PasteBufferStore::new(2);
        store.set("custom", b"named".to_vec());
        store.add(b"a1".to_vec());
        store.add(b"a2".to_vec());
        store.add(b"a3".to_vec()); // Should evict oldest auto, not "custom"
        assert!(store.get_by_name("custom").is_some());
        // 2 automatic + 1 named = 3
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn set_replaces_existing_named() {
        let mut store = PasteBufferStore::new(50);
        store.set("mybuf", b"old".to_vec());
        store.set("mybuf", b"new".to_vec());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get_by_name("mybuf").unwrap().data, b"new");
    }

    #[test]
    fn set_empty_name_auto_generates() {
        let mut store = PasteBufferStore::new(50);
        store.set("", b"data".to_vec());
        let top = store.get_top().unwrap();
        assert!(top.name.starts_with("buffer"));
        assert!(top.automatic);
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let mut store = PasteBufferStore::new(50);
        assert!(!store.delete("nope"));
    }

    #[test]
    fn empty_data_buffer() {
        let mut store = PasteBufferStore::new(50);
        store.add(Vec::new());
        assert_eq!(store.get_top().unwrap().data.len(), 0);
    }

    #[test]
    fn large_buffer_data() {
        let mut store = PasteBufferStore::new(50);
        let big = vec![b'X'; 1_024];
        store.add(big);
        assert_eq!(store.get_top().unwrap().data.len(), 1_024);
    }

    #[test]
    fn mixed_named_auto_ordering() {
        let mut store = PasteBufferStore::new(50);
        store.add(b"auto1".to_vec());
        store.set("named1", b"n1".to_vec());
        store.add(b"auto2".to_vec());
        let list = store.list();
        assert_eq!(list.len(), 3);
        // Most recent first: auto2, named1, auto1
        assert_eq!(list[0].data, b"auto2");
        assert_eq!(list[1].data, b"n1");
        assert_eq!(list[2].data, b"auto1");
    }

    #[test]
    fn limit_one() {
        let mut store = PasteBufferStore::new(1);
        store.add(b"a".to_vec());
        store.add(b"b".to_vec());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get_top().unwrap().data, b"b");
    }

    #[test]
    fn get_top_empty_store() {
        let store = PasteBufferStore::new(50);
        assert!(store.get_top().is_none());
    }
}

// ============================================================
// Extended paste command tests
// ============================================================

mod paste_command_extended {
    use super::*;
    #[test]
    fn paste_buffer_named() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "mybuf", "hello"]).unwrap();
        let result = exec(&mut s, &["paste-buffer", "-b", "mybuf"]);
        assert!(result.is_ok());
    }

    #[test]
    fn paste_buffer_named_nonexistent_errors() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["paste-buffer", "-b", "nope"]);
        assert!(result.is_err());
    }

    #[test]
    fn show_buffer_default_name() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "buffer0000", "default"]).unwrap();
        // show-buffer without -b should default to buffer0000
        let output = output_text(exec(&mut s, &["show-buffer"]));
        assert_eq!(output, "default");
    }

    #[test]
    fn multiple_set_delete_cycle() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "a", "aaa"]).unwrap();
        exec(&mut s, &["set-buffer", "-b", "b", "bbb"]).unwrap();
        exec(&mut s, &["set-buffer", "-b", "c", "ccc"]).unwrap();

        let output = output_text(exec(&mut s, &["list-buffers"]));
        assert!(output.contains("a:"));
        assert!(output.contains("b:"));
        assert!(output.contains("c:"));

        exec(&mut s, &["delete-buffer", "-b", "b"]).unwrap();
        let output = output_text(exec(&mut s, &["list-buffers"]));
        assert!(!output.contains("b:"));
        assert!(output.contains("a:"));
        assert!(output.contains("c:"));

        // show-buffer for deleted should fail
        assert!(exec(&mut s, &["show-buffer", "-b", "b"]).is_err());
    }

    #[test]
    fn set_buffer_replaces_content() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "test", "old"]).unwrap();
        exec(&mut s, &["set-buffer", "-b", "test", "new"]).unwrap();
        let output = output_text(exec(&mut s, &["show-buffer", "-b", "test"]));
        assert_eq!(output, "new");
    }

    #[test]
    fn set_buffer_no_data_errors() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["set-buffer"]);
        assert!(result.is_err());
    }

    #[test]
    fn list_buffers_shows_sizes() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "x", "hello"]).unwrap();
        let output = output_text(exec(&mut s, &["list-buffers"]));
        assert!(output.contains("5 bytes")); // "hello" is 5 bytes
    }
}

// ============================================================
// Extended copy mode navigation edge cases
// ============================================================

mod copy_mode_navigation_extended {
    use super::make_screen;
    use crate::copymode::CopyModeState;
    use rmux_core::screen::Screen;

    #[test]
    fn cursor_up_scrolls_into_history_and_back() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..10 {
            screen.grid.scroll_up();
        }
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cursor_up(&screen, 3);
        assert_eq!(cm.oy, 3);
        assert_eq!(cm.cy, 0);

        // cursor_down first moves cy until max_y, then decreases oy
        cm.cursor_down(&screen, 3);
        assert_eq!(cm.cy, 3);
        assert_eq!(cm.oy, 3); // oy unchanged because cy wasn't at max
    }

    #[test]
    fn page_up_clamps_at_max_history() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..10 {
            screen.grid.scroll_up();
        }
        let mut cm = CopyModeState::enter(&screen, "vi");
        // Page up should cap at history_size
        cm.page_up(&screen);
        assert_eq!(cm.oy, 10); // Clamped to max
        cm.page_up(&screen);
        assert_eq!(cm.oy, 10); // Still clamped
    }

    #[test]
    fn page_down_clamps_at_zero() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.page_down(&screen);
        assert_eq!(cm.oy, 0); // Can't go below zero
    }

    #[test]
    fn halfpage_up_down_symmetry() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..100 {
            screen.grid.scroll_up();
        }
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.halfpage_up(&screen);
        let oy1 = cm.oy;
        cm.halfpage_down(&screen);
        assert_eq!(cm.oy, 0); // Back to start
        cm.halfpage_up(&screen);
        assert_eq!(cm.oy, oy1); // Same as before
    }

    #[test]
    fn cursor_right_clamps_at_width() {
        let screen = Screen::new(10, 5, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        for _ in 0..20 {
            cm.cursor_right(&screen);
        }
        assert_eq!(cm.cx, 9); // Max for width 10
    }

    #[test]
    fn word_nav_at_line_start() {
        let screen = make_screen(80, 24, &["hello world"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;
        cm.previous_word(&screen);
        assert_eq!(cm.cx, 0); // Already at start, stays there
    }

    #[test]
    fn word_nav_at_line_end() {
        let screen = make_screen(80, 24, &["hello world"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 10; // End of "world"
        cm.next_word(&screen);
        // No more words, should stay put or go to end
        assert!(cm.cx >= 10);
    }

    #[test]
    fn back_to_indentation_no_indentation() {
        let screen = make_screen(80, 24, &["noindent"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 5;
        cm.back_to_indentation(&screen);
        assert_eq!(cm.cx, 0); // First char is non-space
    }

    #[test]
    fn next_word_end_from_start() {
        let screen = make_screen(80, 24, &["abc def ghi"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;
        cm.next_word_end(&screen);
        assert_eq!(cm.cx, 2); // End of "abc"
    }

    #[test]
    fn absolute_y_no_history() {
        let screen = Screen::new(80, 24, 2000);
        let cm = CopyModeState::enter(&screen, "vi");
        assert_eq!(cm.absolute_y(0), 23); // cy=23, oy=0, hs=0
    }

    #[test]
    fn absolute_y_with_history() {
        let mut screen = Screen::new(80, 24, 2000);
        for _ in 0..50 {
            screen.grid.scroll_up();
        }
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.oy = 10;
        cm.cy = 5;
        // abs_y = hs - oy + cy = 50 - 10 + 5 = 45
        assert_eq!(cm.absolute_y(50), 45);
    }

    #[test]
    fn small_screen_navigation() {
        let screen = Screen::new(5, 3, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        assert_eq!(cm.cy, 2); // Height-1
        cm.cursor_up(&screen, 10);
        assert_eq!(cm.cy, 0);
        cm.cursor_right(&screen);
        cm.cursor_right(&screen);
        cm.cursor_right(&screen);
        cm.cursor_right(&screen);
        assert_eq!(cm.cx, 4); // Width-1
        cm.cursor_right(&screen);
        assert_eq!(cm.cx, 4); // Clamped
    }
}

// ============================================================
// Extended selection edge cases
// ============================================================

mod copy_mode_selection_extended {
    use super::make_screen;
    use crate::copymode::{CopyModeState, copy_selection};
    use rmux_core::screen::Screen;
    use rmux_core::screen::selection::SelectionType;

    #[test]
    fn single_char_selection() {
        let screen = make_screen(80, 24, &["ABCDE"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 2;
        cm.begin_selection(screen.grid.history_size());
        // Don't move - select just one char
        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(data, b"C");
    }

    #[test]
    fn selection_reversed_direction() {
        let screen = make_screen(80, 24, &["ABCDE"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 4;
        cm.begin_selection(screen.grid.history_size());
        cm.cx = 0; // Move left (reverse)
        let data = copy_selection(&screen, &cm).unwrap();
        assert_eq!(data, b"ABCDE");
    }

    #[test]
    fn multiline_selection_trims_trailing_spaces() {
        let screen = make_screen(80, 24, &["Hello   ", "World   "]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;
        cm.begin_selection(screen.grid.history_size());
        cm.cy = 1;
        cm.cx = 4;
        let data = copy_selection(&screen, &cm).unwrap();
        let text = String::from_utf8_lossy(&data);
        // Lines should have trailing spaces trimmed between them
        assert!(text.starts_with("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn line_selection_multiple_lines() {
        let screen = make_screen(80, 24, &["AAA", "BBB", "CCC"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 0;
        cm.select_line(screen.grid.history_size());
        cm.cy = 2; // Extend to line 2
        let data = copy_selection(&screen, &cm).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("AAA"));
        assert!(text.contains("BBB"));
        assert!(text.contains("CCC"));
    }

    #[test]
    fn rectangle_toggle_from_line_stays_line() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.select_line(screen.grid.history_size());
        assert_eq!(cm.sel_type, SelectionType::Line);
        cm.rectangle_toggle();
        // Line stays Line (only Normal <-> Block toggle)
        assert_eq!(cm.sel_type, SelectionType::Line);
    }

    #[test]
    fn rectangle_toggle_normal_block_cycle() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.begin_selection(screen.grid.history_size());
        assert_eq!(cm.sel_type, SelectionType::Normal);
        cm.rectangle_toggle();
        assert_eq!(cm.sel_type, SelectionType::Block);
        cm.rectangle_toggle();
        assert_eq!(cm.sel_type, SelectionType::Normal);
    }

    #[test]
    fn rectangle_toggle_without_selection_noop() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        assert!(!cm.selecting);
        cm.rectangle_toggle();
        // Should not crash; selection type is unchanged
        assert_eq!(cm.sel_type, SelectionType::Normal);
    }

    #[test]
    fn current_selection_none_when_not_selecting() {
        let screen = Screen::new(80, 24, 2000);
        let cm = CopyModeState::enter(&screen, "vi");
        assert!(cm.current_selection(0).is_none());
    }

    #[test]
    fn current_selection_some_when_selecting() {
        let screen = Screen::new(80, 24, 2000);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 5;
        cm.cx = 10;
        cm.begin_selection(screen.grid.history_size());
        cm.cy = 8;
        cm.cx = 20;
        let sel = cm.current_selection(screen.grid.history_size()).unwrap();
        assert_eq!(sel.start_x, 10);
        assert_eq!(sel.end_x, 20);
        assert!(sel.active);
    }

    #[test]
    fn block_selection_columns() {
        let screen = make_screen(80, 24, &["0123456789", "ABCDEFGHIJ", "abcdefghij"]);
        let mut cm = CopyModeState::enter(&screen, "vi");
        cm.cy = 0;
        cm.cx = 2;
        cm.begin_selection(screen.grid.history_size());
        cm.rectangle_toggle();
        cm.cy = 2;
        cm.cx = 5;
        let data = copy_selection(&screen, &cm).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.contains("2345"));
        assert!(text.contains("CDEF"));
        assert!(text.contains("cdef"));
    }
}

// ============================================================
// Extended key table and string_to_key tests
// ============================================================

mod key_table_extended {
    use crate::keybind::{KeyBindings, string_to_key};
    use rmux_core::key::*;

    #[test]
    fn vi_word_bindings() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'w' as KeyCode),
            Some(&vec!["next-word".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'b' as KeyCode),
            Some(&vec!["previous-word".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'e' as KeyCode),
            Some(&vec!["next-word-end".to_string()])
        );
    }

    #[test]
    fn vi_line_bindings() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'0' as KeyCode),
            Some(&vec!["start-of-line".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'$' as KeyCode),
            Some(&vec!["end-of-line".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'^' as KeyCode),
            Some(&vec!["back-to-indentation".to_string()])
        );
    }

    #[test]
    fn vi_page_bindings() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_PPAGE),
            Some(&vec!["page-up".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_NPAGE),
            Some(&vec!["page-down".to_string()])
        );
    }

    #[test]
    fn vi_top_bottom_bindings() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'g' as KeyCode),
            Some(&vec!["history-top".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'G' as KeyCode),
            Some(&vec!["history-bottom".to_string()])
        );
    }

    #[test]
    fn vi_line_selection() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'V' as KeyCode),
            Some(&vec!["select-line".to_string()])
        );
    }

    #[test]
    fn vi_arrow_keys() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_UP),
            Some(&vec!["cursor-up".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_DOWN),
            Some(&vec!["cursor-down".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_LEFT),
            Some(&vec!["cursor-left".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_RIGHT),
            Some(&vec!["cursor-right".to_string()])
        );
    }

    #[test]
    fn vi_home_end() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_HOME),
            Some(&vec!["start-of-line".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_END),
            Some(&vec!["end-of-line".to_string()])
        );
    }

    #[test]
    fn vi_enter_copies() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_RETURN),
            Some(&vec!["copy-selection-and-cancel".to_string()])
        );
    }

    #[test]
    fn vi_space_begins_selection() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", KEYC_SPACE),
            Some(&vec!["begin-selection".to_string()])
        );
    }

    #[test]
    fn emacs_navigation_bindings() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_UP),
            Some(&vec!["cursor-up".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_DOWN),
            Some(&vec!["cursor-down".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_LEFT),
            Some(&vec!["cursor-left".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_RIGHT),
            Some(&vec!["cursor-right".to_string()])
        );
    }

    #[test]
    fn emacs_page_bindings() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_PPAGE),
            Some(&vec!["page-up".to_string()])
        );
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_NPAGE),
            Some(&vec!["page-down".to_string()])
        );
    }

    #[test]
    fn emacs_cancel() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(
            kb.lookup_in_table("copy-mode-emacs", KEYC_ESCAPE),
            Some(&vec!["cancel".to_string()])
        );
    }

    #[test]
    fn custom_binding_in_copy_mode_table() {
        let mut kb = KeyBindings::default_bindings();
        kb.add_binding("copy-mode-vi", b'z' as KeyCode, vec!["custom-action".into()]);
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'z' as KeyCode),
            Some(&vec!["custom-action".to_string()])
        );
    }

    #[test]
    fn binding_overwrite() {
        let mut kb = KeyBindings::default_bindings();
        // h is cursor-left by default
        kb.add_binding("copy-mode-vi", b'h' as KeyCode, vec!["replaced".into()]);
        assert_eq!(
            kb.lookup_in_table("copy-mode-vi", b'h' as KeyCode),
            Some(&vec!["replaced".to_string()])
        );
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut kb = KeyBindings::default_bindings();
        assert!(!kb.remove_binding("copy-mode-vi", 0xDEAD));
    }

    #[test]
    fn lookup_nonexistent_table() {
        let kb = KeyBindings::default_bindings();
        assert!(kb.lookup_in_table("nonexistent", b'a' as KeyCode).is_none());
    }

    // string_to_key tests
    #[test]
    fn string_to_key_single_char() {
        assert_eq!(string_to_key("a"), Some(b'a' as KeyCode));
        assert_eq!(string_to_key("Z"), Some(b'Z' as KeyCode));
    }

    #[test]
    fn string_to_key_special_keys() {
        assert_eq!(string_to_key("Up"), Some(KEYC_UP));
        assert_eq!(string_to_key("Down"), Some(KEYC_DOWN));
        assert_eq!(string_to_key("Left"), Some(KEYC_LEFT));
        assert_eq!(string_to_key("Right"), Some(KEYC_RIGHT));
        assert_eq!(string_to_key("Home"), Some(KEYC_HOME));
        assert_eq!(string_to_key("End"), Some(KEYC_END));
        assert_eq!(string_to_key("Enter"), Some(KEYC_RETURN));
        assert_eq!(string_to_key("Escape"), Some(KEYC_ESCAPE));
        assert_eq!(string_to_key("Space"), Some(KEYC_SPACE));
        assert_eq!(string_to_key("Tab"), Some(KEYC_TAB));
        assert_eq!(string_to_key("BSpace"), Some(KEYC_BACKSPACE));
        assert_eq!(string_to_key("PPage"), Some(KEYC_PPAGE));
        assert_eq!(string_to_key("NPage"), Some(KEYC_NPAGE));
    }

    #[test]
    fn string_to_key_function_keys() {
        assert_eq!(string_to_key("F1"), Some(KEYC_F1));
        assert_eq!(string_to_key("F12"), Some(KEYC_F12));
    }

    #[test]
    fn string_to_key_ctrl_modifier() {
        let key = string_to_key("C-b").unwrap();
        assert_eq!(keyc_base(key), b'b' as KeyCode);
        assert!(keyc_modifiers(key).contains(KeyModifiers::CTRL));
    }

    #[test]
    fn string_to_key_meta_modifier() {
        let key = string_to_key("M-x").unwrap();
        assert_eq!(keyc_base(key), b'x' as KeyCode);
        assert!(keyc_modifiers(key).contains(KeyModifiers::META));
    }

    #[test]
    fn string_to_key_shift_modifier() {
        let key = string_to_key("S-Up").unwrap();
        assert_eq!(keyc_base(key), KEYC_UP);
        assert!(keyc_modifiers(key).contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn string_to_key_invalid() {
        assert!(string_to_key("FooBar").is_none());
        assert!(string_to_key("F99").is_none());
    }

    #[test]
    fn string_to_key_aliases() {
        assert_eq!(string_to_key("CR"), Some(KEYC_RETURN));
        assert_eq!(string_to_key("Esc"), Some(KEYC_ESCAPE));
        assert_eq!(string_to_key("IC"), Some(KEYC_INSERT));
        assert_eq!(string_to_key("DC"), Some(KEYC_DELETE));
        assert_eq!(string_to_key("PageUp"), Some(KEYC_PPAGE));
        assert_eq!(string_to_key("PageDown"), Some(KEYC_NPAGE));
    }
}

// ============================================================
// Extended mouse parsing edge cases
// ============================================================

mod mouse_extended {
    use rmux_core::key::*;
    use rmux_terminal::mouse;

    #[test]
    fn x10_middle_button() {
        // Button 1 (middle): cb bit 1 set
        let data = [32 + 1, 43, 38]; // button=1, x=10, y=5
        let result = mouse::try_parse_mouse_csi(&[b'M', data[0], data[1], data[2]]).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN2);
    }

    #[test]
    fn x10_right_button() {
        let data = [32 + 2, 43, 38];
        let result = mouse::try_parse_mouse_csi(&[b'M', data[0], data[1], data[2]]).unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN3);
    }

    #[test]
    fn sgr_button2_click() {
        let result = mouse::try_parse_mouse_csi(b"<1;5;10M").unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN2);
        assert_eq!(result.x, 4);
        assert_eq!(result.y, 9);
    }

    #[test]
    fn sgr_button3_click() {
        let result = mouse::try_parse_mouse_csi(b"<2;5;10M").unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN3);
    }

    #[test]
    fn sgr_button2_release() {
        let result = mouse::try_parse_mouse_csi(b"<1;5;10m").unwrap();
        assert_eq!(result.key, KEYC_MOUSEUP2);
    }

    #[test]
    fn sgr_button3_release() {
        let result = mouse::try_parse_mouse_csi(b"<2;5;10m").unwrap();
        assert_eq!(result.key, KEYC_MOUSEUP3);
    }

    #[test]
    fn sgr_drag_button2() {
        let result = mouse::try_parse_mouse_csi(b"<33;15;20M").unwrap();
        assert_eq!(result.key, KEYC_MOUSEDRAG2);
    }

    #[test]
    fn sgr_drag_button3() {
        let result = mouse::try_parse_mouse_csi(b"<34;15;20M").unwrap();
        assert_eq!(result.key, KEYC_MOUSEDRAG3);
    }

    #[test]
    fn sgr_large_coordinates() {
        let result = mouse::try_parse_mouse_csi(b"<0;256;128M").unwrap();
        assert_eq!(result.key, KEYC_MOUSEDOWN1);
        assert_eq!(result.x, 255);
        assert_eq!(result.y, 127);
    }

    #[test]
    fn encode_all_mouse_events() {
        // Test roundtrip for every type of mouse event
        let events: [(KeyCode, u8); 9] = [
            (KEYC_MOUSEDOWN1, b'M'),
            (KEYC_MOUSEDOWN2, b'M'),
            (KEYC_MOUSEDOWN3, b'M'),
            (KEYC_MOUSEUP1, b'm'),
            (KEYC_MOUSEUP2, b'm'),
            (KEYC_MOUSEUP3, b'm'),
            (KEYC_MOUSEDRAG1, b'M'),
            (KEYC_WHEELUP, b'M'),
            (KEYC_WHEELDOWN, b'M'),
        ];
        for &(key, expected_final) in &events {
            let encoded = mouse::encode_sgr_mouse(key, 5, 3);
            // Check the final byte
            assert_eq!(*encoded.last().unwrap(), expected_final, "key={key:#x}");
        }
    }

    #[test]
    fn invalid_csi_returns_none() {
        // Not a mouse sequence
        assert!(mouse::try_parse_mouse_csi(b"A").is_none()); // Arrow key
        assert!(mouse::try_parse_mouse_csi(b"").is_none());
    }

    #[test]
    fn incomplete_sgr_returns_none() {
        // Missing final M/m
        assert!(mouse::try_parse_mouse_csi(b"<0;5;").is_none());
    }

    #[test]
    fn encode_sgr_coordinate_values() {
        // SGR is 1-based
        let encoded = mouse::encode_sgr_mouse(KEYC_MOUSEDOWN1, 0, 0);
        assert_eq!(encoded, b"\x1b[<0;1;1M"); // 0+1=1, 0+1=1
    }
}

// ============================================================
// Extended layout/pane_at tests
// ============================================================

mod layout_extended {
    use rmux_core::layout::{LayoutCell, layout_even_horizontal, layout_even_vertical};

    #[test]
    fn pane_at_out_of_bounds_returns_none() {
        let layout = LayoutCell::new_pane(0, 0, 80, 24, 1);
        assert!(layout.pane_at(80, 0).is_none());
        assert!(layout.pane_at(0, 24).is_none());
        assert!(layout.pane_at(100, 100).is_none());
    }

    #[test]
    fn pane_at_horizontal_boundaries() {
        let layout = layout_even_horizontal(80, 24, &[1, 2]);
        // Check that each x coord maps to the right pane
        let pane_0_0 = layout.pane_at(0, 0);
        let pane_79_0 = layout.pane_at(79, 0);
        assert_eq!(pane_0_0, Some(1));
        assert_eq!(pane_79_0, Some(2));
        // They should be different panes
        assert_ne!(pane_0_0, pane_79_0);
    }

    #[test]
    fn pane_at_vertical_boundaries() {
        let layout = layout_even_vertical(80, 24, &[1, 2]);
        let pane_top = layout.pane_at(0, 0);
        let pane_bot = layout.pane_at(0, 23);
        assert_eq!(pane_top, Some(1));
        assert_eq!(pane_bot, Some(2));
    }

    #[test]
    fn find_pane_returns_cell() {
        let layout = layout_even_horizontal(80, 24, &[10, 20]);
        let cell = layout.find_pane(10);
        assert!(cell.is_some());
        let cell = cell.unwrap();
        assert_eq!(cell.pane_id, Some(10));
    }

    #[test]
    fn find_pane_nonexistent() {
        let layout = layout_even_horizontal(80, 24, &[1, 2]);
        assert!(layout.find_pane(99).is_none());
    }
}

// ============================================================
// Full workflow / integration tests
// ============================================================

mod workflow_tests {
    use super::*;

    #[test]
    fn set_buffer_list_paste_workflow() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Set multiple buffers
        exec(&mut s, &["set-buffer", "-b", "buf1", "first"]).unwrap();
        exec(&mut s, &["set-buffer", "-b", "buf2", "second"]).unwrap();
        exec(&mut s, &["set-buffer", "-b", "buf3", "third"]).unwrap();

        // List should show all three
        let output = output_text(exec(&mut s, &["list-buffers"]));
        assert!(output.contains("buf1"));
        assert!(output.contains("buf2"));
        assert!(output.contains("buf3"));

        // Paste the most recent (top of stack)
        let result = exec(&mut s, &["paste-buffer"]);
        assert!(result.is_ok());

        // Paste specific named buffer
        let result = exec(&mut s, &["paste-buffer", "-b", "buf1"]);
        assert!(result.is_ok());

        // Delete one and verify it's gone
        exec(&mut s, &["delete-buffer", "-b", "buf2"]).unwrap();
        assert!(exec(&mut s, &["show-buffer", "-b", "buf2"]).is_err());

        // Others still exist
        let output = output_text(exec(&mut s, &["show-buffer", "-b", "buf1"]));
        assert_eq!(output, "first");
    }

    #[test]
    fn copy_mode_enter_via_command() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        assert!(!s.copy_mode_entered);
        exec(&mut s, &["copy-mode"]).unwrap();
        assert!(s.copy_mode_entered);
    }

    #[test]
    fn bind_key_for_copy_mode_lookup() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Add a custom binding to copy-mode-vi
        exec(&mut s, &["bind-key", "-T", "copy-mode-vi", "z", "custom-cmd"]).unwrap();

        // Verify via list-keys
        let output = output_text(exec(&mut s, &["list-keys"]));
        assert!(output.contains("copy-mode-vi"));
        assert!(output.contains("custom-cmd"));
    }

    #[test]
    fn unbind_key_in_copy_mode_table() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Unbind 'q' from copy-mode-vi
        exec(&mut s, &["unbind-key", "-T", "copy-mode-vi", "q"]).unwrap();

        // Verify it's gone by checking list-keys doesn't have q -> cancel
        let output = output_text(exec(&mut s, &["list-keys"]));
        // The binding for 'q' with 'cancel' should be gone
        let has_q_cancel = output
            .lines()
            .any(|l| l.contains("copy-mode-vi") && l.contains(" q ") && l.contains("cancel"));
        assert!(!has_q_cancel);
    }

    #[test]
    fn list_commands_includes_phase5_commands() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let output = output_text(exec(&mut s, &["list-commands"]));
        assert!(output.contains("copy-mode"));
        assert!(output.contains("paste-buffer"));
        assert!(output.contains("list-buffers"));
        assert!(output.contains("show-buffer"));
        assert!(output.contains("set-buffer"));
        assert!(output.contains("delete-buffer"));
    }

    #[test]
    fn buffer_overflow_lifo_ordering() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Add buffers and verify LIFO ordering
        for i in 0..5 {
            exec(&mut s, &["set-buffer", &format!("data{i}")]).unwrap();
        }

        let output = output_text(exec(&mut s, &["list-buffers"]));
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
        // Most recent should be first
        assert!(lines[0].contains("data4"));
    }

    #[test]
    fn copy_mode_on_pane_directly() {
        use crate::copymode::{copy_selection, dispatch_copy_mode_action};
        use crate::pane::Pane;

        let mut pane = Pane::new(80, 24, 2000);
        pane.screen = make_screen(80, 24, &["Hello World"]);

        pane.enter_copy_mode("vi");
        assert!(pane.is_in_copy_mode());

        let cm = pane.copy_mode.as_mut().unwrap();
        cm.cy = 0;
        cm.cx = 0;

        dispatch_copy_mode_action(&pane.screen, cm, "begin-selection");
        cm.cx = 4;

        let data = copy_selection(&pane.screen, cm);
        assert_eq!(data.unwrap(), b"Hello");

        pane.exit_copy_mode();
        assert!(!pane.is_in_copy_mode());
    }

    #[test]
    fn pasteb_alias_works() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-buffer", "-b", "x", "data"]).unwrap();
        let result = exec(&mut s, &["pasteb", "-b", "x"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// Selection contains tests (e2e over selection module)
// ============================================================

mod selection_contains_tests {
    use rmux_core::screen::selection::{Selection, SelectionType};

    #[test]
    fn normal_single_line() {
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 3,
            start_y: 5,
            end_x: 8,
            end_y: 5,
            active: true,
        };
        assert!(sel.contains(3, 5));
        assert!(sel.contains(5, 5));
        assert!(sel.contains(8, 5));
        assert!(!sel.contains(2, 5));
        assert!(!sel.contains(9, 5));
        assert!(!sel.contains(5, 4));
        assert!(!sel.contains(5, 6));
    }

    #[test]
    fn normal_multiline_middle_row_full() {
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 5,
            start_y: 2,
            end_x: 10,
            end_y: 4,
            active: true,
        };
        // Middle row (y=3) should include all columns
        assert!(sel.contains(0, 3));
        assert!(sel.contains(100, 3));
    }

    #[test]
    fn block_selection_strict_bounds() {
        let sel = Selection {
            sel_type: SelectionType::Block,
            start_x: 3,
            start_y: 1,
            end_x: 7,
            end_y: 5,
            active: true,
        };
        assert!(sel.contains(3, 1));
        assert!(sel.contains(7, 5));
        assert!(sel.contains(5, 3));
        assert!(!sel.contains(2, 3)); // Before left edge
        assert!(!sel.contains(8, 3)); // After right edge
        assert!(!sel.contains(5, 0)); // Above
        assert!(!sel.contains(5, 6)); // Below
    }

    #[test]
    fn line_selection_ignores_x() {
        let sel = Selection {
            sel_type: SelectionType::Line,
            start_x: 10,
            start_y: 3,
            end_x: 20,
            end_y: 5,
            active: true,
        };
        assert!(sel.contains(0, 3));
        assert!(sel.contains(1000, 4));
        assert!(sel.contains(0, 5));
        assert!(!sel.contains(0, 2));
        assert!(!sel.contains(0, 6));
    }

    #[test]
    fn selection_normalized_reversed() {
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 10,
            start_y: 5,
            end_x: 3,
            end_y: 2,
            active: true,
        };
        let (sx, sy, ex, ey) = sel.normalized();
        assert_eq!((sx, sy, ex, ey), (3, 2, 10, 5));
    }

    #[test]
    fn selection_normalized_same_line_reversed() {
        let sel = Selection {
            sel_type: SelectionType::Normal,
            start_x: 10,
            start_y: 3,
            end_x: 2,
            end_y: 3,
            active: true,
        };
        let (sx, sy, ex, ey) = sel.normalized();
        assert_eq!((sx, sy, ex, ey), (2, 3, 10, 3));
    }
}

// ============================================================
// Key input processing tests
// ============================================================

mod key_input_tests {
    use crate::keybind::{KeyAction, KeyBindings};

    #[test]
    fn normal_char_passes_through() {
        let mut kb = KeyBindings::default_bindings();
        assert!(kb.process_input(b"a").0.is_none());
        assert!(kb.process_input(b"z").0.is_none());
        assert!(kb.process_input(b"1").0.is_none());
    }

    #[test]
    fn prefix_then_unknown_key_ignored() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02"); // Prefix
        let (result, _) = kb.process_input(b"@"); // Not bound
        assert!(result.is_none());
        // Prefix should be cleared after processing a key
        // Verify by sending another key that is not prefix - should pass through
        let (result2, _) = kb.process_input(b"a");
        assert!(result2.is_none()); // Not in prefix mode anymore
    }

    #[test]
    fn double_prefix_sends_ctrl_b() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02"); // First prefix
        let (result, _) = kb.process_input(b"\x02"); // Second prefix
        match result {
            Some(KeyAction::SendToPane(data)) => {
                assert_eq!(data, vec![0x02]);
            }
            _ => panic!("expected SendToPane"),
        }
    }

    #[test]
    fn prefix_window_number_keys() {
        let mut kb = KeyBindings::default_bindings();
        for i in 0u8..=9 {
            let _ = kb.process_input(b"\x02");
            let (result, _) = kb.process_input(&[b'0' + i]);
            match result {
                Some(KeyAction::Command(argv)) => {
                    assert_eq!(argv[0], "select-window");
                    assert_eq!(argv[2], i.to_string());
                }
                _ => panic!("expected Command for key {i}"),
            }
        }
    }

    #[test]
    fn prefix_c_creates_window() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        match kb.process_input(b"c").0 {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["new-window"]);
            }
            _ => panic!("expected Command for new-window"),
        }
    }

    #[test]
    fn prefix_x_kills_pane() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        match kb.process_input(b"x").0 {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["kill-pane"]);
            }
            _ => panic!("expected Command for kill-pane"),
        }
    }

    #[test]
    fn prefix_colon_command_prompt() {
        let mut kb = KeyBindings::default_bindings();
        let _ = kb.process_input(b"\x02");
        match kb.process_input(b":").0 {
            Some(KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["command-prompt"]);
            }
            _ => panic!("expected Command for command-prompt"),
        }
    }
}

// ── Iteration 7: Missing commands batch 2 ──

mod iter7_commands {
    use super::*;

    #[test]
    fn switch_client_changes_session() {
        let mut server = MockCommandServer::new();
        let (sid1, _, _) = server.create_test_session("sess1");
        let (sid2, _, _) = server.create_test_session("sess2");
        server.client_session_id = Some(sid1);

        let result = exec(&mut server, &["switch-client", "-t", "sess2"]);
        assert!(matches!(result, Ok(CommandResult::Ok)));
        assert_eq!(server.client_session_id, Some(sid2));
    }

    #[test]
    fn switch_client_missing_session() {
        let mut server = MockCommandServer::new();
        server.create_test_session("sess1");

        let result = exec(&mut server, &["switch-client", "-t", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn set_and_show_environment() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        exec(&mut server, &["set-environment", "FOO", "bar"]).unwrap();
        exec(&mut server, &["set-environment", "BAZ", "qux"]).unwrap();

        let out = output_text(exec(&mut server, &["show-environment"]));
        assert!(out.contains("BAZ=qux"));
        assert!(out.contains("FOO=bar"));
    }

    #[test]
    fn unset_environment() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        exec(&mut server, &["set-environment", "KEEP", "yes"]).unwrap();
        exec(&mut server, &["set-environment", "REMOVE_ME", "val"]).unwrap();
        exec(&mut server, &["set-environment", "-u", "REMOVE_ME"]).unwrap();

        let out = output_text(exec(&mut server, &["show-environment"]));
        assert!(!out.contains("REMOVE_ME"));
        assert!(out.contains("KEEP=yes"));
    }

    #[test]
    fn show_environment_single_var() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        exec(&mut server, &["set-environment", "MYVAR", "hello"]).unwrap();

        let out = output_text(exec(&mut server, &["show-environment", "MYVAR"]));
        assert_eq!(out.trim(), "MYVAR=hello");
    }

    #[test]
    fn show_environment_missing_var() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        let out = output_text(exec(&mut server, &["show-environment", "NOPE"]));
        assert_eq!(out.trim(), "-NOPE");
    }

    #[test]
    fn find_window_matches() {
        let mut server = MockCommandServer::new();
        let (sid, _, _) = server.create_test_session("main");
        server.add_window_to_session(sid, "vim");
        server.add_window_to_session(sid, "logs");

        let out = output_text(exec(&mut server, &["find-window", "vim"]));
        assert!(out.contains("vim"));
        assert!(!out.contains("logs"));
    }

    #[test]
    fn find_window_no_match() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        let result = exec(&mut server, &["find-window", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn save_and_load_buffer() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        // Set a buffer
        exec(&mut server, &["set-buffer", "-b", "test", "hello world"]).unwrap();

        // Save to temp file
        let path = "/tmp/rmux-test-save-buffer.txt";
        exec(&mut server, &["save-buffer", "-b", "test", path]).unwrap();

        // Delete the buffer
        exec(&mut server, &["delete-buffer", "-b", "test"]).unwrap();

        // Load it back
        exec(&mut server, &["load-buffer", "-b", "reloaded", path]).unwrap();

        // Verify
        let out = output_text(exec(&mut server, &["show-buffer", "-b", "reloaded"]));
        assert_eq!(out, "hello world");

        // Cleanup
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn refresh_client_marks_redraw() {
        let mut server = MockCommandServer::new();
        let (sid, _, _) = server.create_test_session("main");
        server.redraw_sessions.clear();

        exec(&mut server, &["refresh-client"]).unwrap();
        assert_eq!(server.redraw_sessions, vec![sid]);
    }

    #[test]
    fn switch_client_prefix_match() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");
        server.create_test_session("other");

        // "switchc" is registered as an alias
        let result = exec(&mut server, &["switchc", "-t", "other"]);
        assert!(matches!(result, Ok(CommandResult::Ok)));
    }

    #[test]
    fn setenv_alias() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        exec(&mut server, &["setenv", "X", "1"]).unwrap();
        let out = output_text(exec(&mut server, &["showenv", "X"]));
        assert_eq!(out.trim(), "X=1");
    }

    #[test]
    fn set_and_show_hooks() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        exec(
            &mut server,
            &["set-hook", "after-new-session", "display-message", "hello"],
        )
        .unwrap();

        let out = output_text(exec(&mut server, &["show-hooks"]));
        assert!(out.contains("after-new-session[0]: display-message hello"));
    }

    #[test]
    fn set_hook_unset() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        exec(
            &mut server,
            &["set-hook", "test-hook", "display-message", "hi"],
        )
        .unwrap();
        exec(&mut server, &["set-hook", "-u", "test-hook"]).unwrap();

        let result = exec(&mut server, &["show-hooks"]);
        assert!(matches!(result, Ok(CommandResult::Ok)));
    }

    #[test]
    fn set_hook_missing_command() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        let result = exec(&mut server, &["set-hook", "test-hook"]);
        assert!(result.is_err());
    }

    #[test]
    fn unset_nonexistent_hook() {
        let mut server = MockCommandServer::new();
        server.create_test_session("main");

        let result = exec(&mut server, &["set-hook", "-u", "nope"]);
        assert!(result.is_err());
    }
}
