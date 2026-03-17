//! Session management commands: new-session, kill-session, list-sessions, has-session, rename-session.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::server::ServerError;

/// new-session [-AdDEPX] [-s session-name] [-n window-name] [-c start-directory]
///   [-e environment] [-F format] [-f flags] [-x width] [-y height]
pub fn cmd_new_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use crate::command::positional_args;

    let detached = has_flag(args, "-d");
    let name = get_option(args, "-s").unwrap_or("0");
    let window_name = get_option(args, "-n");
    let sx: u32 =
        get_option(args, "-x").and_then(|s| s.parse().ok()).unwrap_or_else(|| server.client_sx());
    let sy: u32 =
        get_option(args, "-y").and_then(|s| s.parse().ok()).unwrap_or_else(|| server.client_sy());
    let _detach_other = has_flag(args, "-D");
    let _no_update_env = has_flag(args, "-E");
    let _format = get_option(args, "-F");
    let _client_flags = get_option(args, "-f");
    let _print = has_flag(args, "-P");
    let _use_given_size = has_flag(args, "-X");
    let _environment = get_option(args, "-e");

    // -A: attach to existing session if it exists
    if has_flag(args, "-A") && server.has_session(name) {
        let session_id = server
            .find_session_id(name)
            .ok_or_else(|| ServerError::Command(format!("session not found: {name}")))?;
        return if detached {
            Ok(CommandResult::Ok)
        } else {
            Ok(CommandResult::Attach(session_id))
        };
    }

    // Check for duplicate name
    if server.has_session(name) {
        return Err(ServerError::Command(format!("duplicate session: {name}")));
    }

    let cwd = if let Some(dir) = get_option(args, "-c") {
        dir.to_string()
    } else {
        std::env::current_dir()
            .map_or_else(|_| "/".to_string(), |p| p.to_string_lossy().into_owned())
    };

    // Remaining positional args form the shell command for the initial window
    let shell_args = positional_args(args, &["-s", "-n", "-c", "-x", "-y", "-t", "-e", "-F", "-f"]);
    let shell_cmd = if shell_args.is_empty() { None } else { Some(shell_args.join(" ")) };

    let session_id = server.create_session(name, &cwd, sx, sy, shell_cmd.as_deref())?;

    // Rename initial window if -n was given
    if let Some(win_name) = window_name {
        if let Some(widx) = server.active_window_for(session_id) {
            server.rename_window(session_id, widx, win_name)?;
        }
    }

    if detached { Ok(CommandResult::Ok) } else { Ok(CommandResult::Attach(session_id)) }
}

/// kill-session [-aC] [-t target-session]
pub fn cmd_kill_session(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let kill_all_except = has_flag(args, "-a");
    let _clear_alerts = has_flag(args, "-C");
    let target = get_option(args, "-t");

    if kill_all_except {
        // Kill all sessions except the target (or current)
        let keep_name = if let Some(t) = target {
            t.to_string()
        } else {
            let sid = server
                .client_session_id()
                .ok_or_else(|| ServerError::Command("no current session".into()))?;
            server
                .session_name_for_id(sid)
                .ok_or_else(|| ServerError::Command("session not found".into()))?
        };
        let sessions = server.list_sessions();
        let names: Vec<String> = sessions
            .iter()
            .filter_map(|s| {
                let name = s.split(':').next().unwrap_or("").to_string();
                if name == keep_name { None } else { Some(name) }
            })
            .collect();
        for name in names {
            server.kill_session(&name)?;
        }
    } else {
        let name = target.unwrap_or("0");
        server.kill_session(name)?;
    }
    Ok(CommandResult::Ok)
}

/// list-sessions [-F format] [-f filter]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_sessions(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _format = get_option(args, "-F");
    let _filter = get_option(args, "-f");
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
        // Use the attached session — resolve by ID to get the actual name
        let sid = server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?;
        server
            .session_name_for_id(sid)
            .ok_or_else(|| ServerError::Command(format!("session not found: {sid}")))?
    };

    server.rename_session(&session_name, new_name)?;
    Ok(CommandResult::Ok)
}
