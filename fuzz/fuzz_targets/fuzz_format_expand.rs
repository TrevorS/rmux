#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_server::format::{FormatContext, format_expand};

fuzz_target!(|data: &[u8]| {
    if let Ok(template) = std::str::from_utf8(data) {
        let mut ctx = FormatContext::new();
        ctx.set("session_name", "main");
        ctx.set("window_index", "3");
        ctx.set("pane_title", "shell");
        // format_expand should handle any valid UTF-8 template without panicking
        let _ = format_expand(template, &ctx);
    }
});
