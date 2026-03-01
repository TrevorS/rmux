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

use std::env;
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

    const FLAG_256_COLOURS: u32 = 0x1;
    const FLAG_CONTROL: u32 = 0x2;
    const FLAG_NO_DAEMON: u32 = 0x4;
    const FLAG_LOGIN_SHELL: u32 = 0x8;
    const FLAG_NO_START: u32 = 0x10;
    const FLAG_UTF8: u32 = 0x20;
    const FLAG_VERBOSE: u32 = 0x40;

    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') {
            break; // Start of command
        }

        let chars: Vec<char> = arg[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            match chars[j] {
                '2' => _flags |= FLAG_256_COLOURS,
                'C' => _flags |= FLAG_CONTROL,
                'D' => _flags |= FLAG_NO_DAEMON,
                'l' => _flags |= FLAG_LOGIN_SHELL,
                'N' => _flags |= FLAG_NO_START,
                'u' => _flags |= FLAG_UTF8,
                'v' => _flags |= FLAG_VERBOSE,
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
                    j = chars.len(); // Consume rest
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

    println!("rmux {} - Rust tmux replacement", env!("CARGO_PKG_VERSION"));
    println!("Protocol version: {}", rmux_protocol::message::PROTOCOL_VERSION);
    if !command_args.is_empty() {
        println!("Command: {:?}", command_args);
    }
    println!("Socket: {}", socket_path.as_deref().unwrap_or(&socket_name));

    // TODO: Connect to server and execute command
    eprintln!("rmux: server not yet implemented");
    process::exit(1);
}
