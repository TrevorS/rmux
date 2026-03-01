//! Built-in command implementations.

mod client;
mod server_cmds;
mod session;

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
    CommandEntry {
        name: "has-session",
        min_args: 0,
        handler: session::cmd_has_session,
        usage: "[-t target-session]",
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
    CommandEntry {
        name: "detach",
        min_args: 0,
        handler: client::cmd_detach_client,
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
