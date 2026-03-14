#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_server::config::{ConfigContext, parse_config_lines, parse_config_with_context};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // parse_config_lines should handle any valid UTF-8 input without panicking
        let _ = parse_config_lines(s);

        // Test with context (exercises %if/%elif/%else/%endif, %hidden, ${VAR})
        let mut ctx = ConfigContext::new();
        ctx.set_format_expand(std::string::ToString::to_string);
        ctx.hidden_vars.insert("MODULE_NAME".to_string(), "session".to_string());
        ctx.hidden_vars.insert("COLOR".to_string(), "blue".to_string());
        let _ = parse_config_with_context(s, &mut ctx);
    }
});
