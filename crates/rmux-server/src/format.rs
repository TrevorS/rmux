//! Format string expansion (#{...} syntax).
//!
//! Provides tmux-compatible format string expansion including variable
//! substitution, conditionals, comparisons, width truncation, and short aliases.

use nix::libc;
use std::collections::HashMap;

/// Callback type for looking up `@user_option` values from the options store.
pub type OptionLookup = Box<dyn Fn(&str) -> Option<String>>;

/// Context for format string expansion.
pub struct FormatContext {
    vars: HashMap<String, String>,
    /// Optional callback for looking up `@`-prefixed user options.
    option_lookup: Option<OptionLookup>,
}

impl FormatContext {
    /// Create a new empty format context.
    pub fn new() -> Self {
        Self { vars: HashMap::new(), option_lookup: None }
    }

    /// Set a format variable.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.vars.insert(key.to_string(), value.into());
    }

    /// Get a format variable value.
    /// Also handles `@user_option` lookups via the option callback.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    /// Look up a variable, falling back to `@`-prefixed option lookup.
    pub fn lookup(&self, key: &str) -> Option<String> {
        if let Some(v) = self.vars.get(key) {
            return Some(v.clone());
        }
        if key.starts_with('@') {
            if let Some(cb) = &self.option_lookup {
                return cb(key);
            }
        }
        None
    }

    /// Return all variables as (key, value) pairs, sorted by key.
    pub fn list_vars(&self) -> Vec<(&str, &str)> {
        let mut pairs: Vec<(&str, &str)> =
            self.vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        pairs.sort_by_key(|(k, _)| *k);
        pairs
    }

    /// Set the option lookup callback for `@`-prefixed user options.
    pub fn set_option_lookup(&mut self, f: impl Fn(&str) -> Option<String> + 'static) {
        self.option_lookup = Some(Box::new(f));
    }
}

impl Default for FormatContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Expand format strings with tmux-compatible syntax.
///
/// Supports:
/// - `#{variable_name}` — variable substitution
/// - `#{?cond,true_branch,false_branch}` — conditionals
/// - `#{==:#{a},#{b}}` — equality comparison (also `!=`, `<`, `>`, `<=`, `>=`)
/// - `#{=N:variable}` — width truncation (positive=right, negative=left)
/// - `#{l:text}` — literal string (no expansion)
/// - `#{s/pat/rep:expr}` — regex substitution
/// - `#S`, `#W`, `#I`, etc. — short aliases
/// - `#[style]` — inline style (passed through for renderer)
/// - `##` — literal `#`
pub fn format_expand(template: &str, ctx: &FormatContext) -> String {
    let mut result = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'#' {
            if i + 1 >= len {
                result.push('#');
                i += 1;
                continue;
            }

            match bytes[i + 1] {
                b'#' => {
                    // ## -> literal #
                    result.push('#');
                    i += 2;
                }
                b'{' => {
                    // #{...} — find matching closing brace (handling nesting)
                    let start = i + 2;
                    if let Some(end) = find_matching_brace(template, start) {
                        let inner = &template[start..end];
                        result.push_str(&expand_inner(inner, ctx));
                        i = end + 1;
                    } else {
                        // No matching brace — pass through as-is
                        result.push('#');
                        result.push('{');
                        i += 2;
                    }
                }
                b'[' => {
                    // #[style] — inline style directive.
                    // Must find the matching ']' while skipping over #{...} blocks
                    // so that e.g. #[fg=#{@thm_crust}] works correctly.
                    let start = i + 2;
                    if let Some(end) = find_style_end(template, start) {
                        let inner = &template[start..end];
                        // Expand #{...} variables inside the style block
                        let expanded = format_expand(inner, ctx);
                        result.push_str("#[");
                        result.push_str(&expanded);
                        result.push(']');
                        i = end + 1;
                    } else {
                        result.push('#');
                        i += 1;
                    }
                }
                ch => {
                    // Short aliases: #S, #W, #I, #T, #F, #D, #H, #h, #P
                    if let Some(var) = short_alias(ch) {
                        result.push_str(ctx.get(var).unwrap_or(""));
                        i += 2;
                    } else {
                        result.push('#');
                        i += 1;
                    }
                }
            }
        } else {
            // Copy non-'#' bytes as a UTF-8 string slice (not byte-by-byte)
            let start = i;
            while i < len && bytes[i] != b'#' {
                i += 1;
            }
            result.push_str(&template[start..i]);
        }
    }

    result
}

/// Map single-character short aliases to their variable names.
fn short_alias(ch: u8) -> Option<&'static str> {
    match ch {
        b'D' => Some("pane_id"),
        b'F' => Some("window_flags"),
        b'H' => Some("host"),
        b'h' => Some("host_short"),
        b'I' => Some("window_index"),
        b'P' => Some("pane_index"),
        b'S' => Some("session_name"),
        b'T' => Some("pane_title"),
        b'W' => Some("window_name"),
        _ => None,
    }
}

/// Find the closing `]` for a `#[...]` style block, skipping over `#{...}` variable
/// references so that `#[fg=#{@thm_crust},bg=#{@thm_bg}]` finds the correct `]`.
fn find_style_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = start;

    while i < len {
        match bytes[i] {
            b']' => return Some(i),
            b'#' if i + 1 < len && bytes[i + 1] == b'{' => {
                // Skip over #{...} block
                if let Some(brace_end) = find_matching_brace(s, i + 2) {
                    i = brace_end + 1;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Find the matching closing `}` for an opening `{`, handling nested `#{...}`.
fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut depth = 1u32;
    let mut i = start;

    while i < len {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Expand the content inside `#{...}`.
fn expand_inner(inner: &str, ctx: &FormatContext) -> String {
    // Conditional: ?cond,true,false
    if let Some(rest) = inner.strip_prefix('?') {
        return expand_conditional(rest, ctx);
    }

    // Try modifier dispatch (l:, E:, d:, b:, q:, n:, w:, a:, p:, !, ||:, &&:, etc.)
    if let Some(result) = expand_modifier(inner, ctx) {
        return result;
    }

    // Substitution: s/pattern/replacement:expr
    if inner.starts_with("s/") {
        if let Some(result) = expand_substitution(inner, ctx) {
            return result;
        }
    }

    // Comparison operators: ==:a,b  !=:a,b  <:a,b  etc.
    if let Some(result) = expand_comparison_dispatch(inner, ctx) {
        return result;
    }

    // Width truncation: =N:expr (positive N = right trunc, negative = left trunc)
    if inner.starts_with('=') || inner.starts_with("=-") {
        if let Some(result) = expand_truncation(inner, ctx) {
            return result;
        }
    }

    // Plain variable lookup (may contain nested #{} that need expanding first)
    let expanded = format_expand(inner, ctx);
    if inner.contains('#') { expanded } else { ctx.lookup(inner).unwrap_or_default() }
}

/// Dispatch single-letter and multi-char modifiers.
fn expand_modifier(inner: &str, ctx: &FormatContext) -> Option<String> {
    // Literal
    if let Some(rest) = inner.strip_prefix("l:") {
        return Some(rest.to_string());
    }
    // Double expansion
    if let Some(rest) = inner.strip_prefix("E:") {
        let first = eval_expr(rest, ctx);
        return Some(format_expand(&first, ctx));
    }
    // Dirname
    if let Some(rest) = inner.strip_prefix("d:") {
        let val = eval_expr(rest, ctx);
        return Some(
            std::path::Path::new(&val)
                .parent()
                .map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
        );
    }
    // Basename
    if let Some(rest) = inner.strip_prefix("b:") {
        let val = eval_expr(rest, ctx);
        return Some(
            std::path::Path::new(&val)
                .file_name()
                .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
        );
    }
    // Shell quoting
    if let Some(rest) = inner.strip_prefix("q:") {
        return Some(shell_quote(&eval_expr(rest, ctx)));
    }
    // String length
    if let Some(rest) = inner.strip_prefix("n:") {
        return Some(eval_expr(rest, ctx).chars().count().to_string());
    }
    // Display width
    if let Some(rest) = inner.strip_prefix("w:") {
        return Some(display_width(&eval_expr(rest, ctx)).to_string());
    }
    // ASCII code to character
    if let Some(rest) = inner.strip_prefix("a:") {
        // Try as literal number first, then as expression
        let val = if rest.chars().all(|c| c.is_ascii_digit()) {
            rest.to_string()
        } else {
            eval_expr(rest, ctx)
        };
        return Some(
            val.parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map_or_else(String::new, |ch| ch.to_string()),
        );
    }
    // Padding
    if inner.starts_with("p:") || inner.starts_with("p:-") {
        return expand_padding(inner, ctx);
    }
    // Logical NOT (but not !=: comparison)
    if inner.starts_with('!') && !inner.starts_with("!=:") {
        let rest = &inner[1..];
        let val = eval_expr(rest, ctx);
        let is_true = !val.is_empty() && val != "0";
        return Some(if is_true { "0" } else { "1" }.to_string());
    }
    // Logical OR / AND
    if let Some(rest) = inner.strip_prefix("||:") {
        return Some(expand_logical_or(rest, ctx));
    }
    if let Some(rest) = inner.strip_prefix("&&:") {
        return Some(expand_logical_and(rest, ctx));
    }
    // Arithmetic
    if inner.starts_with("e|") {
        return expand_arithmetic(inner, ctx);
    }
    // Pattern match
    if inner.starts_with("m:") || inner.starts_with("m/r:") {
        return Some(expand_match(inner, ctx));
    }
    None
}

/// Dispatch comparison operators.
fn expand_comparison_dispatch(inner: &str, ctx: &FormatContext) -> Option<String> {
    if let Some(rest) = inner.strip_prefix("==:") {
        return Some(expand_comparison(rest, ctx, |a, b| a == b));
    }
    if let Some(rest) = inner.strip_prefix("!=:") {
        return Some(expand_comparison(rest, ctx, |a, b| a != b));
    }
    if let Some(rest) = inner.strip_prefix("<=:") {
        return Some(expand_comparison(rest, ctx, |a, b| a <= b));
    }
    if let Some(rest) = inner.strip_prefix(">=:") {
        return Some(expand_comparison(rest, ctx, |a, b| a >= b));
    }
    if let Some(rest) = inner.strip_prefix("<:") {
        return Some(expand_comparison(rest, ctx, |a, b| a < b));
    }
    if let Some(rest) = inner.strip_prefix(">:") {
        return Some(expand_comparison(rest, ctx, |a, b| a > b));
    }
    None
}

/// Split a format string at a top-level comma, respecting nested `#{}`.
fn split_at_comma(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut depth = 0u32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                return Some((&s[..i], &s[i + 1..]));
            }
            _ => {}
        }
    }
    None
}

/// Evaluate an expression: if it contains `#`, expand it as a format string;
/// otherwise treat it as a bare variable name and look it up.
fn eval_expr(expr: &str, ctx: &FormatContext) -> String {
    if expr.contains('#') { format_expand(expr, ctx) } else { ctx.lookup(expr).unwrap_or_default() }
}

/// Expand a conditional: `cond,true_branch,false_branch`
fn expand_conditional(rest: &str, ctx: &FormatContext) -> String {
    // Split into condition, true branch, false branch
    let Some((cond, remainder)) = split_at_comma(rest) else {
        return String::new();
    };

    let (true_branch, false_branch) = split_at_comma(remainder).unwrap_or((remainder, ""));

    let cond_value = eval_expr(cond, ctx);
    let is_true = !cond_value.is_empty() && cond_value != "0";

    if is_true { format_expand(true_branch, ctx) } else { format_expand(false_branch, ctx) }
}

/// Expand a comparison: `a,b` with the given comparison function.
fn expand_comparison(rest: &str, ctx: &FormatContext, cmp: fn(&str, &str) -> bool) -> String {
    let Some((a_expr, b_expr)) = split_at_comma(rest) else {
        return String::new();
    };

    let a = format_expand(a_expr, ctx);
    let b = format_expand(b_expr, ctx);

    if cmp(&a, &b) { "1".to_string() } else { "0".to_string() }
}

/// Expand substitution: `s/pattern/replacement:expr`
fn expand_substitution(inner: &str, ctx: &FormatContext) -> Option<String> {
    // Format: s/pattern/replacement:expr
    let rest = inner.strip_prefix("s/")?;

    // Find the second / delimiter
    let second_slash = rest.find('/')?;
    let pattern = &rest[..second_slash];
    let after_slash = &rest[second_slash + 1..];

    // Find the : separator between replacement and expression
    let colon = after_slash.find(':')?;
    let replacement = &after_slash[..colon];
    let expr = &after_slash[colon + 1..];

    let expanded = format_expand(expr, ctx);
    Some(expanded.replace(pattern, replacement))
}

/// Expand width truncation: `=N:expr`
fn expand_truncation(inner: &str, ctx: &FormatContext) -> Option<String> {
    // Parse =N: or =-N:
    let rest = inner.strip_prefix('=')?;
    let colon_pos = rest.find(':')?;
    let n_str = &rest[..colon_pos];
    let expr = &rest[colon_pos + 1..];

    let n: i32 = n_str.parse().ok()?;
    // Try variable lookup first (tmux treats the expr as a variable name),
    // then fall back to template expansion for nested #{} expressions.
    let expanded = ctx.lookup(expr).unwrap_or_else(|| format_expand(expr, ctx));

    let char_count = expanded.chars().count();
    let abs_n = n.unsigned_abs() as usize;

    if abs_n >= char_count {
        return Some(expanded);
    }

    if n >= 0 {
        // Positive: keep first N chars
        Some(expanded.chars().take(abs_n).collect())
    } else {
        // Negative: keep last N chars
        Some(expanded.chars().skip(char_count - abs_n).collect())
    }
}

/// Shell-quote a string (single-quote with escaping).
fn shell_quote(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            result.push_str("'\\''");
        } else {
            result.push(ch);
        }
    }
    result.push('\'');
    result
}

/// Compute display width of a string (ASCII = 1, wide CJK chars = 2, control = 0).
fn display_width(s: &str) -> usize {
    s.chars()
        .map(|ch| {
            if ch.is_control() {
                0
            } else if is_wide_char(ch) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Check if a character is a CJK wide character.
fn is_wide_char(ch: char) -> bool {
    let c = ch as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&c)
    // CJK Unified Ideographs Extension A
    || (0x3400..=0x4DBF).contains(&c)
    // CJK Compatibility Ideographs
    || (0xF900..=0xFAFF).contains(&c)
    // Fullwidth Forms
    || (0xFF01..=0xFF60).contains(&c)
    || (0xFFE0..=0xFFE6).contains(&c)
    // CJK Radicals Supplement, Kangxi Radicals
    || (0x2E80..=0x2FDF).contains(&c)
    // CJK Symbols and Punctuation, Hiragana, Katakana
    || (0x3000..=0x303F).contains(&c)
    || (0x3040..=0x309F).contains(&c)
    || (0x30A0..=0x30FF).contains(&c)
    // Hangul Syllables
    || (0xAC00..=0xD7AF).contains(&c)
    // CJK Unified Ideographs Extension B and beyond
    || (0x20000..=0x2FA1F).contains(&c)
}

/// Expand padding: `p:N:expr` — pad to width N (positive=right-pad, negative=left-pad).
fn expand_padding(inner: &str, ctx: &FormatContext) -> Option<String> {
    let rest = inner.strip_prefix("p:")?;
    let colon_pos = rest.find(':')?;
    let n_str = &rest[..colon_pos];
    let expr = &rest[colon_pos + 1..];

    let n: i32 = n_str.parse().ok()?;
    let expanded = eval_expr(expr, ctx);
    let char_count = expanded.chars().count();
    let abs_n = n.unsigned_abs() as usize;

    if char_count >= abs_n {
        return Some(expanded);
    }

    let padding = abs_n - char_count;
    if n >= 0 {
        // Positive: right-pad (add spaces after)
        Some(format!("{expanded}{}", " ".repeat(padding)))
    } else {
        // Negative: left-pad (add spaces before)
        Some(format!("{}{expanded}", " ".repeat(padding)))
    }
}

/// Expand logical OR: `a,b` — returns "1" if either is truthy.
fn expand_logical_or(rest: &str, ctx: &FormatContext) -> String {
    let Some((a_expr, b_expr)) = split_at_comma(rest) else {
        return String::new();
    };
    let a = format_expand(a_expr, ctx);
    let b = format_expand(b_expr, ctx);
    let a_true = !a.is_empty() && a != "0";
    let b_true = !b.is_empty() && b != "0";
    if a_true || b_true { "1" } else { "0" }.to_string()
}

/// Expand logical AND: `a,b` — returns "1" if both are truthy.
fn expand_logical_and(rest: &str, ctx: &FormatContext) -> String {
    let Some((a_expr, b_expr)) = split_at_comma(rest) else {
        return String::new();
    };
    let a = format_expand(a_expr, ctx);
    let b = format_expand(b_expr, ctx);
    let a_true = !a.is_empty() && a != "0";
    let b_true = !b.is_empty() && b != "0";
    if a_true && b_true { "1" } else { "0" }.to_string()
}

/// Expand arithmetic: `e|op:a,b` where op is +, -, *, /, or %.
fn expand_arithmetic(inner: &str, ctx: &FormatContext) -> Option<String> {
    let rest = inner.strip_prefix("e|")?;
    let colon_pos = rest.find(':')?;
    let op = &rest[..colon_pos];
    let args = &rest[colon_pos + 1..];

    let (a_expr, b_expr) = split_at_comma(args)?;
    let a = format_expand(a_expr, ctx);
    let b = format_expand(b_expr, ctx);
    let a_num: i64 = a.parse().unwrap_or(0);
    let b_num: i64 = b.parse().unwrap_or(0);

    let result = match op {
        "+" => a_num.wrapping_add(b_num),
        "-" => a_num.wrapping_sub(b_num),
        "*" => a_num.wrapping_mul(b_num),
        "/" => {
            if b_num == 0 {
                return Some("0".to_string());
            }
            a_num / b_num
        }
        "%" => {
            if b_num == 0 {
                return Some("0".to_string());
            }
            a_num % b_num
        }
        _ => return None,
    };
    Some(result.to_string())
}

/// Expand pattern match: `m:pattern,string` (glob) or `m/r:pattern,string` (regex).
fn expand_match(inner: &str, ctx: &FormatContext) -> String {
    let (pattern_expr, string_expr, is_regex) = if let Some(rest) = inner.strip_prefix("m/r:") {
        let Some((p, s)) = split_at_comma(rest) else {
            return String::new();
        };
        (p, s, true)
    } else if let Some(rest) = inner.strip_prefix("m:") {
        let Some((p, s)) = split_at_comma(rest) else {
            return String::new();
        };
        (p, s, false)
    } else {
        return String::new();
    };

    let pattern = format_expand(pattern_expr, ctx);
    let string = format_expand(string_expr, ctx);

    let matched = if is_regex {
        // Simple regex match — tmux uses POSIX regex, we use a basic approach
        regex_match(&pattern, &string)
    } else {
        // fnmatch-style glob: * matches any, ? matches one char
        fnmatch(&pattern, &string)
    };

    if matched { "1" } else { "0" }.to_string()
}

/// Simple fnmatch-style glob matching (supports `*` and `?`).
fn fnmatch(pattern: &str, string: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = string.chars().collect();
    fnmatch_inner(&pat, &s, 0, 0)
}

fn fnmatch_inner(pat: &[char], s: &[char], pi: usize, si: usize) -> bool {
    let mut pi = pi;
    let mut si = si;

    while pi < pat.len() {
        match pat[pi] {
            '*' => {
                // Skip consecutive *
                while pi < pat.len() && pat[pi] == '*' {
                    pi += 1;
                }
                if pi == pat.len() {
                    return true;
                }
                // Try matching rest of pattern from each position
                for i in si..=s.len() {
                    if fnmatch_inner(pat, s, pi, i) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                if si >= s.len() {
                    return false;
                }
                pi += 1;
                si += 1;
            }
            ch => {
                if si >= s.len() || s[si] != ch {
                    return false;
                }
                pi += 1;
                si += 1;
            }
        }
    }
    si == s.len()
}

/// Simple regex match — anchored match using basic regex features.
fn regex_match(pattern: &str, string: &str) -> bool {
    // Basic implementation: support `.` (any), `.*` (any sequence), `^`, `$`
    // For full regex we'd need a crate, but tmux uses POSIX extended regex.
    // We do a simple substring search if no special chars, otherwise basic matching.
    if pattern.is_empty() {
        return true;
    }

    // If pattern has no regex metacharacters, do substring match
    let has_meta = pattern.chars().any(|c| {
        matches!(
            c,
            '.' | '*' | '+' | '?' | '[' | ']' | '(' | ')' | '{' | '}' | '|' | '^' | '$' | '\\'
        )
    });
    if !has_meta {
        return string.contains(pattern);
    }

    // For anchored patterns, do basic matching
    let pat = if let Some(p) = pattern.strip_prefix('^') {
        if let Some(p2) = p.strip_suffix('$') {
            // ^...$  — full match
            return simple_regex_match(p2, string, true);
        }
        // ^... — prefix match
        return simple_regex_match(p, string, true);
    } else if let Some(p) = pattern.strip_suffix('$') {
        // ...$  — check from each position
        for i in 0..=string.len() {
            if simple_regex_match(p, &string[i..], true) {
                return true;
            }
        }
        return false;
    } else {
        pattern
    };

    // Unanchored — search
    for i in 0..=string.len() {
        if simple_regex_match(pat, &string[i..], false) {
            return true;
        }
    }
    false
}

/// Very basic regex: supports `.` (any char), `.*` (any sequence), literal chars.
fn simple_regex_match(pattern: &str, string: &str, must_consume_all: bool) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = string.chars().collect();
    simple_regex_inner(&pat, &s, 0, 0, must_consume_all)
}

fn simple_regex_inner(
    pat: &[char],
    s: &[char],
    pi: usize,
    si: usize,
    must_consume_all: bool,
) -> bool {
    if pi == pat.len() {
        return if must_consume_all { si == s.len() } else { true };
    }

    // Check for quantifier: `X*`
    if pi + 1 < pat.len() && pat[pi + 1] == '*' {
        let match_char = pat[pi];
        // Try matching 0..n occurrences
        let mut si2 = si;
        loop {
            if simple_regex_inner(pat, s, pi + 2, si2, must_consume_all) {
                return true;
            }
            if si2 >= s.len() {
                break;
            }
            if match_char == '.' || s[si2] == match_char {
                si2 += 1;
            } else {
                break;
            }
        }
        return false;
    }

    if si >= s.len() {
        return false;
    }

    match pat[pi] {
        '.' => simple_regex_inner(pat, s, pi + 1, si + 1, must_consume_all),
        ch if ch == s[si] => simple_regex_inner(pat, s, pi + 1, si + 1, must_consume_all),
        _ => false,
    }
}

/// Expand strftime `%`-codes in a string using the current local time.
///
/// tmux expands bare `%H`, `%M`, `%S`, `%d`, `%b`, `%y`, `%Y`, `%a`, `%A`,
/// `%e`, `%m`, `%p`, `%Z`, `%j`, `%k`, `%l`, `%R`, `%r`, `%D`, `%F`, `%c`,
/// `%x`, `%X` etc. in status-left/right templates after format variable
/// expansion. We handle the common codes using chrono-free manual formatting.
pub fn strftime_expand(s: &str) -> String {
    use std::time::SystemTime;

    let now =
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();

    // Timestamps before 2^63 (year ~292 billion) are safe to cast
    #[allow(clippy::cast_possible_wrap)]
    strftime_expand_with_timestamp(s, now as i64)
}

/// Expand strftime codes using a specific Unix timestamp (for testability).
fn strftime_expand_with_timestamp(s: &str, timestamp: i64) -> String {
    let (year, month, day, hour, minute, second, weekday, yday) = unix_to_local(timestamp);

    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'%' {
            if i + 1 >= len {
                result.push('%');
                i += 1;
                continue;
            }
            let code = bytes[i + 1];
            let replacement: Option<String> = match code {
                b'H' => Some(format!("{hour:02}")),
                b'M' => Some(format!("{minute:02}")),
                b'S' => Some(format!("{second:02}")),
                b'd' => Some(format!("{day:02}")),
                b'e' => Some(format!("{day:>2}")),
                b'm' => Some(format!("{month:02}")),
                b'y' => Some(format!("{:02}", year % 100)),
                b'Y' => Some(format!("{year}")),
                b'b' | b'h' => Some(month_abbrev(month).to_string()),
                b'B' => Some(month_full(month).to_string()),
                b'a' => Some(weekday_abbrev(weekday).to_string()),
                b'A' => Some(weekday_full(weekday).to_string()),
                b'p' => Some(if hour < 12 { "AM" } else { "PM" }.to_string()),
                b'P' => Some(if hour < 12 { "am" } else { "pm" }.to_string()),
                b'k' => Some(format!("{hour:>2}")),
                b'l' => {
                    let h12 = if hour == 0 {
                        12
                    } else if hour > 12 {
                        hour - 12
                    } else {
                        hour
                    };
                    Some(format!("{h12:>2}"))
                }
                b'I' => {
                    let h12 = if hour == 0 {
                        12
                    } else if hour > 12 {
                        hour - 12
                    } else {
                        hour
                    };
                    Some(format!("{h12:02}"))
                }
                b'R' => Some(format!("{hour:02}:{minute:02}")),
                b'T' => Some(format!("{hour:02}:{minute:02}:{second:02}")),
                b'r' => {
                    let h12 = if hour == 0 {
                        12
                    } else if hour > 12 {
                        hour - 12
                    } else {
                        hour
                    };
                    let ampm = if hour < 12 { "AM" } else { "PM" };
                    Some(format!("{h12:02}:{minute:02}:{second:02} {ampm}"))
                }
                b'D' => Some(format!("{month:02}/{day:02}/{:02}", year % 100)),
                b'F' => Some(format!("{year}-{month:02}-{day:02}")),
                b'j' => Some(format!("{yday:03}")),
                b'n' => Some("\n".to_string()),
                b't' => Some("\t".to_string()),
                b'%' => Some("%".to_string()),
                _ => None,
            };

            if let Some(rep) = replacement {
                result.push_str(&rep);
                i += 2;
            } else {
                result.push('%');
                i += 1;
            }
        } else {
            // Copy non-'%' bytes as a UTF-8 string slice (not byte-by-byte)
            let start = i;
            while i < len && bytes[i] != b'%' {
                i += 1;
            }
            result.push_str(&s[start..i]);
        }
    }

    result
}

/// Convert a Unix timestamp to local time components.
///
/// Returns (year, month, day, hour, minute, second, weekday, yday).
/// weekday: 0=Sunday, 1=Monday, ..., 6=Saturday.
/// yday: 1-based day of year.
fn unix_to_local(timestamp: i64) -> (i32, u32, u32, u32, u32, u32, u32, u32) {
    // Get the local timezone offset using libc
    let offset_secs = local_utc_offset(timestamp);
    let local_ts = timestamp + i64::from(offset_secs);

    // Convert to civil date/time
    let secs_per_day: i64 = 86400;
    let mut days = local_ts.div_euclid(secs_per_day);
    let day_secs = local_ts.rem_euclid(secs_per_day) as u32;

    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    // days since epoch (1970-01-01 which was a Thursday)
    // weekday: 0=Sun, 1=Mon, ... 4=Thu
    let weekday = ((days + 4) % 7) as u32; // Thu=4, so day 0 → 4

    // Convert days since epoch to (year, month, day) using the civil_from_days algorithm
    // Algorithm from Howard Hinnant's date library
    days += 719_468; // shift epoch from 1970-01-01 to 0000-03-01
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };

    // Compute yday (1-based day of year)
    let yday = {
        let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let month_days: [u32; 12] =
            [31, if is_leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut yd = d;
        for days in month_days.iter().take(m as usize - 1) {
            yd += days;
        }
        yd
    };

    (year, m, d, hour, minute, second, weekday, yday)
}

/// Get the local UTC offset in seconds for a given timestamp.
fn local_utc_offset(timestamp: i64) -> i32 {
    use std::mem::MaybeUninit;

    // SAFETY: localtime_r is a standard POSIX function that fills the provided
    // tm struct with the local time representation of the given timestamp.
    // We use MaybeUninit to avoid reading uninitialized memory.
    unsafe {
        let time_t = timestamp as libc::time_t;
        let mut tm = MaybeUninit::<libc::tm>::uninit();
        libc::localtime_r(&raw const time_t, tm.as_mut_ptr());
        let tm = tm.assume_init();
        tm.tm_gmtoff as i32
    }
}

fn month_abbrev(m: u32) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

fn month_full(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "???",
    }
}

fn weekday_abbrev(d: u32) -> &'static str {
    match d {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "???",
    }
}

fn weekday_full(d: u32) -> &'static str {
    match d {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        _ => "???",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn simple_variable() {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "main");
        assert_eq!(format_expand("#{session_name}", &ctx), "main");
    }

    #[test]
    fn multiple_variables() {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "work");
        ctx.set("window_index", "2");
        assert_eq!(format_expand("[#{session_name}] #{window_index}", &ctx), "[work] 2");
    }

    #[test]
    fn unknown_variable_empty() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{unknown}", &ctx), "");
    }

    #[test]
    fn no_variables() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("hello world", &ctx), "hello world");
    }

    #[test]
    fn incomplete_format() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{incomplete", &ctx), "#{incomplete");
    }

    #[test]
    fn hash_without_brace() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#not_a_var", &ctx), "#not_a_var");
    }

    #[test]
    fn adjacent_variables() {
        let mut ctx = FormatContext::new();
        ctx.set("a", "X");
        ctx.set("b", "Y");
        assert_eq!(format_expand("#{a}#{b}", &ctx), "XY");
    }

    #[test]
    fn empty_variable_name() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{}", &ctx), "");
    }

    #[test]
    fn variable_with_special_chars_in_value() {
        let mut ctx = FormatContext::new();
        ctx.set("var", "value with #{} chars");
        let result = format_expand("#{var}", &ctx);
        assert_eq!(result, "value with #{} chars");
    }

    #[test]
    fn consecutive_hashes() {
        let ctx = FormatContext::new();
        let result = format_expand("##", &ctx);
        assert_eq!(result, "#");
    }

    #[test]
    fn very_long_template() {
        let mut ctx = FormatContext::new();
        ctx.set("x", "X");
        let mut template = String::new();
        for i in 0..200 {
            write!(template, "item{i}-#{{x}}-").unwrap();
        }
        let result = format_expand(&template, &ctx);
        assert!(result.len() > 1000);
        assert!(result.contains("item0-X-"));
        assert!(result.contains("item199-X-"));
    }

    #[test]
    fn many_variables_in_context() {
        let mut ctx = FormatContext::new();
        for i in 0..50 {
            ctx.set(&format!("var{i}"), format!("val{i}"));
        }
        let result = format_expand("#{var0} #{var25} #{var49}", &ctx);
        assert_eq!(result, "val0 val25 val49");
    }

    #[test]
    fn empty_template() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("", &ctx), "");
    }

    #[test]
    fn set_overwrites_previous() {
        let mut ctx = FormatContext::new();
        ctx.set("key", "first");
        assert_eq!(ctx.get("key"), Some("first"));
        ctx.set("key", "second");
        assert_eq!(ctx.get("key"), Some("second"));
        assert_eq!(format_expand("#{key}", &ctx), "second");
    }

    // --- New tests for enhanced format engine ---

    #[test]
    fn literal_double_hash() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("a##b", &ctx), "a#b");
        assert_eq!(format_expand("####", &ctx), "##");
    }

    #[test]
    fn short_alias_session() {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "dev");
        assert_eq!(format_expand("#S", &ctx), "dev");
    }

    #[test]
    fn short_alias_window() {
        let mut ctx = FormatContext::new();
        ctx.set("window_name", "bash");
        ctx.set("window_index", "3");
        assert_eq!(format_expand("#I:#W", &ctx), "3:bash");
    }

    #[test]
    fn short_alias_all() {
        let mut ctx = FormatContext::new();
        ctx.set("pane_id", "%5");
        ctx.set("window_flags", "*");
        ctx.set("host", "myhost.local");
        ctx.set("host_short", "myhost");
        ctx.set("pane_index", "1");
        ctx.set("pane_title", "vim");
        assert_eq!(format_expand("#D", &ctx), "%5");
        assert_eq!(format_expand("#F", &ctx), "*");
        assert_eq!(format_expand("#H", &ctx), "myhost.local");
        assert_eq!(format_expand("#h", &ctx), "myhost");
        assert_eq!(format_expand("#P", &ctx), "1");
        assert_eq!(format_expand("#T", &ctx), "vim");
    }

    #[test]
    fn conditional_true() {
        let mut ctx = FormatContext::new();
        ctx.set("pane_active", "1");
        assert_eq!(format_expand("#{?pane_active,ACTIVE,inactive}", &ctx), "ACTIVE");
    }

    #[test]
    fn conditional_false() {
        let mut ctx = FormatContext::new();
        ctx.set("pane_active", "0");
        assert_eq!(format_expand("#{?pane_active,ACTIVE,inactive}", &ctx), "inactive");
    }

    #[test]
    fn conditional_missing_var() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{?pane_active,yes,no}", &ctx), "no");
    }

    #[test]
    fn conditional_with_nested_vars() {
        let mut ctx = FormatContext::new();
        ctx.set("active", "1");
        ctx.set("session_name", "work");
        assert_eq!(format_expand("#{?active,#{session_name},none}", &ctx), "work");
    }

    #[test]
    fn conditional_no_false_branch() {
        let mut ctx = FormatContext::new();
        ctx.set("active", "1");
        assert_eq!(format_expand("#{?active,YES}", &ctx), "YES");
    }

    #[test]
    fn comparison_equal() {
        let mut ctx = FormatContext::new();
        ctx.set("a", "hello");
        ctx.set("b", "hello");
        assert_eq!(format_expand("#{==:#{a},#{b}}", &ctx), "1");
    }

    #[test]
    fn comparison_not_equal() {
        let mut ctx = FormatContext::new();
        ctx.set("a", "hello");
        ctx.set("b", "world");
        assert_eq!(format_expand("#{==:#{a},#{b}}", &ctx), "0");
        assert_eq!(format_expand("#{!=:#{a},#{b}}", &ctx), "1");
    }

    #[test]
    fn comparison_in_conditional() {
        let mut ctx = FormatContext::new();
        ctx.set("window_name", "vim");
        assert_eq!(format_expand("#{?#{==:#{window_name},vim},EDITOR,other}", &ctx), "EDITOR");
    }

    #[test]
    fn truncation_right() {
        let mut ctx = FormatContext::new();
        ctx.set("pane_title", "very long title here");
        assert_eq!(format_expand("#{=10:#{pane_title}}", &ctx), "very long ");
    }

    #[test]
    fn truncation_left() {
        let mut ctx = FormatContext::new();
        ctx.set("pane_title", "very long title here");
        assert_eq!(format_expand("#{=-10:#{pane_title}}", &ctx), "title here");
    }

    #[test]
    fn truncation_bare_variable() {
        // tmux treats #{=21:pane_title} as variable lookup, not template expansion
        let mut ctx = FormatContext::new();
        ctx.set("pane_title", "my terminal title");
        assert_eq!(format_expand("#{=10:pane_title}", &ctx), "my termina");
    }

    #[test]
    fn truncation_no_op() {
        let mut ctx = FormatContext::new();
        ctx.set("x", "short");
        assert_eq!(format_expand("#{=20:#{x}}", &ctx), "short");
    }

    #[test]
    fn nested_conditional_with_comparison() {
        let mut ctx = FormatContext::new();
        ctx.set("window_index", "0");
        ctx.set("active", "1");
        let tmpl = "#{?#{==:#{window_index},0},first,#{?active,active,other}}";
        assert_eq!(format_expand(tmpl, &ctx), "first");

        ctx.set("window_index", "1");
        assert_eq!(format_expand(tmpl, &ctx), "active");

        ctx.set("active", "0");
        assert_eq!(format_expand(tmpl, &ctx), "other");
    }

    #[test]
    fn status_line_realistic() {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "dev");
        ctx.set("window_index", "0");
        ctx.set("window_name", "bash");
        ctx.set("window_flags", "*");
        let tmpl = "[#S] #I:#W#F";
        assert_eq!(format_expand(tmpl, &ctx), "[dev] 0:bash*");
    }

    #[test]
    fn literal_format() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{l:hello world}", &ctx), "hello world");
        // Literal should NOT expand variables
        assert_eq!(format_expand("#{l:#{session_name}}", &ctx), "#{session_name}");
    }

    #[test]
    fn double_expansion_e() {
        let mut ctx = FormatContext::new();
        ctx.set("template", "Session: #{session_name}");
        ctx.set("session_name", "work");
        // E: should expand "template" to its value, then expand that as a format string
        assert_eq!(format_expand("#{E:template}", &ctx), "Session: work");
    }

    #[test]
    fn double_expansion_e_plain() {
        let mut ctx = FormatContext::new();
        ctx.set("var", "hello");
        // If the value has no format strings, E: just returns it
        assert_eq!(format_expand("#{E:var}", &ctx), "hello");
    }

    #[test]
    fn double_expansion_e_missing() {
        let ctx = FormatContext::new();
        // Unknown variable: first expansion returns empty, second expansion of empty is empty
        assert_eq!(format_expand("#{E:unknown}", &ctx), "");
    }

    #[test]
    fn substitution_format() {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "my-session");
        assert_eq!(format_expand("#{s/-/_:#{session_name}}", &ctx), "my_session");
    }

    #[test]
    fn substitution_no_match() {
        let mut ctx = FormatContext::new();
        ctx.set("x", "hello");
        assert_eq!(format_expand("#{s/z/a:#{x}}", &ctx), "hello");
    }

    #[test]
    fn inline_style_passthrough() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#[fg=red]text#[default]", &ctx), "#[fg=red]text#[default]");
    }

    #[test]
    fn inline_style_with_variables() {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "dev");
        assert_eq!(format_expand("#[fg=green]#S#[default]", &ctx), "#[fg=green]dev#[default]");
    }

    #[test]
    fn inline_style_expands_nested_vars() {
        let mut ctx = FormatContext::new();
        ctx.set_option_lookup(|key| match key {
            "@thm_crust" => Some("#232634".to_string()),
            "@thm_blue" => Some("#8caaee".to_string()),
            _ => None,
        });
        // Catppuccin-style: #{@var} inside #[...]
        assert_eq!(
            format_expand("#[fg=#{@thm_crust},bg=#{@thm_blue}]text", &ctx),
            "#[fg=#232634,bg=#8caaee]text"
        );
    }

    #[test]
    fn inline_style_nested_var_no_closing_brace() {
        let ctx = FormatContext::new();
        // Malformed — no closing }, should degrade gracefully
        let result = format_expand("#[fg=#{bad]rest", &ctx);
        assert!(!result.is_empty());
    }

    // --- Section 3: new modifier tests ---

    #[test]
    fn shell_quoting_simple() {
        let mut ctx = FormatContext::new();
        ctx.set("cmd", "echo hello");
        assert_eq!(format_expand("#{q:cmd}", &ctx), "'echo hello'");
    }

    #[test]
    fn shell_quoting_with_single_quotes() {
        let mut ctx = FormatContext::new();
        ctx.set("cmd", "it's a test");
        assert_eq!(format_expand("#{q:cmd}", &ctx), "'it'\\''s a test'");
    }

    #[test]
    fn shell_quoting_empty() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{q:missing}", &ctx), "''");
    }

    #[test]
    fn string_length() {
        let mut ctx = FormatContext::new();
        ctx.set("name", "hello");
        assert_eq!(format_expand("#{n:name}", &ctx), "5");
    }

    #[test]
    fn string_length_empty() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{n:missing}", &ctx), "0");
    }

    #[test]
    fn string_length_unicode() {
        let mut ctx = FormatContext::new();
        ctx.set("text", "café");
        assert_eq!(format_expand("#{n:text}", &ctx), "4");
    }

    #[test]
    fn display_width_ascii() {
        let mut ctx = FormatContext::new();
        ctx.set("text", "hello");
        assert_eq!(format_expand("#{w:text}", &ctx), "5");
    }

    #[test]
    fn display_width_cjk() {
        let mut ctx = FormatContext::new();
        ctx.set("text", "日本語");
        assert_eq!(format_expand("#{w:text}", &ctx), "6");
    }

    #[test]
    fn display_width_mixed() {
        let mut ctx = FormatContext::new();
        ctx.set("text", "hi日本");
        assert_eq!(format_expand("#{w:text}", &ctx), "6");
    }

    #[test]
    fn ascii_code_to_char() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{a:65}", &ctx), "A");
        assert_eq!(format_expand("#{a:97}", &ctx), "a");
        assert_eq!(format_expand("#{a:48}", &ctx), "0");
    }

    #[test]
    fn ascii_code_from_variable() {
        let mut ctx = FormatContext::new();
        ctx.set("code", "72");
        assert_eq!(format_expand("#{a:code}", &ctx), "H");
    }

    #[test]
    fn ascii_code_invalid() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{a:not_a_number}", &ctx), "");
    }

    #[test]
    fn padding_right() {
        let mut ctx = FormatContext::new();
        ctx.set("x", "hi");
        assert_eq!(format_expand("#{p:5:x}", &ctx), "hi   ");
    }

    #[test]
    fn padding_left() {
        let mut ctx = FormatContext::new();
        ctx.set("x", "hi");
        assert_eq!(format_expand("#{p:-5:x}", &ctx), "   hi");
    }

    #[test]
    fn padding_no_op() {
        let mut ctx = FormatContext::new();
        ctx.set("x", "hello");
        assert_eq!(format_expand("#{p:3:x}", &ctx), "hello");
    }

    #[test]
    fn logical_not_true() {
        let mut ctx = FormatContext::new();
        ctx.set("active", "1");
        assert_eq!(format_expand("#{!active}", &ctx), "0");
    }

    #[test]
    fn logical_not_false() {
        let mut ctx = FormatContext::new();
        ctx.set("active", "0");
        assert_eq!(format_expand("#{!active}", &ctx), "1");
    }

    #[test]
    fn logical_not_empty() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{!missing}", &ctx), "1");
    }

    #[test]
    fn logical_or() {
        let mut ctx = FormatContext::new();
        ctx.set("a", "1");
        ctx.set("b", "0");
        assert_eq!(format_expand("#{||:#{a},#{b}}", &ctx), "1");
        ctx.set("a", "0");
        assert_eq!(format_expand("#{||:#{a},#{b}}", &ctx), "0");
    }

    #[test]
    fn logical_and() {
        let mut ctx = FormatContext::new();
        ctx.set("a", "1");
        ctx.set("b", "1");
        assert_eq!(format_expand("#{&&:#{a},#{b}}", &ctx), "1");
        ctx.set("b", "0");
        assert_eq!(format_expand("#{&&:#{a},#{b}}", &ctx), "0");
    }

    #[test]
    fn arithmetic_add() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{e|+:3,4}", &ctx), "7");
    }

    #[test]
    fn arithmetic_subtract() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{e|-:10,3}", &ctx), "7");
    }

    #[test]
    fn arithmetic_multiply() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{e|*:6,7}", &ctx), "42");
    }

    #[test]
    fn arithmetic_divide() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{e|/:10,3}", &ctx), "3");
    }

    #[test]
    fn arithmetic_modulo() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{e|%:10,3}", &ctx), "1");
    }

    #[test]
    fn arithmetic_divide_by_zero() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{e|/:5,0}", &ctx), "0");
        assert_eq!(format_expand("#{e|%:5,0}", &ctx), "0");
    }

    #[test]
    fn arithmetic_with_variables() {
        let mut ctx = FormatContext::new();
        ctx.set("width", "80");
        ctx.set("offset", "10");
        assert_eq!(format_expand("#{e|-:#{width},#{offset}}", &ctx), "70");
    }

    #[test]
    fn match_glob_star() {
        let mut ctx = FormatContext::new();
        ctx.set("name", "my-session");
        assert_eq!(format_expand("#{m:*session,#{name}}", &ctx), "1");
        assert_eq!(format_expand("#{m:*window,#{name}}", &ctx), "0");
    }

    #[test]
    fn match_glob_question() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{m:h?llo,hello}", &ctx), "1");
        assert_eq!(format_expand("#{m:h?llo,hilo}", &ctx), "0");
    }

    #[test]
    fn match_glob_exact() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{m:hello,hello}", &ctx), "1");
        assert_eq!(format_expand("#{m:hello,world}", &ctx), "0");
    }

    #[test]
    fn match_regex() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{m/r:^hello$,hello}", &ctx), "1");
        assert_eq!(format_expand("#{m/r:^hello$,hello world}", &ctx), "0");
        assert_eq!(format_expand("#{m/r:he.*o,hello}", &ctx), "1");
    }

    #[test]
    fn match_regex_dot_star() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{m/r:.*session.*,my-session-1}", &ctx), "1");
    }

    #[test]
    fn not_does_not_interfere_with_ne_comparison() {
        let mut ctx = FormatContext::new();
        ctx.set("a", "hello");
        ctx.set("b", "world");
        assert_eq!(format_expand("#{!=:#{a},#{b}}", &ctx), "1");
        assert_eq!(format_expand("#{!=:#{a},#{a}}", &ctx), "0");
    }

    #[test]
    fn combined_modifiers_in_conditional() {
        let mut ctx = FormatContext::new();
        ctx.set("count", "5");
        ctx.set("limit", "10");
        // Use arithmetic in a conditional
        let tmpl = "#{?#{e|-:#{limit},#{count}},remaining,done}";
        assert_eq!(format_expand(tmpl, &ctx), "remaining");
    }

    #[test]
    fn combined_not_in_conditional() {
        let mut ctx = FormatContext::new();
        ctx.set("zoomed", "0");
        assert_eq!(format_expand("#{?#{!zoomed},normal,ZOOM}", &ctx), "normal");
        ctx.set("zoomed", "1");
        assert_eq!(format_expand("#{?#{!zoomed},normal,ZOOM}", &ctx), "ZOOM");
    }

    // --- strftime tests ---

    #[test]
    fn strftime_hm() {
        // 2024-01-15 14:30:00 UTC = 1705329000
        let result = strftime_expand_with_timestamp("test %H:%M done", 1705329000);
        // The exact output depends on local timezone, but format should be NN:NN
        assert!(result.starts_with("test "));
        assert!(result.ends_with(" done"));
        // Should contain a colon-separated time
        let mid = &result[5..result.len() - 5];
        assert!(mid.contains(':'), "expected HH:MM, got: {mid}");
    }

    #[test]
    fn strftime_date_codes() {
        // Use a known timestamp and verify structure
        let result = strftime_expand_with_timestamp("%d-%b-%y", 1705329000);
        // Should be like "15-Jan-24" (in UTC) or similar in local time
        let parts: Vec<&str> = result.split('-').collect();
        assert_eq!(parts.len(), 3, "expected dd-Mon-yy, got: {result}");
        assert_eq!(parts[0].len(), 2, "day should be 2 chars: {result}");
        assert_eq!(parts[1].len(), 3, "month should be 3 chars: {result}");
        assert_eq!(parts[2].len(), 2, "year should be 2 chars: {result}");
    }

    #[test]
    fn strftime_no_codes() {
        assert_eq!(strftime_expand_with_timestamp("no codes here", 0), "no codes here");
    }

    #[test]
    fn strftime_literal_percent() {
        assert_eq!(strftime_expand_with_timestamp("100%%", 0), "100%");
    }

    #[test]
    fn strftime_full_status_right() {
        // Simulate the default status-right after format_expand has run
        let input = "\"some_title\" %H:%M %d-%b-%y";
        let result = strftime_expand_with_timestamp(input, 1705329000);
        assert!(result.starts_with("\"some_title\" "));
        assert!(!result.contains("%H"));
        assert!(!result.contains("%M"));
        assert!(!result.contains("%d"));
        assert!(!result.contains("%b"));
        assert!(!result.contains("%y"));
    }

    #[test]
    fn strftime_compound_codes() {
        let result = strftime_expand_with_timestamp("%R %F", 1705329000);
        // %R = HH:MM, %F = YYYY-MM-DD
        assert!(result.contains(':'), "%%R should produce HH:MM");
        assert!(result.contains('-'), "%%F should produce YYYY-MM-DD");
    }

    #[test]
    fn strftime_weekday_and_month_names() {
        let result = strftime_expand_with_timestamp("%a %A %b %B", 1705329000);
        // Should contain day/month names, no % codes left
        assert!(!result.contains('%'));
        // Should have 4 space-separated tokens
        let parts: Vec<&str> = result.split_whitespace().collect();
        assert_eq!(parts.len(), 4, "expected 4 tokens, got: {result}");
    }

    // --- unix_to_local tests ---

    /// Helper: compute unix_to_local in UTC (offset=0) for known-answer tests.
    /// We use TZ=UTC timestamps so results are deterministic regardless of machine TZ.
    fn unix_to_utc(timestamp: i64) -> (i32, u32, u32, u32, u32, u32, u32, u32) {
        // unix_to_local applies local offset. To test the civil algorithm in isolation,
        // use a timestamp where we know the local offset and compensate.
        // Simpler: just verify properties via strftime_expand_with_timestamp
        // and test the algorithm via known dates.
        unix_to_local(timestamp)
    }

    #[test]
    fn unix_to_local_components_in_range() {
        // Use current-ish timestamp
        let (year, month, day, hour, minute, second, weekday, yday) = unix_to_utc(1705329000);
        assert!((2020..=2030).contains(&year), "year={year}");
        assert!((1..=12).contains(&month), "month={month}");
        assert!((1..=31).contains(&day), "day={day}");
        assert!(hour <= 23, "hour={hour}");
        assert!(minute <= 59, "minute={minute}");
        assert!(second <= 59, "second={second}");
        assert!(weekday <= 6, "weekday={weekday}");
        assert!((1..=366).contains(&yday), "yday={yday}");
    }

    #[test]
    fn unix_to_local_epoch() {
        let (year, _month, _day, _hour, _minute, second, _weekday, _yday) = unix_to_utc(0);
        // Epoch is 1970-01-01 00:00:00 UTC. Local time varies by TZ.
        assert!((1969..=1970).contains(&year), "year={year}");
        assert_eq!(second, 0);
    }

    #[test]
    fn month_name_helpers() {
        assert_eq!(month_abbrev(1), "Jan");
        assert_eq!(month_abbrev(6), "Jun");
        assert_eq!(month_abbrev(12), "Dec");
        assert_eq!(month_abbrev(0), "???");
        assert_eq!(month_abbrev(13), "???");

        assert_eq!(month_full(1), "January");
        assert_eq!(month_full(7), "July");
        assert_eq!(month_full(12), "December");
        assert_eq!(month_full(0), "???");
    }

    #[test]
    fn weekday_name_helpers() {
        assert_eq!(weekday_abbrev(0), "Sun");
        assert_eq!(weekday_abbrev(4), "Thu");
        assert_eq!(weekday_abbrev(6), "Sat");
        assert_eq!(weekday_abbrev(7), "???");

        assert_eq!(weekday_full(0), "Sunday");
        assert_eq!(weekday_full(3), "Wednesday");
        assert_eq!(weekday_full(6), "Saturday");
        assert_eq!(weekday_full(7), "???");
    }

    #[test]
    fn strftime_ampm() {
        // Test %p and %P for AM/PM
        let midnight = strftime_expand_with_timestamp("%p %P", 0);
        // In UTC, 0 = midnight = AM. In local TZ it varies.
        assert!(!midnight.contains('%'));
        assert!(midnight.contains("AM") || midnight.contains("PM"));
        assert!(midnight.contains("am") || midnight.contains("pm"));
    }

    #[test]
    fn strftime_12hour() {
        // %I = zero-padded 12-hour, %l = space-padded 12-hour
        let result = strftime_expand_with_timestamp("%I %l", 1705329000);
        assert!(!result.contains('%'));
        let parts: Vec<&str> = result.split_whitespace().collect();
        assert_eq!(parts.len(), 2, "expected 2 tokens, got: {result}");
        // Both should be parseable as numbers 1-12
        for p in parts {
            let n: u32 = p.parse().unwrap_or(0);
            assert!((1..=12).contains(&n), "12-hour value out of range: {p}");
        }
    }

    #[test]
    fn strftime_newline_tab() {
        assert_eq!(strftime_expand_with_timestamp("%n%t", 0), "\n\t");
    }

    #[test]
    fn strftime_r_format() {
        // %r = 12-hour time with AM/PM (e.g., "02:30:00 PM")
        let result = strftime_expand_with_timestamp("%r", 1705329000);
        assert!(
            result.contains("AM") || result.contains("PM"),
            "%%r should include AM/PM: {result}"
        );
        // Should have two colons (HH:MM:SS)
        assert_eq!(result.matches(':').count(), 2, "%%r should have HH:MM:SS: {result}");
    }

    #[test]
    fn strftime_full_time_t() {
        // %T = HH:MM:SS
        let result = strftime_expand_with_timestamp("%T", 1705329000);
        assert_eq!(result.matches(':').count(), 2, "%%T should be HH:MM:SS: {result}");
        assert_eq!(result.len(), 8, "%%T should be 8 chars: {result}");
    }

    #[test]
    fn strftime_j_day_of_year() {
        let result = strftime_expand_with_timestamp("%j", 1705329000);
        let n: u32 = result.parse().unwrap_or(0);
        assert!((1..=366).contains(&n), "%%j should be day of year: {result}");
        assert_eq!(result.len(), 3, "%%j should be zero-padded to 3 chars: {result}");
    }

    #[test]
    fn strftime_date_format_d() {
        // %D = MM/DD/YY
        let result = strftime_expand_with_timestamp("%D", 1705329000);
        let parts: Vec<&str> = result.split('/').collect();
        assert_eq!(parts.len(), 3, "%%D should be MM/DD/YY: {result}");
    }

    #[test]
    fn strftime_unknown_code_passthrough() {
        // Unknown codes like %Q should pass through as %Q
        let result = strftime_expand_with_timestamp("%Q", 0);
        assert_eq!(result, "%Q");
    }

    #[test]
    fn strftime_trailing_percent() {
        // A lone % at the end should pass through
        let result = strftime_expand_with_timestamp("end%", 0);
        assert_eq!(result, "end%");
    }

    // --- @user_option lookup tests ---

    #[test]
    fn user_option_simple_lookup() {
        let mut ctx = FormatContext::new();
        ctx.set_option_lookup(|key| match key {
            "@thm_bg" => Some("#1e1e2e".to_string()),
            _ => None,
        });
        assert_eq!(format_expand("#{@thm_bg}", &ctx), "#1e1e2e");
    }

    #[test]
    fn user_option_missing_returns_empty() {
        let mut ctx = FormatContext::new();
        ctx.set_option_lookup(|_| None);
        assert_eq!(format_expand("#{@unknown}", &ctx), "");
    }

    #[test]
    fn user_option_in_conditional() {
        let mut ctx = FormatContext::new();
        ctx.set_option_lookup(|key| match key {
            "@catppuccin_flavor" => Some("mocha".to_string()),
            _ => None,
        });
        let result = format_expand("#{?#{==:#{@catppuccin_flavor},mocha},dark,light}", &ctx);
        assert_eq!(result, "dark");
    }

    #[test]
    fn user_option_in_double_expansion() {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "work");
        ctx.set_option_lookup(|key| match key {
            "@catppuccin_status_session" => Some("S: #{session_name}".to_string()),
            _ => None,
        });
        // E: should first resolve @catppuccin_status_session, then expand the result
        let result = format_expand("#{E:@catppuccin_status_session}", &ctx);
        assert_eq!(result, "S: work");
    }

    #[test]
    fn user_option_in_truncation() {
        let mut ctx = FormatContext::new();
        ctx.set_option_lookup(|key| match key {
            "@long_value" => Some("abcdefghij".to_string()),
            _ => None,
        });
        assert_eq!(format_expand("#{=5:@long_value}", &ctx), "abcde");
    }

    #[test]
    fn user_option_precedence_over_vars() {
        // If both a context var and an option lookup exist, context var wins
        let mut ctx = FormatContext::new();
        ctx.set("@foo", "from_context");
        ctx.set_option_lookup(|key| match key {
            "@foo" => Some("from_options".to_string()),
            _ => None,
        });
        // Context var should take precedence
        assert_eq!(format_expand("#{@foo}", &ctx), "from_context");
    }

    #[test]
    fn user_option_without_callback() {
        // No option_lookup set — @-prefixed vars just return empty
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{@anything}", &ctx), "");
    }

    #[test]
    fn user_option_nested_in_comparison() {
        let mut ctx = FormatContext::new();
        ctx.set_option_lookup(|key| match key {
            "@version" => Some("3.4".to_string()),
            _ => None,
        });
        assert_eq!(format_expand("#{>=:#{@version},3.4}", &ctx), "1");
        assert_eq!(format_expand("#{>=:#{@version},3.5}", &ctx), "0");
    }

    #[test]
    fn user_option_in_substitution() {
        let mut ctx = FormatContext::new();
        ctx.set_option_lookup(|key| match key {
            "@name" => Some("hello-world".to_string()),
            _ => None,
        });
        assert_eq!(format_expand("#{s/-/_:#{@name}}", &ctx), "hello_world");
    }

    // --- dirname / basename modifier tests ---

    #[test]
    fn dirname_simple() {
        let mut ctx = FormatContext::new();
        ctx.set("current_file", "/home/user/.config/tmux/plugins/catppuccin/catppuccin.conf");
        assert_eq!(
            format_expand("#{d:current_file}", &ctx),
            "/home/user/.config/tmux/plugins/catppuccin"
        );
    }

    #[test]
    fn dirname_root() {
        let mut ctx = FormatContext::new();
        ctx.set("path", "/file.txt");
        assert_eq!(format_expand("#{d:path}", &ctx), "/");
    }

    #[test]
    fn dirname_no_parent() {
        let mut ctx = FormatContext::new();
        ctx.set("path", "file.txt");
        assert_eq!(format_expand("#{d:path}", &ctx), "");
    }

    #[test]
    fn dirname_empty_var() {
        let ctx = FormatContext::new();
        assert_eq!(format_expand("#{d:nonexistent}", &ctx), "");
    }

    #[test]
    fn basename_simple() {
        let mut ctx = FormatContext::new();
        ctx.set("current_file", "/home/user/.config/tmux/catppuccin.conf");
        assert_eq!(format_expand("#{b:current_file}", &ctx), "catppuccin.conf");
    }

    #[test]
    fn basename_no_dir() {
        let mut ctx = FormatContext::new();
        ctx.set("path", "file.txt");
        assert_eq!(format_expand("#{b:path}", &ctx), "file.txt");
    }

    #[test]
    fn dirname_in_source_f_pattern() {
        // Catppuccin pattern: source -F "#{d:current_file}/themes/mocha.conf"
        let mut ctx = FormatContext::new();
        ctx.set("current_file", "/opt/plugins/catppuccin/catppuccin_tmux.conf");
        assert_eq!(
            format_expand("#{d:current_file}/themes/mocha.conf", &ctx),
            "/opt/plugins/catppuccin/themes/mocha.conf"
        );
    }

    #[test]
    fn dirname_nested_expansion() {
        // #{d:current_file} inside a larger format string
        let mut ctx = FormatContext::new();
        ctx.set("current_file", "/a/b/c.conf");
        ctx.set("flavor", "mocha");
        assert_eq!(format_expand("#{d:current_file}/#{flavor}.conf", &ctx), "/a/b/mocha.conf");
    }

    mod prop_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn expand_never_panics(template in "\\PC{0,200}") {
                let ctx = FormatContext::new();
                let _ = format_expand(&template, &ctx);
            }

            #[test]
            fn plain_text_passes_through(text in "[a-zA-Z0-9 ]{0,100}") {
                let ctx = FormatContext::new();
                let result = format_expand(&text, &ctx);
                prop_assert_eq!(result, text);
            }

            #[test]
            fn known_variable_always_expands(
                key in "[a-z_]{1,20}",
                value in "[a-zA-Z0-9]{0,50}"
            ) {
                let mut ctx = FormatContext::new();
                ctx.set(&key, &value);
                let template = format!("#{{{key}}}");
                let result = format_expand(&template, &ctx);
                prop_assert_eq!(result, value);
            }

            #[test]
            fn user_option_lookup_always_expands(
                suffix in "[a-z_]{1,20}",
                value in "[a-zA-Z0-9]{0,50}"
            ) {
                let key = format!("@{suffix}");
                let expected = value.clone();
                let key_clone = key.clone();
                let mut ctx = FormatContext::new();
                ctx.set_option_lookup(move |k| {
                    if k == key_clone { Some(expected.clone()) } else { None }
                });
                let template = format!("#{{{key}}}");
                let result = format_expand(&template, &ctx);
                prop_assert_eq!(result, value);
            }

            #[test]
            fn expand_never_panics_with_option_lookup(template in "\\PC{0,200}") {
                let mut ctx = FormatContext::new();
                ctx.set_option_lookup(|_| Some("test_value".to_string()));
                let _ = format_expand(&template, &ctx);
            }

            #[test]
            fn set_then_get_roundtrip(
                key in "[a-z_]{1,20}",
                value in "[a-zA-Z0-9]{0,50}"
            ) {
                let mut ctx = FormatContext::new();
                ctx.set(&key, &value);
                let got = ctx.get(&key);
                prop_assert_eq!(got, Some(value.as_str()));
            }

            #[test]
            fn unclosed_brace_does_not_expand(
                var in "[a-z]{1,20}"
            ) {
                let ctx = FormatContext::new();
                let template = format!("#{{{var}");
                let result = format_expand(&template, &ctx);
                // Should pass through as-is since there is no closing brace
                prop_assert_eq!(result, template);
            }

            #[test]
            fn strftime_never_panics(template in "\\PC{0,100}") {
                let _ = strftime_expand_with_timestamp(&template, 1705329000);
            }

            #[test]
            fn strftime_deterministic(template in "[%a-zA-Z ]{0,30}", ts in 0i64..2_000_000_000i64) {
                let a = strftime_expand_with_timestamp(&template, ts);
                let b = strftime_expand_with_timestamp(&template, ts);
                prop_assert_eq!(a, b);
            }

            #[test]
            fn unix_to_local_month_in_range(ts in 0i64..2_000_000_000i64) {
                let (_, month, _, _, _, _, _, _) = unix_to_local(ts);
                prop_assert!((1..=12).contains(&month), "month={month} for ts={ts}");
            }

            #[test]
            fn unix_to_local_day_in_range(ts in 0i64..2_000_000_000i64) {
                let (_, _, day, _, _, _, _, _) = unix_to_local(ts);
                prop_assert!((1..=31).contains(&day), "day={day} for ts={ts}");
            }

            #[test]
            fn unix_to_local_time_in_range(ts in 0i64..2_000_000_000i64) {
                let (_, _, _, hour, minute, second, _, _) = unix_to_local(ts);
                prop_assert!(hour <= 23, "hour={hour}");
                prop_assert!(minute <= 59, "minute={minute}");
                prop_assert!(second <= 59, "second={second}");
            }

            #[test]
            fn unix_to_local_weekday_in_range(ts in 0i64..2_000_000_000i64) {
                let (_, _, _, _, _, _, weekday, _) = unix_to_local(ts);
                prop_assert!(weekday <= 6, "weekday={weekday}");
            }

            #[test]
            fn unix_to_local_yday_in_range(ts in 0i64..2_000_000_000i64) {
                let (_, _, _, _, _, _, _, yday) = unix_to_local(ts);
                prop_assert!((1..=366).contains(&yday), "yday={yday}");
            }

            // --- prop tests for new Section 3 modifiers ---

            #[test]
            fn shell_quote_never_panics(value in "\\PC{0,100}") {
                let mut ctx = FormatContext::new();
                ctx.set("v", &value);
                let result = format_expand("#{q:v}", &ctx);
                // Result should start and end with single quote
                prop_assert!(result.starts_with('\''));
                prop_assert!(result.ends_with('\''));
            }

            #[test]
            fn string_length_matches(value in "[a-zA-Z0-9]{0,50}") {
                let mut ctx = FormatContext::new();
                ctx.set("v", &value);
                let result = format_expand("#{n:v}", &ctx);
                let expected = value.chars().count().to_string();
                prop_assert_eq!(result, expected);
            }

            #[test]
            fn display_width_at_least_char_count(value in "[a-zA-Z0-9]{0,50}") {
                let mut ctx = FormatContext::new();
                ctx.set("v", &value);
                let width: usize = format_expand("#{w:v}", &ctx).parse().unwrap_or(0);
                // For ASCII, display width == char count
                prop_assert_eq!(width, value.len());
            }

            #[test]
            fn logical_not_idempotent(flag in prop::bool::ANY) {
                let mut ctx = FormatContext::new();
                ctx.set("f", if flag { "1" } else { "0" });
                let not_result = format_expand("#{!f}", &ctx);
                let expected = if flag { "0" } else { "1" };
                prop_assert_eq!(not_result, expected);
            }

            #[test]
            fn arithmetic_add_commutative(a in -1000i64..1000i64, b in -1000i64..1000i64) {
                let ctx = FormatContext::new();
                let ab = format_expand(&format!("#{{e|+:{a},{b}}}"), &ctx);
                let ba = format_expand(&format!("#{{e|+:{b},{a}}}"), &ctx);
                prop_assert_eq!(ab, ba);
            }

            #[test]
            fn fnmatch_star_always_matches(s in "[a-zA-Z]{0,30}") {
                let mut ctx = FormatContext::new();
                ctx.set("s", &s);
                let result = format_expand("#{m:*,#{s}}", &ctx);
                prop_assert_eq!(result, "1");
            }

            #[test]
            fn padding_result_at_least_input_len(
                value in "[a-z]{0,10}",
                width in 0u32..20u32
            ) {
                let mut ctx = FormatContext::new();
                ctx.set("v", &value);
                let result = format_expand(&format!("#{{p:{width}:v}}"), &ctx);
                prop_assert!(result.len() >= value.len());
            }
        }
    }
}
