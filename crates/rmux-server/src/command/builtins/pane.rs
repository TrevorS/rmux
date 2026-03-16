//! Pane management commands.

use crate::command::{CommandResult, CommandServer, Direction, SplitSize, get_option, has_flag};
use crate::server::ServerError;

/// split-window [-h] [-v] [-d] [-l size] [-p percentage] [-c start-directory] [-t target-window]
/// -h: horizontal split (left-right panes, what tmux calls -h)
/// No flag or -v: vertical split (top-bottom panes)
/// -l: specify new pane size in lines/columns
/// -p: specify new pane size as percentage
pub fn cmd_split_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let horizontal = has_flag(args, "-h");

    let (session_id, window_idx) = resolve_session_window(args, server)?;

    let cwd = if let Some(dir) = get_option(args, "-c") {
        dir.to_string()
    } else {
        std::env::current_dir()
            .map_or_else(|_| "/".to_string(), |p| p.to_string_lossy().into_owned())
    };

    let size = if let Some(l) = get_option(args, "-l") {
        if let Some(pct) = l.strip_suffix('%') {
            pct.parse().ok().map(SplitSize::Percent)
        } else {
            l.parse().ok().map(SplitSize::Lines)
        }
    } else if let Some(p) = get_option(args, "-p") {
        p.parse().ok().map(SplitSize::Percent)
    } else {
        None
    };

    server.split_window(session_id, window_idx, horizontal, &cwd, size)?;

    Ok(CommandResult::Ok)
}

/// select-pane [-D] [-d] [-e] [-L] [-l] [-M] [-m] [-R] [-T title] [-U] [-Z] [-t target-pane]
/// -m: set the marked pane (not yet stored)
/// -M: clear the marked pane (not yet stored)
/// -T: set the pane title
/// -Z: toggle zoom on the target pane
/// -d: disable input to the pane
/// -e: enable input to the pane
pub fn cmd_select_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;

    // -Z: toggle zoom
    if has_flag(args, "-Z") {
        let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;
        server.toggle_zoom(session_id, window_idx, pane_id)?;
        return Ok(CommandResult::Ok);
    }

    // -m/-M: mark/unmark pane (parse but no-op for now)
    if has_flag(args, "-m") || has_flag(args, "-M") {
        return Ok(CommandResult::Ok);
    }

    // -T: set pane title
    if let Some(_title) = get_option(args, "-T") {
        // TODO: store pane title
        return Ok(CommandResult::Ok);
    }

    // -d/-e: disable/enable input (parse but no-op for now)
    if has_flag(args, "-d") || has_flag(args, "-e") {
        return Ok(CommandResult::Ok);
    }

    // -l: last (previously active) pane
    if has_flag(args, "-l") {
        return server.last_pane(session_id, window_idx).map(|()| CommandResult::Ok);
    }

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

/// kill-pane [-a] [-t target-pane]
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

    if has_flag(args, "-a") {
        // Kill all panes except the target
        let panes = server.list_panes(session_id, window_idx);
        let ids: Vec<u32> = panes
            .iter()
            .filter_map(|s| {
                s.strip_prefix('%')
                    .and_then(|rest| rest.split(':').next())
                    .and_then(|id| id.parse().ok())
            })
            .filter(|&id| id != pane_id)
            .collect();
        for id in ids {
            server.kill_pane(session_id, window_idx, id)?;
        }
    } else {
        server.kill_pane(session_id, window_idx, pane_id)?;
    }
    Ok(CommandResult::Ok)
}

/// list-panes [-a] [-s] [-t target-window]
/// -a: list panes for all windows in all sessions
/// -s: list panes for all windows in the target session
pub fn cmd_list_panes(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    if has_flag(args, "-a") {
        let sessions = server.list_sessions();
        let mut output = Vec::new();
        for s in &sessions {
            let name = s.split(':').next().unwrap_or("");
            if let Some(sid) = server.find_session_id(name) {
                let windows = server.list_windows(sid);
                for w in &windows {
                    let widx: u32 =
                        w.split(':').next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                    let panes = server.list_panes(sid, widx);
                    for p in panes {
                        output.push(format!("{name}:{widx}: {p}"));
                    }
                }
            }
        }
        if output.is_empty() {
            Ok(CommandResult::Output("(no panes)\n".to_string()))
        } else {
            Ok(CommandResult::Output(output.join("\n") + "\n"))
        }
    } else if has_flag(args, "-s") {
        let session_id = resolve_session_window(args, server)?.0;
        let windows = server.list_windows(session_id);
        let mut output = Vec::new();
        for w in &windows {
            let widx: u32 = w.split(':').next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            let panes = server.list_panes(session_id, widx);
            for p in panes {
                output.push(format!("{widx}: {p}"));
            }
        }
        if output.is_empty() {
            Ok(CommandResult::Output("(no panes)\n".to_string()))
        } else {
            Ok(CommandResult::Output(output.join("\n") + "\n"))
        }
    } else {
        let (session_id, window_idx) = resolve_session_window(args, server)?;
        let panes = server.list_panes(session_id, window_idx);
        if panes.is_empty() {
            Ok(CommandResult::Output("(no panes)\n".to_string()))
        } else {
            Ok(CommandResult::Output(panes.join("\n") + "\n"))
        }
    }
}

/// capture-pane [-p] [-q] [-J] [-e] [-S start] [-E end] [-b buffer-name] [-t target-pane]
/// -p: output to stdout (default behavior)
/// -q: quiet, suppress errors
/// -J: join wrapped lines (strip trailing whitespace)
/// -e: include escape sequences (not yet supported)
/// -S/-E: start/end line (negative = scrollback, 0 = top of visible)
/// -b: store in named buffer instead of stdout
pub fn cmd_capture_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _quiet = has_flag(args, "-q");
    let join_lines = has_flag(args, "-J");
    let _escapes = has_flag(args, "-e");
    let _start_line = get_option(args, "-S");
    let _end_line = get_option(args, "-E");
    let buffer_name = get_option(args, "-b");
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;

    let mut content = server.capture_pane(session_id, window_idx, pane_id)?;

    if join_lines {
        // Strip trailing whitespace from each line to join wrapped lines
        content = content.lines().map(str::trim_end).collect::<Vec<_>>().join("\n") + "\n";
    }

    if let Some(name) = buffer_name {
        server.set_buffer(name, &content)?;
        Ok(CommandResult::Ok)
    } else {
        Ok(CommandResult::Output(content))
    }
}

/// resize-pane [-U|-D|-L|-R|-Z] [-t target-pane] [amount]
pub fn cmd_resize_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;

    // -Z toggles zoom
    if has_flag(args, "Z") {
        server.toggle_zoom(session_id, window_idx, pane_id)?;
        return Ok(CommandResult::Ok);
    }

    let direction = if has_flag(args, "U") {
        Some(Direction::Up)
    } else if has_flag(args, "D") {
        Some(Direction::Down)
    } else if has_flag(args, "L") {
        Some(Direction::Left)
    } else if has_flag(args, "R") {
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

/// swap-pane [-U] [-D] [-d] [-Z] [-s src-pane] [-t target-pane]
/// -Z: unzoom the window before swapping
pub fn cmd_swap_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    if has_flag(args, "-Z") {
        server.unzoom_window(session_id, window_idx)?;
    }

    // -s: explicit source pane, otherwise use active
    let src_pane = if let Some(src) = get_option(args, "-s") {
        src.strip_prefix('%')
            .unwrap_or(src)
            .parse()
            .map_err(|_| ServerError::Command(format!("invalid pane: {src}")))?
    } else {
        resolve_pane_id(args, server, session_id, window_idx)?
    };

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
                    if pos > 0 {
                        pos - 1
                    } else {
                        ids.len() - 1
                    }
                }
                Direction::Down | Direction::Right => (pos + 1) % ids.len(),
            };
            let dst_pane = ids[dst_idx];
            server.swap_pane(session_id, window_idx, src_pane, dst_pane)?;
        }
    }

    Ok(CommandResult::Ok)
}

/// break-pane [-d] [-t target-pane]
pub fn cmd_break_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let detached = has_flag(args, "-d");
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    let pane_id = resolve_pane_id(args, server, session_id, window_idx)?;
    let new_window_idx = server.break_pane(session_id, window_idx, pane_id)?;
    if !detached {
        server.select_window(session_id, new_window_idx)?;
    }
    Ok(CommandResult::Ok)
}

/// join-pane [-h] [-v] [-d] [-l size] [-p percentage] [-s src-pane] [-t dst-pane]
/// -h: horizontal split (left-right)
/// -v: vertical split (top-bottom, default)
/// -d: don't change the active pane
pub fn cmd_join_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _detached = has_flag(args, "-d");
    // -h means horizontal, -v means vertical (default if neither specified)
    let horizontal = has_flag(args, "-h");

    // Source: -s flag or current pane
    let (src_session, src_window) = if let Some(src_target) = get_option(args, "-s") {
        resolve_target(src_target, server)?
    } else {
        let sid = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let wid = server
            .client_active_window()
            .ok_or_else(|| ServerError::Command("no current window".into()))?;
        (sid, wid)
    };
    let src_pane = server
        .active_pane_id_for(src_session, src_window)
        .ok_or_else(|| ServerError::Command("no source pane".into()))?;

    // Destination: -t flag or current
    let (dst_session, dst_window) = resolve_session_window(args, server)?;

    server.join_pane(src_session, src_window, src_pane, dst_session, dst_window, horizontal)?;
    Ok(CommandResult::Ok)
}

/// last-pane [-Z] [-t target-window]
/// -Z: unzoom the window if it is zoomed
pub fn cmd_last_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let unzoom = has_flag(args, "-Z");
    let (session_id, window_idx) = resolve_session_window(args, server)?;
    if unzoom {
        server.unzoom_window(session_id, window_idx)?;
    }
    server.last_pane(session_id, window_idx)?;
    Ok(CommandResult::Ok)
}

/// respawn-pane [-k] [-t target-pane]
/// -k: kill the pane before respawning (allow respawn even if not dead)
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
            return stripped
                .parse()
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
        let session_id = server
            .find_session_id(session_name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {session_name}")))?;
        let window_idx = window_str
            .parse()
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
