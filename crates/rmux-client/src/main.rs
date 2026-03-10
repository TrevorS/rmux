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

use rmux_client::{connect, dispatch, parse_args};
use rmux_protocol::codec::{MessageReader, MessageWriter};
use std::env;
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    let opts = match parse_args(&args) {
        Ok(opts) => opts,
        Err(e) if e == "version" => {
            println!("rmux {}", env!("CARGO_PKG_VERSION"));
            process::exit(0);
        }
        Err(e) => {
            eprintln!("rmux: {e}");
            process::exit(1);
        }
    };

    // Remaining args are the command
    let command_args: Vec<&str> = args[opts.command_start..].iter().map(String::as_str).collect();

    // Resolve socket path
    let path = if let Some(ref p) = opts.socket_path {
        PathBuf::from(p)
    } else {
        let tmpdir = env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
        let uid = nix::unistd::getuid();
        PathBuf::from(format!("{tmpdir}/rmux-{uid}/{}", opts.socket_name))
    };

    // Default command: if no command given, try "new-session"
    let command_args = if command_args.is_empty() { vec!["new-session"] } else { command_args };

    // Run the async client
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let exit_code =
        rt.block_on(async { run_client(&path, &command_args, opts.no_start_server).await });

    process::exit(exit_code);
}

async fn run_client(
    socket_path: &std::path::Path,
    command_args: &[&str],
    no_start_server: bool,
) -> i32 {
    // Check if we need to start the server
    let needs_server = !no_start_server
        && matches!(command_args.first().copied(), Some("new-session") | Some("new") | None);

    // Try to connect
    let stream = match connect::connect(socket_path).await {
        Ok(s) => s,
        Err(_) if needs_server => {
            // Start the server
            if let Err(e) = start_server(socket_path) {
                eprintln!("rmux: failed to start server: {e}");
                return 1;
            }

            // Retry with backoff — server needs time to bind the socket
            let mut connected = None;
            for attempt in 0..10 {
                tokio::time::sleep(tokio::time::Duration::from_millis(50 * (attempt + 1))).await;
                match connect::connect(socket_path).await {
                    Ok(s) => {
                        connected = Some(s);
                        break;
                    }
                    Err(_) => continue,
                }
            }

            match connected {
                Some(s) => s,
                None => {
                    eprintln!("rmux: failed to connect to server (timed out)");
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

    let socket_str = socket_path
        .to_str()
        .ok_or_else(|| std::io::Error::other("socket path contains invalid UTF-8"))?;

    std::process::Command::new(server_bin)
        .arg(socket_str)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    Ok(())
}
