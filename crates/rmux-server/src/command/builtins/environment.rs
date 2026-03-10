//! Environment commands: set-environment, show-environment.

use crate::command::{CommandResult, CommandServer, get_option, has_flag, positional_args};
use crate::server::ServerError;

/// set-environment [-g] [-u] [-t target-session] name [value]
pub fn cmd_set_environment(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let unset = has_flag(args, "-u");
    let session_id = resolve_env_session(args, server)?;

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

/// show-environment [-g] [-t target-session] [name]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_environment(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = resolve_env_session(args, server)?;
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
