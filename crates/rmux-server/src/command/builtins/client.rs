//! Client commands: attach-session, detach-client, switch-client, refresh-client, suspend-client.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::server::ServerError;

/// attach-session [-dErx] [-c working-directory] [-f flags] [-t target-session]
/// -d: detach other clients attached to the target session
pub fn cmd_attach_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let detach_others = has_flag(args, "-d");
    let _working_dir = get_option(args, "-c");
    let _no_update_env = has_flag(args, "-E");
    let _client_flags = get_option(args, "-f");
    let _read_only = has_flag(args, "-r");
    let _require_width = has_flag(args, "-x");

    // Find target session
    let target =
        args.iter().position(|a| a == "-t").and_then(|i| args.get(i + 1)).map(String::as_str);

    let session_id = if let Some(name) = target {
        server
            .find_session_id(name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {name}")))?
    } else {
        // Attach to most recent session (first one)
        let sessions = server.list_sessions();
        if sessions.is_empty() {
            return Err(ServerError::Command("no sessions".into()));
        }
        // Find any session
        server
            .find_session_id("0")
            .or_else(|| {
                // Try to find any session by iterating
                let list = server.list_sessions();
                list.first().and_then(|s| {
                    let name = s.split(':').next().unwrap_or("");
                    server.find_session_id(name)
                })
            })
            .ok_or_else(|| ServerError::Command("no sessions".into()))?
    };

    if detach_others {
        server.detach_other_clients()?;
    }

    Ok(CommandResult::Attach(session_id))
}

/// detach-client [-aP] [-E exit-message] [-s target-session] [-t target-client]
/// -a: detach all other clients on the same session (keep current attached)
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_detach_client(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _exit_message = get_option(args, "-E");
    let _target_session = get_option(args, "-s");
    let _target_client = get_option(args, "-t");
    let _print = has_flag(args, "-P");
    if has_flag(args, "-a") {
        server.detach_other_clients()?;
        return Ok(CommandResult::Ok);
    }
    Ok(CommandResult::Detach)
}

/// switch-client [-Elnpr] [-c target-client] [-t target-session] [-T key-table] [-Z]
pub fn cmd_switch_client(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let next = has_flag(args, "-n");
    let prev = has_flag(args, "-p");
    let last = has_flag(args, "-l");
    let _no_update_env = has_flag(args, "-E");
    let _read_only = has_flag(args, "-r");
    let _zoom = has_flag(args, "-Z");
    let _target_client = get_option(args, "-c");
    let _key_table = get_option(args, "-T");
    let _format = get_option(args, "-F");
    let _sort_order = get_option(args, "-O");

    if last {
        let last_id = server
            .client_last_session_id()
            .ok_or_else(|| ServerError::Command("no last session".into()))?;
        server.switch_client(last_id)?;
        return Ok(CommandResult::Ok);
    }

    if next || prev {
        // Switch to next/previous session
        let sessions = server.list_sessions();
        if sessions.is_empty() {
            return Err(ServerError::Command("no sessions".into()));
        }

        let current_id = server.client_session_id();
        let session_ids: Vec<u32> = sessions
            .iter()
            .filter_map(|s| {
                let name = s.split(':').next().unwrap_or("");
                server.find_session_id(name)
            })
            .collect();

        if session_ids.is_empty() {
            return Err(ServerError::Command("no sessions".into()));
        }

        let current_pos =
            current_id.and_then(|id| session_ids.iter().position(|&sid| sid == id)).unwrap_or(0);

        let new_pos = if next {
            (current_pos + 1) % session_ids.len()
        } else {
            (current_pos + session_ids.len() - 1) % session_ids.len()
        };

        server.switch_client(session_ids[new_pos])?;
        return Ok(CommandResult::Ok);
    }

    let target = get_option(args, "-t")
        .ok_or_else(|| ServerError::Command("switch-client: missing -t target".into()))?;

    let session_id = server
        .find_session_id(target)
        .ok_or_else(|| ServerError::Command(format!("session not found: {target}")))?;

    server.switch_client(session_id)?;
    Ok(CommandResult::Ok)
}

/// refresh-client [-cDlrRSU] [-A pane:visible-area] [-B subscription]
///   [-C widthxheight] [-f flags] [-L forward-to-client] [-t target-client]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_refresh_client(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _target_client = get_option(args, "-t");
    let _status_only = has_flag(args, "-S");
    let _size = get_option(args, "-C");
    let _request_clipboard = has_flag(args, "-l");
    let _visible_area = get_option(args, "-A");
    let _subscription = get_option(args, "-B");
    let _apply_visible = has_flag(args, "-D");
    let _client_flags = get_option(args, "-f");
    let _forward_to = get_option(args, "-L");
    let _reset_terminal = has_flag(args, "-R");
    let _unlock = has_flag(args, "-U");
    let _tracking_cursor = has_flag(args, "-c");
    let _report_size = has_flag(args, "-r");
    server.refresh_client();
    Ok(CommandResult::Ok)
}

/// suspend-client [-t target-client]
///
/// Suspend the client by sending SIGTSTP to itself.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_suspend_client(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _target = get_option(args, "-t");
    Ok(CommandResult::Suspend)
}
