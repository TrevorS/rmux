//! Command dispatch system.
//!
//! Parses and executes tmux-compatible commands.

pub mod builtins;

use crate::server::ServerError;

/// Result of executing a command.
pub enum CommandResult {
    /// Command completed successfully with no output.
    Ok,
    /// Command produced output text (for list-sessions, show-options, etc.).
    Output(String),
    /// Client should attach to the given session.
    Attach(u32),
    /// Client should detach.
    Detach,
    /// Server should exit.
    Exit,
}

/// A registered command handler.
pub struct CommandEntry {
    /// Command name (e.g., "new-session").
    pub name: &'static str,
    /// Minimum number of arguments (excluding the command name).
    pub min_args: usize,
    /// Command handler function.
    pub handler: fn(args: &[String], server: &mut dyn CommandServer) -> Result<CommandResult, ServerError>,
    /// Usage string.
    pub usage: &'static str,
}

/// Trait providing the server interface needed by commands.
///
/// Commands interact with the server through this trait rather than
/// having direct access to the full Server struct.
pub trait CommandServer {
    fn create_session(&mut self, name: &str, cwd: &str, sx: u32, sy: u32) -> Result<u32, ServerError>;
    fn kill_session(&mut self, name: &str) -> Result<(), ServerError>;
    fn has_session(&self, name: &str) -> bool;
    fn list_sessions(&self) -> Vec<String>;
    fn find_session_id(&self, name: &str) -> Option<u32>;
}

/// Look up a command by name.
pub fn find_command(name: &str) -> Option<&'static CommandEntry> {
    builtins::COMMANDS.iter().find(|cmd| cmd.name == name)
}

/// Execute a command given its arguments.
pub fn execute_command(
    argv: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    if argv.is_empty() {
        return Err(ServerError::Command("no command specified".into()));
    }

    let cmd_name = &argv[0];
    let cmd = find_command(cmd_name).ok_or_else(|| {
        ServerError::Command(format!("unknown command: {cmd_name}"))
    })?;

    let args = &argv[1..];
    if args.len() < cmd.min_args {
        return Err(ServerError::Command(format!(
            "usage: {} {}",
            cmd.name, cmd.usage
        )));
    }

    (cmd.handler)(&argv[1..], server)
}
