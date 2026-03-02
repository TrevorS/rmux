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
    pub handler:
        fn(args: &[String], server: &mut dyn CommandServer) -> Result<CommandResult, ServerError>,
    /// Usage string.
    pub usage: &'static str,
}

/// Direction for pane navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Trait providing the server interface needed by commands.
///
/// Commands interact with the server through this trait rather than
/// having direct access to the full Server struct.
pub trait CommandServer {
    // --- Client context ---
    /// Set the client ID that is executing the current command.
    fn set_command_client(&mut self, client_id: u64);
    /// Get the client ID executing the current command.
    fn command_client_id(&self) -> u64;
    /// Get the session ID the command client is attached to.
    fn client_session_id(&self) -> Option<u32>;
    /// Get the active window index for the command client's session.
    fn client_active_window(&self) -> Option<u32>;
    /// Get the active pane ID for the command client's session/window.
    fn client_active_pane_id(&self) -> Option<u32>;
    /// Get the client's terminal width.
    fn client_sx(&self) -> u32;
    /// Get the client's terminal height.
    fn client_sy(&self) -> u32;

    // --- Session operations ---
    fn create_session(
        &mut self,
        name: &str,
        cwd: &str,
        sx: u32,
        sy: u32,
    ) -> Result<u32, ServerError>;
    fn kill_session(&mut self, name: &str) -> Result<(), ServerError>;
    fn has_session(&self, name: &str) -> bool;
    fn list_sessions(&self) -> Vec<String>;
    fn find_session_id(&self, name: &str) -> Option<u32>;
    fn rename_session(&mut self, name: &str, new_name: &str) -> Result<(), ServerError>;

    // --- Window operations ---
    fn create_window(
        &mut self,
        session_id: u32,
        name: Option<&str>,
        cwd: &str,
    ) -> Result<(u32, u32), ServerError>;
    fn kill_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError>;
    fn select_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError>;
    fn next_window(&mut self, session_id: u32) -> Result<(), ServerError>;
    fn previous_window(&mut self, session_id: u32) -> Result<(), ServerError>;
    fn last_window(&mut self, session_id: u32) -> Result<(), ServerError>;
    fn rename_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        name: &str,
    ) -> Result<(), ServerError>;
    fn list_windows(&self, session_id: u32) -> Vec<String>;

    // --- Pane operations ---
    fn split_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        horizontal: bool,
        cwd: &str,
    ) -> Result<u32, ServerError>;
    fn kill_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<(), ServerError>;
    fn select_pane_id(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<(), ServerError>;
    fn select_pane_direction(
        &mut self,
        session_id: u32,
        window_idx: u32,
        direction: Direction,
    ) -> Result<(), ServerError>;
    fn list_panes(&self, session_id: u32, window_idx: u32) -> Vec<String>;
    /// Get the active pane ID for a specific session's active window.
    fn active_pane_id_for(&self, session_id: u32, window_idx: u32) -> Option<u32>;
    /// Get the active window index for a given session.
    fn active_window_for(&self, session_id: u32) -> Option<u32>;

    // --- Info ---
    fn list_clients(&self) -> Vec<String>;
    fn list_all_commands(&self) -> Vec<String>;
    fn list_key_bindings(&self) -> Vec<String>;

    // --- Redraw ---
    fn mark_clients_redraw(&mut self, session_id: u32);
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
    let cmd = find_command(cmd_name)
        .ok_or_else(|| ServerError::Command(format!("unknown command: {cmd_name}")))?;

    let args = &argv[1..];
    if args.len() < cmd.min_args {
        return Err(ServerError::Command(format!("usage: {} {}", cmd.name, cmd.usage)));
    }

    (cmd.handler)(&argv[1..], server)
}

/// Parse a simple `-flag value` option from arguments.
pub fn get_option<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
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
pub fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}
