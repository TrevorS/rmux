//! Display and information commands.

use crate::command::{CommandResult, CommandServer, get_option, has_flag};
use crate::overlay::{ListItem, ListKind, ListOverlay, MenuItem, MenuOverlay, OverlayState};
use crate::server::ServerError;

/// display-message [-a] [-l] [-p] [-v] [-c target-client] [-d delay] [-t target-pane] [message]
/// -a: list format variables
/// -l: message length
/// -v: verbose (print expanded variables)
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_display_message(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let print = has_flag(args, "-p");
    let list_vars = has_flag(args, "-a");
    let _verbose = has_flag(args, "-v");
    let _delay = get_option(args, "-d");

    if list_vars {
        let ctx = server.build_format_context();
        let output: Vec<String> =
            ctx.list_vars().into_iter().map(|(k, v)| format!("{k} {v}")).collect();
        return Ok(CommandResult::Output(output.join("\n") + "\n"));
    }

    // Collect non-flag arguments as the message
    let mut skip_next = false;
    let mut message_parts = Vec::new();
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "-F" || arg == "-t" || arg == "-c" || arg == "-d" {
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

/// list-keys [-N] [-T table]
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_keys(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let table_filter = get_option(args, "-T");
    let show_notes = has_flag(args, "-N");
    let bindings =
        if show_notes { server.list_key_bindings_with_notes() } else { server.list_key_bindings() };
    let filtered: Vec<&String> = if let Some(table) = table_filter {
        let pattern = format!("-T {table} ");
        bindings.iter().filter(|b| b.contains(&pattern)).collect()
    } else {
        bindings.iter().collect()
    };
    if filtered.is_empty() {
        Ok(CommandResult::Output("(no bindings)\n".to_string()))
    } else {
        let output: Vec<&str> = filtered.iter().map(|s| s.as_str()).collect();
        Ok(CommandResult::Output(output.join("\n") + "\n"))
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
    let _target = get_option(args, "-t");

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

/// choose-tree [-s] [-w] [-t target-pane]
///
/// Interactive tree view of sessions and windows.
/// Default shows sessions expanded with windows. `-s` collapses to sessions only.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_choose_tree(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let sessions_only = has_flag(args, "-s");
    let tree_info = server.session_tree_info();
    let mut items = Vec::new();

    for (session_name, attached, windows) in tree_info {
        let attached_str =
            if attached > 0 { format!(" (attached: {attached})") } else { String::new() };
        let win_count = windows.len();
        items.push(ListItem {
            display: format!("{session_name}: {win_count} windows{attached_str}"),
            command: vec!["switch-client".into(), "-t".into(), session_name.clone()],
            indent: 0,
            collapsed: sessions_only,
            hidden_children: win_count,
            deletable: true,
            delete_command: vec!["kill-session".into(), "-t".into(), session_name.clone()],
        });

        if !sessions_only {
            for (idx, win_name, is_active, pane_count) in &windows {
                let active_str = if *is_active { "*" } else { "" };
                let panes_str =
                    if *pane_count > 1 { format!(" ({pane_count} panes)") } else { String::new() };
                items.push(ListItem {
                    display: format!("{idx}: {win_name}{active_str}{panes_str}"),
                    command: vec![
                        "select-window".into(),
                        "-t".into(),
                        format!("{session_name}:{idx}"),
                    ],
                    indent: 1,
                    collapsed: false,
                    hidden_children: 0,
                    deletable: true,
                    delete_command: vec![
                        "kill-window".into(),
                        "-t".into(),
                        format!("{session_name}:{idx}"),
                    ],
                });
            }
        }
    }

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

/// display-popup [-B] [-C] [-E|-EE] [-T title] [-t target-pane]
///               [-w width] [-h height] [-x pos] [-y pos] [command]
///
/// Show a popup window with an embedded shell or command.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_display_popup(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    // -C closes any existing popup
    if has_flag(args, "-C") {
        server.close_popup();
        return Ok(CommandResult::Ok);
    }

    let title = get_option(args, "-T").unwrap_or("").to_string();
    let has_border = !has_flag(args, "-B"); // -B disables border
    let close_on_exit = has_flag(args, "-E") || has_flag(args, "-EE");

    // Parse dimensions — support percentage or absolute values
    let client_sx = server.client_sx();
    let client_sy = server.client_sy().saturating_sub(1); // reserve status line

    let width = get_option(args, "-w")
        .map_or(client_sx * 80 / 100, |v| parse_popup_dimension(v, client_sx))
        .max(1);
    let height = get_option(args, "-h")
        .map_or(client_sy * 80 / 100, |v| parse_popup_dimension(v, client_sy))
        .max(1);

    // Position — default centers the popup
    let x = get_option(args, "-x")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| client_sx.saturating_sub(width + u32::from(has_border) * 2) / 2);
    let y = get_option(args, "-y")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| client_sy.saturating_sub(height + u32::from(has_border) * 2) / 2);

    // Collect the command (remaining positional args)
    let positional = crate::command::positional_args(args, &["-t", "-T", "-w", "-h", "-x", "-y"]);
    let command = if positional.is_empty() { None } else { Some(positional.join(" ")) };

    Ok(CommandResult::SpawnPopup(crate::command::PopupConfig {
        x,
        y,
        width,
        height,
        title,
        has_border,
        close_on_exit,
        command,
    }))
}

/// Parse a popup dimension value — either absolute or percentage (e.g., "80%").
fn parse_popup_dimension(value: &str, base: u32) -> u32 {
    if let Some(pct) = value.strip_suffix('%') {
        pct.parse::<u32>().unwrap_or(80) * base / 100
    } else {
        value.parse::<u32>().unwrap_or(base * 80 / 100)
    }
}

/// customize-mode [-t target-pane]
///
/// Interactive options browser showing all options by scope.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_customize_mode(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let _ = args;
    let mut items = Vec::new();

    // Server options
    let server_opts = server.show_options("server", None);
    items.push(ListItem {
        display: format!("Server Options ({} items)", server_opts.len()),
        command: vec![],
        indent: 0,
        collapsed: false,
        hidden_children: 0,
        deletable: false,
        delete_command: vec![],
    });
    for opt in &server_opts {
        let (key, value) = opt.split_once(' ').unwrap_or((opt, ""));
        items.push(ListItem {
            display: format!("{key}: {value}"),
            command: vec!["set-option".into(), "-g".into(), key.to_string(), value.to_string()],
            indent: 1,
            collapsed: false,
            hidden_children: 0,
            deletable: false,
            delete_command: vec![],
        });
    }

    // Session options
    let session_id = server.client_session_id();
    let session_opts = server.show_options("session", session_id);
    let session_label = if let Some(sid) = session_id {
        server.session_name_for_id(sid).unwrap_or_else(|| sid.to_string())
    } else {
        "none".to_string()
    };
    items.push(ListItem {
        display: format!("Session Options [{session_label}] ({} items)", session_opts.len()),
        command: vec![],
        indent: 0,
        collapsed: false,
        hidden_children: 0,
        deletable: false,
        delete_command: vec![],
    });
    for opt in &session_opts {
        let (key, value) = opt.split_once(' ').unwrap_or((opt, ""));
        items.push(ListItem {
            display: format!("{key}: {value}"),
            command: vec!["set-option".into(), key.to_string(), value.to_string()],
            indent: 1,
            collapsed: false,
            hidden_children: 0,
            deletable: false,
            delete_command: vec![],
        });
    }

    // Window options
    let window_opts = server.show_options("window", session_id);
    items.push(ListItem {
        display: format!("Window Options ({} items)", window_opts.len()),
        command: vec![],
        indent: 0,
        collapsed: false,
        hidden_children: 0,
        deletable: false,
        delete_command: vec![],
    });
    for opt in &window_opts {
        let (key, value) = opt.split_once(' ').unwrap_or((opt, ""));
        items.push(ListItem {
            display: format!("{key}: {value}"),
            command: vec!["set-option".into(), "-w".into(), key.to_string(), value.to_string()],
            indent: 1,
            collapsed: false,
            hidden_children: 0,
            deletable: false,
            delete_command: vec![],
        });
    }

    if items.is_empty() {
        return Ok(CommandResult::Output("(no options)\n".to_string()));
    }

    Ok(CommandResult::Overlay(OverlayState::List(ListOverlay {
        items,
        selected: 0,
        scroll_offset: 0,
        filter: String::new(),
        filtering: false,
        title: "customize-mode".into(),
        kind: ListKind::Tree,
    })))
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

/// resize-window [-A] [-D] [-L] [-R] [-U] [-t target-window] [-x width] [-y height] [adjustment]
///
/// Resize a window. -A adjusts to smallest client. -D/-U/-L/-R adjust by amount.
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

    // Directional adjustment
    let amount: u32 = crate::command::positional_args(args, &["-t", "-x", "-y"])
        .first()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    if has_flag(args, "-D") || has_flag(args, "-U") || has_flag(args, "-L") || has_flag(args, "-R")
    {
        // Get current window size (approximate from client size)
        let cur_sx = server.client_sx();
        let cur_sy = server.client_sy().saturating_sub(1);
        let (new_sx, new_sy) = if has_flag(args, "-D") {
            (None, Some(cur_sy + amount))
        } else if has_flag(args, "-U") {
            (None, Some(cur_sy.saturating_sub(amount)))
        } else if has_flag(args, "-R") {
            (Some(cur_sx + amount), None)
        } else {
            (Some(cur_sx.saturating_sub(amount)), None)
        };
        server.resize_window(session_id, window_idx, new_sx, new_sy)?;
        return Ok(CommandResult::Ok);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // parse_popup_dimension
    // ============================================================

    #[test]
    fn popup_dimension_absolute() {
        assert_eq!(parse_popup_dimension("40", 100), 40);
    }

    #[test]
    fn popup_dimension_percentage() {
        assert_eq!(parse_popup_dimension("50%", 100), 50);
    }

    #[test]
    fn popup_dimension_percentage_rounding() {
        // 33% of 100 = 33
        assert_eq!(parse_popup_dimension("33%", 100), 33);
    }

    #[test]
    fn popup_dimension_100_percent() {
        assert_eq!(parse_popup_dimension("100%", 120), 120);
    }

    #[test]
    fn popup_dimension_0_percent() {
        assert_eq!(parse_popup_dimension("0%", 100), 0);
    }

    #[test]
    fn popup_dimension_invalid_falls_back() {
        // Invalid absolute falls back to 80% of base
        assert_eq!(parse_popup_dimension("abc", 100), 80);
    }

    #[test]
    fn popup_dimension_invalid_percent_falls_back() {
        // Invalid percentage value falls back to 80% of base
        assert_eq!(parse_popup_dimension("abc%", 100), 80);
    }

    #[test]
    fn popup_dimension_large_base() {
        assert_eq!(parse_popup_dimension("75%", 200), 150);
    }

    #[test]
    fn popup_dimension_zero_base() {
        assert_eq!(parse_popup_dimension("50%", 0), 0);
        assert_eq!(parse_popup_dimension("abc", 0), 0);
    }
}
