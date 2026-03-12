//! Command dispatch and attached mode.
//!
//! Handles the main client loop when attached to a session:
//! reads keyboard input, sends it to the server, and writes
//! server output to the terminal.

use crate::terminal;
use rmux_protocol::codec::{CodecError, MessageReader, MessageWriter};
use rmux_protocol::message::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{SignalKind, signal};

/// Run the attached client loop.
///
/// This is the main loop for a client that is attached to a session.
/// It reads keyboard input from stdin, sends it to the server,
/// and writes server output to stdout.
pub async fn run_attached(
    reader: &mut MessageReader,
    writer: &mut MessageWriter,
) -> Result<(), CodecError> {
    let _raw =
        terminal::RawTerminal::enter().map_err(|e| CodecError::Io(std::io::Error::other(e)))?;

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut input_buf = vec![0u8; 4096];

    // Set up signal handlers
    let mut sigwinch = signal(SignalKind::window_change()).map_err(CodecError::Io)?;
    let mut sigterm = signal(SignalKind::terminate()).map_err(CodecError::Io)?;
    let mut sigcont = signal(SignalKind::from_raw(nix::libc::SIGCONT)).map_err(CodecError::Io)?;

    // Send initial size
    let (sx, sy) = terminal::get_terminal_size();
    writer.write_message(&Message::Resize { sx, sy, xpixel: 0, ypixel: 0 }).await?;

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
                            Message::Suspend => {
                                // Server asked us to suspend — send SIGTSTP to ourselves
                                nix::sys::signal::raise(nix::sys::signal::Signal::SIGTSTP).ok();
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

            // Handle SIGTERM — detach cleanly
            _ = sigterm.recv() => {
                break;
            }

            // Handle SIGCONT — redraw by re-sending size
            _ = sigcont.recv() => {
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

/// Run the control mode client loop.
///
/// In control mode, stdin is read line-by-line (not raw mode) and each line
/// is sent as a command. Server sends text notifications (e.g., `%output`)
/// which are printed to stdout as-is.
pub async fn run_control(
    reader: &mut MessageReader,
    writer: &mut MessageWriter,
) -> Result<(), CodecError> {
    use tokio::io::AsyncBufReadExt;

    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut line_buf = String::new();

    loop {
        tokio::select! {
            // Read lines from stdin (commands)
            result = stdin.read_line(&mut line_buf) => {
                match result {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line = line_buf.trim();
                        if !line.is_empty() {
                            let argv: Vec<&str> = line.split_whitespace().collect();
                            crate::connect::send_command(writer, &argv).await?;
                        }
                        line_buf.clear();
                    }
                    Err(e) => return Err(CodecError::Io(e)),
                }
            }

            // Read messages from server
            result = reader.read_message() => {
                match result {
                    Ok(Some(msg)) => match msg {
                        Message::OutputData(data) => {
                            stdout.write_all(&data).await.map_err(CodecError::Io)?;
                            stdout.flush().await.map_err(CodecError::Io)?;
                        }
                        Message::Detach | Message::Exit | Message::Exited => {
                            break;
                        }
                        _ => {}
                    },
                    Ok(None) => break,
                    Err(e) => return Err(e),
                }
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
    let mut exit_code = 0;
    loop {
        match reader.read_message().await {
            Ok(Some(msg)) => match msg {
                Message::OutputData(data) => {
                    let mut stdout = tokio::io::stdout();
                    stdout.write_all(&data).await.map_err(CodecError::Io)?;
                    stdout.flush().await.map_err(CodecError::Io)?;
                }
                Message::ErrorOutput(data) => {
                    let mut stderr = tokio::io::stderr();
                    stderr.write_all(&data).await.map_err(CodecError::Io)?;
                    stderr.flush().await.map_err(CodecError::Io)?;
                    exit_code = 1;
                }
                Message::Ready => {
                    // Server is ready for us to attach
                    return Ok(-1); // Signal to switch to attached mode
                }
                Message::Exit | Message::Exited => {
                    break;
                }
                _ => {}
            },
            Ok(None) => break,
            Err(e) => return Err(e),
        }
    }

    Ok(exit_code)
}
