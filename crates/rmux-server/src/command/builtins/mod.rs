//! Built-in command implementations.

mod client;
mod display;
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
        usage: "[-t target-pane] key ...",
    },
];
