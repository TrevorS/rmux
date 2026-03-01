//! Server-level commands: kill-server, send-keys.

use crate::command::{CommandResult, CommandServer};
use crate::server::ServerError;

/// kill-server
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_kill_server(args: &[String], _server: &mut dyn CommandServer) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Exit)
}

/// send-keys [-t target-pane] key ...
///
/// For Phase 2, this is a simplified version that just collects the key arguments.
/// The actual key sending is handled by the server event loop.
pub fn cmd_send_keys(args: &[String], _server: &mut dyn CommandServer) -> Result<CommandResult, ServerError> {
    if args.is_empty() {
        return Err(ServerError::Command("send-keys: no keys specified".into()));
    }
    // Keys will be processed by the server event loop
    Ok(CommandResult::Ok)
}
