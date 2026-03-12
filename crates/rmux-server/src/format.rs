//! Format string expansion (#{...} syntax).
//!
//! Provides tmux-compatible format string expansion including variable
//! substitution, conditionals, comparisons, width truncation, and short aliases.

use nix::libc;
use std::collections::HashMap;

/// Context for format string expansion.
pub struct FormatContext {
    vars: HashMap<String, String>,
}

impl FormatContext {
    /// Create a new empty format context.
    pub fn new() -> Self {
        Self { vars: HashMap::new() }
    }

    /// Set a format variable.
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.vars.insert(key.to_string(), value.into());
    }

    /// Get a format variable value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
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
                    // #[style] — inline style directive, pass through as-is
                    let start = i + 2;
                    if let Some(end_bracket) = template[start..].find(']') {
                        let end = start + end_bracket;
                        // Pass through #[...] verbatim for the renderer
                        result.push_str(&template[i..=end]);
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
            result.push(bytes[i] as char);
            i += 1;
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

    // Literal: l:text (no further expansion)
    if let Some(rest) = inner.strip_prefix("l:") {
        return rest.to_string();
    }

    // Substitution: s/pattern/replacement:expr
    if inner.starts_with("s/") {
        if let Some(result) = expand_substitution(inner, ctx) {
            return result;
        }
    }

    // Comparison operators: ==:a,b  !=:a,b  <:a,b  etc.
    if let Some(rest) = inner.strip_prefix("==:") {
        return expand_comparison(rest, ctx, |a, b| a == b);
    }
    if let Some(rest) = inner.strip_prefix("!=:") {
        return expand_comparison(rest, ctx, |a, b| a != b);
    }
    if let Some(rest) = inner.strip_prefix("<=:") {
        return expand_comparison(rest, ctx, |a, b| a <= b);
    }
    if let Some(rest) = inner.strip_prefix(">=:") {
        return expand_comparison(rest, ctx, |a, b| a >= b);
    }
    if let Some(rest) = inner.strip_prefix("<:") {
        return expand_comparison(rest, ctx, |a, b| a < b);
    }
    if let Some(rest) = inner.strip_prefix(">:") {
        return expand_comparison(rest, ctx, |a, b| a > b);
    }

    // Width truncation: =N:expr (positive N = right trunc, negative = left trunc)
    if inner.starts_with('=') || inner.starts_with("=-") {
        if let Some(result) = expand_truncation(inner, ctx) {
            return result;
        }
    }

    // Plain variable lookup (may contain nested #{} that need expanding first)
    let expanded = format_expand(inner, ctx);
    // If the expanded result is itself a variable name, look it up
    // But first check if the original inner was a plain variable name
    if inner.contains('#') {
        // Had nested expansions — return the expanded result
        expanded
    } else {
        // Plain variable name
        ctx.get(inner).unwrap_or("").to_string()
    }
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
    if expr.contains('#') {
        format_expand(expr, ctx)
    } else {
        ctx.get(expr).unwrap_or("").to_string()
    }
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
    let expanded = format_expand(expr, ctx);

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
        if bytes[i] == b'%' && i + 1 < len {
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
            result.push(bytes[i] as char);
            i += 1;
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
        }
    }
}
