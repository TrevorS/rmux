//! Configuration file parser (tmux.conf compatible).
//!
//! Parses tmux-compatible configuration files. Each line is a command
//! with arguments. Comments start with `#`. Quoted strings are handled.

/// Parse a config file's content into a list of command argument vectors.
///
/// Each non-empty, non-comment line becomes one command.
/// Supports double-quoted strings and backslash escaping.
pub fn parse_config_lines(content: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Handle semicolons as command separators (like tmux)
        for part in split_on_semicolons(trimmed) {
            let part = part.trim();
            if part.is_empty() || part.starts_with('#') {
                continue;
            }
            let argv = tokenize_command(part);
            if !argv.is_empty() {
                commands.push(argv);
            }
        }
    }

    commands
}

/// Split a line on unquoted semicolons.
fn split_on_semicolons(line: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_quote = false;

    while i < bytes.len() {
        match bytes[i] {
            b'"' if i == 0 || bytes[i - 1] != b'\\' => {
                in_quote = !in_quote;
            }
            b';' if !in_quote => {
                parts.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }

    if start < line.len() {
        parts.push(&line[start..]);
    } else if start == line.len() {
        // Trailing semicolon
    }

    parts
}

/// Tokenize a command string into arguments, handling quotes.
pub fn tokenize_command(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_double_quote = false;
    let mut in_single_quote = false;

    while let Some(&ch) = chars.peek() {
        match ch {
            '#' if !in_double_quote && !in_single_quote => {
                // Rest of line is a comment
                break;
            }
            '"' if !in_single_quote => {
                chars.next();
                in_double_quote = !in_double_quote;
            }
            '\'' if !in_double_quote => {
                chars.next();
                in_single_quote = !in_single_quote;
            }
            '\\' if in_double_quote => {
                chars.next();
                if let Some(&next) = chars.peek() {
                    match next {
                        '"' | '\\' | '$' | '`' => {
                            current.push(next);
                            chars.next();
                        }
                        'n' => {
                            current.push('\n');
                            chars.next();
                        }
                        't' => {
                            current.push('\t');
                            chars.next();
                        }
                        _ => {
                            current.push('\\');
                            current.push(next);
                            chars.next();
                        }
                    }
                } else {
                    current.push('\\');
                }
            }
            '\\' if !in_single_quote && !in_double_quote => {
                chars.next();
                if let Some(&next) = chars.peek() {
                    current.push(next);
                    chars.next();
                }
            }
            ' ' | '\t' if !in_double_quote && !in_single_quote => {
                chars.next();
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => {
                current.push(ch);
                chars.next();
            }
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// Load a configuration file and parse it into commands.
pub fn load_config_file(path: &str) -> Result<Vec<Vec<String>>, std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_config_lines(&content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_comments() {
        let input = "\n# comment\n  \n  # another\n";
        assert_eq!(parse_config_lines(input), Vec::<Vec<String>>::new());
    }

    #[test]
    fn simple_command() {
        let input = "set-option -g history-limit 5000\n";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set-option", "-g", "history-limit", "5000"]);
    }

    #[test]
    fn quoted_strings() {
        let input = r#"set-option -g status-left "[#S] ""#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set-option", "-g", "status-left", "[#S] "]);
    }

    #[test]
    fn single_quoted() {
        let input = "bind-key -T prefix '\"' split-window\n";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["bind-key", "-T", "prefix", "\"", "split-window"]);
    }

    #[test]
    fn semicolon_separator() {
        let input = "set -g mouse on ; set -g status on\n";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], vec!["set", "-g", "mouse", "on"]);
        assert_eq!(cmds[1], vec!["set", "-g", "status", "on"]);
    }

    #[test]
    fn inline_comment() {
        let input = "set -g mouse on # enable mouse\n";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "mouse", "on"]);
    }

    #[test]
    fn multiple_lines() {
        let input = "new-session -d -s main\nbind c new-window\n# done\n";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn backslash_escaping() {
        // Backslash followed by special characters in double quotes
        let input = r#"set -g foo "a\"b\\c""#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "a\"b\\c"]);
    }

    #[test]
    fn newline_escape_in_double_quotes() {
        let input = r#"set -g foo "line1\nline2""#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "line1\nline2"]);
    }

    #[test]
    fn tab_escape_in_double_quotes() {
        let input = r#"set -g foo "col1\tcol2""#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "col1\tcol2"]);
    }

    #[test]
    fn empty_quoted_string() {
        // The tokenizer skips empty tokens, so "" produces no arg.
        // This documents the current behavior.
        let input = r#"set -g foo """#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo"]);
    }

    #[test]
    fn consecutive_semicolons() {
        // ";;" should produce two empty segments, both skipped
        let input = ";;";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 0);
    }

    #[test]
    fn whitespace_only_line() {
        let input = "   \t  \n  \t\t  \n";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 0);
    }

    #[test]
    fn trailing_semicolon() {
        let input = "set -g foo; ";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo"]);
    }

    #[test]
    fn mixed_quotes() {
        let input = r#"set -g foo "hello" 'world' "it's""#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "hello", "world", "it's"]);
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_config_never_panics(content in "\\PC{0,200}") {
                let _ = parse_config_lines(&content);
            }

            #[test]
            fn comment_lines_always_ignored(
                comment in "# [^\n]{0,100}"
            ) {
                let result = parse_config_lines(&comment);
                prop_assert!(result.is_empty());
            }

            #[test]
            fn empty_lines_always_ignored(
                spaces in "[ \t]{0,50}"
            ) {
                let result = parse_config_lines(&spaces);
                prop_assert!(result.is_empty());
            }

            #[test]
            fn simple_words_parsed_correctly(
                word1 in "[a-z]{1,20}",
                word2 in "[a-z]{1,20}",
            ) {
                let input = format!("{word1} {word2}");
                let result = parse_config_lines(&input);
                prop_assert_eq!(result.len(), 1);
                prop_assert_eq!(&result[0][0], &word1);
                prop_assert_eq!(&result[0][1], &word2);
            }

            #[test]
            fn multiple_comment_lines_all_ignored(
                n in 1usize..20,
            ) {
                let input: String = (0..n).fold(String::new(), |mut acc, i| {
                    use std::fmt::Write;
                    writeln!(acc, "# comment {i}").unwrap();
                    acc
                });
                let result = parse_config_lines(&input);
                prop_assert!(result.is_empty());
            }
        }
    }
}
