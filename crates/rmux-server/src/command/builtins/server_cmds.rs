//! Server-level commands: kill-server, start-server, send-keys, bind-key, unbind-key, source-file,
//! run-shell, command-prompt, set-hook, show-hooks.

use crate::command::{CommandResult, CommandServer, get_option, has_flag, positional_args};
use crate::server::ServerError;

/// kill-server
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_kill_server(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Exit)
}

/// start-server — no-op since the server is already running.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_start_server(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// send-keys [-l] [-R] [-X] [-H] [-N count] [-t target-pane] key ...
/// -R: reset the terminal for the target pane
/// -H: send hex key codes (not yet implemented)
pub fn cmd_send_keys(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let literal = has_flag(args, "-l");
    let copy_mode_cmd = has_flag(args, "-X");
    let _hex_mode = has_flag(args, "-H");

    // -R: reset the terminal
    if has_flag(args, "-R") {
        // Send a terminal reset sequence (ESC c = RIS)
        server.send_bytes_to_pane(b"\x1bc")?;
    }

    // -X: dispatch copy-mode command
    if copy_mode_cmd {
        let keys = positional_args(args, &["-t", "-N"]);
        if keys.is_empty() {
            return Err(ServerError::Command("send-keys -X: no command specified".into()));
        }
        let command = keys.join(" ");
        server.dispatch_copy_mode_command(&command)?;
        return Ok(CommandResult::Ok);
    }

    let (session_id, window_idx) = resolve_send_keys_target(args, server)?;

    // Get the active pane for the resolved window
    let pane_id = server
        .active_pane_id_for(session_id, window_idx)
        .or_else(|| server.client_active_pane_id())
        .ok_or_else(|| ServerError::Command("no target pane".into()))?;

    // Collect non-flag arguments as key names
    let keys = positional_args(args, &["-t", "-N"]);
    if keys.is_empty() {
        return Err(ServerError::Command("send-keys: no keys specified".into()));
    }

    // -N count: repeat the key(s) N times
    let repeat: u32 = get_option(args, "-N").and_then(|s| s.parse().ok()).unwrap_or(1).max(1);

    for _ in 0..repeat {
        for key_arg in &keys {
            let bytes = if literal {
                key_arg.as_bytes().to_vec()
            } else {
                rmux_terminal::keys::key_name_to_bytes(key_arg)
                    .unwrap_or_else(|| key_arg.as_bytes().to_vec())
            };
            server.write_to_pane(session_id, window_idx, pane_id, &bytes)?;
        }
    }

    Ok(CommandResult::Ok)
}

/// Resolve session/window target for send-keys.
fn resolve_send_keys_target(
    args: &[String],
    server: &dyn CommandServer,
) -> Result<(u32, u32), ServerError> {
    if let Some(target) = get_option(args, "-t") {
        if let Some(colon_pos) = target.find(':') {
            let session_name = &target[..colon_pos];
            let window_str = &target[colon_pos + 1..];
            let session_id = server.find_session_id(session_name).ok_or_else(|| {
                ServerError::Command(format!("session not found: {session_name}"))
            })?;
            let window_idx = window_str
                .parse()
                .map_err(|_| ServerError::Command(format!("invalid window: {window_str}")))?;
            Ok((session_id, window_idx))
        } else if let Some(session_id) = server.find_session_id(target) {
            let window_idx = server.active_window_for(session_id).unwrap_or(0);
            Ok((session_id, window_idx))
        } else {
            let session_id = server
                .client_session_id()
                .ok_or_else(|| ServerError::Command("no current session".into()))?;
            // Maybe a pane target like %N
            let window_idx = server
                .client_active_window()
                .ok_or_else(|| ServerError::Command("no current window".into()))?;
            Ok((session_id, window_idx))
        }
    } else {
        let session_id = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let window_idx = server
            .client_active_window()
            .ok_or_else(|| ServerError::Command("no current window".into()))?;
        Ok((session_id, window_idx))
    }
}

/// bind-key [-r] [-N note] [-T table] [-n] key command [args...]
pub fn cmd_bind_key(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let table =
        if has_flag(args, "-n") { "root" } else { get_option(args, "-T").unwrap_or("prefix") };
    let repeatable = has_flag(args, "-r");
    let note = get_option(args, "-N").map(String::from);

    // Custom arg parsing: after consuming flags, the first remaining arg is the key
    // (even if it's "-" or another flag-like string), followed by command + args.
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-r" || arg == "-n" {
            i += 1;
        } else if arg == "-T" || arg == "-N" {
            i += 2; // skip flag and its value
        } else if arg.starts_with('-') && arg.len() > 1 && arg.as_bytes()[1] != b'-' {
            // Combined flags like "-rn" — skip
            i += 1;
        } else {
            break;
        }
    }

    if i >= args.len() {
        return Err(ServerError::Command("bind-key: missing key".into()));
    }
    let key_name = &args[i];
    i += 1;

    // Remaining args are the command and its arguments
    if i >= args.len() {
        return Err(ServerError::Command("bind-key: missing command".into()));
    }
    let argv: Vec<String> = args[i..].to_vec();
    server.add_key_binding(table, key_name, argv, repeatable, note)?;
    Ok(CommandResult::Ok)
}

/// unbind-key [-a] [-n] [-T table] [-q] key
pub fn cmd_unbind_key(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let quiet = has_flag(args, "-q");
    let unbind_all = has_flag(args, "-a");
    let table =
        if has_flag(args, "-n") { "root" } else { get_option(args, "-T").unwrap_or("prefix") };

    if unbind_all {
        server.clear_key_table(table);
        return Ok(CommandResult::Ok);
    }

    let positional = positional_args(args, &["-T"]);
    if positional.is_empty() {
        return Err(ServerError::Command("unbind-key: missing key".into()));
    }
    let result = server.remove_key_binding(table, positional[0]);
    if quiet { Ok(CommandResult::Ok) } else { result.map(|()| CommandResult::Ok) }
}

/// source-file [-F] [-q] path [path ...]
///
/// Supports glob patterns (e.g., `~/.config/tmux/conf.d/*.conf`).
pub fn cmd_source_file(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let format_flag = has_flag(args, "F");
    let quiet_flag = has_flag(args, "q");
    let positional = positional_args(args, &[]);
    if positional.is_empty() {
        return Err(ServerError::Command("source-file: missing path".into()));
    }

    let mut all_errors = Vec::new();
    let mut had_load_failure = false;

    for raw_path in &positional {
        // Resolve the path (optionally format-expanding it, then tilde-expanding)
        let expanded = if format_flag {
            let fmt_ctx = server.build_format_context();
            crate::config::expand_tilde(&crate::format::format_expand(raw_path, &fmt_ctx))
        } else {
            crate::config::expand_tilde(raw_path)
        };

        // Expand glob patterns; if no glob chars, treat as a literal path
        let paths: Vec<String> = if expanded.contains('*') || expanded.contains('?') {
            match glob::glob(&expanded) {
                Ok(entries) => {
                    let mut matched: Vec<String> = entries
                        .filter_map(Result::ok)
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                    matched.sort();
                    matched
                }
                Err(e) => {
                    if quiet_flag {
                        continue;
                    }
                    return Err(ServerError::Command(format!("source-file: {e}")));
                }
            }
        } else {
            vec![expanded]
        };

        for path in &paths {
            let (load_failed, errors) = source_single_file(path, quiet_flag, server);
            if load_failed {
                had_load_failure = true;
            }
            all_errors.extend(errors);
        }
    }

    if all_errors.is_empty() {
        Ok(CommandResult::Ok)
    } else if had_load_failure && all_errors.len() == 1 {
        // File-not-found on a single path: return as a proper error (matches tmux behavior)
        Err(ServerError::Command(all_errors.into_iter().next().unwrap()))
    } else {
        Ok(CommandResult::Output(all_errors.join("\n") + "\n"))
    }
}

/// Source a single config file, returning (load_failed, errors).
/// `load_failed` is true if the file could not be read at all.
fn source_single_file(
    path: &str,
    quiet: bool,
    server: &mut dyn CommandServer,
) -> (bool, Vec<String>) {
    let mut ctx = server.build_config_context();
    let abs_path = std::path::Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(path));
    ctx.hidden_vars.insert("current_file".to_string(), abs_path.to_string_lossy().into_owned());

    let commands = match crate::config::load_config_file_with_context(path, &mut ctx) {
        Ok(cmds) => cmds,
        Err(e) => {
            if quiet {
                return (true, Vec::new());
            }
            return (true, vec![format!("source-file: {e}")]);
        }
    };

    let prev_hidden = server.get_config_hidden_vars();
    server.set_config_hidden_vars(ctx.hidden_vars.clone());

    let prev_current_file = server.get_server_option("current_file").ok();
    let _ = server.set_server_option("current_file", &abs_path.to_string_lossy());

    let errors = server.execute_config_commands(commands);

    server.set_config_hidden_vars(prev_hidden);
    if let Some(prev) = prev_current_file {
        let _ = server.set_server_option("current_file", &prev);
    } else {
        let _ = server.unset_server_option("current_file");
    }

    (false, errors)
}

/// run-shell [-b] command
pub fn cmd_run_shell(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let background = has_flag(args, "-b");
    let positional = positional_args(args, &[]);
    if positional.is_empty() {
        return Err(ServerError::Command("run-shell: missing command".into()));
    }
    let command = positional.join(" ");
    if background {
        Ok(CommandResult::RunShellBackground(command))
    } else {
        Ok(CommandResult::RunShell(command))
    }
}

/// command-prompt [-I initial-text] [-p prompt] [template]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_command_prompt(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let initial_text = get_option(args, "-I");
    let prompt_str = get_option(args, "-p");
    let template = positional_args(args, &["-I", "-p", "-t", "-T"]);
    let template_str = template.first().copied();
    server.enter_command_prompt_with(initial_text, prompt_str, template_str);
    Ok(CommandResult::Ok)
}

/// if-shell [-b] [-F] shell-command command [command]
///
/// Execute shell-command; if it returns success (exit 0), run the first command,
/// otherwise run the second command (if given).
/// With -F, treat the shell-command as a format string: non-empty = true.
/// -b: run in background (currently executes synchronously).
pub fn cmd_if_shell(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let format_flag = has_flag(args, "-F");
    let _background = has_flag(args, "-b");
    let _target = get_option(args, "-t");
    let positional = positional_args(args, &["-t"]);
    if positional.len() < 2 {
        return Err(ServerError::Command("if-shell: requires shell-command and command".into()));
    }

    let shell_cmd = positional[0];
    let true_cmd = positional[1];
    let false_cmd = positional.get(2).copied();

    let success = if format_flag {
        // -F: expand as format string, non-empty/non-zero result = true
        let ctx = server.build_format_context();
        let expanded = crate::format::format_expand(shell_cmd, &ctx);
        !expanded.is_empty() && expanded != "0"
    } else {
        let output = std::process::Command::new("sh").arg("-c").arg(shell_cmd).output();
        output.is_ok_and(|o| o.status.success())
    };
    let cmd_str = if success {
        true_cmd
    } else if let Some(fc) = false_cmd {
        fc
    } else {
        return Ok(CommandResult::Ok);
    };

    // Parse and execute the resulting command
    let cmd_args = crate::config::tokenize_command(cmd_str);
    if !cmd_args.is_empty() {
        server.execute_command(&cmd_args)?;
    }
    Ok(CommandResult::Ok)
}

/// send-prefix [-2] [-t target-pane]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_send_prefix(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _target = get_option(args, "-t");
    // Read the actual prefix/prefix2 from options instead of hardcoding
    let option_key = if has_flag(args, "-2") { "prefix2" } else { "prefix" };
    let prefix_str = server.get_server_option(option_key).unwrap_or_else(|_| "C-b".to_string());
    let prefix_bytes =
        rmux_terminal::keys::key_name_to_bytes(&prefix_str).unwrap_or_else(|| vec![0x02]); // fallback to Ctrl-b
    server.send_bytes_to_pane(&prefix_bytes)?;
    Ok(CommandResult::Ok)
}

/// clear-history [-H] [-t target-pane]
/// -H: clear history and screen (not just history)
pub fn cmd_clear_history(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _clear_screen = has_flag(args, "-H");
    let _target = get_option(args, "-t");
    server.clear_history()?;
    Ok(CommandResult::Ok)
}

/// set-hook [-u] hook-name [command]
///
/// Set or unset (-u) a hook. When set, the command is executed when the hook fires.
pub fn cmd_set_hook(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let unset = has_flag(args, "-u");
    let positional = positional_args(args, &[]);

    if positional.is_empty() {
        return Err(ServerError::Command("set-hook: missing hook name".into()));
    }

    let hook_name = positional[0];

    if unset {
        if !server.remove_hook(hook_name) {
            return Err(ServerError::Command(format!("hook not found: {hook_name}")));
        }
    } else {
        if positional.len() < 2 {
            return Err(ServerError::Command("set-hook: missing command".into()));
        }
        let argv: Vec<String> = positional[1..].iter().map(|s| (*s).to_string()).collect();
        server.set_hook(hook_name, argv);
    }

    Ok(CommandResult::Ok)
}

/// show-hooks
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_hooks(
    _args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let hooks = server.show_hooks();
    if hooks.is_empty() {
        Ok(CommandResult::Ok)
    } else {
        Ok(CommandResult::Output(hooks.join("\n") + "\n"))
    }
}

/// confirm-before [-p prompt] command
///
/// Ask for confirmation before executing a command.
/// Shows a y/n prompt; the command executes only if the user types "y".
pub fn cmd_confirm_before(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let custom_prompt = get_option(args, "-p");
    let _target = get_option(args, "-t");
    let positional = positional_args(args, &["-p", "-t"]);
    if positional.is_empty() {
        return Err(ServerError::Command("confirm-before: missing command".into()));
    }
    let cmd_str = positional.join(" ");

    // Build the prompt string — tmux defaults to the command name
    let prompt_str = if let Some(p) = custom_prompt {
        format!("{p} (y/n) ")
    } else {
        let cmd_name = cmd_str.split_whitespace().next().unwrap_or("confirm");
        format!("{cmd_name}? (y/n) ")
    };

    // Use command-prompt with a template that wraps the command in an if-shell
    // checking if the input was "y"
    let template = format!("if-shell -F '#{{==:%%,y}}' '{cmd_str}'");
    server.enter_command_prompt_with(None, Some(&prompt_str), Some(&template));
    Ok(CommandResult::Ok)
}

/// wait-for [-L|-U|-S] channel
///
/// Wait for or signal a named channel for scripting synchronization.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_wait_for(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}
