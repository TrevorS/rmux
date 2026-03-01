//! Session management commands: new-session, kill-session, list-sessions, has-session.

use crate::command::{CommandResult, CommandServer};
use crate::server::ServerError;

/// Parse a simple `-flag value` option from arguments.
fn get_option<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(&args[i + 1]);
        }
        i += 1;
    }
    None
}

/// Check if a flag is present in arguments.
fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// new-session [-d] [-s session-name] [-x width] [-y height]
pub fn cmd_new_session(args: &[String], server: &mut dyn CommandServer) -> Result<CommandResult, ServerError> {
    let detached = has_flag(args, "-d");
    let name = get_option(args, "-s").unwrap_or("0");
    let sx: u32 = get_option(args, "-x").and_then(|s| s.parse().ok()).unwrap_or(80);
    let sy: u32 = get_option(args, "-y").and_then(|s| s.parse().ok()).unwrap_or(24);

    // Check for duplicate name
    if server.has_session(name) {
        return Err(ServerError::Command(format!(
            "duplicate session: {name}"
        )));
    }

    let cwd = std::env::current_dir()
        .map_or_else(|_| "/".to_string(), |p| p.to_string_lossy().into_owned());

    let session_id = server.create_session(name, &cwd, sx, sy)?;

    if detached {
        Ok(CommandResult::Ok)
    } else {
        Ok(CommandResult::Attach(session_id))
    }
}

/// kill-session [-t target-session]
pub fn cmd_kill_session(args: &[String], server: &mut dyn CommandServer) -> Result<CommandResult, ServerError> {
    let target = get_option(args, "-t").unwrap_or("0");
    server.kill_session(target)?;
    Ok(CommandResult::Ok)
}

/// list-sessions
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_sessions(args: &[String], server: &mut dyn CommandServer) -> Result<CommandResult, ServerError> {
    let _ = args;
    let sessions = server.list_sessions();
    if sessions.is_empty() {
        Ok(CommandResult::Output("no server running on this socket\n".to_string()))
    } else {
        Ok(CommandResult::Output(sessions.join("\n") + "\n"))
    }
}

/// has-session [-t target-session]
pub fn cmd_has_session(args: &[String], server: &mut dyn CommandServer) -> Result<CommandResult, ServerError> {
    let target = get_option(args, "-t").unwrap_or("0");
    if server.has_session(target) {
        Ok(CommandResult::Ok)
    } else {
        Err(ServerError::Command(format!(
            "session not found: {target}"
        )))
    }
}
