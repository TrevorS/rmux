//! Configuration file parser (tmux.conf compatible).
//!
//! Parses tmux-compatible configuration files. Each line is a command
//! with arguments. Comments start with `#`. Quoted strings are handled.
//!
//! Supports:
//! - Line continuation (backslash at end of line)
//! - `%if`/`%elif`/`%else`/`%endif` conditional directives
//! - `%hidden VAR=VALUE` hidden variable assignment
//! - `${VAR}` interpolation from hidden vars and environment

use std::collections::HashMap;

/// Callback type for format expansion in `%if` conditions.
type FormatExpander = Box<dyn Fn(&str) -> String>;

/// Context for config file parsing with conditional evaluation support.
pub struct ConfigContext {
    /// Hidden variables set by `%hidden VAR=VALUE`.
    pub hidden_vars: HashMap<String, String>,
    /// Format expander for evaluating `%if` conditions.
    /// Takes a format string (e.g., `"#{==:#{@opt},val}"`) and returns the expanded result.
    format_expand: Option<FormatExpander>,
}

impl ConfigContext {
    /// Create a new empty config context (no format expansion).
    pub fn new() -> Self {
        Self { hidden_vars: HashMap::new(), format_expand: None }
    }

    /// Set the format expander callback (used for `%if` condition evaluation).
    pub fn set_format_expand(&mut self, f: impl Fn(&str) -> String + 'static) {
        self.format_expand = Some(Box::new(f));
    }

    /// Evaluate a `%if` condition string by expanding it as a format string.
    /// Returns true if the result is non-empty and not "0".
    fn eval_condition(&self, expr: &str) -> bool {
        // Strip surrounding quotes if present (tmux uses %if "#{...}")
        let expr = expr.trim();
        let expr = if (expr.starts_with('"') && expr.ends_with('"'))
            || (expr.starts_with('\'') && expr.ends_with('\''))
        {
            &expr[1..expr.len() - 1]
        } else {
            expr
        };

        if let Some(ref expand) = self.format_expand {
            let result = expand(expr);
            !result.is_empty() && result != "0"
        } else {
            // Without a format expander, treat non-empty expressions as true
            !expr.is_empty()
        }
    }

    /// Expand `${VAR}` references in a string.
    /// Checks hidden_vars first, then falls back to environment variables.
    fn expand_vars(&self, input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' && chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut var_name = String::new();
                let mut found_close = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        found_close = true;
                        break;
                    }
                    var_name.push(c);
                }
                if found_close && !var_name.is_empty() {
                    if let Some(val) = self.hidden_vars.get(&var_name) {
                        result.push_str(val);
                    } else if let Ok(val) = std::env::var(&var_name) {
                        result.push_str(&val);
                    }
                    // If not found, expand to empty string (tmux behavior)
                } else {
                    // Malformed ${...}, emit literally
                    result.push('$');
                    result.push('{');
                    result.push_str(&var_name);
                    if !found_close {
                        // unclosed brace
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }
}

impl Default for ConfigContext {
    fn default() -> Self {
        Self::new()
    }
}

/// State for tracking nested `%if`/`%elif`/`%else`/`%endif` blocks.
#[derive(Clone, Copy)]
struct CondState {
    /// Whether any branch in this if/elif/else chain has been taken.
    any_taken: bool,
    /// Whether the current branch is active (lines should be emitted).
    active: bool,
}

/// Join lines ending with backslash (line continuation).
fn join_continuation_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end.ends_with('\\') && !trimmed_end.ends_with("\\\\") {
            // Line continuation: strip trailing backslash and append next line
            current.push_str(&trimmed_end[..trimmed_end.len() - 1]);
        } else {
            current.push_str(line);
            lines.push(std::mem::take(&mut current));
        }
    }

    // If the last line had a trailing backslash, include it as-is
    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

/// Parse a config file's content into a list of command argument vectors.
///
/// Each non-empty, non-comment line becomes one command.
/// Supports double-quoted strings and backslash escaping.
pub fn parse_config_lines(content: &str) -> Vec<Vec<String>> {
    parse_config_with_context(content, &mut ConfigContext::new())
}

/// Parse a config file with a context for conditional evaluation and variable expansion.
pub fn parse_config_with_context(content: &str, ctx: &mut ConfigContext) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    let mut cond_stack: Vec<CondState> = Vec::new();

    let lines = join_continuation_lines(content);

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Handle conditional directives
        if let Some(expr) = trimmed.strip_prefix("%if ") {
            let active = is_active(&cond_stack);
            if active {
                let result = ctx.eval_condition(expr);
                cond_stack.push(CondState { any_taken: result, active: result });
            } else {
                // Inside a false block — push inactive state
                cond_stack.push(CondState { any_taken: true, active: false });
            }
            continue;
        }

        if let Some(expr) = trimmed.strip_prefix("%elif ") {
            let len = cond_stack.len();
            if len > 0 {
                let parent_active = len <= 1 || is_active(&cond_stack[..len - 1]);
                let state = &mut cond_stack[len - 1];
                if state.any_taken {
                    state.active = false;
                } else if parent_active {
                    let result = ctx.eval_condition(expr);
                    state.any_taken = result;
                    state.active = result;
                }
            }
            continue;
        }

        if trimmed == "%else" {
            let len = cond_stack.len();
            if len > 0 {
                let parent_active = len <= 1 || is_active(&cond_stack[..len - 1]);
                let state = &mut cond_stack[len - 1];
                if state.any_taken {
                    state.active = false;
                } else {
                    state.active = parent_active;
                    state.any_taken = true;
                }
            }
            continue;
        }

        if trimmed == "%endif" {
            cond_stack.pop();
            continue;
        }

        // Handle %hidden directive
        if let Some(rest) = trimmed.strip_prefix("%hidden ") {
            if is_active(&cond_stack) {
                if let Some((name, value)) = parse_hidden_assignment(rest) {
                    // Expand ${VAR} in the value
                    let expanded_value = ctx.expand_vars(&value);
                    ctx.hidden_vars.insert(name, expanded_value);
                }
            }
            continue;
        }

        // Skip lines inside false conditional blocks
        if !is_active(&cond_stack) {
            continue;
        }

        // Expand ${VAR} references
        let expanded = ctx.expand_vars(trimmed);

        // Handle semicolons as command separators (like tmux)
        for part in split_on_semicolons(&expanded) {
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

/// Check if all levels of the condition stack are active.
fn is_active(stack: &[CondState]) -> bool {
    stack.iter().all(|s| s.active)
}

/// Parse a `%hidden VAR=VALUE` assignment.
/// Returns (var_name, value) or None if malformed.
fn parse_hidden_assignment(input: &str) -> Option<(String, String)> {
    let input = input.trim();
    let eq_pos = input.find('=')?;
    let name = input[..eq_pos].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let value_part = input[eq_pos + 1..].trim();
    // Strip quotes from value if present
    let value = if (value_part.starts_with('"') && value_part.ends_with('"'))
        || (value_part.starts_with('\'') && value_part.ends_with('\''))
    {
        value_part[1..value_part.len() - 1].to_string()
    } else {
        value_part.to_string()
    };
    Some((name, value))
}

/// Split a line on unquoted, unescaped semicolons.
fn split_on_semicolons(line: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_double_quote = false;
    let mut in_single_quote = false;

    while i < bytes.len() {
        match bytes[i] {
            b'"' if !in_single_quote && (i == 0 || bytes[i - 1] != b'\\') => {
                in_double_quote = !in_double_quote;
            }
            b'\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            b'\\' if !in_single_quote && !in_double_quote => {
                // Skip escaped character (handles \;)
                i += 1;
            }
            b';' if !in_double_quote && !in_single_quote => {
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
    let mut has_quotes = false; // Track if current token had quotes (preserves empty strings)

    while let Some(&ch) = chars.peek() {
        match ch {
            '#' if !in_double_quote && !in_single_quote => {
                // Rest of line is a comment
                break;
            }
            '"' if !in_single_quote => {
                chars.next();
                in_double_quote = !in_double_quote;
                has_quotes = true;
            }
            '\'' if !in_double_quote => {
                chars.next();
                in_single_quote = !in_single_quote;
                has_quotes = true;
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
                if !current.is_empty() || has_quotes {
                    args.push(std::mem::take(&mut current));
                    has_quotes = false;
                }
            }
            _ => {
                current.push(ch);
                chars.next();
            }
        }
    }

    if !current.is_empty() || has_quotes {
        args.push(current);
    }

    args
}

/// Load a configuration file and parse it into commands.
pub fn load_config_file(path: &str) -> Result<Vec<Vec<String>>, std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_config_lines(&content))
}

/// Load a configuration file with context for conditional evaluation and variable expansion.
pub fn load_config_file_with_context(
    path: &str,
    ctx: &mut ConfigContext,
) -> Result<Vec<Vec<String>>, std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_config_with_context(&content, ctx))
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
        // Empty quoted strings are preserved as empty string arguments (tmux compat).
        let input = r#"set -g foo """#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", ""]);
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

    #[test]
    fn escaped_semicolon_not_split() {
        // \; in tmux means "next command bound to same key", not a command separator
        let input = r#"bind r source-file foo \; display "Reloaded!""#;
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["bind", "r", "source-file", "foo", ";", "display", "Reloaded!"]);
    }

    #[test]
    fn empty_single_quoted_string() {
        let input = "set -g foo ''";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", ""]);
    }

    // --- Line continuation tests ---

    #[test]
    fn line_continuation_basic() {
        let input = "set -g status-right \\\n\"hello world\"";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "status-right", "hello world"]);
    }

    #[test]
    fn line_continuation_multiple() {
        let input = "set -g \\\nstatus-right \\\n\"hello\"";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "status-right", "hello"]);
    }

    #[test]
    fn line_continuation_trailing_at_eof() {
        let input = "set -g foo\\";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        // Trailing backslash at EOF is just consumed
        assert_eq!(cmds[0], vec!["set", "-g", "foo"]);
    }

    #[test]
    fn double_backslash_not_continuation() {
        // \\\\ at end of line is an escaped backslash, not line continuation
        let input = "set -g foo bar\\\\";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
        // The \\\\ in the raw string is \\, which tokenize_command handles
    }

    // --- %if / %elif / %else / %endif tests ---

    #[test]
    fn if_true_includes_body() {
        let input = "%if 1\nset -g foo bar\n%endif\n";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "bar"]);
    }

    #[test]
    fn if_false_skips_body() {
        let input = "%if 0\nset -g foo bar\n%endif\n";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert!(cmds.is_empty());
    }

    #[test]
    fn if_false_else_includes() {
        let input = "%if 0\nset -g foo bar\n%else\nset -g baz qux\n%endif\n";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "baz", "qux"]);
    }

    #[test]
    fn if_true_else_skips() {
        let input = "%if 1\nset -g foo bar\n%else\nset -g baz qux\n%endif\n";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "bar"]);
    }

    #[test]
    fn elif_chain() {
        let input = "\
%if 0
set -g a 1
%elif 0
set -g b 2
%elif 1
set -g c 3
%else
set -g d 4
%endif
";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "c", "3"]);
    }

    #[test]
    fn elif_first_match_wins() {
        let input = "\
%if 0
set -g a 1
%elif 1
set -g b 2
%elif 1
set -g c 3
%endif
";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "b", "2"]);
    }

    #[test]
    fn nested_if() {
        let input = "\
%if 1
%if 1
set -g inner yes
%endif
set -g outer yes
%endif
";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], vec!["set", "-g", "inner", "yes"]);
        assert_eq!(cmds[1], vec!["set", "-g", "outer", "yes"]);
    }

    #[test]
    fn nested_if_outer_false() {
        let input = "\
%if 0
%if 1
set -g inner yes
%endif
set -g outer yes
%endif
";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert!(cmds.is_empty());
    }

    #[test]
    fn if_with_format_expansion() {
        // Simulate #{==:mocha,mocha} -> "1"
        let input = "%if \"#{==:mocha,mocha}\"\nset -g flavor mocha\n%endif\n";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(|s| {
            if s == "#{==:mocha,mocha}" { "1".to_string() } else { s.to_string() }
        });
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "flavor", "mocha"]);
    }

    #[test]
    fn if_with_format_expansion_false() {
        // Simulate #{==:mocha,latte} -> ""
        let input = "%if \"#{==:mocha,latte}\"\nset -g flavor wrong\n%endif\n";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(
            |s| {
                if s == "#{==:mocha,latte}" { String::new() } else { s.to_string() }
            },
        );
        let cmds = parse_config_with_context(input, &mut ctx);
        assert!(cmds.is_empty());
    }

    #[test]
    fn if_without_format_expander() {
        // Without a format expander, non-empty expressions are true
        let input = "%if anything\nset -g foo bar\n%endif\n";
        let cmds = parse_config_lines(input);
        assert_eq!(cmds.len(), 1);
    }

    // --- %hidden tests ---

    #[test]
    fn hidden_basic() {
        let input = "%hidden MODULE_NAME=\"session\"\nset -g @name ${MODULE_NAME}";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "@name", "session"]);
        assert_eq!(ctx.hidden_vars.get("MODULE_NAME").unwrap(), "session");
    }

    #[test]
    fn hidden_single_quoted() {
        // ${VAR} expands before tokenization, so spaces in values cause word splitting
        // (matching tmux). To preserve spaces, the config must quote the expansion.
        let input = "%hidden FOO='bar baz'\nset -g key \"${FOO}\"";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "key", "bar baz"]);
    }

    #[test]
    fn hidden_unquoted() {
        let input = "%hidden COLOR=blue\nset -g @color ${COLOR}";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "@color", "blue"]);
    }

    #[test]
    fn hidden_inside_false_if_not_set() {
        // ${FOO} expands to empty string since %hidden was inside false block.
        // Without quotes around ${FOO}, empty expansion disappears entirely.
        let input = "%if 0\n%hidden FOO=\"bar\"\n%endif\nset -g @v \"${FOO}\"";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "@v", ""]);
    }

    #[test]
    fn hidden_inside_true_if_set() {
        let input = "%if 1\n%hidden FOO=\"bar\"\n%endif\nset -g @v ${FOO}";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "@v", "bar"]);
    }

    // --- ${VAR} interpolation tests ---

    #[test]
    fn var_interpolation_in_option_name() {
        // Catppuccin pattern: @catppuccin_${MODULE_NAME}_color
        let input = "%hidden MODULE_NAME=\"session\"\nset -g @catppuccin_${MODULE_NAME}_color blue";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "@catppuccin_session_color", "blue"]);
    }

    #[test]
    fn var_interpolation_in_value() {
        let input = "%hidden PREFIX=\"hello\"\nset -g foo \"${PREFIX} world\"";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "hello world"]);
    }

    #[test]
    fn var_interpolation_multiple() {
        let input = "%hidden A=\"x\"\n%hidden B=\"y\"\nset -g foo ${A}-${B}";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "x-y"]);
    }

    #[test]
    fn var_interpolation_undefined_expands_empty() {
        // Without quotes, empty expansion disappears. With quotes, preserved as empty arg.
        let input = "set -g foo \"${UNDEFINED_VAR_12345}\"";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", ""]);
    }

    #[test]
    fn var_interpolation_env_fallback() {
        // Falls back to env vars
        // SAFETY: test-only, single-threaded access to this unique env var name
        unsafe { std::env::set_var("RMUX_TEST_VAR_P1", "from_env") };
        let input = "set -g foo ${RMUX_TEST_VAR_P1}";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "from_env"]);
        // SAFETY: test-only cleanup
        unsafe { std::env::remove_var("RMUX_TEST_VAR_P1") };
    }

    #[test]
    fn var_interpolation_hidden_takes_precedence() {
        // SAFETY: test-only, single-threaded access to this unique env var name
        unsafe { std::env::set_var("RMUX_TEST_VAR_P1B", "from_env") };
        let input = "%hidden RMUX_TEST_VAR_P1B=\"from_hidden\"\nset -g foo ${RMUX_TEST_VAR_P1B}";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "from_hidden"]);
        // SAFETY: test-only cleanup
        unsafe { std::env::remove_var("RMUX_TEST_VAR_P1B") };
    }

    #[test]
    fn var_in_hidden_value() {
        // ${VAR} inside a %hidden value should also expand
        let input = "%hidden A=\"hello\"\n%hidden B=\"${A} world\"\nset -g foo \"${B}\"";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "hello world"]);
    }

    // --- Catppuccin-style integration tests ---

    #[test]
    fn catppuccin_module_pattern() {
        // Simulates catppuccin's status module pattern
        let input = "\
%hidden MODULE_NAME=\"session\"
%hidden COLOR=\"blue\"
set -ogq @catppuccin_${MODULE_NAME}_color \"${COLOR}\"
set -ogq @catppuccin_${MODULE_NAME}_text \" #{${MODULE_NAME}_name}\"
";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], vec!["set", "-ogq", "@catppuccin_session_color", "blue"]);
        assert_eq!(cmds[1], vec!["set", "-ogq", "@catppuccin_session_text", " #{session_name}"]);
    }

    #[test]
    fn catppuccin_conditional_pattern() {
        // Simulates catppuccin's %if for feature detection
        let input = "\
%if \"#{>=:#{version},3.4}\"
set -g option_a on
%else
set -g option_a off
%endif
";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(|s| {
            // Simulate: version is 3.6, so #{>=:3.6,3.4} is "1"
            if s.contains("#{>=:") { "1".to_string() } else { s.to_string() }
        });
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "option_a", "on"]);
    }

    #[test]
    fn catppuccin_full_chain() {
        // Simulates the full catppuccin pattern with conditionals + hidden + var expansion
        let input = "\
%hidden MODULE_NAME=\"session\"
%if 1
set -ogq @catppuccin_${MODULE_NAME}_icon \"icon\"
%hidden MODULE_ICON=\"icon\"
%else
%hidden MODULE_ICON=\"default\"
%endif
set -ogq @catppuccin_${MODULE_NAME}_final_icon \"${MODULE_ICON}\"
";
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], vec!["set", "-ogq", "@catppuccin_session_icon", "icon"]);
        assert_eq!(cmds[1], vec!["set", "-ogq", "@catppuccin_session_final_icon", "icon"]);
    }

    // --- Line continuation combined with other features ---

    #[test]
    fn line_continuation_with_var_expansion() {
        let input = "%hidden X=\"hello\"\nset -g foo \\\n${X}";
        let mut ctx = ConfigContext::new();
        let cmds = parse_config_with_context(input, &mut ctx);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], vec!["set", "-g", "foo", "hello"]);
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
            fn parse_config_with_context_never_panics(content in "\\PC{0,200}") {
                let mut ctx = ConfigContext::new();
                ctx.set_format_expand(std::string::ToString::to_string);
                let _ = parse_config_with_context(&content, &mut ctx);
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

            #[test]
            fn endif_always_balances_if(
                cond in "(0|1|true|false)",
            ) {
                let input = format!("%if {cond}\nset -g foo bar\n%endif\n");
                let mut ctx = ConfigContext::new();
                ctx.set_format_expand(std::string::ToString::to_string);
                let result = parse_config_with_context(&input, &mut ctx);
                // Should be 1 command if condition is truthy, 0 if falsy
                let expected = usize::from(cond != "0");
                prop_assert_eq!(result.len(), expected);
            }

            #[test]
            fn hidden_vars_always_stored(
                name in "[A-Z][A-Z_]{0,10}",
                value in "[a-z]{1,20}",
            ) {
                let input = format!("%hidden {name}=\"{value}\"");
                let mut ctx = ConfigContext::new();
                let _ = parse_config_with_context(&input, &mut ctx);
                prop_assert_eq!(ctx.hidden_vars.get(&name).unwrap(), &value);
            }

            #[test]
            fn var_interpolation_roundtrips(
                name in "[A-Z][A-Z_]{0,10}",
                value in "[a-z]{1,20}",
            ) {
                let input = format!("%hidden {name}=\"{value}\"\nset -g key ${{{name}}}");
                let mut ctx = ConfigContext::new();
                let result = parse_config_with_context(&input, &mut ctx);
                prop_assert_eq!(result.len(), 1);
                prop_assert_eq!(&result[0][3], &value);
            }
        }
    }
}
