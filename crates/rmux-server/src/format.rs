//! Format string expansion (#{...} syntax).
//!
//! Provides simple variable substitution for tmux-compatible format strings.

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

/// Expand format strings with #{variable} syntax.
///
/// Replaces each `#{variable_name}` with its value from the context.
/// Unknown variables are replaced with empty strings.
pub fn format_expand(template: &str, ctx: &FormatContext) -> String {
    let mut result = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && bytes[i] == b'#' && bytes[i + 1] == b'{' {
            // Found #{, look for closing }
            let start = i + 2;
            if let Some(end) = template[start..].find('}') {
                let var_name = &template[start..start + end];
                if let Some(val) = ctx.get(var_name) {
                    result.push_str(val);
                }
                i = start + end + 1;
                continue;
            }
        }
        // Regular character
        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
