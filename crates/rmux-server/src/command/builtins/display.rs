//! Display and information commands.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::server::ServerError;

/// display-message [-p] [-F format] [message]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_display_message(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let print = has_flag(args, "-p");

    // Collect non-flag arguments as the message
    let mut skip_next = false;
    let mut message_parts = Vec::new();
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "-F" {
            skip_next = true;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        message_parts.push(arg.as_str());
    }
    let message = message_parts.join(" ");

    if print || !message.is_empty() {
        // Expand format variables
        let ctx = server.build_format_context();
        let expanded = crate::format::format_expand(&message, &ctx);
        Ok(CommandResult::Output(expanded + "\n"))
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

/// show-messages — display server message log.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_messages(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let messages = server.show_messages();
    if messages.is_empty() {
        Ok(CommandResult::Output(String::new()))
    } else {
        Ok(CommandResult::Output(messages.join("\n") + "\n"))
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

/// display-panes [-d duration] [-t target-client]
///
/// Show pane numbers briefly. In tmux this is an interactive overlay.
/// For now, output pane information as text.
pub fn cmd_display_panes(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let session_id = if let Some(target) = get_option(args, "-t") {
        server
            .find_session_id(target)
            .ok_or_else(|| ServerError::Command(format!("session not found: {target}")))?
    } else {
        server
            .client_session_id()
            .ok_or_else(|| ServerError::Command("no current session".into()))?
    };

    let window_idx = server.active_window_for(session_id).unwrap_or(0);
    let panes = server.list_panes(session_id, window_idx);
    Ok(CommandResult::Output(panes.join("\n") + "\n"))
}

/// clock-mode [-t target-pane]
///
/// Display a large clock. In tmux this renders as an overlay in the pane.
/// We output the current time as large ASCII art digits.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_clock_mode(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use std::fmt::Write;
    let _ = args;

    let now = chrono_free_now();
    let time_str = format!("{:02}:{:02}:{:02}", now.0, now.1, now.2);
    let mut output = String::new();

    // Render each row of the big digits (5 rows tall)
    for row in 0..5 {
        for ch in time_str.chars() {
            let pattern = big_digit(ch, row);
            write!(output, "{pattern} ").ok();
        }
        output.push('\n');
    }

    Ok(CommandResult::Output(output))
}

/// Get hours, minutes, seconds without chrono dependency.
fn chrono_free_now() -> (u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let day_secs = (secs % 86400) as u32;
    (day_secs / 3600, (day_secs % 3600) / 60, day_secs % 60)
}

/// Return one row of a big ASCII digit (5 rows, each 3 chars wide).
fn big_digit(ch: char, row: usize) -> &'static str {
    const DIGITS: [[&str; 5]; 11] = [
        // 0
        ["###", "# #", "# #", "# #", "###"],
        // 1
        ["  #", "  #", "  #", "  #", "  #"],
        // 2
        ["###", "  #", "###", "#  ", "###"],
        // 3
        ["###", "  #", "###", "  #", "###"],
        // 4
        ["# #", "# #", "###", "  #", "  #"],
        // 5
        ["###", "#  ", "###", "  #", "###"],
        // 6
        ["###", "#  ", "###", "# #", "###"],
        // 7
        ["###", "  #", "  #", "  #", "  #"],
        // 8
        ["###", "# #", "###", "# #", "###"],
        // 9
        ["###", "# #", "###", "  #", "###"],
        // :
        ["   ", " # ", "   ", " # ", "   "],
    ];
    let idx = match ch {
        '0'..='9' => (ch as u8 - b'0') as usize,
        ':' => 10,
        _ => return "   ",
    };
    DIGITS[idx].get(row).copied().unwrap_or("   ")
}

/// choose-tree [-t target-pane]
///
/// Interactive tree view of sessions and windows.
/// Stub — interactive mode requires client-side UI overlay.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_choose_tree(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    // Fall back to listing sessions (non-interactive)
    let sessions = server.list_sessions();
    if sessions.is_empty() {
        Ok(CommandResult::Output("(no sessions)\n".to_string()))
    } else {
        Ok(CommandResult::Output(sessions.join("\n") + "\n"))
    }
}

/// choose-buffer [-t target-pane]
///
/// Interactive buffer list. Stub — falls back to list-buffers.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_choose_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let buffers = server.list_buffers();
    if buffers.is_empty() {
        Ok(CommandResult::Output("(no buffers)\n".to_string()))
    } else {
        Ok(CommandResult::Output(buffers.join("\n") + "\n"))
    }
}

/// choose-client [-t target-pane]
///
/// Interactive client list. Stub — falls back to list-clients.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_choose_client(
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

/// display-menu [-t target-pane] [-T title] [-x pos] [-y pos] name key command ...
///
/// Show a menu. Stub — menus require client-side UI.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_display_menu(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// display-popup [-t target-pane] [-w width] [-h height] [command]
///
/// Show a popup window. Stub — popups require client-side UI.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_display_popup(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// customize-mode [-t target-pane]
///
/// Interactive options browser. Stub — requires client-side UI.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_customize_mode(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// clear-prompt-history
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_clear_prompt_history(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// show-prompt-history
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_prompt_history(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Output(String::new()))
}

/// pipe-pane [-o] [-t target-pane] [command]
///
/// Pipe pane output to a shell command. With no command, stops piping.
/// The -o flag opens a pipe only if none is currently active (toggle).
pub fn cmd_pipe_pane(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    use crate::command::positional_args;

    let positional = positional_args(args, &["-t"]);
    if positional.is_empty() {
        // No command — stop piping
        server.pipe_pane(None)?;
    } else {
        let command = positional.join(" ");
        server.pipe_pane(Some(&command))?;
    }
    Ok(CommandResult::Ok)
}

/// resize-window [-t target-window] [-x width] [-y height]
///
/// Resize a window manually. Stub — window sizing is usually automatic.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_resize_window(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// server-access [-adlrw] [user]
///
/// Manage server access control. Stub.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_server_access(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// lock-server
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_lock_server(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// lock-session [-t target-session]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_lock_session(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}

/// lock-client [-t target-client]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_lock_client(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    Ok(CommandResult::Ok)
}
