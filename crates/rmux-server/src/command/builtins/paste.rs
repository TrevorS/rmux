//! Copy mode and paste buffer commands.

use std::io::Write as _;

use crate::command::{CommandResult, CommandServer, get_option, has_flag, positional_args};
use crate::server::ServerError;

/// Enter copy mode on the active pane.
/// copy-mode [-e] [-H] [-M] [-q] [-s src-pane] [-t target-pane] [-u]
/// -e: exit copy mode if in it (otherwise no-op)
/// -q: cancel copy mode
/// -u: scroll up one page
pub fn cmd_copy_mode(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let cancel = has_flag(args, "-q");
    let scroll_up = has_flag(args, "-u");

    if cancel {
        let _ = server.dispatch_copy_mode_command("cancel");
        return Ok(CommandResult::Ok);
    }

    server.enter_copy_mode()?;

    if scroll_up {
        let _ = server.dispatch_copy_mode_command("page-up");
    }

    Ok(CommandResult::Ok)
}

/// Paste the top buffer (or named buffer) to the active pane.
/// paste-buffer [-d] [-p] [-r] [-s separator] [-b buffer-name] [-t target-pane]
/// -d: delete buffer after pasting
/// -p: use bracketed paste mode (wrap in ESC[200~ / ESC[201~)
/// -r: don't use terminal newline translation
pub fn cmd_paste_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let delete = has_flag(args, "-d");
    let _bracket_paste = has_flag(args, "-p");
    let _separator = get_option(args, "-s");
    let name = get_option(args, "-b");
    server.paste_buffer(name)?;
    if delete {
        let buf_name = name.unwrap_or("buffer0000");
        // Ignore error if buffer already gone
        let _ = server.delete_buffer(buf_name);
    }
    Ok(CommandResult::Ok)
}

/// List all paste buffers.
#[allow(clippy::unnecessary_wraps)]
pub fn cmd_list_buffers(
    _args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let buffers = server.list_buffers();
    if buffers.is_empty() {
        Ok(CommandResult::Ok)
    } else {
        Ok(CommandResult::Output(buffers.join("\n") + "\n"))
    }
}

/// Show the contents of a buffer.
pub fn cmd_show_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let name = get_option(args, "-b").unwrap_or("buffer0000");
    let content = server.show_buffer(name)?;
    Ok(CommandResult::Output(content))
}

/// Set a buffer's contents.
/// -a: append to existing buffer instead of replacing
pub fn cmd_set_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let append = has_flag(args, "-a");
    let name = get_option(args, "-b");
    let positionals = positional_args(args, &["-b"]);
    let data = positionals
        .first()
        .ok_or_else(|| ServerError::Command("usage: set-buffer [-b name] data".into()))?;
    let buf_name = name.unwrap_or("");
    if append {
        // Read existing content and append
        let existing =
            server.show_buffer(if buf_name.is_empty() { "buffer0000" } else { buf_name });
        let new_data = match existing {
            Ok(old) => format!("{old}{data}"),
            Err(_) => (*data).to_string(),
        };
        server.set_buffer(buf_name, &new_data)?;
    } else {
        server.set_buffer(buf_name, data)?;
    }
    Ok(CommandResult::Ok)
}

/// Delete a buffer.
pub fn cmd_delete_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let name = get_option(args, "-b")
        .ok_or_else(|| ServerError::Command("usage: delete-buffer -b name".into()))?;
    server.delete_buffer(name)?;
    Ok(CommandResult::Ok)
}

/// save-buffer [-a] [-b buffer-name] path
/// -a: append to file instead of overwriting
pub fn cmd_save_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let append = has_flag(args, "-a");
    let name = get_option(args, "-b");
    let positionals = positional_args(args, &["-b"]);
    let path = positionals
        .first()
        .ok_or_else(|| ServerError::Command("usage: save-buffer [-b name] path".into()))?;
    if append {
        // Get buffer content, then append to file
        let buf_name = name.unwrap_or("buffer0000");
        let content = server.show_buffer(buf_name)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| ServerError::Command(format!("save-buffer: {e}")))?;
        file.write_all(content.as_bytes())
            .map_err(|e| ServerError::Command(format!("save-buffer: {e}")))?;
    } else {
        server.save_buffer(name, path)?;
    }
    Ok(CommandResult::Ok)
}

/// load-buffer [-b buffer-name] path
pub fn cmd_load_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let name = get_option(args, "-b");
    let positionals = positional_args(args, &["-b"]);
    let path = positionals
        .first()
        .ok_or_else(|| ServerError::Command("usage: load-buffer [-b name] path".into()))?;
    server.load_buffer(name, path)?;
    Ok(CommandResult::Ok)
}
