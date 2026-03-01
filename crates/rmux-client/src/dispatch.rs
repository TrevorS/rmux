//! Command dispatch and attached mode.
//!
//! Handles the main client loop when attached to a session:
//! reads keyboard input, sends it to the server, and writes
//! server output to the terminal.

use crate::terminal;
use rmux_protocol::codec::{CodecError, MessageReader, MessageWriter};
use rmux_protocol::message::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{signal, SignalKind};

/// Run the attached client loop.
///
/// This is the main loop for a client that is attached to a session.
/// It reads keyboard input from stdin, sends it to the server,
/// and writes server output to stdout.
pub async fn run_attached(
    reader: &mut MessageReader,
    writer: &mut MessageWriter,
) -> Result<(), CodecError> {
    let _raw = terminal::RawTerminal::enter()
        .map_err(|e| CodecError::Io(std::io::Error::other(e)))?;

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut input_buf = vec![0u8; 4096];

    // Set up SIGWINCH handler
    let mut sigwinch = signal(SignalKind::window_change())
        .map_err(CodecError::Io)?;

    // Send initial size
    let (sx, sy) = terminal::get_terminal_size();
    writer.write_message(&Message::Resize {
        sx,
        sy,
        xpixel: 0,
        ypixel: 0,
    }).await?;

    loop {
        tokio::select! {
            // Read keyboard input from stdin
            result = stdin.read(&mut input_buf) => {
                match result {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        writer.write_message(
                            &Message::InputData(input_buf[..n].to_vec())
                        ).await?;
                    }
                    Err(e) => {
                        return Err(CodecError::Io(e));
                    }
                }
            }

            // Read messages from server
            result = reader.read_message() => {
                match result {
                    Ok(Some(msg)) => {
                        match msg {
                            Message::OutputData(data) => {
                                stdout.write_all(&data).await.map_err(CodecError::Io)?;
                                stdout.flush().await.map_err(CodecError::Io)?;
                            }
                            Message::Detach => {
                                // Server asked us to detach
                                break;
                            }
                            Message::Exit | Message::Exited => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    Ok(None) => break, // Server disconnected
                    Err(e) => return Err(e),
                }
            }

            // Handle SIGWINCH (terminal resize)
            _ = sigwinch.recv() => {
                let (sx, sy) = terminal::get_terminal_size();
                writer.write_message(&Message::Resize {
                    sx,
                    sy,
                    xpixel: 0,
                    ypixel: 0,
                }).await?;
            }
        }
    }

    Ok(())
}

/// Run a non-attached command: send command, read response, exit.
pub async fn run_command(
    reader: &mut MessageReader,
    writer: &mut MessageWriter,
    argv: &[&str],
) -> Result<i32, CodecError> {
    // Send command
    crate::connect::send_command(writer, argv).await?;

    // Read responses until Exit
    let exit_code = 0;
    loop {
        match reader.read_message().await {
            Ok(Some(msg)) => {
                match msg {
                    Message::OutputData(data) => {
                        let mut stdout = tokio::io::stdout();
                        stdout.write_all(&data).await.map_err(CodecError::Io)?;
                        stdout.flush().await.map_err(CodecError::Io)?;
                    }
                    Message::Ready => {
                        // Server is ready for us to attach
                        return Ok(-1); // Signal to switch to attached mode
                    }
                    Message::Exit | Message::Exited => {
                        break;
                    }
                    _ => {}
                }
            }
            Ok(None) => break,
            Err(e) => return Err(e),
        }
    }

    Ok(exit_code)
}
