//! Client commands: attach-session, detach-client, switch-client, refresh-client.

use crate::command::{CommandResult, CommandServer, get_option};
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

/// switch-client [-t target-session]
pub fn cmd_switch_client(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
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
