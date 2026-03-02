//! Server-level commands: kill-server, send-keys, bind-key, unbind-key, source-file, run-shell,
//! command-prompt.

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

/// bind-key [-T table] [-n] key command [args...]
pub fn cmd_bind_key(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let table = if has_flag(args, "-n") {
        "root"
    } else {
        get_option(args, "-T").unwrap_or("prefix")
    };

    let positional = positional_args(args, &["-T"]);
    if positional.is_empty() {
        return Err(ServerError::Command("bind-key: missing key".into()));
    }

    let key_name = positional[0];
    if positional.len() < 2 {
        return Err(ServerError::Command("bind-key: missing command".into()));
    }

    let argv: Vec<String> = positional[1..].iter().map(|s| (*s).to_string()).collect();
    server.add_key_binding(table, key_name, argv)?;
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

/// source-file path
pub fn cmd_source_file(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let positional = positional_args(args, &[]);
    if positional.is_empty() {
        return Err(ServerError::Command("source-file: missing path".into()));
    }
    let path = positional[0];
    let commands = crate::config::load_config_file(path)
        .map_err(|e| ServerError::Command(format!("source-file: {e}")))?;
    let errors = server.execute_config_commands(commands);
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
