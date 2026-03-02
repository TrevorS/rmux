//! Window management commands.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::server::ServerError;

/// new-window [-d] [-n name] [-t target-session]
pub fn cmd_new_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _detached = has_flag(args, "-d");
    let name = get_option(args, "-n");

    let session_id = resolve_session(args, server)?;

    let cwd = std::env::current_dir()
        .map_or_else(|_| "/".to_string(), |p| p.to_string_lossy().into_owned());

    let (window_idx, _pane_id) = server.create_window(session_id, name, &cwd)?;

    // Select the new window (unless -d)
    if !has_flag(args, "-d") {
        server.select_window(session_id, window_idx)?;
    }

    Ok(CommandResult::Ok)
}

/// kill-window [-t target-window]
pub fn cmd_kill_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    server.kill_window(session_id, window_idx)?;
    Ok(CommandResult::Ok)
}

/// select-window [-t target-window]
pub fn cmd_select_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    server.select_window(session_id, window_idx)?;
    Ok(CommandResult::Ok)
}

/// next-window
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_next_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let session_id = server
        .client_session_id()
        .ok_or_else(|| ServerError::Command("no current session".into()))?;
    server.next_window(session_id)?;
    Ok(CommandResult::Ok)
}

/// previous-window
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_previous_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let session_id = server
        .client_session_id()
        .ok_or_else(|| ServerError::Command("no current session".into()))?;
    server.previous_window(session_id)?;
    Ok(CommandResult::Ok)
}

/// last-window
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_last_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let session_id = server
        .client_session_id()
        .ok_or_else(|| ServerError::Command("no current session".into()))?;
    server.last_window(session_id)?;
    Ok(CommandResult::Ok)
}

/// rename-window [-t target-window] new-name
pub fn cmd_rename_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;

    let new_name = args
        .iter()
        .rfind(|a| !a.starts_with('-'))
        .ok_or_else(|| ServerError::Command("rename-window: missing name".into()))?;

    server.rename_window(session_id, window_idx, new_name)?;
    Ok(CommandResult::Ok)
}

/// list-windows [-t target-session]
pub fn cmd_list_windows(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let windows = server.list_windows(session_id);
    if windows.is_empty() {
        Ok(CommandResult::Output("(no windows)\n".to_string()))
    } else {
        Ok(CommandResult::Output(windows.join("\n") + "\n"))
    }
}

/// Resolve the session ID from -t argument or current client session.
fn resolve_session(args: &[String], server: &dyn CommandServer) -> Result<u32, ServerError> {
    if let Some(target) = get_option(args, "-t") {
        // Target could be "session_name" or "session_name:window_idx"
        let session_name = target.split(':').next().unwrap_or(target);
        server
            .find_session_id(session_name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {session_name}")))
    } else {
        server.client_session_id().ok_or_else(|| ServerError::Command("no current session".into()))
    }
}

/// Resolve the window index from -t argument or current active window.
fn resolve_window_idx(
    args: &[String],
    server: &dyn CommandServer,
    _session_id: u32,
) -> Result<u32, ServerError> {
    if let Some(target) = get_option(args, "-t") {
        // Target could contain "session:window_idx"
        if let Some(idx_str) = target.split(':').nth(1) {
            idx_str
                .parse()
                .map_err(|_| ServerError::Command(format!("invalid window index: {idx_str}")))
        } else {
            // Try parsing as a bare window index
            target.parse().map_err(|_| {
                // It might be just a session name, use active window
                server
                    .client_active_window()
                    .ok_or(ServerError::Command("no current window".into()))
                    .unwrap_err()
            })
        }
    } else {
        server
            .client_active_window()
            .ok_or_else(|| ServerError::Command("no current window".into()))
    }
}
