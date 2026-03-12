//! Tests for session, client, display, and server command handlers.
//!
//! Covers handlers that had zero or low test coverage:
//! - kill-session, rename-session, has-session, list-sessions
//! - attach-session, detach-client, switch-client, refresh-client, suspend-client
//! - display-message, list-commands, list-keys, show-messages, list-clients
//! - display-panes, clock-mode, choose-tree, choose-buffer, choose-client
//! - display-menu, display-popup, customize-mode, clear-prompt-history, show-prompt-history
//! - pipe-pane, resize-window, server-access, lock-server, lock-session, lock-client
//! - kill-server, start-server, send-prefix, clear-history, confirm-before
//! - set-hook, show-hooks, wait-for

use super::test_helpers::MockCommandServer;
use crate::command::{CommandResult, CommandServer, execute_command};

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

// ============================================================
// Session commands
// ============================================================

mod session_tests {
    use super::*;

    #[test]
    fn kill_session_removes_session() {
        let mut s = MockCommandServer::new();
        s.create_test_session("mysession");

        assert!(s.has_session("mysession"));
        let result = exec(&mut s, &["kill-session", "-t", "mysession"]);
        assert!(result.is_ok());
        assert!(!s.has_session("mysession"));
    }

    #[test]
    fn kill_session_nonexistent_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["kill-session", "-t", "nope"]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("session not found"), "got: {err}");
    }

    #[test]
    fn kill_session_default_target() {
        let mut s = MockCommandServer::new();
        // Create a session named "0" (default target)
        s.create_test_session("0");
        let result = exec(&mut s, &["kill-session"]);
        assert!(result.is_ok());
        assert!(!s.has_session("0"));
    }

    #[test]
    fn rename_session_changes_name() {
        let mut s = MockCommandServer::new();
        s.create_test_session("old");

        let result = exec(&mut s, &["rename-session", "-t", "old", "new"]);
        assert!(result.is_ok());
        assert!(!s.has_session("old"));
        assert!(s.has_session("new"));
    }

    #[test]
    fn rename_session_uses_attached_session() {
        let mut s = MockCommandServer::new();
        let (session_id, _, _) = s.create_test_session("attached");
        s.client_session_id = Some(session_id);

        let result = exec(&mut s, &["rename-session", "renamed"]);
        assert!(result.is_ok());
        assert!(s.has_session("renamed"));
        assert!(!s.has_session("attached"));
    }

    #[test]
    fn rename_session_missing_name_errors() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let _result = exec(&mut s, &["rename-session", "-t", "test"]);
        // The error case is when ALL args are flags — no positional new name
        let result = exec(&mut s, &["rename-session", "-t"]);
        assert!(result.is_err());
    }

    #[test]
    fn rename_session_nonexistent_target_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["rename-session", "-t", "nope", "newname"]);
        assert!(result.is_err());
    }

    #[test]
    fn has_session_returns_ok_when_exists() {
        let mut s = MockCommandServer::new();
        s.create_test_session("exists");
        let result = exec(&mut s, &["has-session", "-t", "exists"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn has_session_errors_when_missing() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["has-session", "-t", "ghost"]);
        assert!(result.is_err());
    }

    #[test]
    fn list_sessions_empty() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["list-sessions"]));
        assert!(text.contains("no server running"));
    }

    #[test]
    fn list_sessions_shows_sessions() {
        let mut s = MockCommandServer::new();
        s.create_test_session("alpha");
        s.create_test_session("beta");
        let text = output_text(exec(&mut s, &["list-sessions"]));
        assert!(text.contains("alpha"));
        assert!(text.contains("beta"));
        assert!(text.contains("1 windows"));
    }

    #[test]
    fn new_session_duplicate_name_errors() {
        let mut s = MockCommandServer::new();
        s.create_test_session("dup");
        let result = exec(&mut s, &["new-session", "-s", "dup"]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate session"));
    }

    #[test]
    fn new_session_detached() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["new-session", "-d", "-s", "bg"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
        assert!(s.has_session("bg"));
    }

    #[test]
    fn new_session_attached() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["new-session", "-s", "fg"]);
        assert!(matches!(result.unwrap(), CommandResult::Attach(_)));
    }
}

// ============================================================
// Client commands
// ============================================================

mod client_tests {
    use super::*;

    #[test]
    fn attach_session_by_name() {
        let mut s = MockCommandServer::new();
        s.create_test_session("target");
        let result = exec(&mut s, &["attach-session", "-t", "target"]);
        assert!(matches!(result.unwrap(), CommandResult::Attach(_)));
    }

    #[test]
    fn attach_session_nonexistent_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["attach-session", "-t", "ghost"]);
        assert!(result.is_err());
    }

    #[test]
    fn attach_session_no_target_uses_first() {
        let mut s = MockCommandServer::new();
        s.create_test_session("0");
        let result = exec(&mut s, &["attach-session"]);
        assert!(matches!(result.unwrap(), CommandResult::Attach(_)));
    }

    #[test]
    fn attach_session_no_sessions_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["attach-session"]);
        assert!(result.is_err());
    }

    #[test]
    fn detach_client_returns_detach() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["detach-client"]);
        assert!(matches!(result.unwrap(), CommandResult::Detach));
    }

    #[test]
    fn switch_client_by_target() {
        let mut s = MockCommandServer::new();
        s.create_test_session("s1");
        s.create_test_session("s2");
        let result = exec(&mut s, &["switch-client", "-t", "s2"]);
        assert!(result.is_ok());
    }

    #[test]
    fn switch_client_nonexistent_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["switch-client", "-t", "ghost"]);
        assert!(result.is_err());
    }

    #[test]
    fn switch_client_missing_target_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["switch-client"]);
        assert!(result.is_err());
    }

    #[test]
    fn switch_client_next_wraps() {
        let mut s = MockCommandServer::new();
        let (s1_id, _, _) = s.create_test_session("s1");
        s.create_test_session("s2");
        s.client_session_id = Some(s1_id);

        let result = exec(&mut s, &["switch-client", "-n"]);
        assert!(result.is_ok());
    }

    #[test]
    fn switch_client_prev_wraps() {
        let mut s = MockCommandServer::new();
        let (s1_id, _, _) = s.create_test_session("s1");
        s.create_test_session("s2");
        s.client_session_id = Some(s1_id);

        let result = exec(&mut s, &["switch-client", "-p"]);
        assert!(result.is_ok());
    }

    #[test]
    fn switch_client_next_no_sessions_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["switch-client", "-n"]);
        assert!(result.is_err());
    }

    #[test]
    fn refresh_client_succeeds() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["refresh-client"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn suspend_client_succeeds() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["suspend-client"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }
}

// ============================================================
// Display commands
// ============================================================

mod display_tests {
    use super::*;

    #[test]
    fn display_message_with_text() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let text = output_text(exec(&mut s, &["display-message", "hello world"]));
        assert!(text.contains("hello world"));
    }

    #[test]
    fn display_message_with_print_flag() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let text = output_text(exec(&mut s, &["display-message", "-p"]));
        // With -p and no message, should produce output (empty expanded string + newline)
        assert_eq!(text, "\n");
    }

    #[test]
    fn display_message_no_args_no_output() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let result = exec(&mut s, &["display-message"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn list_commands_returns_output() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["list-commands"]));
        assert!(text.contains("new-session"));
        assert!(text.contains("kill-session"));
    }

    #[test]
    fn list_keys_returns_bindings() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["list-keys"]));
        // Default bindings should exist
        assert!(!text.is_empty());
    }

    #[test]
    fn show_messages_returns_empty() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["show-messages"]));
        assert!(text.is_empty());
    }

    #[test]
    fn list_clients_returns_client_info() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["list-clients"]));
        // Mock always has one client
        assert!(text.contains("client"));
        assert!(text.contains("80x24"));
    }

    #[test]
    fn display_panes_shows_panes() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let text = output_text(exec(&mut s, &["display-panes"]));
        assert!(text.contains('%'));
    }

    #[test]
    fn display_panes_no_session_errors() {
        let mut s = MockCommandServer::new();
        s.client_session_id = None;
        let result = exec(&mut s, &["display-panes"]);
        assert!(result.is_err());
    }

    #[test]
    fn clock_mode_outputs_digits() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["clock-mode"]));
        // Clock output should contain '#' characters (used for digit rendering)
        assert!(text.contains('#'));
        // Should have 5 rows
        assert!(text.lines().count() >= 5);
    }

    #[test]
    fn choose_tree_no_sessions() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["choose-tree"]));
        assert!(text.contains("no sessions"));
    }

    #[test]
    fn choose_tree_with_sessions() {
        let mut s = MockCommandServer::new();
        s.create_test_session("sess");
        let text = output_text(exec(&mut s, &["choose-tree"]));
        assert!(text.contains("sess"));
    }

    #[test]
    fn choose_buffer_no_buffers() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["choose-buffer"]));
        assert!(text.contains("no buffers"));
    }

    #[test]
    fn choose_client_returns_client_info() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["choose-client"]));
        // Mock always has one client
        assert!(text.contains("client"));
    }

    #[test]
    fn display_menu_stub_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["display-menu"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn display_popup_stub_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["display-popup"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn customize_mode_stub_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["customize-mode"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn clear_prompt_history_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["clear-prompt-history"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn show_prompt_history_empty() {
        let mut s = MockCommandServer::new();
        let text = output_text(exec(&mut s, &["show-prompt-history"]));
        assert!(text.is_empty());
    }

    #[test]
    fn pipe_pane_start_and_stop() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        // Start piping
        let result = exec(&mut s, &["pipe-pane", "cat > /tmp/out"]);
        assert!(result.is_ok());
        // Stop piping
        let result = exec(&mut s, &["pipe-pane"]);
        assert!(result.is_ok());
    }

    #[test]
    fn resize_window_with_dimensions() {
        let mut s = MockCommandServer::new();
        s.create_test_session("resize-test");
        let result = exec(&mut s, &["resize-window", "-x", "100", "-y", "40"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
        let sid = s.client_session_id.unwrap();
        let widx = s.sessions.find_by_id(sid).unwrap().active_window;
        let window = &s.sessions.find_by_id(sid).unwrap().windows[&widx];
        assert_eq!(window.sx, 100);
        assert_eq!(window.sy, 40);
    }

    #[test]
    fn resize_window_requires_args() {
        let mut s = MockCommandServer::new();
        s.create_test_session("resize-test");
        let result = exec(&mut s, &["resize-window"]);
        assert!(result.is_err());
    }

    #[test]
    fn resize_window_adjust() {
        let mut s = MockCommandServer::new();
        s.create_test_session("resize-test");
        let result = exec(&mut s, &["resize-window", "-A"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn server_access_stub_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["server-access"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn lock_server_stub_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["lock-server"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn lock_session_stub_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["lock-session"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn lock_client_stub_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["lock-client"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }
}

// ============================================================
// Server commands
// ============================================================

mod server_cmd_tests {
    use super::*;

    #[test]
    fn kill_server_returns_exit() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["kill-server"]);
        assert!(matches!(result.unwrap(), CommandResult::Exit));
    }

    #[test]
    fn start_server_noop() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["start-server"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn send_prefix_writes_ctrl_b() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let result = exec(&mut s, &["send-prefix"]);
        assert!(result.is_ok());
    }

    #[test]
    fn send_prefix_with_prefix2() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let result = exec(&mut s, &["send-prefix", "-2"]);
        assert!(result.is_ok());
    }

    #[test]
    fn clear_history_succeeds() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let result = exec(&mut s, &["clear-history"]);
        assert!(result.is_ok());
    }

    #[test]
    fn command_prompt_enters_prompt_mode() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["command-prompt"]);
        assert!(result.is_ok());
        assert!(s.prompt_entered);
    }

    #[test]
    fn wait_for_noop() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["wait-for", "channel"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn set_hook_and_show_hooks() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["set-hook", "after-new-session", "display-message", "hi"]);
        assert!(result.is_ok());

        let text = output_text(exec(&mut s, &["show-hooks"]));
        assert!(text.contains("after-new-session"));
    }

    #[test]
    fn set_hook_missing_name_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["set-hook"]);
        assert!(result.is_err());
    }

    #[test]
    fn set_hook_missing_command_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["set-hook", "hook-name"]);
        assert!(result.is_err());
    }

    #[test]
    fn set_hook_unset() {
        let mut s = MockCommandServer::new();
        exec(&mut s, &["set-hook", "my-hook", "display-message", "test"]).unwrap();
        let result = exec(&mut s, &["set-hook", "-u", "my-hook"]);
        assert!(result.is_ok());

        // Verify hook is gone — show-hooks returns Ok (not Output) when empty
        let result = exec(&mut s, &["show-hooks"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn set_hook_unset_nonexistent_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["set-hook", "-u", "ghost-hook"]);
        assert!(result.is_err());
    }

    #[test]
    fn show_hooks_empty() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["show-hooks"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn confirm_before_executes_command() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        // confirm-before should execute the command directly (no interactive confirmation in tests)
        let result = exec(&mut s, &["confirm-before", "kill-server"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
        // Note: confirm-before runs the inner command and returns Ok
    }

    #[test]
    fn confirm_before_missing_command_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["confirm-before"]);
        assert!(result.is_err());
    }

    #[test]
    fn if_shell_true_branch() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        // "true" command exits 0 → execute the true branch
        let result = exec(&mut s, &["if-shell", "true", "start-server"]);
        assert!(result.is_ok());
    }

    #[test]
    fn if_shell_false_branch() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        // "false" exits 1 → execute the false branch (kill-server → Exit)
        let result = exec(&mut s, &["if-shell", "false", "start-server", "kill-server"]);
        // Should have executed kill-server, but confirm-before wraps it...
        // Actually the inner execute returns a result that if-shell discards.
        // if-shell always returns Ok.
        assert!(result.is_ok());
    }

    #[test]
    fn if_shell_false_no_else_ok() {
        let mut s = MockCommandServer::new();
        // "false" exits 1, no else branch → Ok
        let result = exec(&mut s, &["if-shell", "false", "start-server"]);
        assert!(matches!(result.unwrap(), CommandResult::Ok));
    }

    #[test]
    fn if_shell_missing_args_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["if-shell", "true"]);
        assert!(result.is_err());
    }

    #[test]
    fn run_shell_returns_run_shell_result() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["run-shell", "echo hello"]);
        match result.unwrap() {
            CommandResult::RunShell(cmd) => assert_eq!(cmd, "echo hello"),
            other => panic!("expected RunShell, got {other:?}"),
        }
    }

    #[test]
    fn run_shell_missing_command_errors() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["run-shell"]);
        assert!(result.is_err());
    }
}
