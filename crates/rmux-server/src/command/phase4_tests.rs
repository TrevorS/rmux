//! Comprehensive e2e tests for Phase 4 commands.
//!
//! Tests exercise command handlers through the MockCommandServer, covering:
//! - set-option / show-options
//! - bind-key / unbind-key
//! - source-file (config parsing + execution)
//! - send-keys
//! - capture-pane
//! - resize-pane
//! - swap-pane / break-pane / join-pane / last-pane / respawn-pane
//! - swap-window / move-window / rotate-window / select-layout / respawn-window
//! - run-shell / command-prompt / display-message
//! - list-keys / list-commands / list-clients

use super::test_helpers::MockCommandServer;
use crate::command::{CommandResult, CommandServer, execute_command};

/// Helper to execute a command from a string slice of arguments.
fn exec(
    server: &mut MockCommandServer,
    args: &[&str],
) -> Result<CommandResult, crate::server::ServerError> {
    let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    execute_command(&argv, server)
}

/// Helper: unwrap an Output result and return the text.
fn output_text(result: Result<CommandResult, crate::server::ServerError>) -> String {
    match result.unwrap() {
        CommandResult::Output(text) => text,
        other => panic!("expected Output, got {other:?}"),
    }
}

// ============================================================
// set-option / show-options
// ============================================================

mod options_tests {
    use super::*;

    #[test]
    fn set_and_show_server_option() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Set a server-level option
        let result = exec(&mut s, &["set-option", "-g", "history-limit", "5000"]);
        assert!(result.is_ok());

        // Show it back
        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("history-limit 5000"), "output was: {output}");
    }

    #[test]
    fn set_option_string_value() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-option", "-g", "set-clipboard", "internal"]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "set-clipboard"]));
        assert!(output.contains("set-clipboard internal"), "output was: {output}");
    }

    #[test]
    fn set_option_status_bg_alias() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-option", "-g", "status-bg", "red"]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "status-style"]));
        assert!(output.contains("status-style bg=red"), "output was: {output}");
    }

    #[test]
    fn set_option_status_fg_alias() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-option", "-g", "status-fg", "white"]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "status-style"]));
        assert!(output.contains("status-style fg=white"), "output was: {output}");
    }

    #[test]
    fn set_option_flag_value() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-option", "-g", "focus-events", "on"]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "focus-events"]));
        assert!(output.contains("focus-events on"), "output was: {output}");
    }

    #[test]
    fn show_options_all_server() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let output = output_text(exec(&mut s, &["show-options", "-g"]));
        // Should contain default server options
        assert!(output.contains("escape-time"), "output was: {output}");
        assert!(output.contains("buffer-limit"), "output was: {output}");
    }

    #[test]
    fn show_options_empty_when_no_filter_match() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let output = output_text(exec(&mut s, &["show-options", "-g", "nonexistent-option"]));
        assert!(output.is_empty(), "expected empty, got: {output}");
    }

    #[test]
    fn set_option_missing_key() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["set-option", "-g"]);
        assert!(result.is_err());
    }

    #[test]
    fn set_option_missing_value() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["set-option", "-g", "history-limit"]);
        assert!(result.is_err());
    }

    #[test]
    fn set_session_option_with_target() {
        let mut s = MockCommandServer::new();
        let (sid, _, _) = s.create_test_session("mysession");

        exec(&mut s, &["set-option", "-t", "mysession", "base-index", "1"]).unwrap();

        let session = s.sessions.find_by_id(sid).unwrap();
        let val = session.options.get("base-index");
        assert!(val.is_some());
    }

    #[test]
    fn set_window_option() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");

        exec(&mut s, &["set-option", "-w", "mode-keys", "vi"]).unwrap();

        let session = s.sessions.find_by_id(sid).unwrap();
        let window = session.windows.get(&widx).unwrap();
        let val = window.options.get("mode-keys");
        assert!(val.is_some());
    }

    #[test]
    fn set_alias_works() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // "set" is an alias for "set-option"
        let result = exec(&mut s, &["set", "-g", "history-limit", "3000"]);
        assert!(result.is_ok());

        let output = output_text(exec(&mut s, &["show", "-g", "history-limit"]));
        assert!(output.contains("history-limit 3000"));
    }
}

// ============================================================
// bind-key / unbind-key
// ============================================================

mod keybinding_tests {
    use super::*;

    #[test]
    fn bind_key_prefix_table() {
        let mut s = MockCommandServer::new();

        exec(&mut s, &["bind-key", "z", "kill-session"]).unwrap();

        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z') && b.contains("kill-session")));
    }

    #[test]
    fn bind_key_explicit_table() {
        let mut s = MockCommandServer::new();

        exec(&mut s, &["bind-key", "-T", "prefix", "z", "kill-session"]).unwrap();

        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z') && b.contains("kill-session")));
    }

    #[test]
    fn bind_key_root_table() {
        let mut s = MockCommandServer::new();

        exec(&mut s, &["bind-key", "-n", "F5", "new-window"]).unwrap();

        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains("root") && b.contains("new-window")));
    }

    #[test]
    fn unbind_key() {
        let mut s = MockCommandServer::new();

        // Default binding 'd' = detach-client exists
        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains(" d ") && b.contains("detach")));

        exec(&mut s, &["unbind-key", "d"]).unwrap();

        let bindings = s.keybindings.list_bindings();
        assert!(!bindings.iter().any(|b| b.contains(" d ") && b.contains("detach")));
    }

    #[test]
    fn unbind_nonexistent_key() {
        let mut s = MockCommandServer::new();

        let result = exec(&mut s, &["unbind-key", "Z"]);
        assert!(result.is_err(), "should error when unbinding unbound key");
    }

    #[test]
    fn bind_key_with_args() {
        let mut s = MockCommandServer::new();

        exec(&mut s, &["bind-key", "m", "select-window", "-t", "3"]).unwrap();

        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('m') && b.contains("select-window")));
    }

    #[test]
    fn bind_key_missing_key() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["bind-key"]);
        assert!(result.is_err());
    }

    #[test]
    fn bind_key_missing_command() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["bind-key", "z"]);
        assert!(result.is_err());
    }

    #[test]
    fn bind_key_with_repeat_flag() {
        let mut s = MockCommandServer::new();
        exec(&mut s, &["bind-key", "-r", "z", "resize-pane", "-U"]).unwrap();
        let bindings = s.keybindings.list_bindings();
        let z_binding = bindings.iter().find(|b| b.contains("resize-pane") && b.contains('z'));
        assert!(z_binding.is_some(), "expected binding for z");
        assert!(z_binding.unwrap().contains(" -r"), "expected -r flag");
    }

    #[test]
    fn bind_alias_works() {
        let mut s = MockCommandServer::new();
        exec(&mut s, &["bind", "z", "kill-session"]).unwrap();
        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z') && b.contains("kill-session")));
    }

    #[test]
    fn unbind_alias_works() {
        let mut s = MockCommandServer::new();
        exec(&mut s, &["bind", "z", "kill-session"]).unwrap();
        exec(&mut s, &["unbind", "z"]).unwrap();
        let bindings = s.keybindings.list_bindings();
        assert!(!bindings.iter().any(|b| b.contains(" z ") && b.contains("kill-session")));
    }
}

// ============================================================
// source-file / config parsing
// ============================================================

mod config_tests {
    use super::*;

    #[test]
    fn source_file_executes_commands() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Write a temp config file
        let tmp = "/tmp/rmux_test_config.conf";
        std::fs::write(tmp, "set-option -g history-limit 9999\n").unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("9999"), "config not applied: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_with_comments() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_config_comments.conf";
        std::fs::write(
            tmp,
            "# This is a comment\nset-option -g history-limit 4000\n# Another comment\n",
        )
        .unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("4000"));

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_with_semicolons() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_config_semi.conf";
        std::fs::write(tmp, "set -g history-limit 7000 ; set -g escape-time 100\n").unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("7000"), "output: {output}");

        let output = output_text(exec(&mut s, &["show-options", "-g", "escape-time"]));
        assert!(output.contains("100"), "output: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_missing_path() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["source-file"]);
        assert!(result.is_err());
    }

    #[test]
    fn source_file_nonexistent() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["source-file", "/tmp/rmux_nonexistent_file.conf"]);
        assert!(result.is_err());
    }

    #[test]
    fn source_file_with_bind_key() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_config_bind.conf";
        std::fs::write(tmp, "bind-key z kill-session\n").unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();

        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z') && b.contains("kill-session")));

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_alias_works() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_source_alias.conf";
        std::fs::write(tmp, "set -g history-limit 1234\n").unwrap();

        exec(&mut s, &["source", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("1234"));

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_with_errors_returns_output() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_config_errors.conf";
        std::fs::write(tmp, "nonexistent-command foo bar\n").unwrap();

        let result = exec(&mut s, &["source-file", tmp]).unwrap();
        match result {
            CommandResult::Output(text) => {
                assert!(text.contains("unknown command"), "error output: {text}");
            }
            CommandResult::Ok => {} // Also acceptable if errors are swallowed
            _ => panic!("unexpected result"),
        }

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_with_if_true() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_if_true.conf";
        std::fs::write(tmp, "%if 1\nset -g history-limit 5555\n%endif\n").unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("5555"), "config not applied: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_with_if_false() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_if_false.conf";
        std::fs::write(tmp, "%if 0\nset -g history-limit 6666\n%endif\nset -g escape-time 42\n")
            .unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        // history-limit should NOT have been set to 6666
        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(!output.contains("6666"), "false branch was applied: {output}");
        // escape-time should be set (outside the %if)
        let output = output_text(exec(&mut s, &["show-options", "-g", "escape-time"]));
        assert!(output.contains("42"), "output: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_with_hidden_and_var_expansion() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_hidden.conf";
        std::fs::write(
            tmp,
            "%hidden MODULE_NAME=\"session\"\nset -g @catppuccin_${MODULE_NAME}_color blue\n",
        )
        .unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        let output =
            output_text(exec(&mut s, &["show-options", "-g", "@catppuccin_session_color"]));
        assert!(output.contains("blue"), "var expansion failed: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_with_line_continuation() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_continuation.conf";
        std::fs::write(tmp, "set -g \\\nhistory-limit \\\n8888\n").unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("8888"), "continuation failed: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_if_else_elif_chain() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_elif.conf";
        std::fs::write(
            tmp,
            "%if 0\nset -g @result first\n%elif 0\nset -g @result second\n%else\nset -g @result third\n%endif\n",
        )
        .unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "@result"]));
        assert!(output.contains("third"), "elif chain failed: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_catppuccin_module_simulation() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_catppuccin_module.conf";
        std::fs::write(
            tmp,
            "\
%hidden MODULE_NAME=\"session\"
%hidden MODULE_COLOR=\"blue\"
set -ogq @catppuccin_${MODULE_NAME}_color \"${MODULE_COLOR}\"
set -ogq @catppuccin_${MODULE_NAME}_text \" #{${MODULE_NAME}_name}\"
%if 1
%hidden SHOW_ICON=\"yes\"
%endif
set -ogq @catppuccin_${MODULE_NAME}_icon \"${SHOW_ICON}\"
",
        )
        .unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        let output =
            output_text(exec(&mut s, &["show-options", "-g", "@catppuccin_session_color"]));
        assert!(output.contains("blue"), "module color: {output}");
        let output = output_text(exec(&mut s, &["show-options", "-g", "@catppuccin_session_text"]));
        assert!(output.contains("#{session_name}"), "module text: {output}");
        let output = output_text(exec(&mut s, &["show-options", "-g", "@catppuccin_session_icon"]));
        assert!(output.contains("yes"), "module icon: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_format_flag() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Write a config file, then source it using -F with a format-expanded path
        let tmp = "/tmp/rmux_test_source_f.conf";
        std::fs::write(tmp, "set -g @sourced_via_f yes\n").unwrap();

        // Set a user option that the -F expansion can reference
        exec(&mut s, &["set", "-g", "@test_path", tmp]).unwrap();

        // source-file -F "#{@test_path}"
        exec(&mut s, &["source-file", "-F", "#{@test_path}"]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "@sourced_via_f"]));
        assert!(output.contains("yes"), "source -F failed: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_quiet_flag() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // -q should suppress file-not-found errors
        let result = exec(&mut s, &["source-file", "-q", "/tmp/rmux_nonexistent_12345.conf"]);
        assert!(result.is_ok(), "source-file -q should not error on missing file");
    }

    #[test]
    fn source_file_sets_current_file() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Write a config that reads current_file via format expansion
        let tmp = "/tmp/rmux_test_current_file.conf";
        std::fs::write(tmp, "set -gF @loaded_from \"#{current_file}\"\n").unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "@loaded_from"]));
        // Should contain the canonical path to our temp file
        assert!(output.contains("rmux_test_current_file.conf"), "current_file not set: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn source_file_dirname_current_file() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Write a config that captures #{d:current_file}
        let tmp = "/tmp/rmux_test_dirname.conf";
        std::fs::write(tmp, "set -gF @dir \"#{d:current_file}\"\n").unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "@dir"]));
        // dirname of /tmp/rmux_test_dirname.conf is /tmp (or /private/tmp on macOS)
        assert!(output.contains("tmp"), "dirname failed: {output}");

        std::fs::remove_file(tmp).ok();
    }
}

// ============================================================
// P3: format variables
// ============================================================

mod format_var_tests {
    use super::*;

    #[test]
    fn version_variable_available() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_version.conf";
        std::fs::write(tmp, "set -gF @ver \"#{version}\"\n").unwrap();
        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "@ver"]));
        assert!(output.contains("3.6"), "version should be 3.6.x, got: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn version_comparison_compat() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Catppuccin checks #{>=:#{version},3.4}
        let tmp = "/tmp/rmux_test_version_cmp.conf";
        std::fs::write(tmp, "set -gF @ver_check \"#{>=:#{version},3.4}\"\n").unwrap();
        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "@ver_check"]));
        assert!(output.contains('1'), "3.6 >= 3.4 should be true, got: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn client_prefix_variable() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_client_prefix.conf";
        std::fs::write(tmp, "set -gF @pfx \"#{client_prefix}\"\n").unwrap();
        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "@pfx"]));
        // Not in prefix mode, should be 0
        assert!(output.contains('0'), "client_prefix should be 0, got: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn window_flags_available() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_win_flags.conf";
        std::fs::write(
            tmp,
            "set -gF @zoomed \"#{window_zoomed_flag}\"\nset -gF @last \"#{window_last_flag}\"\n",
        )
        .unwrap();
        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "@zoomed"]));
        assert!(output.contains('0'), "zoomed should be 0, got: {output}");

        let output = output_text(exec(&mut s, &["show-options", "-g", "@last"]));
        assert!(output.contains('0'), "last flag should be 0, got: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn pane_synchronized_variable() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let tmp = "/tmp/rmux_test_pane_sync.conf";
        std::fs::write(tmp, "set -gF @sync \"#{pane_synchronized}\"\n").unwrap();
        exec(&mut s, &["source-file", tmp]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "@sync"]));
        assert!(output.contains('0'), "sync should be 0, got: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn zoom_pane_toggle() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");
        let sid = s.client_session_id().unwrap();

        // Split to have 2 panes
        exec(&mut s, &["split-window"]).unwrap();

        // Initially not zoomed
        let window = &s.sessions.find_by_id(sid).unwrap().windows[&0];
        assert!(window.zoomed_pane.is_none());

        // Zoom active pane
        exec(&mut s, &["resize-pane", "-Z"]).unwrap();
        let window = &s.sessions.find_by_id(sid).unwrap().windows[&0];
        assert!(window.zoomed_pane.is_some());

        // Toggle again to unzoom
        exec(&mut s, &["resize-pane", "-Z"]).unwrap();
        let window = &s.sessions.find_by_id(sid).unwrap().windows[&0];
        assert!(window.zoomed_pane.is_none());
    }

    #[test]
    fn zoom_pane_reflects_in_format() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Split so we can zoom
        exec(&mut s, &["split-window"]).unwrap();

        // Check zoomed flag is 0 before zoom
        let tmp = "/tmp/rmux_test_zoom_fmt.conf";
        std::fs::write(tmp, "set -gF @z \"#{window_zoomed_flag}\"\n").unwrap();
        exec(&mut s, &["source-file", tmp]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "@z"]));
        assert!(output.contains('0'), "should be 0 before zoom, got: {output}");

        // Zoom
        exec(&mut s, &["resize-pane", "-Z"]).unwrap();

        // Re-expand format — should be 1 now
        exec(&mut s, &["source-file", tmp]).unwrap();
        let output = output_text(exec(&mut s, &["show-options", "-g", "@z"]));
        assert!(output.contains('1'), "should be 1 after zoom, got: {output}");

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn zoom_invalid_pane_fails() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Try to zoom a nonexistent pane — should return Err
        let result = exec(&mut s, &["resize-pane", "-Z", "-t", "%999"]);
        assert!(result.is_err());
    }
}

// ============================================================
// send-keys
// ============================================================

mod send_keys_tests {
    use super::*;

    #[test]
    fn send_keys_basic() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // send-keys writes to PTY; in mock it returns Ok
        let result = exec(&mut s, &["send-keys", "ls"]);
        assert!(result.is_ok());
    }

    #[test]
    fn send_keys_with_enter() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["send-keys", "ls", "Enter"]);
        assert!(result.is_ok());
    }

    #[test]
    fn send_keys_literal_mode() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["send-keys", "-l", "Enter"]);
        assert!(result.is_ok());
    }

    #[test]
    fn send_keys_missing_keys() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["send-keys"]);
        assert!(result.is_err(), "send-keys with no keys should error");
    }

    #[test]
    fn send_keys_with_target() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["send-keys", "-t", "test", "hello"]);
        assert!(result.is_ok());
    }

    #[test]
    fn send_keys_with_session_window_target() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["send-keys", "-t", "test:0", "hello"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// capture-pane
// ============================================================

mod capture_pane_tests {
    use super::*;

    #[test]
    fn capture_pane_empty() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let output = output_text(exec(&mut s, &["capture-pane", "-p"]));
        // Should return lines (even if empty)
        assert!(output.contains('\n'));
    }

    #[test]
    fn capture_pane_with_content() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pid) = s.create_test_session("test");

        // Write some content to the pane's screen directly
        let session = s.sessions.find_by_id_mut(sid).unwrap();
        let window = session.windows.get_mut(&widx).unwrap();
        let pane = window.panes.get_mut(&pid).unwrap();
        pane.process_input(b"Hello World");

        let output = output_text(exec(&mut s, &["capture-pane", "-p"]));
        assert!(output.contains("Hello World"), "capture output: {output}");
    }

    #[test]
    fn capture_pane_alias() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["capturep", "-p"]);
        assert!(result.is_ok());
    }

    #[test]
    fn capture_pane_without_p_flag() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Without -p, still returns output (our implementation always does)
        let result = exec(&mut s, &["capture-pane"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// resize-pane
// ============================================================

mod resize_pane_tests {
    use super::*;

    #[test]
    fn resize_pane_right() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["resize-pane", "-R", "5"]);
        assert!(result.is_ok());
    }

    #[test]
    fn resize_pane_left() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["resize-pane", "-L"]);
        assert!(result.is_ok());
    }

    #[test]
    fn resize_pane_up() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["resize-pane", "-U", "3"]);
        assert!(result.is_ok());
    }

    #[test]
    fn resize_pane_down() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["resize-pane", "-D"]);
        assert!(result.is_ok());
    }

    #[test]
    fn resize_pane_alias() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["resizep", "-R"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// swap-pane
// ============================================================

mod swap_pane_tests {
    use super::*;

    #[test]
    fn swap_pane_down() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pane1) = s.create_test_session("test");
        let pane2 = s.add_pane_to_window(sid, widx, false);

        // Get original positions
        let session = s.sessions.find_by_id(sid).unwrap();
        let window = session.windows.get(&widx).unwrap();
        let p1_xoff = window.panes[&pane1].xoff;
        let p2_xoff = window.panes[&pane2].xoff;

        // Set active to pane1
        s.sessions.find_by_id_mut(sid).unwrap().windows.get_mut(&widx).unwrap().active_pane = pane1;

        let result = exec(&mut s, &["swap-pane", "-D"]);
        assert!(result.is_ok());

        // After swap, positions should be exchanged
        let session = s.sessions.find_by_id(sid).unwrap();
        let window = session.windows.get(&widx).unwrap();
        assert_eq!(window.panes[&pane1].xoff, p2_xoff);
        assert_eq!(window.panes[&pane2].xoff, p1_xoff);
    }

    #[test]
    fn swap_pane_up() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _pane1) = s.create_test_session("test");
        let pane2 = s.add_pane_to_window(sid, widx, false);

        // Set active to pane2
        s.sessions.find_by_id_mut(sid).unwrap().windows.get_mut(&widx).unwrap().active_pane = pane2;

        let result = exec(&mut s, &["swap-pane", "-U"]);
        assert!(result.is_ok());
    }

    #[test]
    fn swap_pane_alias() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        s.add_pane_to_window(sid, widx, false);

        let result = exec(&mut s, &["swapp", "-D"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// break-pane
// ============================================================

mod break_pane_tests {
    use super::*;

    #[test]
    fn break_pane_creates_new_window() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _pane1) = s.create_test_session("test");
        let _pane2 = s.add_pane_to_window(sid, widx, true);

        let session = s.sessions.find_by_id(sid).unwrap();
        let initial_windows = session.windows.len();
        let initial_panes = session.windows[&widx].pane_count();
        assert_eq!(initial_panes, 2);

        let result = exec(&mut s, &["break-pane"]);
        assert!(result.is_ok());

        let session = s.sessions.find_by_id(sid).unwrap();
        assert_eq!(session.windows.len(), initial_windows + 1, "should have one more window");
        // Original window should have one fewer pane
        assert_eq!(session.windows[&widx].pane_count(), 1);
    }

    #[test]
    fn break_pane_single_pane_fails() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["break-pane"]);
        assert!(result.is_err(), "cannot break with only one pane");
    }

    #[test]
    fn break_pane_alias() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        s.add_pane_to_window(sid, widx, true);

        let result = exec(&mut s, &["breakp"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// join-pane
// ============================================================

mod join_pane_tests {
    use super::*;

    #[test]
    fn join_pane_moves_pane_to_another_window() {
        let mut s = MockCommandServer::new();
        let (sid, widx0, _) = s.create_test_session("test");
        let (widx1, _pane1) = s.add_window_to_session(sid, "second");

        // widx1 has 1 pane, widx0 has 1 pane
        // Set client's current context to widx1
        s.sessions.find_by_id_mut(sid).unwrap().active_window = widx1;

        let panes_before_w0 = s.sessions.find_by_id(sid).unwrap().windows[&widx0].pane_count();

        // Join current pane into window 0
        let result = exec(&mut s, &["join-pane", "-t", &format!("test:{widx0}")]);
        assert!(result.is_ok(), "join-pane failed: {result:?}");

        let session = s.sessions.find_by_id(sid).unwrap();
        // Window 1 should be gone (had only 1 pane that was moved)
        assert!(!session.windows.contains_key(&widx1), "source window should be removed");
        // Window 0 should have one more pane
        assert_eq!(session.windows[&widx0].pane_count(), panes_before_w0 + 1);
    }

    #[test]
    fn join_pane_alias() {
        let mut s = MockCommandServer::new();
        let (sid, _, _) = s.create_test_session("test");
        let (widx1, _) = s.add_window_to_session(sid, "second");
        s.sessions.find_by_id_mut(sid).unwrap().active_window = widx1;

        let result = exec(&mut s, &["joinp", "-t", "test:0"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// last-pane
// ============================================================

mod last_pane_tests {
    use super::*;

    #[test]
    fn last_pane_switches_back() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pane1) = s.create_test_session("test");
        let pane2 = s.split_window(sid, widx, true, "/tmp", None, None).unwrap();

        // pane2 is now active (set by split_window), pane1 is last
        let window = s.sessions.find_by_id(sid).unwrap().windows.get(&widx).unwrap();
        assert_eq!(window.active_pane, pane2);
        assert_eq!(window.last_active_pane, Some(pane1));

        let result = exec(&mut s, &["last-pane"]);
        assert!(result.is_ok());

        let window = s.sessions.find_by_id(sid).unwrap().windows.get(&widx).unwrap();
        assert_eq!(window.active_pane, pane1);
        assert_eq!(window.last_active_pane, Some(pane2));
    }

    #[test]
    fn last_pane_no_previous_fails() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["last-pane"]);
        assert!(result.is_err(), "no last pane should error");
    }

    #[test]
    fn last_pane_alias() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        s.split_window(sid, widx, true, "/tmp", None, None).unwrap();

        let result = exec(&mut s, &["lastp"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// respawn-pane
// ============================================================

mod respawn_pane_tests {
    use super::*;

    #[test]
    fn respawn_pane_resets_screen() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pid) = s.create_test_session("test");

        // Write content to the screen
        {
            let session = s.sessions.find_by_id_mut(sid).unwrap();
            let window = session.windows.get_mut(&widx).unwrap();
            let pane = window.panes.get_mut(&pid).unwrap();
            pane.process_input(b"This is some content");
        }

        let result = exec(&mut s, &["respawn-pane"]);
        assert!(result.is_ok());

        // After respawn, screen should be reset
        let output = output_text(exec(&mut s, &["capture-pane", "-p"]));
        assert!(!output.contains("This is some content"), "screen should be cleared");
    }

    #[test]
    fn respawn_pane_alias() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["respawnp"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// swap-window
// ============================================================

mod swap_window_tests {
    use super::*;

    #[test]
    fn swap_window_exchanges_windows() {
        let mut s = MockCommandServer::new();
        let (sid, widx0, _) = s.create_test_session("test");
        let (widx1, _) = s.add_window_to_session(sid, "second");

        let session = s.sessions.find_by_id(sid).unwrap();
        let name0 = session.windows[&widx0].name.clone();
        let name1 = session.windows[&widx1].name.clone();

        exec(&mut s, &["swap-window", "-s", &widx0.to_string(), "-t", &format!("test:{widx1}")])
            .unwrap();

        let session = s.sessions.find_by_id(sid).unwrap();
        assert_eq!(session.windows[&widx0].name, name1);
        assert_eq!(session.windows[&widx1].name, name0);
    }

    #[test]
    fn swap_window_nonexistent_fails() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["swap-window", "-s", "0", "-t", "test:99"]);
        assert!(result.is_err());
    }

    #[test]
    fn swap_window_alias() {
        let mut s = MockCommandServer::new();
        let (sid, _, _) = s.create_test_session("test");
        s.add_window_to_session(sid, "second");

        let result = exec(&mut s, &["swapw", "-s", "0", "-t", "test:1"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// move-window
// ============================================================

mod move_window_tests {
    use super::*;

    #[test]
    fn move_window_between_sessions() {
        let mut s = MockCommandServer::new();
        let (sid1, widx0, _) = s.create_test_session("src");
        let (_sid2, _, _) = s.create_test_session("dst");

        // Add second window to src so it's not empty after move
        s.add_window_to_session(sid1, "extra");

        let session1 = s.sessions.find_by_id(sid1).unwrap();
        let win_count_before = session1.windows.len();
        assert!(win_count_before >= 2);

        let result = exec(&mut s, &["move-window", "-s", &format!("src:{widx0}"), "-t", "dst:5"]);
        assert!(result.is_ok(), "move-window failed: {result:?}");

        let session1 = s.sessions.find_by_id(sid1).unwrap();
        assert_eq!(session1.windows.len(), win_count_before - 1);
    }

    #[test]
    fn move_window_alias() {
        let mut s = MockCommandServer::new();
        let (sid1, _, _) = s.create_test_session("src");
        s.add_window_to_session(sid1, "extra");
        s.create_test_session("dst");

        let result = exec(&mut s, &["movew", "-s", "src:0", "-t", "dst:5"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// rotate-window
// ============================================================

mod rotate_window_tests {
    use super::*;

    #[test]
    fn rotate_window_changes_active() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pane1) = s.create_test_session("test");
        let _pane2 = s.add_pane_to_window(sid, widx, true);

        s.sessions.find_by_id_mut(sid).unwrap().windows.get_mut(&widx).unwrap().active_pane = pane1;

        let result = exec(&mut s, &["rotate-window"]);
        assert!(result.is_ok());

        let window = s.sessions.find_by_id(sid).unwrap().windows.get(&widx).unwrap();
        // Active pane should have changed
        assert_ne!(window.active_pane, pane1, "active pane should have rotated");
    }

    #[test]
    fn rotate_single_pane_noop() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["rotate-window"]);
        assert!(result.is_ok());
    }

    #[test]
    fn rotate_window_alias() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        s.add_pane_to_window(sid, widx, true);

        let result = exec(&mut s, &["rotatew"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// select-layout
// ============================================================

mod select_layout_tests {
    use super::*;

    #[test]
    fn select_layout_even_horizontal() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        let _pane2 = s.add_pane_to_window(sid, widx, true);

        let result = exec(&mut s, &["select-layout", "even-horizontal"]);
        assert!(result.is_ok());

        // Check that panes have reasonable horizontal layout
        let session = s.sessions.find_by_id(sid).unwrap();
        let window = session.windows.get(&widx).unwrap();
        let layout = window.layout.as_ref().unwrap();
        assert_eq!(layout.cell_type, rmux_core::layout::LayoutType::LeftRight);
    }

    #[test]
    fn select_layout_even_vertical() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        s.add_pane_to_window(sid, widx, false);

        let result = exec(&mut s, &["select-layout", "even-vertical"]);
        assert!(result.is_ok());

        let session = s.sessions.find_by_id(sid).unwrap();
        let window = session.windows.get(&widx).unwrap();
        let layout = window.layout.as_ref().unwrap();
        assert_eq!(layout.cell_type, rmux_core::layout::LayoutType::TopBottom);
    }

    #[test]
    fn select_layout_unknown_fails() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["select-layout", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn select_layout_alias() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        s.add_pane_to_window(sid, widx, true);

        let result = exec(&mut s, &["selectl", "even-horizontal"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// respawn-window
// ============================================================

mod respawn_window_tests {
    use super::*;

    #[test]
    fn respawn_window_resets_active_pane() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pid) = s.create_test_session("test");

        // Write content
        {
            let session = s.sessions.find_by_id_mut(sid).unwrap();
            let window = session.windows.get_mut(&widx).unwrap();
            let pane = window.panes.get_mut(&pid).unwrap();
            pane.process_input(b"Some content here");
        }

        let result = exec(&mut s, &["respawn-window"]);
        assert!(result.is_ok());

        // Screen should be cleared
        let output = output_text(exec(&mut s, &["capture-pane", "-p"]));
        assert!(!output.contains("Some content here"));
    }

    #[test]
    fn respawn_window_alias() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let result = exec(&mut s, &["respawnw"]);
        assert!(result.is_ok());
    }
}

// ============================================================
// run-shell
// ============================================================

mod run_shell_tests {
    use super::*;

    #[test]
    fn run_shell_returns_command() {
        let mut s = MockCommandServer::new();

        let result = exec(&mut s, &["run-shell", "echo hello"]).unwrap();
        match result {
            CommandResult::RunShell(cmd) => {
                assert_eq!(cmd, "echo hello");
            }
            _ => panic!("expected RunShell"),
        }
    }

    #[test]
    fn run_shell_missing_command() {
        let mut s = MockCommandServer::new();

        let result = exec(&mut s, &["run-shell"]);
        assert!(result.is_err());
    }

    #[test]
    fn run_shell_alias() {
        let mut s = MockCommandServer::new();

        let result = exec(&mut s, &["run", "ls -la"]).unwrap();
        match result {
            CommandResult::RunShell(cmd) => {
                assert_eq!(cmd, "ls -la");
            }
            _ => panic!("expected RunShell"),
        }
    }
}

// ============================================================
// command-prompt
// ============================================================

mod command_prompt_tests {
    use super::*;

    #[test]
    fn command_prompt_enters_prompt_mode() {
        let mut s = MockCommandServer::new();

        assert!(!s.prompt_entered);
        let result = exec(&mut s, &["command-prompt"]);
        assert!(result.is_ok());
        assert!(s.prompt_entered, "prompt should be entered");
    }
}

// ============================================================
// display-message
// ============================================================

mod display_message_tests {
    use super::*;

    #[test]
    fn display_message_with_print_flag() {
        let mut s = MockCommandServer::new();

        let output = output_text(exec(&mut s, &["display-message", "-p", "hello world"]));
        assert!(output.contains("hello world"), "output: {output}");
    }

    #[test]
    fn display_message_with_message() {
        let mut s = MockCommandServer::new();

        let result = exec(&mut s, &["display-message", "test message"]).unwrap();
        match result {
            CommandResult::TimedMessage(msg) => assert!(msg.contains("test message")),
            other => panic!("expected TimedMessage, got {other:?}"),
        }
    }

    #[test]
    fn display_alias() {
        let mut s = MockCommandServer::new();

        let output = output_text(exec(&mut s, &["display", "-p", "foo"]));
        assert!(output.contains("foo"));
    }

    #[test]
    fn display_message_format_expansion() {
        let mut s = MockCommandServer::new();
        let (sid, _widx, _pid) = s.create_test_session("work");
        s.client_session_id = Some(sid);

        let output = output_text(exec(
            &mut s,
            &["display-message", "-p", "#{session_name}:#{window_index}"],
        ));
        assert!(output.contains("work:0"), "output: {output}");
    }

    #[test]
    fn display_message_session_activity() {
        let mut s = MockCommandServer::new();
        let (sid, _widx, _pid) = s.create_test_session("test");
        s.client_session_id = Some(sid);

        let output = output_text(exec(&mut s, &["display-message", "-p", "#{session_activity}"]));
        // session_activity is a unix timestamp, should be numeric
        let trimmed = output.trim();
        assert!(trimmed.parse::<u64>().is_ok(), "expected numeric timestamp, got: {trimmed}");
    }

    #[test]
    fn display_message_pane_start_command() {
        let mut s = MockCommandServer::new();
        let (sid, _widx, _pid) = s.create_test_session("test");
        s.client_session_id = Some(sid);

        // pane_start_command defaults to empty
        let output =
            output_text(exec(&mut s, &["display-message", "-p", "cmd=#{pane_start_command}"]));
        assert!(output.contains("cmd="), "output: {output}");
    }

    #[test]
    fn display_message_no_message_returns_ok() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["display-message"]).unwrap();
        assert!(matches!(result, CommandResult::Ok));
    }

    #[test]
    fn display_message_t_flag_not_included_in_message() {
        let mut s = MockCommandServer::new();
        let (sid, _widx, _pid) = s.create_test_session("work");
        s.client_session_id = Some(sid);

        // -t target should be skipped, not included as message text
        let output = output_text(exec(&mut s, &["display-message", "-t", "work", "-p", "hello"]));
        assert_eq!(output.trim(), "hello", "target 'work' should not appear in output: {output}");
    }

    #[test]
    fn display_message_t_flag_only_message_expanded() {
        let mut s = MockCommandServer::new();
        let (sid, _widx, _pid) = s.create_test_session("mysess");
        s.client_session_id = Some(sid);

        let output = output_text(exec(
            &mut s,
            &["display-message", "-t", "mysess", "-p", "#{session_name}"],
        ));
        // Should show session name, NOT "mysess #{session_name}"
        assert!(!output.contains("mysess mysess"), "target value leaked into message: {output}");
    }
}

// ============================================================
// list-keys / list-commands / list-clients
// ============================================================

mod list_tests {
    use super::*;

    #[test]
    fn list_keys_shows_default_bindings() {
        let mut s = MockCommandServer::new();

        let output = output_text(exec(&mut s, &["list-keys"]));
        // Should contain default bindings like detach-client on 'd'
        assert!(output.contains("detach-client"), "output: {output}");
        assert!(output.contains("new-window"), "output: {output}");
        assert!(output.contains("split-window"), "output: {output}");
    }

    #[test]
    fn list_keys_after_bind() {
        let mut s = MockCommandServer::new();

        exec(&mut s, &["bind-key", "z", "kill-session"]).unwrap();

        let output = output_text(exec(&mut s, &["list-keys"]));
        assert!(output.contains("kill-session"), "output: {output}");
    }

    #[test]
    fn list_commands_returns_all() {
        let mut s = MockCommandServer::new();

        let output = output_text(exec(&mut s, &["list-commands"]));
        // Should contain command names
        assert!(output.contains("new-session"));
        assert!(output.contains("set-option"));
        assert!(output.contains("bind-key"));
        assert!(output.contains("capture-pane"));
        assert!(output.contains("run-shell"));
    }

    #[test]
    fn list_commands_alias() {
        let mut s = MockCommandServer::new();

        let output = output_text(exec(&mut s, &["lscm"]));
        assert!(output.contains("new-session"));
    }

    #[test]
    fn list_clients_shows_mock_client() {
        let mut s = MockCommandServer::new();

        let output = output_text(exec(&mut s, &["list-clients"]));
        assert!(output.contains("client"), "output: {output}");
        assert!(output.contains("80x24"), "output: {output}");
    }
}

// ============================================================
// Integration tests: complex multi-command scenarios
// ============================================================

mod integration_tests {
    use super::*;

    #[test]
    fn full_config_workflow() {
        let mut s = MockCommandServer::new();
        s.create_test_session("main");

        // Simulate loading a tmux config
        let tmp = "/tmp/rmux_integration_test.conf";
        std::fs::write(
            tmp,
            "\
# My tmux config
set-option -g history-limit 10000
set-option -g escape-time 50
bind-key z kill-session
bind-key -n F5 new-window
",
        )
        .unwrap();

        exec(&mut s, &["source-file", tmp]).unwrap();

        // Verify options were set
        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("10000"));

        let output = output_text(exec(&mut s, &["show-options", "-g", "escape-time"]));
        assert!(output.contains("50"));

        // Verify key bindings
        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z') && b.contains("kill-session")));
        assert!(bindings.iter().any(|b| b.contains("root") && b.contains("new-window")));

        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn multi_pane_lifecycle() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pane1) = s.create_test_session("work");

        // Split horizontally
        let pane2 = s.split_window(sid, widx, true, "/tmp", None, None).unwrap();

        let window = s.sessions.find_by_id(sid).unwrap().windows.get(&widx).unwrap();
        assert_eq!(window.pane_count(), 2);
        assert_eq!(window.active_pane, pane2);

        // Switch to last pane
        exec(&mut s, &["last-pane"]).unwrap();
        let window = s.sessions.find_by_id(sid).unwrap().windows.get(&widx).unwrap();
        assert_eq!(window.active_pane, pane1);

        // Swap panes
        exec(&mut s, &["swap-pane", "-D"]).unwrap();

        // Rotate
        exec(&mut s, &["rotate-window"]).unwrap();

        // Break pane into new window
        let session = s.sessions.find_by_id(sid).unwrap();
        let initial_windows = session.windows.len();
        exec(&mut s, &["break-pane"]).unwrap();
        let session = s.sessions.find_by_id(sid).unwrap();
        assert_eq!(session.windows.len(), initial_windows + 1);
    }

    #[test]
    fn option_set_then_show_roundtrip() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        let test_cases = [
            ("history-limit", "5000"),
            ("escape-time", "100"),
            ("focus-events", "on"),
            ("exit-empty", "off"),
            ("set-clipboard", "external"),
        ];

        for (key, value) in &test_cases {
            exec(&mut s, &["set-option", "-g", key, value]).unwrap();
            let output = output_text(exec(&mut s, &["show-options", "-g", key]));
            assert!(output.contains(value), "set-then-show for {key}={value}, got: {output}");
        }
    }

    #[test]
    fn window_management_workflow() {
        let mut s = MockCommandServer::new();
        let (sid, widx0, _) = s.create_test_session("test");

        // Create windows
        exec(&mut s, &["new-window", "-n", "editor"]).unwrap();
        exec(&mut s, &["new-window", "-n", "logs"]).unwrap();

        // List windows
        let output = output_text(exec(&mut s, &["list-windows"]));
        assert!(output.contains("editor") || output.contains("logs"), "windows in list: {output}");

        let session = s.sessions.find_by_id(sid).unwrap();
        let window_count = session.windows.len();
        assert_eq!(window_count, 3, "should have 3 windows");

        // Swap windows
        let indices: Vec<u32> = session.sorted_window_indices();
        if indices.len() >= 2 {
            let idx_a = indices[0];
            let idx_b = indices[1];
            exec(
                &mut s,
                &["swap-window", "-s", &idx_a.to_string(), "-t", &format!("test:{idx_b}")],
            )
            .unwrap();
        }

        // Select layout on first window
        exec(&mut s, &["select-window", "-t", &format!("test:{widx0}")]).unwrap();
    }

    #[test]
    fn capture_pane_after_input() {
        let mut s = MockCommandServer::new();
        let (sid, widx, pid) = s.create_test_session("test");

        // Simulate terminal output
        let session = s.sessions.find_by_id_mut(sid).unwrap();
        let window = session.windows.get_mut(&widx).unwrap();
        let pane = window.panes.get_mut(&pid).unwrap();
        pane.process_input(b"$ ls\r\nfile1.txt\r\nfile2.txt\r\n");

        let output = output_text(exec(&mut s, &["capture-pane", "-p"]));
        assert!(output.contains("$ ls"), "capture: {output}");
        assert!(output.contains("file1.txt"), "capture: {output}");
        assert!(output.contains("file2.txt"), "capture: {output}");
    }

    #[test]
    fn bind_unbind_round_trip() {
        let mut s = MockCommandServer::new();

        // Bind
        exec(&mut s, &["bind-key", "z", "kill-session"]).unwrap();
        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z') && b.contains("kill-session")));

        // Unbind
        exec(&mut s, &["unbind-key", "z"]).unwrap();
        let bindings = s.keybindings.list_bindings();
        assert!(!bindings.iter().any(|b| b.contains(" z ") && b.contains("kill-session")));

        // Rebind to something else
        exec(&mut s, &["bind-key", "z", "new-window"]).unwrap();
        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z') && b.contains("new-window")));
    }

    #[test]
    fn all_command_aliases_resolve() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Test that alias commands exist and can be found
        let aliases = [
            "ls", "attach", "detach", "next", "prev", "set", "show", "bind", "unbind", "source",
            "capturep", "resizep", "swapp", "breakp", "joinp", "lastp", "respawnp", "swapw",
            "movew", "rotatew", "selectl", "respawnw", "run", "display", "lscm",
        ];

        for alias in &aliases {
            let found = crate::command::find_command(alias);
            assert!(found.is_some(), "alias '{alias}' should resolve to a command");
        }
    }
}

// ============================================================
// key_name_to_bytes tests (terminal layer)
// ============================================================

mod key_conversion_tests {
    #[test]
    fn key_name_enter() {
        let bytes = rmux_terminal::keys::key_name_to_bytes("Enter");
        assert_eq!(bytes, Some(b"\r".to_vec()));
    }

    #[test]
    fn key_name_escape() {
        let bytes = rmux_terminal::keys::key_name_to_bytes("Escape");
        assert_eq!(bytes, Some(vec![0x1b]));
    }

    #[test]
    fn key_name_space() {
        let bytes = rmux_terminal::keys::key_name_to_bytes("Space");
        assert_eq!(bytes, Some(b" ".to_vec()));
    }

    #[test]
    fn key_name_ctrl_c() {
        let bytes = rmux_terminal::keys::key_name_to_bytes("C-c");
        assert_eq!(bytes, Some(vec![0x03]));
    }

    #[test]
    fn key_name_tab() {
        let bytes = rmux_terminal::keys::key_name_to_bytes("Tab");
        assert_eq!(bytes, Some(b"\t".to_vec()));
    }

    #[test]
    fn key_name_up_arrow() {
        let bytes = rmux_terminal::keys::key_name_to_bytes("Up");
        assert_eq!(bytes, Some(b"\x1b[A".to_vec()));
    }

    #[test]
    fn key_name_unknown_returns_none() {
        let bytes = rmux_terminal::keys::key_name_to_bytes("C-");
        assert!(bytes.is_none());
    }
}

// ============================================================
// CommandServer trait method edge cases
// ============================================================

mod edge_case_tests {
    use super::*;
    use crate::command::CommandServer;

    #[test]
    fn execute_unknown_command() {
        let mut s = MockCommandServer::new();
        let result = exec(&mut s, &["nonexistent-command"]);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unknown command"));
    }

    #[test]
    fn execute_empty_argv() {
        let mut s = MockCommandServer::new();
        let argv: Vec<String> = Vec::new();
        let result = execute_command(&argv, &mut s);
        assert!(result.is_err());
    }

    #[test]
    fn set_option_overwrite() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        exec(&mut s, &["set-option", "-g", "history-limit", "1000"]).unwrap();
        exec(&mut s, &["set-option", "-g", "history-limit", "2000"]).unwrap();

        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("2000"), "should be overwritten: {output}");
    }

    #[test]
    fn select_layout_with_three_panes() {
        let mut s = MockCommandServer::new();
        let (sid, widx, _) = s.create_test_session("test");
        s.add_pane_to_window(sid, widx, true);
        s.add_pane_to_window(sid, widx, true);

        let window = s.sessions.find_by_id(sid).unwrap().windows.get(&widx).unwrap();
        assert_eq!(window.pane_count(), 3);

        exec(&mut s, &["select-layout", "even-horizontal"]).unwrap();

        let window = s.sessions.find_by_id(sid).unwrap().windows.get(&widx).unwrap();
        let layout = window.layout.as_ref().unwrap();
        assert_eq!(layout.pane_count(), 3);
    }

    #[test]
    fn rename_session_then_find() {
        let mut s = MockCommandServer::new();
        s.create_test_session("original");

        exec(&mut s, &["rename-session", "-t", "original", "renamed"]).unwrap();

        assert!(s.has_session("renamed"));
        assert!(!s.has_session("original"));
    }

    #[test]
    fn multiple_config_commands_in_sequence() {
        let mut s = MockCommandServer::new();
        s.create_test_session("test");

        // Execute multiple commands like a config file would
        exec(&mut s, &["set-option", "-g", "history-limit", "5000"]).unwrap();
        exec(&mut s, &["set-option", "-g", "escape-time", "0"]).unwrap();
        exec(&mut s, &["bind-key", "z", "kill-session"]).unwrap();
        exec(&mut s, &["bind-key", "-n", "F1", "new-window"]).unwrap();

        // Verify all applied
        let output = output_text(exec(&mut s, &["show-options", "-g", "history-limit"]));
        assert!(output.contains("5000"));

        let output = output_text(exec(&mut s, &["show-options", "-g", "escape-time"]));
        assert!(output.contains(" 0"));

        let bindings = s.keybindings.list_bindings();
        assert!(bindings.iter().any(|b| b.contains('z')));
        assert!(bindings.iter().any(|b| b.contains("root")));
    }
}

// ============================================================
// Debug formatting for CommandResult
// ============================================================

impl std::fmt::Debug for crate::command::CommandResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandResult::Ok => write!(f, "Ok"),
            CommandResult::Output(s) => write!(f, "Output({s:?})"),
            CommandResult::Attach(id) => write!(f, "Attach({id})"),
            CommandResult::Detach => write!(f, "Detach"),
            CommandResult::Exit => write!(f, "Exit"),
            CommandResult::RunShell(cmd) => write!(f, "RunShell({cmd:?})"),
            CommandResult::RunShellBackground(cmd) => {
                write!(f, "RunShellBackground({cmd:?})")
            }
            CommandResult::Suspend => write!(f, "Suspend"),
            CommandResult::TimedMessage(msg) => write!(f, "TimedMessage({msg:?})"),
            CommandResult::Overlay(_) => write!(f, "Overlay(...)"),
            CommandResult::SpawnPopup(_) => write!(f, "SpawnPopup(...)"),
        }
    }
}
