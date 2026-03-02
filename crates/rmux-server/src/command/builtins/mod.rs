//! Built-in command implementations.

mod client;
mod display;
mod options;
mod pane;
mod server_cmds;
mod session;
mod window;

use super::CommandEntry;

/// All registered built-in commands.
pub static COMMANDS: &[CommandEntry] = &[
    // Session commands
    CommandEntry {
        name: "new-session",
        min_args: 0,
        handler: session::cmd_new_session,
        usage: "[-d] [-s session-name] [-x width] [-y height]",
    },
    CommandEntry {
        name: "kill-session",
        min_args: 0,
        handler: session::cmd_kill_session,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "list-sessions",
        min_args: 0,
        handler: session::cmd_list_sessions,
        usage: "",
    },
    CommandEntry { name: "ls", min_args: 0, handler: session::cmd_list_sessions, usage: "" },
    CommandEntry {
        name: "has-session",
        min_args: 0,
        handler: session::cmd_has_session,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "rename-session",
        min_args: 1,
        handler: session::cmd_rename_session,
        usage: "[-t target-session] new-name",
    },
    // Client commands
    CommandEntry {
        name: "attach-session",
        min_args: 0,
        handler: client::cmd_attach_session,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "attach",
        min_args: 0,
        handler: client::cmd_attach_session,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "detach-client",
        min_args: 0,
        handler: client::cmd_detach_client,
        usage: "",
    },
    CommandEntry { name: "detach", min_args: 0, handler: client::cmd_detach_client, usage: "" },
    // Window commands
    CommandEntry {
        name: "new-window",
        min_args: 0,
        handler: window::cmd_new_window,
        usage: "[-d] [-n name] [-t target-session]",
    },
    CommandEntry {
        name: "kill-window",
        min_args: 0,
        handler: window::cmd_kill_window,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "select-window",
        min_args: 0,
        handler: window::cmd_select_window,
        usage: "[-t target-window]",
    },
    CommandEntry { name: "next-window", min_args: 0, handler: window::cmd_next_window, usage: "" },
    CommandEntry { name: "next", min_args: 0, handler: window::cmd_next_window, usage: "" },
    CommandEntry {
        name: "previous-window",
        min_args: 0,
        handler: window::cmd_previous_window,
        usage: "",
    },
    CommandEntry { name: "prev", min_args: 0, handler: window::cmd_previous_window, usage: "" },
    CommandEntry { name: "last-window", min_args: 0, handler: window::cmd_last_window, usage: "" },
    CommandEntry {
        name: "rename-window",
        min_args: 1,
        handler: window::cmd_rename_window,
        usage: "[-t target-window] new-name",
    },
    CommandEntry {
        name: "list-windows",
        min_args: 0,
        handler: window::cmd_list_windows,
        usage: "[-t target-session]",
    },
    // Pane commands
    CommandEntry {
        name: "split-window",
        min_args: 0,
        handler: pane::cmd_split_window,
        usage: "[-h] [-v] [-d]",
    },
    CommandEntry {
        name: "select-pane",
        min_args: 0,
        handler: pane::cmd_select_pane,
        usage: "[-U] [-D] [-L] [-R] [-t target-pane]",
    },
    CommandEntry {
        name: "kill-pane",
        min_args: 0,
        handler: pane::cmd_kill_pane,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "list-panes",
        min_args: 0,
        handler: pane::cmd_list_panes,
        usage: "[-t target-window]",
    },
    // Display/info commands
    CommandEntry {
        name: "display-message",
        min_args: 0,
        handler: display::cmd_display_message,
        usage: "[-p] [message]",
    },
    CommandEntry {
        name: "display",
        min_args: 0,
        handler: display::cmd_display_message,
        usage: "[-p] [message]",
    },
    CommandEntry {
        name: "list-commands",
        min_args: 0,
        handler: display::cmd_list_commands,
        usage: "",
    },
    CommandEntry { name: "lscm", min_args: 0, handler: display::cmd_list_commands, usage: "" },
    CommandEntry {
        name: "list-keys",
        min_args: 0,
        handler: display::cmd_list_keys,
        usage: "[-T table]",
    },
    CommandEntry {
        name: "list-clients",
        min_args: 0,
        handler: display::cmd_list_clients,
        usage: "",
    },
    // Server commands
    CommandEntry {
        name: "kill-server",
        min_args: 0,
        handler: server_cmds::cmd_kill_server,
        usage: "",
    },
    CommandEntry {
        name: "send-keys",
        min_args: 1,
        handler: server_cmds::cmd_send_keys,
        usage: "[-l] [-t target-pane] key ...",
    },
    // Option commands
    CommandEntry {
        name: "set-option",
        min_args: 2,
        handler: options::cmd_set_option,
        usage: "[-g] [-w] [-t target] option value",
    },
    CommandEntry {
        name: "set",
        min_args: 2,
        handler: options::cmd_set_option,
        usage: "[-g] [-w] [-t target] option value",
    },
    CommandEntry {
        name: "show-options",
        min_args: 0,
        handler: options::cmd_show_options,
        usage: "[-g] [-w] [-t target] [option]",
    },
    CommandEntry {
        name: "show",
        min_args: 0,
        handler: options::cmd_show_options,
        usage: "[-g] [-w] [-t target] [option]",
    },
    // Key binding commands
    CommandEntry {
        name: "bind-key",
        min_args: 2,
        handler: server_cmds::cmd_bind_key,
        usage: "[-T table] [-n] key command [args...]",
    },
    CommandEntry {
        name: "bind",
        min_args: 2,
        handler: server_cmds::cmd_bind_key,
        usage: "[-T table] [-n] key command [args...]",
    },
    CommandEntry {
        name: "unbind-key",
        min_args: 1,
        handler: server_cmds::cmd_unbind_key,
        usage: "[-T table] key",
    },
    CommandEntry {
        name: "unbind",
        min_args: 1,
        handler: server_cmds::cmd_unbind_key,
        usage: "[-T table] key",
    },
    // Config commands
    CommandEntry {
        name: "source-file",
        min_args: 1,
        handler: server_cmds::cmd_source_file,
        usage: "path",
    },
    CommandEntry {
        name: "source",
        min_args: 1,
        handler: server_cmds::cmd_source_file,
        usage: "path",
    },
    // Pane commands (new)
    CommandEntry {
        name: "capture-pane",
        min_args: 0,
        handler: pane::cmd_capture_pane,
        usage: "[-p] [-t target-pane]",
    },
    CommandEntry {
        name: "capturep",
        min_args: 0,
        handler: pane::cmd_capture_pane,
        usage: "[-p] [-t target-pane]",
    },
    CommandEntry {
        name: "resize-pane",
        min_args: 0,
        handler: pane::cmd_resize_pane,
        usage: "[-U|-D|-L|-R] [-x width] [-y height] [amount]",
    },
    CommandEntry {
        name: "resizep",
        min_args: 0,
        handler: pane::cmd_resize_pane,
        usage: "[-U|-D|-L|-R] [-x width] [-y height] [amount]",
    },
    CommandEntry {
        name: "swap-pane",
        min_args: 0,
        handler: pane::cmd_swap_pane,
        usage: "[-U] [-D] [-t target-pane]",
    },
    CommandEntry {
        name: "swapp",
        min_args: 0,
        handler: pane::cmd_swap_pane,
        usage: "[-U] [-D] [-t target-pane]",
    },
    CommandEntry {
        name: "break-pane",
        min_args: 0,
        handler: pane::cmd_break_pane,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "breakp",
        min_args: 0,
        handler: pane::cmd_break_pane,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "join-pane",
        min_args: 0,
        handler: pane::cmd_join_pane,
        usage: "[-h] [-s src-pane] [-t dst-pane]",
    },
    CommandEntry {
        name: "joinp",
        min_args: 0,
        handler: pane::cmd_join_pane,
        usage: "[-h] [-s src-pane] [-t dst-pane]",
    },
    CommandEntry {
        name: "last-pane",
        min_args: 0,
        handler: pane::cmd_last_pane,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "lastp",
        min_args: 0,
        handler: pane::cmd_last_pane,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "respawn-pane",
        min_args: 0,
        handler: pane::cmd_respawn_pane,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "respawnp",
        min_args: 0,
        handler: pane::cmd_respawn_pane,
        usage: "[-t target-pane]",
    },
    // Window commands (new)
    CommandEntry {
        name: "swap-window",
        min_args: 0,
        handler: window::cmd_swap_window,
        usage: "[-s src] [-t dst]",
    },
    CommandEntry {
        name: "swapw",
        min_args: 0,
        handler: window::cmd_swap_window,
        usage: "[-s src] [-t dst]",
    },
    CommandEntry {
        name: "move-window",
        min_args: 0,
        handler: window::cmd_move_window,
        usage: "[-s src] [-t dst]",
    },
    CommandEntry {
        name: "movew",
        min_args: 0,
        handler: window::cmd_move_window,
        usage: "[-s src] [-t dst]",
    },
    CommandEntry {
        name: "rotate-window",
        min_args: 0,
        handler: window::cmd_rotate_window,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "rotatew",
        min_args: 0,
        handler: window::cmd_rotate_window,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "select-layout",
        min_args: 0,
        handler: window::cmd_select_layout,
        usage: "[-t target-window] layout-name",
    },
    CommandEntry {
        name: "selectl",
        min_args: 0,
        handler: window::cmd_select_layout,
        usage: "[-t target-window] layout-name",
    },
    CommandEntry {
        name: "respawn-window",
        min_args: 0,
        handler: window::cmd_respawn_window,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "respawnw",
        min_args: 0,
        handler: window::cmd_respawn_window,
        usage: "[-t target-window]",
    },
    // Run shell
    CommandEntry {
        name: "run-shell",
        min_args: 1,
        handler: server_cmds::cmd_run_shell,
        usage: "command",
    },
    CommandEntry {
        name: "run",
        min_args: 1,
        handler: server_cmds::cmd_run_shell,
        usage: "command",
    },
    // Command prompt
    CommandEntry {
        name: "command-prompt",
        min_args: 0,
        handler: server_cmds::cmd_command_prompt,
        usage: "",
    },
];
