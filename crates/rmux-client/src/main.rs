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

use rmux_client::{ClientError, ParseResult, connect, dispatch, parse_args};
use rmux_protocol::codec::{MessageReader, MessageWriter};
use std::env;
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    let opts = match parse_args(&args) {
        Ok(ParseResult::Version) => {
            // Plugins (TPM, etc.) parse `tmux -V` and extract digits for
            // version checks. Our version tracks the tmux release we're
            // ported from so plugins see compatible digits (e.g. "36").
            println!("rmux {}", env!("CARGO_PKG_VERSION"));
            process::exit(0);
        }
        Ok(ParseResult::Run(opts)) => opts,
        Err(e) => {
            eprintln!("rmux: {e}");
            process::exit(1);
        }
    };

    // Remaining args are the command
    let command_args: Vec<&str> = args[opts.command_start..].iter().map(String::as_str).collect();

    // Resolve socket path: -S flag > $TMUX env var > default
    let path = if let Some(p) = opts.socket_path {
        p
    } else if let Ok(tmux_env) = env::var("TMUX") {
        // $TMUX format: "socket_path,pid,session_id" — extract socket path
        let socket = tmux_env.split(',').next().unwrap_or(&tmux_env);
        PathBuf::from(socket)
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

    let control_mode = opts.control_mode;
    let config_file = opts.config_file.and_then(|p| p.to_str().map(String::from));
    let exit_code = rt.block_on(async {
        run_client(&path, &command_args, opts.no_start_server, control_mode, config_file.as_deref())
            .await
    });

    match exit_code {
        Ok(code) => process::exit(code),
        Err(e) => {
            eprintln!("rmux: {e}");
            process::exit(1);
        }
    }
}

async fn run_client(
    socket_path: &std::path::Path,
    command_args: &[&str],
    no_start_server: bool,
    control_mode: bool,
    config_file: Option<&str>,
) -> Result<i32, ClientError> {
    // Check if we need to start the server
    let needs_server = !no_start_server
        && matches!(command_args.first().copied(), Some("new-session") | Some("new") | None);

    // Try to connect
    let stream = match connect::connect(socket_path).await {
        Ok(s) => s,
        Err(_) if needs_server => {
            // Start the server
            start_server(socket_path, config_file).map_err(ClientError::ServerStart)?;

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
                None => return Err(ClientError::ConnectionTimeout),
            }
        }
        Err(_) => {
            return Err(ClientError::NoServerRunning(socket_path.to_owned()));
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

    let mut identify_flags = 0i64;
    if control_mode {
        identify_flags |= rmux_protocol::identify::flags::IDENTIFY_CONTROL;
    }
    connect::send_identify(&mut writer, &term, &cwd, identify_flags)
        .await
        .map_err(ClientError::IdentifyFailed)?;

    // Send command and handle response
    match dispatch::run_command(&mut reader, &mut writer, command_args).await? {
        -1 => {
            // Server said Ready - switch to attached/control mode
            if control_mode {
                dispatch::run_control(&mut reader, &mut writer).await?;
            } else {
                dispatch::run_attached(&mut reader, &mut writer).await?;
            }
            Ok(0)
        }
        code => Ok(code),
    }
}

/// Start the server as a background process.
fn start_server(
    socket_path: &std::path::Path,
    config_file: Option<&str>,
) -> Result<(), std::io::Error> {
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

    let mut cmd = std::process::Command::new(server_bin);
    cmd.arg(socket_str);
    if let Some(config) = config_file {
        cmd.args(["-f", config]);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    Ok(())
}
