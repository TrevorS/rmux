//! Built-in command implementations.

mod client;
mod display;
mod environment;
mod options;
mod pane;
mod paste;
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
    CommandEntry {
        name: "switch-client",
        min_args: 0,
        handler: client::cmd_switch_client,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "switchc",
        min_args: 0,
        handler: client::cmd_switch_client,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "refresh-client",
        min_args: 0,
        handler: client::cmd_refresh_client,
        usage: "[-t target-client]",
    },
    CommandEntry {
        name: "refresh",
        min_args: 0,
        handler: client::cmd_refresh_client,
        usage: "[-t target-client]",
    },
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
    CommandEntry {
        name: "find-window",
        min_args: 1,
        handler: window::cmd_find_window,
        usage: "[-t target-session] match-string",
    },
    CommandEntry {
        name: "findw",
        min_args: 1,
        handler: window::cmd_find_window,
        usage: "[-t target-session] match-string",
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
        name: "show-messages",
        min_args: 0,
        handler: display::cmd_show_messages,
        usage: "",
    },
    CommandEntry { name: "showmsgs", min_args: 0, handler: display::cmd_show_messages, usage: "" },
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
        name: "start-server",
        min_args: 0,
        handler: server_cmds::cmd_start_server,
        usage: "",
    },
    CommandEntry { name: "start", min_args: 0, handler: server_cmds::cmd_start_server, usage: "" },
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
    CommandEntry {
        name: "set-window-option",
        min_args: 2,
        handler: options::cmd_set_window_option,
        usage: "[-g] [-t target] option value",
    },
    CommandEntry {
        name: "setw",
        min_args: 2,
        handler: options::cmd_set_window_option,
        usage: "[-g] [-t target] option value",
    },
    CommandEntry {
        name: "show-window-options",
        min_args: 0,
        handler: options::cmd_show_window_options,
        usage: "[-g] [-t target] [option]",
    },
    CommandEntry {
        name: "showw",
        min_args: 0,
        handler: options::cmd_show_window_options,
        usage: "[-g] [-t target] [option]",
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
    // Layout cycling
    CommandEntry {
        name: "next-layout",
        min_args: 0,
        handler: window::cmd_next_layout,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "nextl",
        min_args: 0,
        handler: window::cmd_next_layout,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "previous-layout",
        min_args: 0,
        handler: window::cmd_previous_layout,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "prevl",
        min_args: 0,
        handler: window::cmd_previous_layout,
        usage: "[-t target-window]",
    },
    // if-shell
    CommandEntry {
        name: "if-shell",
        min_args: 2,
        handler: server_cmds::cmd_if_shell,
        usage: "shell-command command [command]",
    },
    CommandEntry {
        name: "if",
        min_args: 2,
        handler: server_cmds::cmd_if_shell,
        usage: "shell-command command [command]",
    },
    // send-prefix
    CommandEntry {
        name: "send-prefix",
        min_args: 0,
        handler: server_cmds::cmd_send_prefix,
        usage: "[-2]",
    },
    // clear-history
    CommandEntry {
        name: "clear-history",
        min_args: 0,
        handler: server_cmds::cmd_clear_history,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "clearhist",
        min_args: 0,
        handler: server_cmds::cmd_clear_history,
        usage: "[-t target-pane]",
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
    // Copy mode & paste buffer commands
    CommandEntry { name: "copy-mode", min_args: 0, handler: paste::cmd_copy_mode, usage: "[-u]" },
    CommandEntry {
        name: "paste-buffer",
        min_args: 0,
        handler: paste::cmd_paste_buffer,
        usage: "[-b buffer-name]",
    },
    CommandEntry {
        name: "pasteb",
        min_args: 0,
        handler: paste::cmd_paste_buffer,
        usage: "[-b buffer-name]",
    },
    CommandEntry { name: "list-buffers", min_args: 0, handler: paste::cmd_list_buffers, usage: "" },
    CommandEntry { name: "lsb", min_args: 0, handler: paste::cmd_list_buffers, usage: "" },
    CommandEntry {
        name: "show-buffer",
        min_args: 0,
        handler: paste::cmd_show_buffer,
        usage: "[-b buffer-name]",
    },
    CommandEntry {
        name: "showb",
        min_args: 0,
        handler: paste::cmd_show_buffer,
        usage: "[-b buffer-name]",
    },
    CommandEntry {
        name: "set-buffer",
        min_args: 1,
        handler: paste::cmd_set_buffer,
        usage: "[-b buffer-name] data",
    },
    CommandEntry {
        name: "setb",
        min_args: 1,
        handler: paste::cmd_set_buffer,
        usage: "[-b buffer-name] data",
    },
    CommandEntry {
        name: "delete-buffer",
        min_args: 0,
        handler: paste::cmd_delete_buffer,
        usage: "-b buffer-name",
    },
    CommandEntry {
        name: "deleteb",
        min_args: 0,
        handler: paste::cmd_delete_buffer,
        usage: "-b buffer-name",
    },
    CommandEntry {
        name: "save-buffer",
        min_args: 1,
        handler: paste::cmd_save_buffer,
        usage: "[-b buffer-name] path",
    },
    CommandEntry {
        name: "saveb",
        min_args: 1,
        handler: paste::cmd_save_buffer,
        usage: "[-b buffer-name] path",
    },
    CommandEntry {
        name: "load-buffer",
        min_args: 1,
        handler: paste::cmd_load_buffer,
        usage: "[-b buffer-name] path",
    },
    CommandEntry {
        name: "loadb",
        min_args: 1,
        handler: paste::cmd_load_buffer,
        usage: "[-b buffer-name] path",
    },
    // Hook commands
    CommandEntry {
        name: "set-hook",
        min_args: 1,
        handler: server_cmds::cmd_set_hook,
        usage: "[-u] hook-name [command]",
    },
    CommandEntry {
        name: "show-hooks",
        min_args: 0,
        handler: server_cmds::cmd_show_hooks,
        usage: "",
    },
    // Environment commands
    CommandEntry {
        name: "set-environment",
        min_args: 1,
        handler: environment::cmd_set_environment,
        usage: "[-g] [-u] [-t target-session] name [value]",
    },
    CommandEntry {
        name: "setenv",
        min_args: 1,
        handler: environment::cmd_set_environment,
        usage: "[-g] [-u] [-t target-session] name [value]",
    },
    CommandEntry {
        name: "show-environment",
        min_args: 0,
        handler: environment::cmd_show_environment,
        usage: "[-g] [-t target-session] [name]",
    },
    CommandEntry {
        name: "showenv",
        min_args: 0,
        handler: environment::cmd_show_environment,
        usage: "[-g] [-t target-session] [name]",
    },
    // Confirmation / synchronization
    CommandEntry {
        name: "confirm-before",
        min_args: 1,
        handler: server_cmds::cmd_confirm_before,
        usage: "[-p prompt] command",
    },
    CommandEntry {
        name: "confirm",
        min_args: 1,
        handler: server_cmds::cmd_confirm_before,
        usage: "[-p prompt] command",
    },
    CommandEntry {
        name: "wait-for",
        min_args: 1,
        handler: server_cmds::cmd_wait_for,
        usage: "[-L|-U|-S] channel",
    },
    CommandEntry {
        name: "wait",
        min_args: 1,
        handler: server_cmds::cmd_wait_for,
        usage: "[-L|-U|-S] channel",
    },
    // Interactive display commands
    CommandEntry {
        name: "display-panes",
        min_args: 0,
        handler: display::cmd_display_panes,
        usage: "[-d duration] [-t target-client]",
    },
    CommandEntry {
        name: "displayp",
        min_args: 0,
        handler: display::cmd_display_panes,
        usage: "[-d duration]",
    },
    CommandEntry {
        name: "clock-mode",
        min_args: 0,
        handler: display::cmd_clock_mode,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "choose-tree",
        min_args: 0,
        handler: display::cmd_choose_tree,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "choose-buffer",
        min_args: 0,
        handler: display::cmd_choose_buffer,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "choose-client",
        min_args: 0,
        handler: display::cmd_choose_client,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "display-menu",
        min_args: 0,
        handler: display::cmd_display_menu,
        usage: "[-t target-pane] [-T title] name key command ...",
    },
    CommandEntry {
        name: "menu",
        min_args: 0,
        handler: display::cmd_display_menu,
        usage: "[-t target-pane]",
    },
    CommandEntry {
        name: "display-popup",
        min_args: 0,
        handler: display::cmd_display_popup,
        usage: "[-t target-pane] [-w width] [-h height] [command]",
    },
    CommandEntry {
        name: "popup",
        min_args: 0,
        handler: display::cmd_display_popup,
        usage: "[command]",
    },
    CommandEntry {
        name: "customize-mode",
        min_args: 0,
        handler: display::cmd_customize_mode,
        usage: "[-t target-pane]",
    },
    // Prompt history
    CommandEntry {
        name: "clear-prompt-history",
        min_args: 0,
        handler: display::cmd_clear_prompt_history,
        usage: "",
    },
    CommandEntry {
        name: "show-prompt-history",
        min_args: 0,
        handler: display::cmd_show_prompt_history,
        usage: "",
    },
    // Client commands
    CommandEntry {
        name: "suspend-client",
        min_args: 0,
        handler: client::cmd_suspend_client,
        usage: "[-t target-client]",
    },
    CommandEntry {
        name: "suspendc",
        min_args: 0,
        handler: client::cmd_suspend_client,
        usage: "[-t target-client]",
    },
    // Lock commands
    CommandEntry { name: "lock-server", min_args: 0, handler: display::cmd_lock_server, usage: "" },
    CommandEntry { name: "lock", min_args: 0, handler: display::cmd_lock_server, usage: "" },
    CommandEntry {
        name: "lock-session",
        min_args: 0,
        handler: display::cmd_lock_session,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "locks",
        min_args: 0,
        handler: display::cmd_lock_session,
        usage: "[-t target-session]",
    },
    CommandEntry {
        name: "lock-client",
        min_args: 0,
        handler: display::cmd_lock_client,
        usage: "[-t target-client]",
    },
    CommandEntry {
        name: "lockc",
        min_args: 0,
        handler: display::cmd_lock_client,
        usage: "[-t target-client]",
    },
    // Window commands (additional)
    CommandEntry {
        name: "link-window",
        min_args: 0,
        handler: window::cmd_link_window,
        usage: "[-s src] [-t dst]",
    },
    CommandEntry {
        name: "linkw",
        min_args: 0,
        handler: window::cmd_link_window,
        usage: "[-s src] [-t dst]",
    },
    CommandEntry {
        name: "unlink-window",
        min_args: 0,
        handler: window::cmd_unlink_window,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "unlinkw",
        min_args: 0,
        handler: window::cmd_unlink_window,
        usage: "[-t target-window]",
    },
    CommandEntry {
        name: "move-pane",
        min_args: 0,
        handler: window::cmd_move_pane,
        usage: "[-s src] [-t dst]",
    },
    CommandEntry {
        name: "movep",
        min_args: 0,
        handler: window::cmd_move_pane,
        usage: "[-s src] [-t dst]",
    },
    // Pipe/resize
    CommandEntry {
        name: "pipe-pane",
        min_args: 0,
        handler: display::cmd_pipe_pane,
        usage: "[-o] [-t target-pane] [command]",
    },
    CommandEntry {
        name: "pipep",
        min_args: 0,
        handler: display::cmd_pipe_pane,
        usage: "[-o] [-t target-pane] [command]",
    },
    CommandEntry {
        name: "resize-window",
        min_args: 0,
        handler: display::cmd_resize_window,
        usage: "[-t target-window] [-x width] [-y height]",
    },
    CommandEntry {
        name: "resizew",
        min_args: 0,
        handler: display::cmd_resize_window,
        usage: "[-t target-window]",
    },
    // Server access
    CommandEntry {
        name: "server-access",
        min_args: 0,
        handler: display::cmd_server_access,
        usage: "[-adlrw] [user]",
    },
];
