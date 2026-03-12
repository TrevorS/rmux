#![deny(unsafe_code)]
#![deny(clippy::all, clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::wildcard_imports,
    clippy::doc_markdown
)]

//! # rmux-client
//!
//! Client process for rmux. Handles CLI parsing, terminal setup, and
//! communication with the server.

use std::path::PathBuf;

use rmux_protocol::codec::CodecError;

pub mod connect;
pub mod dispatch;
pub mod terminal;

/// Parsed CLI options.
#[derive(Debug, Default)]
pub struct ClientOptions {
    pub socket_name: String,
    pub socket_path: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub shell_command: Option<String>,
    pub no_start_server: bool,
    /// Control mode (-C flag, tmux -CC).
    pub control_mode: bool,
    /// Index into the original args where the command starts.
    pub command_start: usize,
}

/// Result of successfully parsing CLI arguments.
#[derive(Debug)]
pub enum ParseResult {
    Run(ClientOptions),
    Version,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("option requires an argument -- {0}")]
    MissingArgument(char),
    #[error("unknown option -- {0}")]
    UnknownOption(char),
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("no server running on {0}")]
    NoServerRunning(PathBuf),
    #[error("failed to connect to server (timed out)")]
    ConnectionTimeout,
    #[error("failed to start server: {0}")]
    ServerStart(std::io::Error),
    #[error("identify failed: {0}")]
    IdentifyFailed(CodecError),
    #[error("{0}")]
    Protocol(CodecError),
    #[error("{0}")]
    Io(std::io::Error),
}

impl From<CodecError> for ClientError {
    fn from(e: CodecError) -> Self {
        ClientError::Protocol(e)
    }
}

/// Parse rmux CLI arguments (matches tmux getopt behavior).
pub fn parse_args(args: &[String]) -> Result<ParseResult, ParseError> {
    let mut opts = ClientOptions { socket_name: "default".to_string(), ..ClientOptions::default() };

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') || arg == "--" {
            if arg == "--" {
                i += 1;
            }
            break;
        }

        let bytes = arg.as_bytes();
        let mut j = 1; // skip leading '-'
        while j < bytes.len() {
            match bytes[j] {
                b'C' => opts.control_mode = true,
                b'2' | b'D' | b'l' | b'u' | b'v' => {}
                b'N' => opts.no_start_server = true,
                b'V' => return Ok(ParseResult::Version),
                b'f' => {
                    if j + 1 < bytes.len() {
                        opts.config_file = Some(PathBuf::from(&arg[j + 1..]));
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.config_file = Some(PathBuf::from(&args[i]));
                        } else {
                            return Err(ParseError::MissingArgument('f'));
                        }
                    }
                    break;
                }
                b'L' => {
                    if j + 1 < bytes.len() {
                        opts.socket_name = arg[j + 1..].to_string();
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.socket_name.clone_from(&args[i]);
                        } else {
                            return Err(ParseError::MissingArgument('L'));
                        }
                    }
                    break;
                }
                b'S' => {
                    if j + 1 < bytes.len() {
                        opts.socket_path = Some(PathBuf::from(&arg[j + 1..]));
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.socket_path = Some(PathBuf::from(&args[i]));
                        } else {
                            return Err(ParseError::MissingArgument('S'));
                        }
                    }
                    break;
                }
                b'c' => {
                    if j + 1 < bytes.len() {
                        opts.shell_command = Some(arg[j + 1..].to_string());
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.shell_command = Some(args[i].clone());
                        } else {
                            return Err(ParseError::MissingArgument('c'));
                        }
                    }
                    break;
                }
                other => {
                    return Err(ParseError::UnknownOption(char::from(other)));
                }
            }
            j += 1;
        }
        i += 1;
    }

    opts.command_start = i;
    Ok(ParseResult::Run(opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    /// Unwrap a `ParseResult::Run` or panic.
    fn unwrap_run(result: Result<ParseResult, ParseError>) -> ClientOptions {
        match result.unwrap() {
            ParseResult::Run(opts) => opts,
            ParseResult::Version => panic!("expected Run, got Version"),
        }
    }

    #[test]
    fn no_args_defaults() {
        let opts = unwrap_run(parse_args(&args(&["rmux"])));
        assert_eq!(opts.socket_name, "default");
        assert_eq!(opts.command_start, 1);
    }

    #[test]
    fn socket_name_separate() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "-L", "mysock", "new"])));
        assert_eq!(opts.socket_name, "mysock");
    }

    #[test]
    fn socket_name_attached() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "-Lmysock", "new"])));
        assert_eq!(opts.socket_name, "mysock");
    }

    #[test]
    fn missing_arg_for_l() {
        let err = parse_args(&args(&["rmux", "-L"])).unwrap_err();
        assert!(matches!(err, ParseError::MissingArgument('L')));
    }

    #[test]
    fn missing_arg_for_s() {
        let err = parse_args(&args(&["rmux", "-S"])).unwrap_err();
        assert!(matches!(err, ParseError::MissingArgument('S')));
    }

    #[test]
    fn missing_arg_for_f() {
        let err = parse_args(&args(&["rmux", "-f"])).unwrap_err();
        assert!(matches!(err, ParseError::MissingArgument('f')));
    }

    #[test]
    fn missing_arg_for_c() {
        let err = parse_args(&args(&["rmux", "-c"])).unwrap_err();
        assert!(matches!(err, ParseError::MissingArgument('c')));
    }

    #[test]
    fn unknown_option() {
        let err = parse_args(&args(&["rmux", "-Z"])).unwrap_err();
        assert!(matches!(err, ParseError::UnknownOption('Z')));
    }

    #[test]
    fn no_start_server_flag() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "-N", "list-sessions"])));
        assert!(opts.no_start_server);
    }

    #[test]
    fn command_args_start_after_options() {
        let opts =
            unwrap_run(parse_args(&args(&["rmux", "-2", "-u", "new-session", "-s", "test"])));
        assert_eq!(opts.command_start, 3);
    }

    #[test]
    fn double_dash_stops_parsing() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "--", "-L", "foo"])));
        assert_eq!(opts.command_start, 2);
        assert_eq!(opts.socket_name, "default");
    }

    #[test]
    fn socket_path_option() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "-S", "/tmp/my.sock"])));
        assert_eq!(opts.socket_path, Some(PathBuf::from("/tmp/my.sock")));
    }

    #[test]
    fn control_mode_flag() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "-C", "new"])));
        assert!(opts.control_mode);
    }

    #[test]
    fn control_mode_stacked() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "-2C", "new"])));
        assert!(opts.control_mode);
    }

    #[test]
    fn stacked_flags() {
        let opts = unwrap_run(parse_args(&args(&["rmux", "-2uv", "new"])));
        assert_eq!(opts.command_start, 2);
    }

    #[test]
    fn version_flag() {
        let result = parse_args(&args(&["rmux", "-V"])).unwrap();
        assert!(matches!(result, ParseResult::Version));
    }

    #[test]
    fn error_display_formats() {
        assert_eq!(
            ParseError::MissingArgument('L').to_string(),
            "option requires an argument -- L"
        );
        assert_eq!(ParseError::UnknownOption('Z').to_string(), "unknown option -- Z");
        assert_eq!(
            ClientError::NoServerRunning(PathBuf::from("/tmp/rmux-501/default")).to_string(),
            "no server running on /tmp/rmux-501/default"
        );
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_args_never_panics(argv in proptest::collection::vec(".*", 1..10)) {
                // parse_args should never panic, only return Ok or Err
                let _ = parse_args(&argv);
            }

            #[test]
            fn socket_name_roundtrip(name in "[a-zA-Z0-9_-]{1,50}") {
                let argv = args(&["rmux", "-L"]);
                let mut argv = argv;
                argv.push(name.clone());
                argv.push("new".to_string());
                let opts = unwrap_run(parse_args(&argv));
                prop_assert_eq!(opts.socket_name, name);
            }

            #[test]
            fn socket_path_roundtrip(path in "/[a-zA-Z0-9/_.-]{1,100}") {
                let argv = args(&["rmux", "-S"]);
                let mut argv = argv;
                argv.push(path.clone());
                argv.push("new".to_string());
                let opts = unwrap_run(parse_args(&argv));
                prop_assert_eq!(opts.socket_path, Some(PathBuf::from(&path)));
            }

            #[test]
            fn config_file_roundtrip(file in "[a-zA-Z0-9/_.-]{1,100}") {
                let argv = args(&["rmux", "-f"]);
                let mut argv = argv;
                argv.push(file.clone());
                argv.push("new".to_string());
                let opts = unwrap_run(parse_args(&argv));
                prop_assert_eq!(opts.config_file, Some(PathBuf::from(&file)));
            }

            #[test]
            fn shell_command_roundtrip(cmd in "[a-zA-Z0-9/ _.-]{1,100}") {
                let argv = args(&["rmux", "-c"]);
                let mut argv = argv;
                argv.push(cmd.clone());
                argv.push("new".to_string());
                let opts = unwrap_run(parse_args(&argv));
                prop_assert_eq!(opts.shell_command.as_deref(), Some(cmd.as_str()));
            }

            #[test]
            fn command_start_after_flags(
                flags in proptest::collection::vec(
                    prop::sample::select(vec!["-2", "-u", "-v", "-l", "-C", "-D"]),
                    0..5
                )
            ) {
                let mut argv: Vec<String> = vec!["rmux".to_string()];
                argv.extend(flags.iter().map(ToString::to_string));
                argv.push("new-session".to_string());
                let opts = unwrap_run(parse_args(&argv));
                // command_start should point to "new-session"
                prop_assert_eq!(opts.command_start, argv.len() - 1);
            }
        }
    }
}
