//! Option management commands: set-option, show-options.

use crate::command::{CommandResult, CommandServer, get_option, has_flag, positional_args};
use crate::format::format_expand;
use crate::server::ServerError;

/// Parsed set-option flags.
#[allow(clippy::struct_excessive_bools)]
struct SetOptionFlags {
    global: bool,
    window_scope: bool,
    quiet: bool,
    only_if_unset: bool,
    unset: bool,
    unset_all: bool,
    append: bool,
    format_expand: bool,
    target: Option<String>,
}

impl SetOptionFlags {
    fn parse(args: &[String]) -> Self {
        Self {
            global: has_flag(args, "-g"),
            window_scope: has_flag(args, "-w"),
            quiet: has_flag(args, "-q"),
            only_if_unset: has_flag(args, "-o"),
            unset: has_flag(args, "-u"),
            unset_all: has_flag(args, "-U"),
            append: has_flag(args, "-a"),
            format_expand: has_flag(args, "-F"),
            target: get_option(args, "-t").map(String::from),
        }
    }
}

/// set-option [-agFgoqsupw] [-t target] key [value]
/// -s: server scope
/// -p: pane scope
pub fn cmd_set_option(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _server_scope = has_flag(args, "-s");
    let _pane_scope = has_flag(args, "-p");
    let flags = SetOptionFlags::parse(args);

    let positional = positional_args(args, &["-t"]);
    if positional.is_empty() {
        if flags.quiet {
            return Ok(CommandResult::Ok);
        }
        return Err(ServerError::Command("set-option: missing option name".into()));
    }
    let raw_key = positional[0];

    // Unset mode: no value needed. -U unsets globally across all sessions/windows.
    if flags.unset || flags.unset_all {
        return handle_unset(raw_key, &flags, server);
    }

    if positional.len() < 2 {
        if flags.quiet {
            return Ok(CommandResult::Ok);
        }
        return Err(ServerError::Command("set-option: missing value".into()));
    }
    let raw_value = positional[1];

    // Handle style aliases: status-bg, status-fg -> status-style
    let (key, value): (&str, String) = match raw_key {
        "status-bg" => ("status-style", format!("bg={raw_value}")),
        "status-fg" => ("status-style", format!("fg={raw_value}")),
        other => (other, raw_value.to_string()),
    };

    // Format-expand the value if -F is set
    let value = if flags.format_expand {
        let ctx = server.build_format_context();
        format_expand(&value, &ctx)
    } else {
        value
    };
    let value = value.as_str();

    // Only-if-unset check
    if flags.only_if_unset && option_exists(key, &flags, server) {
        return Ok(CommandResult::Ok);
    }

    set_or_append(key, value, &flags, server)
}

/// Handle -u (unset) for all scopes.
fn handle_unset(
    key: &str,
    flags: &SetOptionFlags,
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    // -U unsets globally across all scopes
    if flags.unset_all || flags.global || (!flags.window_scope && flags.target.is_none()) {
        server.unset_server_option(key)?;
    } else if flags.window_scope {
        let (session_id, window_idx) = resolve_window_target(flags, server)?;
        server.unset_window_option(session_id, window_idx, key)?;
    } else {
        let session_id = resolve_session_target(flags, server)?;
        server.unset_session_option(session_id, key)?;
    }
    Ok(CommandResult::Ok)
}

/// Check if an option already exists (for -o flag).
fn option_exists(key: &str, flags: &SetOptionFlags, server: &dyn CommandServer) -> bool {
    if flags.global || (!flags.window_scope && flags.target.is_none()) {
        server.has_server_option(key)
    } else if flags.window_scope {
        if let Ok((sid, widx)) = resolve_window_target_ro(flags, server) {
            server.has_window_option(sid, widx, key)
        } else {
            false
        }
    } else if let Ok(sid) = resolve_session_target_ro(flags, server) {
        server.has_session_option(sid, key)
    } else {
        false
    }
}

/// Set or append the value depending on -a flag.
fn set_or_append(
    key: &str,
    value: &str,
    flags: &SetOptionFlags,
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    if flags.global || (!flags.window_scope && flags.target.is_none()) {
        if flags.append {
            server.append_server_option(key, value)?;
        } else {
            server.set_server_option(key, value)?;
        }
    } else if flags.window_scope {
        let (session_id, window_idx) = resolve_window_target(flags, server)?;
        if flags.append {
            server.append_window_option(session_id, window_idx, key, value)?;
        } else {
            server.set_window_option(session_id, window_idx, key, value)?;
        }
    } else {
        let session_id = resolve_session_target(flags, server)?;
        if flags.append {
            server.append_session_option(session_id, key, value)?;
        } else {
            server.set_session_option(session_id, key, value)?;
        }
    }
    Ok(CommandResult::Ok)
}

/// Resolve session ID from target or current client.
fn resolve_session_target(
    flags: &SetOptionFlags,
    server: &mut dyn CommandServer,
) -> Result<u32, ServerError> {
    if let Some(t) = &flags.target {
        server
            .find_session_id(t)
            .ok_or_else(|| ServerError::Command(format!("session not found: {t}")))
    } else {
        server.client_session_id().ok_or_else(|| ServerError::Command("no current session".into()))
    }
}

/// Resolve session ID (read-only version for existence checks).
fn resolve_session_target_ro(
    flags: &SetOptionFlags,
    server: &dyn CommandServer,
) -> Result<u32, ServerError> {
    if let Some(t) = &flags.target {
        server
            .find_session_id(t)
            .ok_or_else(|| ServerError::Command(format!("session not found: {t}")))
    } else {
        server.client_session_id().ok_or_else(|| ServerError::Command("no current session".into()))
    }
}

/// Resolve (session_id, window_idx) from target.
fn resolve_window_target(
    flags: &SetOptionFlags,
    server: &mut dyn CommandServer,
) -> Result<(u32, u32), ServerError> {
    let (session_id, window_idx) = if let Some(t) = &flags.target {
        if let Some(colon) = t.find(':') {
            let session_name = &t[..colon];
            let sid = server.find_session_id(session_name).ok_or_else(|| {
                ServerError::Command(format!("session not found: {session_name}"))
            })?;
            let widx = t[colon + 1..].parse().unwrap_or(0);
            (sid, widx)
        } else {
            let sid = server
                .find_session_id(t)
                .ok_or_else(|| ServerError::Command(format!("session not found: {t}")))?;
            let widx = server.active_window_for(sid).unwrap_or(0);
            (sid, widx)
        }
    } else {
        let sid = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let widx = server.active_window_for(sid).unwrap_or(0);
        (sid, widx)
    };
    Ok((session_id, window_idx))
}

/// Resolve (session_id, window_idx) read-only for existence checks.
fn resolve_window_target_ro(
    flags: &SetOptionFlags,
    server: &dyn CommandServer,
) -> Result<(u32, u32), ServerError> {
    let (session_id, window_idx) = if let Some(t) = &flags.target {
        if let Some(colon) = t.find(':') {
            let session_name = &t[..colon];
            let sid = server.find_session_id(session_name).ok_or_else(|| {
                ServerError::Command(format!("session not found: {session_name}"))
            })?;
            let widx = t[colon + 1..].parse().unwrap_or(0);
            (sid, widx)
        } else {
            let sid = server
                .find_session_id(t)
                .ok_or_else(|| ServerError::Command(format!("session not found: {t}")))?;
            let widx = server.active_window_for(sid).unwrap_or(0);
            (sid, widx)
        }
    } else {
        let sid = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let widx = server.active_window_for(sid).unwrap_or(0);
        (sid, widx)
    };
    Ok((session_id, window_idx))
}

/// set-window-option: wrapper that injects `-w` and delegates to `cmd_set_option`.
pub fn cmd_set_window_option(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let mut new_args = vec!["-w".to_string()];
    new_args.extend_from_slice(args);
    cmd_set_option(&new_args, server)
}

/// show-window-options: wrapper that injects `-w` and delegates to `cmd_show_options`.
pub fn cmd_show_window_options(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let mut new_args = vec!["-w".to_string()];
    new_args.extend_from_slice(args);
    cmd_show_options(&new_args, server)
}

/// show-options [-A] [-g] [-H] [-p] [-q] [-s] [-v] [-w] [-t target] [option-name]
/// -A: show all options
/// -H: include hooks
/// -p: pane options
/// -s: server scope
/// -v: values only
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_options(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let global = has_flag(args, "-g");
    let _quiet = has_flag(args, "-q");
    let window_scope = has_flag(args, "-w");
    let _all_tables = has_flag(args, "-A");
    let _hooks = has_flag(args, "-H");
    let _pane_scope = has_flag(args, "-p");
    let _server_scope = has_flag(args, "-s");
    let _values_only = has_flag(args, "-v");

    let scope = if global {
        "server"
    } else if window_scope {
        "window"
    } else {
        "session"
    };

    let target_id: Option<u32> = if let Some(target) = get_option(args, "-t") {
        server.find_session_id(target)
    } else {
        server.client_session_id()
    };

    let positional = positional_args(args, &["-t"]);

    let options = server.show_options(scope, target_id);

    if let Some(key) = positional.first() {
        // Filter to specific option
        let filtered: Vec<&String> = options.iter().filter(|o| o.starts_with(key)).collect();
        if filtered.is_empty() {
            Ok(CommandResult::Output(String::new()))
        } else {
            Ok(CommandResult::Output(
                filtered.into_iter().cloned().collect::<Vec<_>>().join("\n") + "\n",
            ))
        }
    } else if options.is_empty() {
        Ok(CommandResult::Output(String::new()))
    } else {
        Ok(CommandResult::Output(options.join("\n") + "\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::test_helpers::MockCommandServer;

    fn run_set(
        args: &[&str],
        server: &mut MockCommandServer,
    ) -> Result<CommandResult, ServerError> {
        let args: Vec<String> = args.iter().map(std::string::ToString::to_string).collect();
        cmd_set_option(&args, server)
    }

    // --- Quiet flag (-q) ---

    #[test]
    fn set_quiet_suppresses_missing_value() {
        let mut server = MockCommandServer::new();
        let result = run_set(&["-gq", "some-key"], &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn set_quiet_suppresses_missing_key() {
        let mut server = MockCommandServer::new();
        let result = run_set(&["-gq"], &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn set_without_quiet_errors_on_missing_value() {
        let mut server = MockCommandServer::new();
        let result = run_set(&["-g", "some-key"], &mut server);
        assert!(result.is_err());
    }

    #[test]
    fn set_without_quiet_errors_on_missing_key() {
        let mut server = MockCommandServer::new();
        let result = run_set(&["-g"], &mut server);
        assert!(result.is_err());
    }

    // --- Only-if-unset flag (-o) ---

    #[test]
    fn set_only_if_unset() {
        let mut server = MockCommandServer::new();
        run_set(&["-g", "@foo", "original"], &mut server).unwrap();
        run_set(&["-og", "@foo", "changed"], &mut server).unwrap();
        let val = server.get_server_option("@foo").unwrap();
        assert_eq!(val, "original");
    }

    #[test]
    fn set_only_if_unset_does_set_when_missing() {
        let mut server = MockCommandServer::new();
        run_set(&["-ogq", "@bar", "value"], &mut server).unwrap();
        let val = server.get_server_option("@bar").unwrap();
        assert_eq!(val, "value");
    }

    #[test]
    fn set_only_if_unset_session_scope() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        // Set on session scope
        run_set(&["-t", "test", "@opt", "first"], &mut server).unwrap();
        // -o should not overwrite
        run_set(&["-o", "-t", "test", "@opt", "second"], &mut server).unwrap();
        // Verify via show-options
        let opts = server.show_options("session", Some(sid));
        let found = opts.iter().find(|o| o.starts_with("@opt"));
        assert!(found.is_some_and(|v| v.contains("first")));
    }

    // --- Append flag (-a) ---

    #[test]
    fn set_append() {
        let mut server = MockCommandServer::new();
        run_set(&["-g", "status-left", "hello"], &mut server).unwrap();
        run_set(&["-ag", "status-left", " world"], &mut server).unwrap();
        let val = server.get_server_option("status-left").unwrap();
        assert_eq!(val, "hello world");
    }

    #[test]
    fn set_append_to_nonexistent() {
        let mut server = MockCommandServer::new();
        // Append to a key that doesn't exist should create it
        run_set(&["-ag", "@new_key", "value"], &mut server).unwrap();
        let val = server.get_server_option("@new_key").unwrap();
        assert_eq!(val, "value");
    }

    #[test]
    fn set_append_multiple() {
        let mut server = MockCommandServer::new();
        run_set(&["-g", "@list", "a"], &mut server).unwrap();
        run_set(&["-ag", "@list", ",b"], &mut server).unwrap();
        run_set(&["-ag", "@list", ",c"], &mut server).unwrap();
        let val = server.get_server_option("@list").unwrap();
        assert_eq!(val, "a,b,c");
    }

    #[test]
    fn set_append_window_scope() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        run_set(&["-w", "window-status-format", "A"], &mut server).unwrap();
        run_set(&["-aw", "window-status-format", "B"], &mut server).unwrap();
        let opts = server.show_options("window", Some(sid));
        let found = opts.iter().find(|o| o.starts_with("window-status-format"));
        assert!(found.is_some_and(|v| v.contains("AB")));
    }

    // --- Unset flag (-u) ---

    #[test]
    fn set_unset() {
        let mut server = MockCommandServer::new();
        run_set(&["-g", "@myopt", "value"], &mut server).unwrap();
        assert!(server.has_server_option("@myopt"));
        run_set(&["-ug", "@myopt"], &mut server).unwrap();
        assert!(!server.has_server_option("@myopt"));
    }

    #[test]
    fn set_unset_nonexistent_is_ok() {
        let mut server = MockCommandServer::new();
        // Unsetting something that doesn't exist should not error
        let result = run_set(&["-ug", "@nonexistent"], &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn set_unset_session_scope() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        run_set(&["-t", "test", "@opt", "val"], &mut server).unwrap();
        assert!(server.has_session_option(sid, "@opt"));
        run_set(&["-u", "-t", "test", "@opt"], &mut server).unwrap();
        assert!(!server.has_session_option(sid, "@opt"));
    }

    #[test]
    fn set_unset_window_scope() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        let widx = server.active_window_for(sid).unwrap_or(0);
        run_set(&["-w", "@wopt", "val"], &mut server).unwrap();
        assert!(server.has_window_option(sid, widx, "@wopt"));
        run_set(&["-uw", "@wopt"], &mut server).unwrap();
        assert!(!server.has_window_option(sid, widx, "@wopt"));
    }

    // --- Format expand flag (-F) ---

    #[test]
    fn set_format_expand_plain() {
        let mut server = MockCommandServer::new();
        run_set(&["-gF", "status-style", "bg=blue"], &mut server).unwrap();
        let val = server.get_server_option("status-style").unwrap();
        assert_eq!(val, "bg=blue");
    }

    #[test]
    fn set_format_expand_with_user_option() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        // Set a user option that -F can reference
        run_set(&["-g", "@thm_bg", "#1e1e2e"], &mut server).unwrap();
        // -gF should expand #{@thm_bg}
        run_set(&["-gF", "status-style", "bg=#{@thm_bg}"], &mut server).unwrap();
        let val = server.get_server_option("status-style").unwrap();
        assert_eq!(val, "bg=#1e1e2e");
    }

    #[test]
    fn set_format_expand_with_builtin_var() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("mytest", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        // -F should expand #{session_name}
        run_set(&["-gF", "@info", "s=#{session_name}"], &mut server).unwrap();
        let val = server.get_server_option("@info").unwrap();
        assert_eq!(val, "s=mytest");
    }

    // --- Combined flags ---

    #[test]
    fn set_combined_ogq() {
        let mut server = MockCommandServer::new();
        run_set(&["-ogq", "@catppuccin_flavor", "mocha"], &mut server).unwrap();
        let val = server.get_server_option("@catppuccin_flavor").unwrap();
        assert_eq!(val, "mocha");
        // Second call should not overwrite
        run_set(&["-ogq", "@catppuccin_flavor", "latte"], &mut server).unwrap();
        let val = server.get_server_option("@catppuccin_flavor").unwrap();
        assert_eq!(val, "mocha");
    }

    #[test]
    fn set_combined_agf() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        run_set(&["-g", "@thm_blue", "#89b4fa"], &mut server).unwrap();
        run_set(&["-g", "status-right", "hello"], &mut server).unwrap();
        // -agF: append + global + format-expand
        run_set(&["-agF", "status-right", " #{@thm_blue}"], &mut server).unwrap();
        let val = server.get_server_option("status-right").unwrap();
        assert_eq!(val, "hello #89b4fa");
    }

    #[test]
    fn set_combined_wgf() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);
        run_set(&["-g", "@color", "red"], &mut server).unwrap();
        // -wgF: window + global + format. Since -g takes precedence, sets server-level.
        run_set(&["-wgF", "window-status-format", "c=#{@color}"], &mut server).unwrap();
        // With -g present, this sets at server level
        let val = server.get_server_option("window-status-format").unwrap();
        assert_eq!(val, "c=red");
    }

    #[test]
    fn set_combined_ug() {
        let mut server = MockCommandServer::new();
        run_set(&["-g", "@temp", "val"], &mut server).unwrap();
        assert!(server.has_server_option("@temp"));
        run_set(&["-ug", "@temp"], &mut server).unwrap();
        assert!(!server.has_server_option("@temp"));
    }

    // --- Catppuccin simulation ---

    #[test]
    fn catppuccin_options_phase() {
        let mut server = MockCommandServer::new();
        // Phase 2: catppuccin_options_tmux.conf sets defaults with -ogq
        run_set(&["-ogq", "@catppuccin_flavor", "mocha"], &mut server).unwrap();
        run_set(&["-ogq", "@catppuccin_status_left_separator", ""], &mut server).unwrap();
        run_set(&["-ogq", "@catppuccin_window_text", "#W"], &mut server).unwrap();

        // User config overrides before plugin loads
        run_set(&["-g", "@catppuccin_flavor", "latte"], &mut server).unwrap();

        // Phase 2 runs again, -o means it doesn't overwrite user's choice
        run_set(&["-ogq", "@catppuccin_flavor", "mocha"], &mut server).unwrap();
        assert_eq!(server.get_server_option("@catppuccin_flavor").unwrap(), "latte");

        // But unset options got their defaults
        assert_eq!(server.get_server_option("@catppuccin_window_text").unwrap(), "#W");
    }

    #[test]
    fn catppuccin_theme_colors_phase() {
        let mut server = MockCommandServer::new();
        // Phase 3: catppuccin_mocha_tmux.conf sets color defaults with -ogq
        run_set(&["-ogq", "@thm_bg", "#1e1e2e"], &mut server).unwrap();
        run_set(&["-ogq", "@thm_fg", "#cdd6f4"], &mut server).unwrap();
        run_set(&["-ogq", "@thm_blue", "#89b4fa"], &mut server).unwrap();

        assert_eq!(server.get_server_option("@thm_bg").unwrap(), "#1e1e2e");
        assert_eq!(server.get_server_option("@thm_fg").unwrap(), "#cdd6f4");
        assert_eq!(server.get_server_option("@thm_blue").unwrap(), "#89b4fa");
    }

    #[test]
    fn catppuccin_format_expand_phase() {
        let mut server = MockCommandServer::new();
        let sid = server.create_session("test", "/tmp", 80, 24, None).unwrap();
        server.client_session_id = Some(sid);

        // Set theme colors
        run_set(&["-g", "@thm_mantle", "#181825"], &mut server).unwrap();
        run_set(&["-g", "@thm_blue", "#89b4fa"], &mut server).unwrap();

        // Phase 4: set -gF uses theme colors
        run_set(&["-gF", "status-style", "bg=#{@thm_mantle}"], &mut server).unwrap();
        assert_eq!(server.get_server_option("status-style").unwrap(), "bg=#181825");

        // -agF appends with format expansion
        run_set(&["-g", "@result", "start"], &mut server).unwrap();
        run_set(&["-agF", "@result", "|#{@thm_blue}"], &mut server).unwrap();
        assert_eq!(server.get_server_option("@result").unwrap(), "start|#89b4fa");
    }
}
