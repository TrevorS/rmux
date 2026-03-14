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

/// send-keys [-l] [-t target-pane] key ...
pub fn cmd_send_keys(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let literal = has_flag(args, "-l");
    let (session_id, window_idx) = resolve_send_keys_target(args, server)?;

    // Get the active pane for the resolved window
    let pane_id = server
        .active_pane_id_for(session_id, window_idx)
        .or_else(|| server.client_active_pane_id())
        .ok_or_else(|| ServerError::Command("no target pane".into()))?;

    // Collect non-flag arguments as key names
    let keys = positional_args(args, &["-t"]);
    if keys.is_empty() {
        return Err(ServerError::Command("send-keys: no keys specified".into()));
    }

    for key_arg in keys {
        let bytes = if literal {
            // In literal mode, send the argument text directly
            key_arg.as_bytes().to_vec()
        } else {
            // Try to parse as a named key, fall back to literal bytes
            rmux_terminal::keys::key_name_to_bytes(key_arg)
                .unwrap_or_else(|| key_arg.as_bytes().to_vec())
        };
        server.write_to_pane(session_id, window_idx, pane_id, &bytes)?;
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

/// bind-key [-r] [-T table] [-n] key command [args...]
pub fn cmd_bind_key(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let table =
        if has_flag(args, "-n") { "root" } else { get_option(args, "-T").unwrap_or("prefix") };
    let repeatable = has_flag(args, "-r");

    // Custom arg parsing: after consuming flags, the first remaining arg is the key
    // (even if it's "-" or another flag-like string), followed by command + args.
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-r" || arg == "-n" {
            i += 1;
        } else if arg == "-T" {
            i += 2; // skip -T and its value
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
    server.add_key_binding(table, key_name, argv, repeatable)?;
    Ok(CommandResult::Ok)
}

/// unbind-key [-T table] key
pub fn cmd_unbind_key(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let table = get_option(args, "-T").unwrap_or("prefix");
    let positional = positional_args(args, &["-T"]);
    if positional.is_empty() {
        return Err(ServerError::Command("unbind-key: missing key".into()));
    }
    server.remove_key_binding(table, positional[0])?;
    Ok(CommandResult::Ok)
}

/// source-file [-F] [-q] path
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

    // Resolve the path (optionally format-expanding it)
    let path = if format_flag {
        let fmt_ctx = server.build_format_context();
        crate::format::format_expand(positional[0], &fmt_ctx)
    } else {
        positional[0].to_string()
    };

    let mut ctx = server.build_config_context();
    // Set current_file so #{current_file} and #{d:current_file} work during sourcing
    let abs_path = std::path::Path::new(&path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&path));
    ctx.hidden_vars.insert("current_file".to_string(), abs_path.to_string_lossy().into_owned());

    let commands = match crate::config::load_config_file_with_context(&path, &mut ctx) {
        Ok(cmds) => cmds,
        Err(e) => {
            if quiet_flag {
                return Ok(CommandResult::Ok);
            }
            return Err(ServerError::Command(format!("source-file: {e}")));
        }
    };

    // Set current_file as a format variable for commands executed from this file
    let _ = server.set_server_option("current_file", &abs_path.to_string_lossy());

    let errors = server.execute_config_commands(commands);

    // Clear current_file after sourcing
    let _ = server.unset_server_option("current_file");

    if errors.is_empty() {
        Ok(CommandResult::Ok)
    } else {
        Ok(CommandResult::Output(errors.join("\n") + "\n"))
    }
}

/// run-shell [-b] command
pub fn cmd_run_shell(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let positional = positional_args(args, &[]);
    if positional.is_empty() {
        return Err(ServerError::Command("run-shell: missing command".into()));
    }
    let command = positional.join(" ");
    Ok(CommandResult::RunShell(command))
}

/// command-prompt
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_command_prompt(
    _args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    server.enter_command_prompt();
    Ok(CommandResult::Ok)
}

/// if-shell [-b] shell-command command [command]
///
/// Execute shell-command; if it returns success (exit 0), run the first command,
/// otherwise run the second command (if given).
pub fn cmd_if_shell(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let positional = positional_args(args, &[]);
    if positional.len() < 2 {
        return Err(ServerError::Command("if-shell: requires shell-command and command".into()));
    }

    let shell_cmd = positional[0];
    let true_cmd = positional[1];
    let false_cmd = positional.get(2).copied();

    let output = std::process::Command::new("sh").arg("-c").arg(shell_cmd).output();

    let success = output.is_ok_and(|o| o.status.success());
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
    // Send the prefix key (Ctrl-b by default) to the active pane
    let prefix_bytes = if has_flag(args, "-2") {
        // prefix2 — not commonly used, default to Ctrl-b
        vec![0x02]
    } else {
        vec![0x02] // Ctrl-b
    };
    server.send_bytes_to_pane(&prefix_bytes)?;
    Ok(CommandResult::Ok)
}

/// clear-history [-t target-pane]
pub fn cmd_clear_history(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
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
/// Executes the command directly (interactive confirmation needs client-side UI).
pub fn cmd_confirm_before(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let positional = positional_args(args, &["-p"]);
    if positional.is_empty() {
        return Err(ServerError::Command("confirm-before: missing command".into()));
    }
    let cmd_str = positional.join(" ");
    let cmd_args = crate::config::tokenize_command(&cmd_str);
    if !cmd_args.is_empty() {
        server.execute_command(&cmd_args)?;
    }
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
