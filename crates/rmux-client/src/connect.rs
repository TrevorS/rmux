//! Socket connection to the server.

use rmux_protocol::codec::{CodecError, MessageWriter};
use rmux_protocol::identify::build_identify_sequence;
use rmux_protocol::message::Message;
use std::path::Path;
use tokio::net::UnixStream;

/// Connect to the rmux server at the given socket path.
pub async fn connect(socket_path: &Path) -> Result<UnixStream, std::io::Error> {
    UnixStream::connect(socket_path).await
}

/// Send the client identification sequence to the server.
pub async fn send_identify(
    writer: &mut MessageWriter,
    term: &str,
    cwd: &str,
) -> Result<(), CodecError> {
    let ttyname = nix::unistd::ttyname(std::io::stdin())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    #[allow(clippy::cast_possible_wrap)]
    let pid = std::process::id() as i32;

    let msgs = build_identify_sequence(
        0, // flags
        term,
        &ttyname,
        cwd,
        pid,
        &[], // environ
    );

    for msg in &msgs {
        writer.write_message(msg).await?;
    }

    Ok(())
}

/// Send a command to the server.
pub async fn send_command(writer: &mut MessageWriter, argv: &[&str]) -> Result<(), CodecError> {
    let msg = Message::Command(rmux_protocol::message::MsgCommand {
        #[allow(clippy::cast_possible_wrap)]
        argc: argv.len() as i32,
        argv: argv.iter().map(|s| (*s).to_string()).collect(),
    });
    writer.write_message(&msg).await
}
