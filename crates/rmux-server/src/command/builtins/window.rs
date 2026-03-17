//! Window management commands.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::server::ServerError;

/// new-window [-a] [-b] [-d] [-e env] [-k] [-F format] [-P] [-S] [-n name] [-c start-directory] [-t target-session]
/// -a: insert after current window (not yet fully implemented)
/// -b: insert before current window (not yet fully implemented)
/// -e: set environment variable
/// -k: destroy existing window at target index
/// -F: format string for -P output
/// -P: print window info after creation
/// -S: select existing window if it exists
pub fn cmd_new_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use crate::command::positional_args;

    let _detached = has_flag(args, "-d");
    let _after = has_flag(args, "-a");
    let _before = has_flag(args, "-b");
    let _kill_existing = has_flag(args, "-k");
    let _select_existing = has_flag(args, "-S");
    let _env = get_option(args, "-e");
    let _format = get_option(args, "-F");
    let _print = has_flag(args, "-P");
    let name = get_option(args, "-n");

    let session_id = resolve_session(args, server)?;

    let cwd = if let Some(dir) = get_option(args, "-c") {
        dir.to_string()
    } else {
        std::env::current_dir()
            .map_or_else(|_| "/".to_string(), |p| p.to_string_lossy().into_owned())
    };

    // Remaining positional args form the shell command
    let shell_args = positional_args(args, &["-n", "-c", "-t", "-e", "-F"]);
    let shell_cmd = if shell_args.is_empty() { None } else { Some(shell_args.join(" ")) };

    let (window_idx, _pane_id) =
        server.create_window(session_id, name, &cwd, shell_cmd.as_deref())?;

    // Select the new window (unless -d)
    if !has_flag(args, "-d") {
        server.select_window(session_id, window_idx)?;
    }

    Ok(CommandResult::Ok)
}

/// kill-window [-a] [-t target-window]
pub fn cmd_kill_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;

    if has_flag(args, "-a") {
        // Kill all windows except the target
        let windows = server.list_windows(session_id);
        let indices: Vec<u32> = windows
            .iter()
            .filter_map(|w| {
                // Window list format: "idx: name ..."
                w.split(':').next().and_then(|s| s.trim().parse().ok())
            })
            .filter(|&idx| idx != window_idx)
            .collect();
        for idx in indices {
            server.kill_window(session_id, idx)?;
        }
    } else {
        server.kill_window(session_id, window_idx)?;
    }
    Ok(CommandResult::Ok)
}

/// select-window [-l] [-n] [-p] [-T] [-t target-window]
pub fn cmd_select_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;

    // -l: last window
    if has_flag(args, "-l") {
        return server.last_window(session_id).map(|()| CommandResult::Ok);
    }
    // -n: next window
    if has_flag(args, "-n") {
        return server.next_window(session_id).map(|()| CommandResult::Ok);
    }
    // -p: previous window
    if has_flag(args, "-p") {
        return server.previous_window(session_id).map(|()| CommandResult::Ok);
    }
    // -T: toggle — if already selected, select last; else select target
    if has_flag(args, "-T") {
        let window_idx = resolve_window_idx(args, server, session_id)?;
        let active = server.active_window_for(session_id);
        if active == Some(window_idx) {
            return server.last_window(session_id).map(|()| CommandResult::Ok);
        }
    }

    let window_idx = resolve_window_idx(args, server, session_id)?;
    server.select_window(session_id, window_idx)?;
    Ok(CommandResult::Ok)
}

/// next-window [-a] [-t target-session]
/// -a: move to the next window with an alert (activity/bell/silence)
pub fn cmd_next_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _alert = has_flag(args, "-a");
    let session_id = resolve_session(args, server)?;
    // TODO: -a should skip non-alert windows; for now behaves like plain next
    server.next_window(session_id)?;
    Ok(CommandResult::Ok)
}

/// previous-window [-a] [-t target-session]
/// -a: move to the previous window with an alert
pub fn cmd_previous_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _alert = has_flag(args, "-a");
    let session_id = resolve_session(args, server)?;
    // TODO: -a should skip non-alert windows; for now behaves like plain previous
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

/// list-windows [-a] [-F format] [-f filter] [-t target-session]
/// -a: list windows for all sessions
pub fn cmd_list_windows(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _format = get_option(args, "-F");
    let _filter = get_option(args, "-f");
    if has_flag(args, "-a") {
        let sessions = server.list_sessions();
        let mut output = Vec::new();
        for s in &sessions {
            let name = s.split(':').next().unwrap_or("");
            if let Some(sid) = server.find_session_id(name) {
                let windows = server.list_windows(sid);
                for w in windows {
                    output.push(format!("{name}: {w}"));
                }
            }
        }
        if output.is_empty() {
            Ok(CommandResult::Output("(no windows)\n".to_string()))
        } else {
            Ok(CommandResult::Output(output.join("\n") + "\n"))
        }
    } else {
        let session_id = resolve_session(args, server)?;
        let windows = server.list_windows(session_id);
        if windows.is_empty() {
            Ok(CommandResult::Output("(no windows)\n".to_string()))
        } else {
            Ok(CommandResult::Output(windows.join("\n") + "\n"))
        }
    }
}

/// swap-window [-d] [-s src] [-t dst]
pub fn cmd_swap_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let detached = has_flag(args, "-d");
    let session_id = resolve_session(args, server)?;

    let src_idx = get_option(args, "-s")
        .and_then(|s| s.split(':').next_back())
        .and_then(|s| s.parse().ok())
        .or_else(|| server.active_window_for(session_id))
        .ok_or_else(|| ServerError::Command("no source window".into()))?;

    let dst_idx = resolve_window_idx(args, server, session_id)?;

    server.swap_window(session_id, src_idx, dst_idx)?;

    // By default, select the destination window. -d keeps original selection.
    if !detached {
        server.select_window(session_id, dst_idx)?;
    }
    Ok(CommandResult::Ok)
}

/// move-window [-a] [-b] [-d] [-k] [-r] [-s src] [-t dst]
/// -a: insert after current window
/// -b: insert before current window
/// -d: don't select the destination window
/// -k: kill target window if it exists (allow overwrite)
/// -r: renumber windows sequentially after move
pub fn cmd_move_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _detached = has_flag(args, "-d");
    let _kill_existing = has_flag(args, "-k");
    let _renumber = has_flag(args, "-r");
    let _after = has_flag(args, "-a");
    let _before = has_flag(args, "-b");
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

/// rotate-window [-D] [-U] [-t target-window]
pub fn cmd_rotate_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    // -U rotates up (reverse), -D rotates down (default)
    let reverse = has_flag(args, "-U");
    server.rotate_window(session_id, window_idx, reverse)?;
    Ok(CommandResult::Ok)
}

/// select-layout [-E] [-n] [-o] [-p] [-t target-window] [layout-name]
/// -E: spread panes evenly (same as even-horizontal or even-vertical depending on orientation)
/// -n: next layout
/// -o: restore previous layout (undo last layout change)
/// -p: previous layout
pub fn cmd_select_layout(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use crate::command::positional_args;

    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    let _undo = has_flag(args, "-o");

    if has_flag(args, "-n") {
        let current = server.current_layout_name(session_id, window_idx);
        let idx = LAYOUT_CYCLE.iter().position(|&n| n == current).unwrap_or(0);
        let next = LAYOUT_CYCLE[(idx + 1) % LAYOUT_CYCLE.len()];
        server.select_layout(session_id, window_idx, next)?;
        return Ok(CommandResult::Ok);
    }
    if has_flag(args, "-p") {
        let current = server.current_layout_name(session_id, window_idx);
        let idx = LAYOUT_CYCLE.iter().position(|&n| n == current).unwrap_or(0);
        let prev = LAYOUT_CYCLE[(idx + LAYOUT_CYCLE.len() - 1) % LAYOUT_CYCLE.len()];
        server.select_layout(session_id, window_idx, prev)?;
        return Ok(CommandResult::Ok);
    }
    if has_flag(args, "-E") {
        server.select_layout(session_id, window_idx, "tiled")?;
        return Ok(CommandResult::Ok);
    }

    let positional = positional_args(args, &["-t"]);
    let layout_name = positional.first().copied().unwrap_or("even-horizontal");

    server.select_layout(session_id, window_idx, layout_name)?;
    Ok(CommandResult::Ok)
}

/// find-window [-C] [-N] [-r] [-T] [-Z] [-t target-session] match-string
/// -C: search window content
/// -N: search window names
/// -r: use regex for matching
/// -T: search window titles
/// -Z: zoom pane if necessary
pub fn cmd_find_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use crate::command::positional_args;

    let _content_search = has_flag(args, "-C");
    let _name_search = has_flag(args, "-N");
    let _regex = has_flag(args, "-r");
    let _title_search = has_flag(args, "-T");
    let _zoom = has_flag(args, "-Z");
    let session_id = resolve_session(args, server)?;
    let positional = positional_args(args, &["-t"]);
    let pattern = positional
        .first()
        .ok_or_else(|| ServerError::Command("find-window: missing match string".into()))?;

    let results = server.find_windows(session_id, pattern);
    if results.is_empty() {
        Err(ServerError::Command(format!("no windows matching: {pattern}")))
    } else {
        Ok(CommandResult::Output(results.join("\n") + "\n"))
    }
}

/// Layout names in cycle order.
const LAYOUT_CYCLE: &[&str] =
    &["even-horizontal", "even-vertical", "main-horizontal", "main-vertical", "tiled"];

/// next-layout [-t target-window]
pub fn cmd_next_layout(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;

    let current = server.current_layout_name(session_id, window_idx);
    let idx = LAYOUT_CYCLE.iter().position(|&n| n == current).unwrap_or(0);
    let next = LAYOUT_CYCLE[(idx + 1) % LAYOUT_CYCLE.len()];
    server.select_layout(session_id, window_idx, next)?;
    Ok(CommandResult::Ok)
}

/// previous-layout [-t target-window]
pub fn cmd_previous_layout(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;

    let current = server.current_layout_name(session_id, window_idx);
    let idx = LAYOUT_CYCLE.iter().position(|&n| n == current).unwrap_or(0);
    let prev = LAYOUT_CYCLE[(idx + LAYOUT_CYCLE.len() - 1) % LAYOUT_CYCLE.len()];
    server.select_layout(session_id, window_idx, prev)?;
    Ok(CommandResult::Ok)
}

/// respawn-window [-k] [-t target-window] [shell-command]
/// -k: kill the window before respawning (allow respawn even if not dead)
pub fn cmd_respawn_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use crate::command::positional_args;

    let _kill_first = has_flag(args, "-k");
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    let pane_id = server
        .active_pane_id_for(session_id, window_idx)
        .ok_or_else(|| ServerError::Command("no active pane".into()))?;

    // Remaining positional args form the shell command
    let shell_args = positional_args(args, &["-t"]);
    let shell_cmd = if shell_args.is_empty() { None } else { Some(shell_args.join(" ")) };

    server.respawn_pane(session_id, window_idx, pane_id, shell_cmd.as_deref())?;
    Ok(CommandResult::Ok)
}

/// link-window [-dk] [-s src-window] [-t dst-window]
///
/// Link (copy) a window from one session to another.
/// -d: do not select the linked window
/// -k: kill target window if it exists
pub fn cmd_link_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _detached = has_flag(args, "-d");
    let kill_existing = has_flag(args, "-k");

    // Parse source: -s session:window
    let src_target = get_option(args, "-s");
    let (src_session, src_window_idx) = if let Some(t) = src_target {
        resolve_target_pair(t, server)?
    } else {
        let sid = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let widx = server
            .active_window_for(sid)
            .ok_or_else(|| ServerError::Command("no current window".into()))?;
        (sid, widx)
    };

    // Parse destination: -t session:window or session
    let dst_target = get_option(args, "-t");
    let (dst_session, dst_window_idx) = if let Some(t) = dst_target {
        if t.contains(':') {
            let (s, w) = resolve_target_pair(t, server)?;
            (s, Some(w))
        } else {
            let sid = server
                .find_session_id(t)
                .ok_or_else(|| ServerError::Command(format!("session not found: {t}")))?;
            (sid, None)
        }
    } else {
        let sid = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        (sid, None)
    };

    server.link_window(src_session, src_window_idx, dst_session, dst_window_idx, kill_existing)?;
    Ok(CommandResult::Ok)
}

/// unlink-window [-k] [-t target-window]
///
/// Unlink a window. Since rmux does not support shared windows,
/// this behaves like kill-window when the window has only one link (always).
/// -k: kill even if it's the last window
pub fn cmd_unlink_window(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _kill_last = has_flag(args, "-k");
    let session_id = resolve_session(args, server)?;
    let window_idx = resolve_window_idx(args, server, session_id)?;
    server.unlink_window(session_id, window_idx)?;
    Ok(CommandResult::Ok)
}

/// Resolve a "session:window" target string into (session_id, window_idx).
fn resolve_target_pair(
    target: &str,
    server: &dyn CommandServer,
) -> Result<(u32, u32), ServerError> {
    if let Some(colon) = target.find(':') {
        let session_name = &target[..colon];
        let window_part = &target[colon + 1..];
        let session_id = server
            .find_session_id(session_name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {session_name}")))?;
        let window_idx: u32 = window_part
            .parse()
            .map_err(|_| ServerError::Command(format!("invalid window index: {window_part}")))?;
        Ok((session_id, window_idx))
    } else {
        // Just a session name — use its active window
        let session_id = server
            .find_session_id(target)
            .ok_or_else(|| ServerError::Command(format!("session not found: {target}")))?;
        let window_idx = server
            .active_window_for(session_id)
            .ok_or_else(|| ServerError::Command("no active window".into()))?;
        Ok((session_id, window_idx))
    }
}

/// move-pane [-s src-pane] [-t dst-pane]
///
/// Move a pane to a different window. Similar to join-pane.
pub fn cmd_move_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    // move-pane is equivalent to join-pane (moves pane between windows)
    super::pane::cmd_join_pane(args, server)
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
