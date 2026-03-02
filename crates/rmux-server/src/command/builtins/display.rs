//! Display and information commands.

use crate::command::{CommandResult, CommandServer, has_flag};
use crate::server::ServerError;

/// display-message [-p] [message]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_display_message(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let print = has_flag(args, "-p");
    let message: String =
        args.iter().filter(|a| !a.starts_with('-')).cloned().collect::<Vec<_>>().join(" ");

    if print || !message.is_empty() {
        Ok(CommandResult::Output(message + "\n"))
    } else {
        Ok(CommandResult::Ok)
    }
}

/// list-commands
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_commands(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let commands = server.list_all_commands();
    Ok(CommandResult::Output(commands.join("\n") + "\n"))
}

/// list-keys [-T table]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_keys(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let bindings = server.list_key_bindings();
    if bindings.is_empty() {
        Ok(CommandResult::Output("(no bindings)\n".to_string()))
    } else {
        Ok(CommandResult::Output(bindings.join("\n") + "\n"))
    }
}

/// list-clients
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_clients(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let clients = server.list_clients();
    if clients.is_empty() {
        Ok(CommandResult::Output("(no clients)\n".to_string()))
    } else {
        Ok(CommandResult::Output(clients.join("\n") + "\n"))
    }
}
