//! Comprehensive e2e tests for Phase 5: Copy mode, paste buffers, mouse support.
//!
//! Tests exercise:
//! - Paste buffer store operations
//! - Copy mode commands (copy-mode, paste-buffer, list/show/set/delete-buffer)
//! - Copy mode navigation and selection
//! - Key table bindings (copy-mode-vi, copy-mode-emacs)
//! - Mouse event parsing
//! - Layout pane_at coordinate lookup

#![cfg(test)]

use super::test_helpers::MockCommandServer;
use crate::command::{execute_command, CommandResult};

fn exec(
    server: &mut MockCommandServer,
    args: &[&str],
) -> Result<CommandResult, crate::server::ServerError> {
    let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    execute_command(&argv, server)
}

fn output_text(
    result: Result<CommandResult, crate::server::ServerError>,
) -> String {
    match result.unwrap() {
        CommandResult::Output(text) => text,
        other => panic!("expected Output, got {other:?}"),
    }
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
    use crate::copymode::{CopyModeAction, CopyModeState, copy_selection, dispatch_copy_mode_action};
    use rmux_core::grid::cell::GridCell;
    use rmux_core::screen::Screen;
    use rmux_core::screen::selection::SelectionType;
    use rmux_core::style::Style;
    use rmux_core::utf8::Utf8Char;

    fn make_screen(width: u32, height: u32, lines: &[&str]) -> Screen {
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
        kb.process_input(b"\x02");
        let result = kb.process_input(b"[");
        match result {
            Some(crate::keybind::KeyAction::Command(argv)) => {
                assert_eq!(argv, vec!["copy-mode"]);
            }
            _ => panic!("expected Command for copy-mode"),
        }

        // Prefix+] should paste buffer
        kb.process_input(b"\x02");
        let result = kb.process_input(b"]");
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
        assert_eq!(
            pane.copy_mode.as_ref().unwrap().key_table,
            "copy-mode-emacs"
        );
    }
}
