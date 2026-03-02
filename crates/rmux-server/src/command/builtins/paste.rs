//! Copy mode and paste buffer commands.

use crate::command::{CommandResult, CommandServer, get_option, positional_args};
use crate::server::ServerError;

/// Enter copy mode on the active pane.
pub fn cmd_copy_mode(
    _args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    server.enter_copy_mode()?;
    Ok(CommandResult::Ok)
}

/// Paste the top buffer (or named buffer) to the active pane.
pub fn cmd_paste_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let name = get_option(args, "-b");
    server.paste_buffer(name)?;
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
pub fn cmd_set_buffer(
    args: &[String],
    server: &mut dyn CommandServer,
) -> Result<CommandResult, ServerError> {
    let name = get_option(args, "-b");
    let positionals = positional_args(args, &["-b"]);
    let data = positionals
        .first()
        .ok_or_else(|| ServerError::Command("usage: set-buffer [-b name] data".into()))?;
    let buf_name = name.unwrap_or("");
    server.set_buffer(buf_name, data)?;
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
