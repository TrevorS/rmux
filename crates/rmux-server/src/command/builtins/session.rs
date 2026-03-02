//! Session management commands: new-session, kill-session, list-sessions, has-session, rename-session.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::server::ServerError;

/// new-session [-d] [-s session-name] [-x width] [-y height]
pub fn cmd_new_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let detached = has_flag(args, "-d");
    let name = get_option(args, "-s").unwrap_or("0");
    let sx: u32 =
        get_option(args, "-x").and_then(|s| s.parse().ok()).unwrap_or_else(|| server.client_sx());
    let sy: u32 =
        get_option(args, "-y").and_then(|s| s.parse().ok()).unwrap_or_else(|| server.client_sy());

    // Check for duplicate name
    if server.has_session(name) {
        return Err(ServerError::Command(format!("duplicate session: {name}")));
    }

    let cwd = std::env::current_dir()
        .map_or_else(|_| "/".to_string(), |p| p.to_string_lossy().into_owned());

    let session_id = server.create_session(name, &cwd, sx, sy)?;

    if detached { Ok(CommandResult::Ok) } else { Ok(CommandResult::Attach(session_id)) }
}

/// kill-session [-t target-session]
pub fn cmd_kill_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let target = get_option(args, "-t").unwrap_or("0");
    server.kill_session(target)?;
    Ok(CommandResult::Ok)
}

/// list-sessions
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_sessions(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let sessions = server.list_sessions();
    if sessions.is_empty() {
        Ok(CommandResult::Output("no server running on this socket\n".to_string()))
    } else {
        Ok(CommandResult::Output(sessions.join("\n") + "\n"))
    }
}

/// has-session [-t target-session]
pub fn cmd_has_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let target = get_option(args, "-t").unwrap_or("0");
    if server.has_session(target) {
        Ok(CommandResult::Ok)
    } else {
        Err(ServerError::Command(format!("session not found: {target}")))
    }
}

/// rename-session [-t target-session] new-name
pub fn cmd_rename_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let target = get_option(args, "-t");
    // The new name is the last non-flag argument
    let new_name = args
        .iter()
        .rfind(|a| !a.starts_with('-'))
        .ok_or_else(|| ServerError::Command("rename-session: missing name".into()))?;

    let session_name = if let Some(t) = target {
        t.to_string()
    } else {
        // Use the attached session
        let sid = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        let sessions = server.list_sessions();
        sessions
            .first()
            .and_then(|s| s.split(':').next().map(str::to_string))
            .unwrap_or_else(|| sid.to_string())
    };

    server.rename_session(&session_name, new_name)?;
    Ok(CommandResult::Ok)
}
