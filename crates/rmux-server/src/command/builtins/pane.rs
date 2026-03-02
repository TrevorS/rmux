//! Pane management commands.

use crate::command::{CommandResult, CommandServer, Direction, get_option, has_flag};
use crate::server::ServerError;

/// split-window [-h] [-v] [-d] [-t target-window]
/// -h: horizontal split (left-right panes, what tmux calls -h)
/// No flag or -v: vertical split (top-bottom panes)
pub fn cmd_split_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let horizontal = has_flag(args, "-h");

    let (session_id, window_idx) = resolve_session_window(args, server)?;

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
    let (session_id, window_idx) = resolve_session_window(args, server)?;

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

    // Target pane by ID (non-direction -t usage)
    if let Some(target) = get_option(args, "-t") {
        // Skip if it's a session:window target (already resolved above)
        if target.contains(':') || server.find_session_id(target).is_some() {
            return Ok(CommandResult::Ok);
        }

        if target == "+" {
            // Next pane
            if let Some(current) = server.client_active_pane_id() {
                let panes = server.list_panes(session_id, window_idx);
                if panes.len() > 1 {
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
    let (session_id, window_idx) = resolve_session_window(args, server)?;

    let pane_id = if let Some(target) = get_option(args, "-t") {
        // Skip session:window targets - use active pane of that window
        if target.contains(':') || server.find_session_id(target).is_some() {
            server
                .active_pane_id_for(session_id, window_idx)
                .ok_or_else(|| ServerError::Command("no current pane".into()))?
        } else {
            target
                .strip_prefix('%')
                .unwrap_or(target)
                .parse()
                .map_err(|_| ServerError::Command(format!("invalid pane: {target}")))?
        }
    } else {
        server
            .active_pane_id_for(session_id, window_idx)
            .or_else(|| server.client_active_pane_id())
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
    let (session_id, window_idx) = resolve_session_window(args, server)?;

    let panes = server.list_panes(session_id, window_idx);
    if panes.is_empty() {
        Ok(CommandResult::Output("(no panes)\n".to_string()))
    } else {
        Ok(CommandResult::Output(panes.join("\n") + "\n"))
    }
}

/// capture-pane [-p] [-t target-pane]
pub fn cmd_capture_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;

    let content = server.capture_pane(session_id, window_idx, pane_id)?;

    // -p flag means print to stdout (always do this for now)
    Ok(CommandResult::Output(content))
}

/// resize-pane [-U|-D|-L|-R] [-t target-pane] [amount]
pub fn cmd_resize_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;

    let direction = if has_flag(args, "-U") {
        Some(Direction::Up)
    } else if has_flag(args, "-D") {
        Some(Direction::Down)
    } else if has_flag(args, "-L") {
        Some(Direction::Left)
    } else if has_flag(args, "-R") {
        Some(Direction::Right)
    } else {
        None
    };

    // Get amount from positional arg (last non-flag arg that parses as a number)
    let amount = args
        .iter()
        .rev()
        .find(|a| !a.starts_with('-'))
        .and_then(|a| a.parse::<u32>().ok())
        .unwrap_or(1);

    server.resize_pane(session_id, window_idx, pane_id, direction, amount)?;
    Ok(CommandResult::Ok)
}

/// swap-pane [-U] [-D] [-t target-pane]
pub fn cmd_swap_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let src_pane = resolve_pane_id(args, server, session_id, window_idx)?;

    // -U or -D: swap with pane in that direction
    let direction = if has_flag(args, "-U") {
        Some(Direction::Up)
    } else if has_flag(args, "-D") {
        Some(Direction::Down)
    } else {
        None
    };

    if let Some(dir) = direction {
        // Find the pane in the given direction and swap
        // For now, use the adjacent pane IDs from list_panes
        let panes = server.list_panes(session_id, window_idx);
        let ids: Vec<u32> = panes
            .iter()
            .filter_map(|s| {
                s.strip_prefix('%')
                    .and_then(|rest| rest.split(':').next())
                    .and_then(|id| id.parse().ok())
            })
            .collect();

        if let Some(pos) = ids.iter().position(|&id| id == src_pane) {
            let dst_idx = match dir {
                Direction::Up | Direction::Left => {
                    if pos > 0 { pos - 1 } else { ids.len() - 1 }
                }
                Direction::Down | Direction::Right => (pos + 1) % ids.len(),
            };
            let dst_pane = ids[dst_idx];
            server.swap_pane(session_id, window_idx, src_pane, dst_pane)?;
        }
    }

    Ok(CommandResult::Ok)
}

/// break-pane [-t target-pane]
pub fn cmd_break_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;
    server.break_pane(session_id, window_idx, pane_id)?;
    Ok(CommandResult::Ok)
}

/// join-pane [-h] [-s src-pane] [-t dst-pane]
pub fn cmd_join_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let horizontal = has_flag(args, "-h");

    // Source: -s flag or current pane
    let (src_session, src_window) = if let Some(src_target) = get_option(args, "-s") {
        resolve_target(src_target, server)?
    } else {
        let sid = server.client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let wid = server.client_active_window()
            .ok_or_else(|| ServerError::Command("no current window".into()))?;
        (sid, wid)
    };
    let src_pane = server.active_pane_id_for(src_session, src_window)
        .ok_or_else(|| ServerError::Command("no source pane".into()))?;

    // Destination: -t flag or current
    let (dst_session, dst_window) = resolve_session_window(args, server)?;

    server.join_pane(src_session, src_window, src_pane, dst_session, dst_window, horizontal)?;
    Ok(CommandResult::Ok)
}

/// last-pane [-t target-window]
pub fn cmd_last_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    server.last_pane(session_id, window_idx)?;
    Ok(CommandResult::Ok)
}

/// respawn-pane [-t target-pane]
pub fn cmd_respawn_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;
    server.respawn_pane(session_id, window_idx, pane_id)?;
    Ok(CommandResult::Ok)
}

/// Helper: resolve a pane ID from args or use active pane.
fn resolve_pane_id(
    args: &[String],
    server: &dyn CommandServer,
    session_id: u32,
    window_idx: u32,
) -> Result<u32, ServerError> {
    if let Some(target) = get_option(args, "-t") {
        if let Some(stripped) = target.strip_prefix('%') {
            return stripped.parse()
                .map_err(|_| ServerError::Command(format!("invalid pane: {target}")));
        }
        // For session:window or session targets, use active pane
        if target.contains(':') || server.find_session_id(target).is_some() {
            return server
                .active_pane_id_for(session_id, window_idx)
                .ok_or_else(|| ServerError::Command("no current pane".into()));
        }
    }
    server
        .active_pane_id_for(session_id, window_idx)
        .or_else(|| server.client_active_pane_id())
        .ok_or_else(|| ServerError::Command("no current pane".into()))
}

/// Helper: resolve a "session:window" target string.
fn resolve_target(target: &str, server: &dyn CommandServer) -> Result<(u32, u32), ServerError> {
    if let Some(colon) = target.find(':') {
        let session_name = &target[..colon];
        let window_str = &target[colon + 1..];
        let session_id = server.find_session_id(session_name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {session_name}")))?;
        let window_idx = window_str.parse()
            .map_err(|_| ServerError::Command(format!("invalid window: {window_str}")))?;
        Ok((session_id, window_idx))
    } else if let Some(session_id) = server.find_session_id(target) {
        let window_idx = server.active_window_for(session_id).unwrap_or(0);
        Ok((session_id, window_idx))
    } else {
        Err(ServerError::Command(format!("session not found: {target}")))
    }
}

/// Resolve session ID and window index from -t argument or current client context.
/// -t can be "session_name", "session_name:window_idx", or bare "window_idx".
fn resolve_session_window(
    args: &[String],
    server: &dyn CommandServer,
) -> Result<(u32, u32), ServerError> {
    if let Some(target) = get_option(args, "-t") {
        if let Some(colon_pos) = target.find(':') {
            // "session:window" format
            let session_name = &target[..colon_pos];
            let window_str = &target[colon_pos + 1..];
            let session_id = server.find_session_id(session_name).ok_or_else(|| {
                ServerError::Command(format!("session not found: {session_name}"))
            })?;
            let window_idx = window_str
                .parse()
                .map_err(|_| ServerError::Command(format!("invalid window index: {window_str}")))?;
            Ok((session_id, window_idx))
        } else if let Some(session_id) = server.find_session_id(target) {
            // Just a session name - use that session's active window
            let window_idx = server
                .active_window_for(session_id)
                .or_else(|| server.client_active_window())
                .unwrap_or(0);
            Ok((session_id, window_idx))
        } else {
            // Maybe a bare window index
            let session_id = server
                .client_session_id()
                .ok_or_else(|| ServerError::Command("no current session".into()))?;
            let window_idx = target
                .parse()
                .map_err(|_| ServerError::Command(format!("session not found: {target}")))?;
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
