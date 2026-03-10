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

pub mod connect;
pub mod dispatch;
pub mod terminal;

/// Parsed CLI options.
#[derive(Debug, Default)]
pub struct ClientOptions {
    pub socket_name: String,
    pub socket_path: Option<String>,
    pub config_file: Option<String>,
    pub shell_command: Option<String>,
    pub no_start_server: bool,
    /// Control mode (-C flag, tmux -CC).
    pub control_mode: bool,
    /// Index into the original args where the command starts.
    pub command_start: usize,
}

/// Parse rmux CLI arguments (matches tmux getopt behavior).
///
/// Returns parsed options or an error string.
pub fn parse_args(args: &[String]) -> Result<ClientOptions, String> {
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

        let chars: Vec<char> = arg[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            match chars[j] {
                'C' => opts.control_mode = true,
                '2' | 'D' | 'l' | 'u' | 'v' => {}
                'N' => opts.no_start_server = true,
                'V' => return Err("version".to_string()),
                'f' => {
                    if j + 1 < chars.len() {
                        opts.config_file = Some(chars[j + 1..].iter().collect());
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.config_file = Some(args[i].clone());
                        } else {
                            return Err("option requires an argument -- f".to_string());
                        }
                    }
                    j = chars.len();
                }
                'L' => {
                    if j + 1 < chars.len() {
                        opts.socket_name = chars[j + 1..].iter().collect();
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.socket_name.clone_from(&args[i]);
                        } else {
                            return Err("option requires an argument -- L".to_string());
                        }
                    }
                    j = chars.len();
                }
                'S' => {
                    if j + 1 < chars.len() {
                        opts.socket_path = Some(chars[j + 1..].iter().collect());
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.socket_path = Some(args[i].clone());
                        } else {
                            return Err("option requires an argument -- S".to_string());
                        }
                    }
                    j = chars.len();
                }
                'c' => {
                    if j + 1 < chars.len() {
                        opts.shell_command = Some(chars[j + 1..].iter().collect());
                    } else {
                        i += 1;
                        if i < args.len() {
                            opts.shell_command = Some(args[i].clone());
                        } else {
                            return Err("option requires an argument -- c".to_string());
                        }
                    }
                    j = chars.len();
                }
                other => {
                    return Err(format!("unknown option -- {other}"));
                }
            }
            j += 1;
        }
        i += 1;
    }

    opts.command_start = i;
    Ok(opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn no_args_defaults() {
        let opts = parse_args(&args(&["rmux"])).unwrap();
        assert_eq!(opts.socket_name, "default");
        assert_eq!(opts.command_start, 1);
    }

    #[test]
    fn socket_name_separate() {
        let opts = parse_args(&args(&["rmux", "-L", "mysock", "new"])).unwrap();
        assert_eq!(opts.socket_name, "mysock");
    }

    #[test]
    fn socket_name_attached() {
        let opts = parse_args(&args(&["rmux", "-Lmysock", "new"])).unwrap();
        assert_eq!(opts.socket_name, "mysock");
    }

    #[test]
    fn missing_arg_for_l() {
        let result = parse_args(&args(&["rmux", "-L"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("argument"));
    }

    #[test]
    fn missing_arg_for_s() {
        let result = parse_args(&args(&["rmux", "-S"]));
        assert!(result.is_err());
    }

    #[test]
    fn missing_arg_for_f() {
        let result = parse_args(&args(&["rmux", "-f"]));
        assert!(result.is_err());
    }

    #[test]
    fn missing_arg_for_c() {
        let result = parse_args(&args(&["rmux", "-c"]));
        assert!(result.is_err());
    }

    #[test]
    fn unknown_option() {
        let result = parse_args(&args(&["rmux", "-Z"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown option"));
    }

    #[test]
    fn no_start_server_flag() {
        let opts = parse_args(&args(&["rmux", "-N", "list-sessions"])).unwrap();
        assert!(opts.no_start_server);
    }

    #[test]
    fn command_args_start_after_options() {
        let opts = parse_args(&args(&["rmux", "-2", "-u", "new-session", "-s", "test"])).unwrap();
        assert_eq!(opts.command_start, 3);
    }

    #[test]
    fn double_dash_stops_parsing() {
        let opts = parse_args(&args(&["rmux", "--", "-L", "foo"])).unwrap();
        assert_eq!(opts.command_start, 2);
        assert_eq!(opts.socket_name, "default");
    }

    #[test]
    fn socket_path_option() {
        let opts = parse_args(&args(&["rmux", "-S", "/tmp/my.sock"])).unwrap();
        assert_eq!(opts.socket_path.as_deref(), Some("/tmp/my.sock"));
    }

    #[test]
    fn control_mode_flag() {
        let opts = parse_args(&args(&["rmux", "-C", "new"])).unwrap();
        assert!(opts.control_mode);
    }

    #[test]
    fn control_mode_stacked() {
        let opts = parse_args(&args(&["rmux", "-2C", "new"])).unwrap();
        assert!(opts.control_mode);
    }

    #[test]
    fn stacked_flags() {
        let opts = parse_args(&args(&["rmux", "-2uv", "new"])).unwrap();
        assert_eq!(opts.command_start, 2);
    }

    #[test]
    fn version_flag() {
        let result = parse_args(&args(&["rmux", "-V"]));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "version");
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
                let opts = parse_args(&argv).unwrap();
                prop_assert_eq!(opts.socket_name, name);
            }

            #[test]
            fn socket_path_roundtrip(path in "/[a-zA-Z0-9/_.-]{1,100}") {
                let argv = args(&["rmux", "-S"]);
                let mut argv = argv;
                argv.push(path.clone());
                argv.push("new".to_string());
                let opts = parse_args(&argv).unwrap();
                prop_assert_eq!(opts.socket_path.as_deref(), Some(path.as_str()));
            }

            #[test]
            fn config_file_roundtrip(file in "[a-zA-Z0-9/_.-]{1,100}") {
                let argv = args(&["rmux", "-f"]);
                let mut argv = argv;
                argv.push(file.clone());
                argv.push("new".to_string());
                let opts = parse_args(&argv).unwrap();
                prop_assert_eq!(opts.config_file.as_deref(), Some(file.as_str()));
            }

            #[test]
            fn shell_command_roundtrip(cmd in "[a-zA-Z0-9/ _.-]{1,100}") {
                let argv = args(&["rmux", "-c"]);
                let mut argv = argv;
                argv.push(cmd.clone());
                argv.push("new".to_string());
                let opts = parse_args(&argv).unwrap();
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
                let opts = parse_args(&argv).unwrap();
                // command_start should point to "new-session"
                prop_assert_eq!(opts.command_start, argv.len() - 1);
            }
        }
    }
}
