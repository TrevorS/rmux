//! Display and information commands.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::overlay::{ListItem, ListKind, ListOverlay, MenuItem, MenuOverlay, OverlayState};
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

    // Expand format variables
    let ctx = server.build_format_context();
    let expanded = crate::format::format_expand(&message, &ctx);
    if print {
        Ok(CommandResult::Output(expanded + "\n"))
    } else if !message.is_empty() {
        Ok(CommandResult::TimedMessage(expanded))
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
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_choose_tree(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let sessions = server.session_info_list();
    let items: Vec<ListItem> = sessions
        .into_iter()
        .map(|(name, win_count, attached)| {
            let attached_str =
                if attached > 0 { format!(" (attached: {attached})") } else { String::new() };
            ListItem {
                display: format!("{name}: {win_count} windows{attached_str}"),
                command: vec!["switch-client".into(), "-t".into(), name.clone()],
                indent: 0,
                collapsed: false,
                hidden_children: 0,
                deletable: true,
                delete_command: vec!["kill-session".into(), "-t".into(), name],
            }
        })
        .collect();

    if items.is_empty() {
        return Ok(CommandResult::Output("(no sessions)\n".to_string()));
    }

    Ok(CommandResult::Overlay(OverlayState::List(ListOverlay {
        items,
        selected: 0,
        scroll_offset: 0,
        filter: String::new(),
        filtering: false,
        title: "choose-tree".into(),
        kind: ListKind::Tree,
    })))
}

/// choose-buffer [-t target-pane]
///
/// Interactive buffer list.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_choose_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let buffers = server.buffer_info_list();
    let items: Vec<ListItem> = buffers
        .into_iter()
        .map(|(name, size, preview)| ListItem {
            display: format!("{name}: {size} bytes \"{preview}\""),
            command: vec!["paste-buffer".into(), "-b".into(), name.clone()],
            indent: 0,
            collapsed: false,
            hidden_children: 0,
            deletable: true,
            delete_command: vec!["delete-buffer".into(), "-b".into(), name],
        })
        .collect();

    if items.is_empty() {
        return Ok(CommandResult::Output("(no buffers)\n".to_string()));
    }

    Ok(CommandResult::Overlay(OverlayState::List(ListOverlay {
        items,
        selected: 0,
        scroll_offset: 0,
        filter: String::new(),
        filtering: false,
        title: "choose-buffer".into(),
        kind: ListKind::Buffer,
    })))
}

/// choose-client [-t target-pane]
///
/// Interactive client list.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_choose_client(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let clients = server.client_info_list();
    let items: Vec<ListItem> = clients
        .into_iter()
        .map(|(id, session_name, size)| ListItem {
            display: format!("client {id}: {session_name} [{size}]"),
            command: vec!["switch-client".into(), "-t".into(), session_name.clone()],
            indent: 0,
            collapsed: false,
            hidden_children: 0,
            deletable: true,
            delete_command: vec!["detach-client".into(), "-t".into(), id.to_string()],
        })
        .collect();

    if items.is_empty() {
        return Ok(CommandResult::Output("(no clients)\n".to_string()));
    }

    Ok(CommandResult::Overlay(OverlayState::List(ListOverlay {
        items,
        selected: 0,
        scroll_offset: 0,
        filter: String::new(),
        filtering: false,
        title: "choose-client".into(),
        kind: ListKind::Client,
    })))
}

/// display-menu [-t target-pane] [-T title] [-x pos] [-y pos] name key command ...
///
/// Show a menu overlay.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_display_menu(
    args: &[String],
    _server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let title = get_option(args, "-T").unwrap_or("Menu");
    let x: u32 = get_option(args, "-x").and_then(|v| v.parse().ok()).unwrap_or(0);
    let y: u32 = get_option(args, "-y").and_then(|v| v.parse().ok()).unwrap_or(0);

    // Parse positional args as triplets: name key command
    let positional = crate::command::positional_args(args, &["-t", "-T", "-x", "-y"]);
    let mut items = Vec::new();
    let mut i = 0;
    while i + 2 < positional.len() {
        let name = positional[i];
        let key_str = positional[i + 1];
        let command_str = positional[i + 2];
        let key = if key_str.len() == 1 { Some(key_str.chars().next().unwrap()) } else { None };

        if name.is_empty() {
            // Separator
            items.push(MenuItem { name: String::new(), key: None, command: vec![] });
        } else {
            items.push(MenuItem {
                name: name.to_string(),
                key,
                command: vec![command_str.to_string()],
            });
        }
        i += 3;
    }

    if items.is_empty() {
        return Ok(CommandResult::Ok);
    }

    // Find first non-separator for initial selection
    let selected = items.iter().position(|it| !it.name.is_empty()).unwrap_or(0);
    let width = items
        .iter()
        .map(|it| it.name.len() + it.key.map_or(0, |_| 4) + 2)
        .max()
        .unwrap_or(10)
        .max(title.len() + 2) as u32;

    Ok(CommandResult::Overlay(OverlayState::Menu(MenuOverlay {
        items,
        selected,
        title: title.to_string(),
        x,
        y,
        width,
    })))
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
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    server.clear_prompt_history();
    Ok(CommandResult::Ok)
}

/// show-prompt-history
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_show_prompt_history(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let history = server.show_prompt_history();
    if history.is_empty() {
        Ok(CommandResult::Output(String::new()))
    } else {
        Ok(CommandResult::Output(history.join("\n") + "\n"))
    }
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

/// resize-window [-t target-window] [-x width] [-y height] [-A]
///
/// Resize a window to a specific size.
pub fn cmd_resize_window(
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

    let sx = get_option(args, "-x")
        .map(|v| v.parse::<u32>().map_err(|_| ServerError::Command(format!("invalid width: {v}"))))
        .transpose()?;
    let sy = get_option(args, "-y")
        .map(|v| v.parse::<u32>().map_err(|_| ServerError::Command(format!("invalid height: {v}"))))
        .transpose()?;

    if sx.is_none() && sy.is_none() && !has_flag(args, "-A") {
        return Err(ServerError::Command("usage: resize-window [-x width] [-y height]".into()));
    }

    // -A adjusts to smallest client (for now, use client dimensions)
    let (sx, sy) = if has_flag(args, "-A") {
        (Some(server.client_sx()), Some(server.client_sy().saturating_sub(1)))
    } else {
        (sx, sy)
    };

    server.resize_window(session_id, window_idx, sx, sy)?;
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
