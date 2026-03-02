//! Pane management commands.

use crate::command::{CommandResult, CommandServer, Direction, get_option, has_flag};
use crate::server::ServerError;

/// split-window [-h] [-v] [-d]
/// -h: horizontal split (left-right panes, what tmux calls -h)
/// No flag or -v: vertical split (top-bottom panes)
pub fn cmd_split_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let horizontal = has_flag(args, "-h");

    let session_id = server
        .client_session_id()
        .ok_or_else(|| ServerError::Command("no current session".into()))?;
    let window_idx = server
        .client_active_window()
        .ok_or_else(|| ServerError::Command("no current window".into()))?;

    let cwd = std::env::current_dir()
        .map_or_else(|_| "/".to_string(), |p| p.to_string_lossy().into_owned());

    server.split_window(session_id, window_idx, horizontal, &cwd)?;

    Ok(CommandResult::Ok)
}

/// select-pane [-U] [-D] [-L] [-R] [-t target-pane]
pub fn cmd_select_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = server
        .client_session_id()
        .ok_or_else(|| ServerError::Command("no current session".into()))?;
    let window_idx = server
        .client_active_window()
        .ok_or_else(|| ServerError::Command("no current window".into()))?;

    // Direction flags
    if has_flag(args, "-U") {
        return server
            .select_pane_direction(session_id, window_idx, Direction::Up)
            .map(|()| CommandResult::Ok);
    }
    if has_flag(args, "-D") {
        return server
            .select_pane_direction(session_id, window_idx, Direction::Down)
            .map(|()| CommandResult::Ok);
    }
    if has_flag(args, "-L") {
        return server
            .select_pane_direction(session_id, window_idx, Direction::Left)
            .map(|()| CommandResult::Ok);
    }
    if has_flag(args, "-R") {
        return server
            .select_pane_direction(session_id, window_idx, Direction::Right)
            .map(|()| CommandResult::Ok);
    }

    // Target pane by ID
    if let Some(target) = get_option(args, "-t") {
        if target == "+" {
            // Next pane
            if let Some(current) = server.client_active_pane_id() {
                // We need to find the next pane - use the list
                let panes = server.list_panes(session_id, window_idx);
                if panes.len() > 1 {
                    // Just cycle through by using a simple approach:
                    // get all pane IDs from the listing
                    let ids: Vec<u32> = panes
                        .iter()
                        .filter_map(|s| {
                            s.strip_prefix('%')
                                .and_then(|rest| rest.split(':').next())
                                .and_then(|id| id.parse().ok())
                        })
                        .collect();
                    if let Some(pos) = ids.iter().position(|&id| id == current) {
                        let next = ids[(pos + 1) % ids.len()];
                        return server
                            .select_pane_id(session_id, window_idx, next)
                            .map(|()| CommandResult::Ok);
                    }
                }
            }
            return Ok(CommandResult::Ok);
        }

        // Parse pane ID (could be %N format)
        let pane_id: u32 = target
            .strip_prefix('%')
            .unwrap_or(target)
            .parse()
            .map_err(|_| ServerError::Command(format!("invalid pane: {target}")))?;
        return server.select_pane_id(session_id, window_idx, pane_id).map(|()| CommandResult::Ok);
    }

    Ok(CommandResult::Ok)
}

/// kill-pane [-t target-pane]
pub fn cmd_kill_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = server
        .client_session_id()
        .ok_or_else(|| ServerError::Command("no current session".into()))?;
    let window_idx = server
        .client_active_window()
        .ok_or_else(|| ServerError::Command("no current window".into()))?;

    let pane_id = if let Some(target) = get_option(args, "-t") {
        target
            .strip_prefix('%')
            .unwrap_or(target)
            .parse()
            .map_err(|_| ServerError::Command(format!("invalid pane: {target}")))?
    } else {
        server
            .client_active_pane_id()
            .ok_or_else(|| ServerError::Command("no current pane".into()))?
    };

    server.kill_pane(session_id, window_idx, pane_id)?;
    Ok(CommandResult::Ok)
}

/// list-panes [-t target-window]
pub fn cmd_list_panes(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = server
        .client_session_id()
        .ok_or_else(|| ServerError::Command("no current session".into()))?;

    let window_idx = if let Some(target) = get_option(args, "-t") {
        if let Some(idx_str) = target.split(':').nth(1) {
            idx_str
                .parse()
                .map_err(|_| ServerError::Command(format!("invalid window: {target}")))?
        } else {
            target.parse().unwrap_or_else(|_| server.client_active_window().unwrap_or(0))
        }
    } else {
        server
            .client_active_window()
            .ok_or_else(|| ServerError::Command("no current window".into()))?
    };

    let panes = server.list_panes(session_id, window_idx);
    if panes.is_empty() {
        Ok(CommandResult::Output("(no panes)\n".to_string()))
    } else {
        Ok(CommandResult::Output(panes.join("\n") + "\n"))
    }
}
