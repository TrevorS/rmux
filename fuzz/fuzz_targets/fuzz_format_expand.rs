#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_server::format::{FormatContext, format_expand};

fuzz_target!(|data: &[u8]| {
    if let Ok(template) = std::str::from_utf8(data) {
        // Test without option lookup
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "main");
        ctx.set("window_index", "3");
        ctx.set("pane_title", "shell");
        ctx.set("template", "Session: #{session_name}");
        let _ = format_expand(template, &ctx);

        // Test with option lookup (exercises #{@...} paths)
        let mut ctx2 = FormatContext::new();
        ctx2.set("session_name", "main");
        ctx2.set("window_index", "3");
        ctx2.set_option_lookup(|key| match key {
            "@thm_bg" => Some("#1e1e2e".to_string()),
            "@catppuccin_flavor" => Some("mocha".to_string()),
            "@catppuccin_status_session" => Some("#{session_name}".to_string()),
            _ => None,
        });
        let _ = format_expand(template, &ctx2);
    }
});
