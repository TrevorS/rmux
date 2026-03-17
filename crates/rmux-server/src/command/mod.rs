//! Command dispatch system.
//!
//! Parses and executes tmux-compatible commands.

pub mod builtins;
#[cfg(test)]
mod phase4_tests;
#[cfg(test)]
mod phase5_tests;
#[cfg(test)]
mod phase6_tests;
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
    /// Server should run a shell command in the background (no output capture).
    RunShellBackground(String),
    /// Client should be suspended (SIGTSTP).
    Suspend,
    /// Show a timed message in the status bar (display-message without -p).
    TimedMessage(String),
    /// Open an overlay on the client (choose-tree, display-menu, etc.).
    Overlay(crate::overlay::OverlayState),
    /// Spawn a popup window with an embedded PTY process.
    SpawnPopup(PopupConfig),
}

/// Configuration for spawning a popup window.
pub struct PopupConfig {
    /// X position (column offset).
    pub x: u32,
    /// Y position (row offset).
    pub y: u32,
    /// Content area width.
    pub width: u32,
    /// Content area height.
    pub height: u32,
    /// Title for the popup border.
    pub title: String,
    /// Whether to draw a border.
    pub has_border: bool,
    /// Close the popup when the command exits.
    pub close_on_exit: bool,
    /// Shell command to run (None = default shell).
    pub command: Option<String>,
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

/// Info about a window within a session tree: (index, name, is_active, pane_count).
pub type WindowTreeInfo = (u32, String, bool, usize);

/// Info about a session in the tree: (session_name, attached_count, windows).
pub type SessionTreeInfo = (String, usize, Vec<WindowTreeInfo>);

/// Direction for pane navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Size specification for split-window.
#[derive(Debug, Clone, Copy)]
pub enum SplitSize {
    /// Absolute number of lines/columns.
    Lines(u32),
    /// Percentage of the available space.
    Percent(u32),
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
        shell_cmd: Option<&str>,
    ) -> Result<u32, ServerError>;
    fn kill_session(&mut self, name: &str) -> Result<(), ServerError>;
    fn has_session(&self, name: &str) -> bool;
    fn list_sessions(&self) -> Vec<String>;
    fn find_session_id(&self, name: &str) -> Option<u32>;
    fn session_name_for_id(&self, id: u32) -> Option<String>;
    fn rename_session(&mut self, name: &str, new_name: &str) -> Result<(), ServerError>;

    // --- Window operations ---
    fn create_window(
        &mut self,
        session_id: u32,
        name: Option<&str>,
        cwd: &str,
        shell_cmd: Option<&str>,
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
    /// Link (copy) a window from one session to another.
    /// Creates a new window in `dst_session` with a fresh pane running the default shell.
    /// True shared-window semantics are not yet supported.
    fn link_window(
        &mut self,
        src_session: u32,
        src_window_idx: u32,
        dst_session: u32,
        dst_window_idx: Option<u32>,
        kill_existing: bool,
    ) -> Result<u32, ServerError>;
    /// Unlink a window from a session. Since rmux does not support shared windows,
    /// this is equivalent to killing the window.
    fn unlink_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError>;

    // --- Pane operations ---
    fn split_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        horizontal: bool,
        cwd: &str,
        size: Option<SplitSize>,
        shell_cmd: Option<&str>,
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
    /// Unset a server-level option (revert to default).
    fn unset_server_option(&mut self, key: &str) -> Result<(), ServerError>;
    /// Append to a server-level string option.
    fn append_server_option(&mut self, key: &str, value: &str) -> Result<(), ServerError>;
    /// Set a session-level option.
    fn set_session_option(
        &mut self,
        session_id: u32,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError>;
    /// Unset a session-level option.
    fn unset_session_option(&mut self, session_id: u32, key: &str) -> Result<(), ServerError>;
    /// Append to a session-level string option.
    fn append_session_option(
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
    /// Unset a window-level option.
    fn unset_window_option(
        &mut self,
        session_id: u32,
        window_idx: u32,
        key: &str,
    ) -> Result<(), ServerError>;
    /// Append to a window-level string option.
    fn append_window_option(
        &mut self,
        session_id: u32,
        window_idx: u32,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError>;
    /// Check if a server-level option exists.
    fn has_server_option(&self, key: &str) -> bool;
    /// Check if a session-level option exists (local, not inherited).
    fn has_session_option(&self, session_id: u32, key: &str) -> bool;
    /// Check if a window-level option exists (local, not inherited).
    fn has_window_option(&self, session_id: u32, window_idx: u32, key: &str) -> bool;
    /// Show all options for a given scope. Returns "key value" lines.
    fn show_options(&self, scope: &str, target_id: Option<u32>) -> Vec<String>;

    // --- Key bindings ---
    /// Add a key binding.
    fn add_key_binding(
        &mut self,
        table: &str,
        key_name: &str,
        argv: Vec<String>,
        repeatable: bool,
        note: Option<String>,
    ) -> Result<(), ServerError>;
    /// Remove a key binding.
    fn remove_key_binding(&mut self, table: &str, key_name: &str) -> Result<(), ServerError>;
    /// Remove all key bindings from a table.
    fn clear_key_table(&mut self, table: &str);

    // --- Config ---
    /// Build a config context for conditional evaluation and variable expansion.
    fn build_config_context(&self) -> crate::config::ConfigContext;
    /// Get current hidden vars (for nested source-file propagation).
    fn get_config_hidden_vars(&self) -> std::collections::HashMap<String, String>;
    /// Set hidden vars (for nested source-file propagation).
    fn set_config_hidden_vars(&mut self, vars: std::collections::HashMap<String, String>);
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
    /// Resize a window to the given dimensions.
    fn resize_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        sx: Option<u32>,
        sy: Option<u32>,
    ) -> Result<(), ServerError>;
    /// Resize a pane by direction or absolute size.
    fn resize_pane(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
        direction: Option<Direction>,
        amount: u32,
    ) -> Result<(), ServerError>;
    /// Toggle zoom on a pane (expand to fill window, or unzoom).
    fn toggle_zoom(
        &mut self,
        session_id: u32,
        window_idx: u32,
        pane_id: u32,
    ) -> Result<(), ServerError>;
    /// Unzoom the window if it is currently zoomed (no-op if not zoomed).
    fn unzoom_window(&mut self, session_id: u32, window_idx: u32) -> Result<(), ServerError>;

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
    fn rotate_window(
        &mut self,
        session_id: u32,
        window_idx: u32,
        reverse: bool,
    ) -> Result<(), ServerError>;
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
        shell_cmd: Option<&str>,
    ) -> Result<(), ServerError>;

    // --- Command prompt ---
    /// Put the current client into command prompt mode.
    /// `initial_text` pre-fills the prompt, `prompt_str` sets a custom prompt,
    /// `template` is the command template (with %% for the input).
    fn enter_command_prompt_with(
        &mut self,
        initial_text: Option<&str>,
        prompt_str: Option<&str>,
        template: Option<&str>,
    );
    /// Shorthand: enter command prompt with no arguments.
    fn enter_command_prompt(&mut self) {
        self.enter_command_prompt_with(None, None, None);
    }

    // --- Copy mode ---
    /// Enter copy mode on the active pane.
    fn enter_copy_mode(&mut self) -> Result<(), ServerError>;
    /// Dispatch a copy-mode command by name (e.g., "cancel", "copy-selection-and-cancel").
    /// Returns true if the pane was in copy mode and the command was dispatched.
    fn dispatch_copy_mode_command(&mut self, command: &str) -> Result<bool, ServerError>;
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
    /// List all key bindings, including -N notes.
    fn list_key_bindings_with_notes(&self) -> Vec<String>;
    /// Return recent server messages for show-messages.
    fn show_messages(&self) -> Vec<String>;
    /// Build a format context with current session/window/pane variables.
    fn build_format_context(&self) -> crate::format::FormatContext;

    // --- Layout ---
    /// Get the name of the current layout for a window.
    fn current_layout_name(&self, session_id: u32, window_idx: u32) -> String;

    // --- Misc ---
    /// Execute a parsed command (for if-shell, etc.).
    fn execute_command(&mut self, argv: &[String]) -> Result<CommandResult, ServerError>;
    /// Send raw bytes to the active pane's PTY.
    fn send_bytes_to_pane(&self, bytes: &[u8]) -> Result<(), ServerError>;
    /// Clear scrollback history for the active pane.
    fn clear_history(&mut self) -> Result<(), ServerError>;
    /// Signal a wait-for channel, waking any waiters.
    fn wait_channel_signal(&mut self, channel: &str);
    /// Lock a wait-for channel.
    fn wait_channel_lock(&mut self, channel: &str) -> Result<(), ServerError>;
    /// Unlock a wait-for channel.
    fn wait_channel_unlock(&mut self, channel: &str) -> Result<(), ServerError>;

    // --- Client switching ---
    /// Switch the current client to a different session.
    fn switch_client(&mut self, session_id: u32) -> Result<(), ServerError>;
    /// Get the last session ID for the current client (for switch-client -l).
    fn client_last_session_id(&self) -> Option<u32>;
    /// Detach all other clients attached to the same session (for attach -d, detach -a).
    fn detach_other_clients(&mut self) -> Result<(), ServerError>;

    // --- Environment ---
    /// Set an environment variable. `None` session_id means global (server-level).
    fn set_environment(
        &mut self,
        session_id: Option<u32>,
        key: &str,
        value: &str,
    ) -> Result<(), ServerError>;
    /// Unset (remove) an environment variable. `None` session_id means global.
    fn unset_environment(&mut self, session_id: Option<u32>, key: &str) -> Result<(), ServerError>;
    /// Show environment variables. `None` session_id means global. Returns "KEY=VALUE" lines.
    fn show_environment(&self, session_id: Option<u32>) -> Vec<String>;

    // --- Buffer file I/O ---
    /// Save the named (or top) buffer to a file.
    fn save_buffer(&self, name: Option<&str>, path: &str) -> Result<(), ServerError>;
    /// Load a file into the paste buffer store.
    fn load_buffer(&mut self, name: Option<&str>, path: &str) -> Result<(), ServerError>;

    // --- Window search ---
    /// Find windows matching a pattern. Returns formatted result strings.
    fn find_windows(&self, session_id: u32, pattern: &str) -> Vec<String>;

    // --- Client redraw ---
    /// Force a full redraw for the current client.
    fn refresh_client(&mut self);

    // --- Hooks ---
    /// Add a command to a named hook.
    fn set_hook(&mut self, hook_name: &str, argv: Vec<String>);
    /// Remove a named hook.
    fn remove_hook(&mut self, hook_name: &str) -> bool;
    /// List all registered hooks as formatted strings.
    fn show_hooks(&self) -> Vec<String>;

    // --- Redraw ---
    fn mark_clients_redraw(&mut self, session_id: u32);

    // --- Pipe ---
    /// Start or stop piping a pane's output to a shell command.
    /// If `command` is `None`, stop the existing pipe.
    fn pipe_pane(&mut self, command: Option<&str>) -> Result<(), ServerError>;

    // --- Prompt history ---
    /// Get the prompt history entries.
    fn show_prompt_history(&self) -> Vec<String>;
    /// Clear the prompt history.
    fn clear_prompt_history(&mut self);
    /// Add an entry to the prompt history.
    fn add_prompt_history(&mut self, entry: String);

    // --- Overlay data ---
    /// Get structured session info for choose-tree overlay.
    /// Returns Vec of (session_name, window_count, attached_count).
    fn session_info_list(&self) -> Vec<(String, usize, usize)>;
    /// Get session tree info: sessions with their windows.
    /// Returns Vec of (session_name, attached_count, windows: Vec<(idx, name, is_active, pane_count)>).
    fn session_tree_info(&self) -> Vec<SessionTreeInfo>;
    /// Get structured buffer info for choose-buffer overlay.
    /// Returns Vec of (buffer_name, byte_length, preview).
    fn buffer_info_list(&self) -> Vec<(String, usize, String)>;
    /// Get structured client info for choose-client overlay.
    /// Returns Vec of (client_id, session_name, terminal_size).
    fn client_info_list(&self) -> Vec<(u64, String, String)>;

    // --- Popup ---
    /// Close any active popup overlay on the current client.
    fn close_popup(&mut self);
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

    // Split argv on semicolons (tmux-compatible command chaining).
    // "start-server;" or bare ";" tokens act as command separators.
    let sub_commands = split_argv_on_semicolons(argv);
    if sub_commands.len() > 1 {
        let mut last_result = CommandResult::Ok;
        for sub_argv in sub_commands {
            if sub_argv.is_empty() {
                continue;
            }
            last_result = execute_single_command(&sub_argv, server)?;
        }
        return Ok(last_result);
    }

    execute_single_command(argv, server)
}

/// Split argv on `;` boundaries. Handles trailing semicolons on tokens
/// (e.g., `["start-server;", "show-options"]` → `[["start-server"], ["show-options"]]`).
fn split_argv_on_semicolons(argv: &[String]) -> Vec<Vec<String>> {
    let mut commands: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for arg in argv {
        if arg == ";" {
            if !current.is_empty() {
                commands.push(std::mem::take(&mut current));
            }
        } else if let Some(prefix) = arg.strip_suffix(';') {
            if !prefix.is_empty() {
                current.push(prefix.to_string());
            }
            if !current.is_empty() {
                commands.push(std::mem::take(&mut current));
            }
        } else {
            current.push(arg.clone());
        }
    }
    if !current.is_empty() {
        commands.push(current);
    }
    commands
}

fn execute_single_command(
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
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].as_str())
}

/// Check if a flag is present in arguments.
/// Supports both exact flags (`-g`) and combined single-char flags (`-gF` contains `-g`).
pub fn has_flag(args: &[String], flag: &str) -> bool {
    // flag is like "-g", so the char is flag[1..]
    let flag_char = flag.strip_prefix('-').unwrap_or(flag);
    args.iter().any(|a| {
        if a == flag {
            return true;
        }
        // Check combined flags: "-gF" contains both "-g" and "-F"
        if let Some(chars) = a.strip_prefix('-') {
            if !chars.is_empty() && chars.chars().all(|c| c.is_ascii_alphabetic()) {
                return chars.contains(flag_char);
            }
        }
        false
    })
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
