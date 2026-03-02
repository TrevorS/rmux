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

/// next-window [-t target-session]
pub fn cmd_next_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    server.next_window(session_id)?;
    Ok(CommandResult::Ok)
}

/// previous-window [-t target-session]
pub fn cmd_previous_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    server.previous_window(session_id)?;
    Ok(CommandResult::Ok)
}

/// last-window [-t target-session]
pub fn cmd_last_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
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

/// swap-window [-s src] [-t dst]
pub fn cmd_swap_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;

    let src_idx = get_option(args, "-s")
        .and_then(|s| s.split(':').next_back())
        .and_then(|s| s.parse().ok())
        .or_else(|| server.active_window_for(session_id))
        .ok_or_else(|| ServerError::Command("no source window".into()))?;

    let dst_idx = resolve_window_idx(args, server, session_id)?;

    server.swap_window(session_id, src_idx, dst_idx)?;
    Ok(CommandResult::Ok)
}

/// move-window [-s src] [-t dst]
pub fn cmd_move_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    // Source session/window
    let src_session_id = if let Some(src) = get_option(args, "-s") {
        let name = src.split(':').next().unwrap_or(src);
        server
            .find_session_id(name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {name}")))?
    } else {
        server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?
    };

    let src_idx = get_option(args, "-s")
        .and_then(|s| s.split(':').nth(1))
        .and_then(|s| s.parse().ok())
        .or_else(|| server.active_window_for(src_session_id))
        .ok_or_else(|| ServerError::Command("no source window".into()))?;

    // Destination session/window
    let dst_session_id = resolve_session(args, server)?;
    let dst_idx = resolve_window_idx(args, server, dst_session_id)?;

    server.move_window(src_session_id, src_idx, dst_session_id, dst_idx)?;
    Ok(CommandResult::Ok)
}

/// rotate-window [-t target-window]
pub fn cmd_rotate_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    server.rotate_window(session_id, window_idx)?;
    Ok(CommandResult::Ok)
}

/// select-layout [-t target-window] layout-name
pub fn cmd_select_layout(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use crate::command::positional_args;

    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;

    let positional = positional_args(args, &["-t"]);
    let layout_name = positional.first().copied().unwrap_or("even-horizontal");

    server.select_layout(session_id, window_idx, layout_name)?;
    Ok(CommandResult::Ok)
}

/// respawn-window [-t target-window]
pub fn cmd_respawn_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    let pane_id = server
        .active_pane_id_for(session_id, window_idx)
        .ok_or_else(|| ServerError::Command("no active pane".into()))?;
    server.respawn_pane(session_id, window_idx, pane_id)?;
    Ok(CommandResult::Ok)
}

/// Resolve the session ID from -t argument or current client session.
///
/// Target format: `[session_name:]window_idx` or `session_name`.
/// A bare number (e.g. "1") is treated as a window index, not a session name,
/// so we fall back to the current session.
fn resolve_session(args: &[String], server: &dyn CommandServer) -> Result<u32, ServerError> {
    if let Some(target) = get_option(args, "-t") {
        if target.contains(':') {
            // Explicit "session:window" form
            let session_name = target.split(':').next().unwrap_or(target);
            server
                .find_session_id(session_name)
                .ok_or_else(|| ServerError::Command(format!("session not found: {session_name}")))
        } else if target.parse::<u32>().is_ok() {
            // Bare number — treat as window index, use current session
            server
                .client_session_id()
                .ok_or_else(|| ServerError::Command("no current session".into()))
        } else {
            // Non-numeric string — treat as session name
            server
                .find_session_id(target)
                .ok_or_else(|| ServerError::Command(format!("session not found: {target}")))
        }
    } else {
        server.client_session_id().ok_or_else(|| ServerError::Command("no current session".into()))
    }
}

/// Resolve the window index from -t argument or current active window.
fn resolve_window_idx(
    args: &[String],
    server: &dyn CommandServer,
    session_id: u32,
) -> Result<u32, ServerError> {
    if let Some(target) = get_option(args, "-t") {
        // Target could contain "session:window_idx"
        if let Some(idx_str) = target.split(':').nth(1) {
            idx_str
                .parse()
                .map_err(|_| ServerError::Command(format!("invalid window index: {idx_str}")))
        } else if target.parse::<u32>().is_ok() {
            // Bare window index
            Ok(target.parse().unwrap())
        } else {
            // It's a session name - use the session's active window
            server
                .active_window_for(session_id)
                .or_else(|| server.client_active_window())
                .ok_or_else(|| ServerError::Command("no current window".into()))
        }
    } else {
        server
            .active_window_for(session_id)
            .or_else(|| server.client_active_window())
            .ok_or_else(|| ServerError::Command("no current window".into()))
    }
}
