//! Command dispatch system.
//!
//! Parses and executes tmux-compatible commands.

pub mod builtins;
#[cfg(test)]
mod phase4_tests;
#[cfg(test)]
mod phase5_tests;
#[cfg(test)]
mod test_helpers;

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
    /// Server should run a shell command asynchronously and return output.
    RunShell(String),
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

    // --- PTY I/O ---
    /// Write raw bytes to a specific pane's PTY.
    fn write_to_pane(
        &self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
        data: &[u8],
    ) -> Result<(), ServerError>;

    // --- Options ---
    /// Get a server-level option value as a string.
    fn get_server_option(&self, key: &str) -> Result<String, ServerError>;
    /// Set a server-level option.
    fn set_server_option(&mut self, key: &str, value: &str) -> Result<(), ServerError>;
    /// Set a session-level option.
    fn set_session_option(
        &mut self,
        session_id: u32,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError>;
    /// Set a window-level option.
    fn set_window_option(
        &mut self,
        session_id: u32,
        window_idx: u32,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError>;
    /// Show all options for a given scope. Returns "key value" lines.
    fn show_options(&self, scope: &str, target_id: Option<u32>) -> Vec<String>;

    // --- Key bindings ---
    /// Add a key binding.
    fn add_key_binding(
        &mut self,
        table: &str,
        key_name: &str,
        argv: Vec<String>,
    ) -> Result<(), ServerError>;
    /// Remove a key binding.
    fn remove_key_binding(&mut self, table: &str, key_name: &str) -> Result<(), ServerError>;

    // --- Config ---
    /// Execute a list of parsed config commands, returning any error messages.
    fn execute_config_commands(&mut self, commands: Vec<Vec<String>>) -> Vec<String>;

    // --- Capture ---
    /// Capture the visible content of a pane as text.
    fn capture_pane(
        &self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<String, ServerError>;

    // --- Resize ---
    /// Resize a pane by direction or absolute size.
    fn resize_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
        direction: Option<Direction>,
        amount: u32,
    ) -> Result<(), ServerError>;

    // --- Swap/Move ---
    fn swap_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        src: u32,
        dst: u32,
    ) -> Result<(), ServerError>;
    fn swap_window(
        &mut self,
        session_id: u32,
        src_idx: u32,
        dst_idx: u32,
    ) -> Result<(), ServerError>;
    fn move_window(
        &mut self,
        src_session_id: u32,
        src_idx: u32,
        dst_session_id: u32,
        dst_idx: u32,
    ) -> Result<(), ServerError>;
    fn break_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<u32, ServerError>;
    fn join_pane(
        &mut self,
        src_session_id: u32,
        src_window_idx: u32,
        src_pane_id: u32,
        dst_session_id: u32,
        dst_window_idx: u32,
        horizontal: bool,
    ) -> Result<(), ServerError>;
    fn last_pane(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError>;
    fn rotate_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError>;
    fn select_layout(
        &mut self,
        session_id: u32,
        window_idx: u32,
        layout_name: &str,
    ) -> Result<(), ServerError>;
    fn respawn_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<(), ServerError>;

    // --- Command prompt ---
    /// Put the current client into command prompt mode.
    fn enter_command_prompt(&mut self);

    // --- Copy mode ---
    /// Enter copy mode on the active pane.
    fn enter_copy_mode(&mut self) -> Result<(), ServerError>;
    /// Get the mode-keys setting for the active pane's window.
    fn pane_mode_keys(&self) -> String;

    // --- Paste buffers ---
    /// Add data to the paste buffer store (automatic naming).
    fn paste_buffer_add(&mut self, data: Vec<u8>);
    /// Paste the top buffer (or named buffer) to the active pane.
    fn paste_buffer(&self, name: Option<&str>) -> Result<(), ServerError>;
    /// List all paste buffers as human-readable strings.
    fn list_buffers(&self) -> Vec<String>;
    /// Get a buffer's contents by name.
    fn show_buffer(&self, name: &str) -> Result<String, ServerError>;
    /// Delete a buffer by name.
    fn delete_buffer(&mut self, name: &str) -> Result<(), ServerError>;
    /// Set a buffer's contents by name.
    fn set_buffer(&mut self, name: &str, data: &str) -> Result<(), ServerError>;

    // --- Info ---
    fn list_clients(&self) -> Vec<String>;
    fn list_all_commands(&self) -> Vec<String>;
    fn list_key_bindings(&self) -> Vec<String>;

    // --- Redraw ---
    fn mark_clients_redraw(&mut self, session_id: u32);
}

/// Look up a command by name or unambiguous prefix (matching tmux behavior).
pub fn find_command(name: &str) -> Option<&'static CommandEntry> {
    // Exact match first
    if let Some(cmd) = builtins::COMMANDS.iter().find(|cmd| cmd.name == name) {
        return Some(cmd);
    }
    // Unambiguous prefix match
    let matches: Vec<_> =
        builtins::COMMANDS.iter().filter(|cmd| cmd.name.starts_with(name)).collect();
    if matches.len() == 1 { Some(matches[0]) } else { None }
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

/// Collect positional arguments (those that aren't flags or flag values).
/// Flags are arguments starting with '-'. Flag values follow flags specified in `flags_with_values`.
pub fn positional_args<'a>(args: &'a [String], flags_with_values: &[&str]) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with('-') {
            // If this flag takes a value, skip the next arg too
            if flags_with_values.contains(&args[i].as_str()) {
                i += 1; // skip value
            }
            i += 1;
            continue;
        }
        result.push(args[i].as_str());
        i += 1;
    }
    result
}
