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
        // Two '#' characters: first '#' is not followed by '{', so it's literal.
        // Second '#' is also not followed by '{', so also literal.
        assert_eq!(result, "##");
    }

    #[test]
    fn very_long_template() {
        let mut ctx = FormatContext::new();
        ctx.set("x", "X");
        // Build a template with 1000+ characters
        let mut template = String::new();
        for i in 0..200 {
            write!(template, "item{i}-#{{x}}-").unwrap();
        }
        let result = format_expand(&template, &ctx);
        // Each "item{i}-#{x}-" expands #{x} to "X"
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
        }
    }
}
