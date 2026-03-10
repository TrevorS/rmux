//! Option management commands: set-option, show-options.

use crate::command::{CommandResult, CommandServer, get_option, has_flag, positional_args};
use crate::server::ServerError;

/// set-option [-g] [-s] [-w] [-t target] key value
pub fn cmd_set_option(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let global = has_flag(args, "-g");
    let window_scope = has_flag(args, "-w");
    let target = get_option(args, "-t");

    let positional = positional_args(args, &["-t"]);
    if positional.is_empty() {
        return Err(ServerError::Command("set-option: missing option name".into()));
    }
    let raw_key = positional[0];

    if positional.len() < 2 {
        return Err(ServerError::Command("set-option: missing value".into()));
    }
    let raw_value = positional[1];

    // Handle style aliases: status-bg, status-fg -> status-style
    let (key, value): (&str, String) = match raw_key {
        "status-bg" => ("status-style", format!("bg={raw_value}")),
        "status-fg" => ("status-style", format!("fg={raw_value}")),
        other => (other, raw_value.to_string()),
    };
    let value = value.as_str();

    if global || (!window_scope && target.is_none()) {
        // Server-level option
        server.set_server_option(key, value)?;
    } else if window_scope {
        // Window-level option
        let session_id = if let Some(t) = target {
            if let Some(colon) = t.find(':') {
                let session_name = &t[..colon];
                server.find_session_id(session_name).ok_or_else(|| {
                    ServerError::Command(format!("session not found: {session_name}"))
                })?
            } else {
                server
                    .find_session_id(t)
                    .ok_or_else(|| ServerError::Command(format!("session not found: {t}")))?
            }
        } else {
            server
                .client_session_id()
                .ok_or_else(|| ServerError::Command("no current session".into()))?
        };

        let window_idx = if let Some(t) = target {
            if let Some(colon) = t.find(':') {
                t[colon + 1..].parse().unwrap_or(0)
            } else {
                server.active_window_for(session_id).unwrap_or(0)
            }
        } else {
            server.active_window_for(session_id).unwrap_or(0)
        };

        server.set_window_option(session_id, window_idx, key, value)?;
    } else {
        // Session-level option
        let session_id = if let Some(t) = target {
            server
                .find_session_id(t)
                .ok_or_else(|| ServerError::Command(format!("session not found: {t}")))?
        } else {
            server
                .client_session_id()
                .ok_or_else(|| ServerError::Command("no current session".into()))?
        };
        server.set_session_option(session_id, key, value)?;
    }

    Ok(CommandResult::Ok)
}

/// show-options [-g] [-s] [-w] [-t target] [option-name]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_options(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let global = has_flag(args, "-g");
    let window_scope = has_flag(args, "-w");

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
