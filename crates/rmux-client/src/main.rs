//! rmux client entry point.
//!
//! Usage matches tmux exactly:
//!   rmux [options] [command [flags]]
//!
//! Options:
//!   -2            Force 256 colors
//!   -C            Start in control mode
//!   -c shell-command  Execute shell-command using the default shell
//!   -D            Do not start the server as a daemon
//!   -f file       Specify an alternative configuration file
//!   -L socket-name  Use a different socket name
//!   -l            Behave as a login shell
//!   -N            Do not start the server
//!   -S socket-path  Specify a full alternative path to the socket
//!   -u            Request UTF-8
//!   -V            Report version
//!   -v            Request verbose logging

use rmux_client::{connect, dispatch};
use rmux_protocol::codec::{MessageReader, MessageWriter};
use std::env;
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Manual getopt to match tmux's exact behavior
    let mut i = 1;
    let mut socket_name = "default".to_string();
    let mut socket_path: Option<String> = None;
    let mut _config_file: Option<String> = None;
    let mut _shell_command: Option<String> = None;
    let mut _flags: u32 = 0;

    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') || arg == "--" {
            if arg == "--" {
                i += 1;
            }
            break; // Start of command
        }

        let chars: Vec<char> = arg[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            match chars[j] {
                '2' => _flags |= 0x1,
                'C' => _flags |= 0x2,
                'D' => _flags |= 0x4,
                'l' => _flags |= 0x8,
                'N' => _flags |= 0x10,
                'u' => _flags |= 0x20,
                'v' => _flags |= 0x40,
                'V' => {
                    println!("rmux {}", env!("CARGO_PKG_VERSION"));
                    process::exit(0);
                }
                'f' => {
                    if j + 1 < chars.len() {
                        _config_file = Some(chars[j + 1..].iter().collect());
                    } else {
                        i += 1;
                        if i < args.len() {
                            _config_file = Some(args[i].clone());
                        }
                    }
                    j = chars.len();
                }
                'L' => {
                    if j + 1 < chars.len() {
                        socket_name = chars[j + 1..].iter().collect();
                    } else {
                        i += 1;
                        if i < args.len() {
                            socket_name = args[i].clone();
                        }
                    }
                    j = chars.len();
                }
                'S' => {
                    if j + 1 < chars.len() {
                        socket_path = Some(chars[j + 1..].iter().collect());
                    } else {
                        i += 1;
                        if i < args.len() {
                            socket_path = Some(args[i].clone());
                        }
                    }
                    j = chars.len();
                }
                'c' => {
                    if j + 1 < chars.len() {
                        _shell_command = Some(chars[j + 1..].iter().collect());
                    } else {
                        i += 1;
                        if i < args.len() {
                            _shell_command = Some(args[i].clone());
                        }
                    }
                    j = chars.len();
                }
                _ => {
                    eprintln!("rmux: unknown option -- {}", chars[j]);
                    process::exit(1);
                }
            }
            j += 1;
        }
        i += 1;
    }

    // Remaining args are the command
    let command_args: Vec<&str> = args[i..].iter().map(String::as_str).collect();

    // Resolve socket path
    let path = if let Some(ref p) = socket_path {
        PathBuf::from(p)
    } else {
        let tmpdir = env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        let uid = nix::unistd::getpid();
        PathBuf::from(format!("{tmpdir}/rmux-{uid}/{socket_name}"))
    };

    // Default command: if no command given, try "new-session"
    let command_args = if command_args.is_empty() { vec!["new-session"] } else { command_args };

    // Run the async client
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let exit_code = rt.block_on(async { run_client(&path, &command_args).await });

    process::exit(exit_code);
}

async fn run_client(socket_path: &std::path::Path, command_args: &[&str]) -> i32 {
    // Check if we need to start the server
    let needs_server =
        matches!(command_args.first().copied(), Some("new-session") | Some("new") | None);

    // Try to connect
    let stream = match connect::connect(socket_path).await {
        Ok(s) => s,
        Err(_) if needs_server => {
            // Start the server
            if let Err(e) = start_server(socket_path) {
                eprintln!("rmux: failed to start server: {e}");
                return 1;
            }
            // Wait a bit for the server to start
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            // Retry connection
            match connect::connect(socket_path).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("rmux: failed to connect to server: {e}");
                    return 1;
                }
            }
        }
        Err(e) => {
            eprintln!("rmux: failed to connect to server: {e}");
            eprintln!("rmux: no server running on {}", socket_path.display());
            return 1;
        }
    };

    let (read_half, write_half) = stream.into_split();
    let mut reader = MessageReader::new(read_half);
    let mut writer = MessageWriter::new(write_half);

    // Send identification
    let term = env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/".to_string());

    if let Err(e) = connect::send_identify(&mut writer, &term, &cwd).await {
        eprintln!("rmux: identify failed: {e}");
        return 1;
    }

    // Send command and handle response
    match dispatch::run_command(&mut reader, &mut writer, command_args).await {
        Ok(-1) => {
            // Server said Ready - switch to attached mode
            if let Err(e) = dispatch::run_attached(&mut reader, &mut writer).await {
                eprintln!("\rrmux: {e}");
                return 1;
            }
            0
        }
        Ok(code) => code,
        Err(e) => {
            eprintln!("rmux: {e}");
            1
        }
    }
}

/// Start the server as a background process.
fn start_server(socket_path: &std::path::Path) -> Result<(), std::io::Error> {
    let server_bin = env::current_exe()?
        .parent()
        .map(|p| p.join("rmux-server"))
        .unwrap_or_else(|| PathBuf::from("rmux-server"));

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::process::Command::new(server_bin)
        .arg(socket_path.to_str().unwrap_or(""))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    Ok(())
}
