//! Environment commands: set-environment, show-environment.

use crate::command::{CommandResult, CommandServer, get_option, has_flag, positional_args};
use crate::server::ServerError;

/// set-environment [-F] [-g] [-h] [-r] [-u] [-t target-session] name [value]
/// -F: expand value as format
/// -h: mark as hidden
/// -r: remove from environment on use
pub fn cmd_set_environment(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let global = has_flag(args, "-g");
    let unset = has_flag(args, "-u");
    let _format = has_flag(args, "-F");
    let _hidden = has_flag(args, "-h");
    let _remove = has_flag(args, "-r");
    let session_id = if global { None } else { Some(resolve_env_session(args, server)?) };

    let positionals = positional_args(args, &["-t"]);
    if positionals.is_empty() {
        return Err(ServerError::Command(
            "usage: set-environment [-g] [-u] [-t target] name [value]".into(),
        ));
    }

    let name = positionals[0];

    if unset {
        server.unset_environment(session_id, name)?;
    } else {
        let value = positionals.get(1).copied().unwrap_or("");
        server.set_environment(session_id, name, value)?;
    }

    Ok(CommandResult::Ok)
}

/// show-environment [-g] [-h] [-s] [-t target-session] [name]
/// -h: include hidden vars
/// -s: output in shell format
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_environment(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let global = has_flag(args, "-g");
    let _hidden = has_flag(args, "-h");
    let _shell = has_flag(args, "-s");
    let session_id = if global { None } else { Some(resolve_env_session(args, server)?) };
    let positionals = positional_args(args, &["-t"]);

    let env_lines = server.show_environment(session_id);

    if let Some(name) = positionals.first() {
        // Show a specific variable
        let prefix = format!("{name}=");
        let found: Vec<_> = env_lines.iter().filter(|l| l.starts_with(&prefix)).cloned().collect();
        if found.is_empty() {
            Ok(CommandResult::Output(format!("-{name}\n")))
        } else {
            Ok(CommandResult::Output(found.join("\n") + "\n"))
        }
    } else if env_lines.is_empty() {
        Ok(CommandResult::Ok)
    } else {
        Ok(CommandResult::Output(env_lines.join("\n") + "\n"))
    }
}

fn resolve_env_session(args: &[String], server: &dyn CommandServer) -> Result<u32, ServerError> {
    if let Some(target) = get_option(args, "-t") {
        server
            .find_session_id(target)
            .ok_or_else(|| ServerError::Command(format!("session not found: {target}")))
    } else {
        server.client_session_id().ok_or_else(|| ServerError::Command("no current session".into()))
    }
}
