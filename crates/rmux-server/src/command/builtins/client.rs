//! Client commands: attach-session, detach-client, switch-client, refresh-client, suspend-client.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::server::ServerError;

/// attach-session [-t target-session]
pub fn cmd_attach_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
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

    Ok(CommandResult::Attach(session_id))
}

/// detach-client
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_detach_client(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Detach)
}

/// switch-client [-t target-session] [-n] [-p]
pub fn cmd_switch_client(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let next = has_flag(args, "-n");
    let prev = has_flag(args, "-p");

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

/// refresh-client [-t target-client]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_refresh_client(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
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
    let _ = args;
    Ok(CommandResult::Suspend)
}
