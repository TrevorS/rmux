#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_server::config::parse_config_lines;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // parse_config_lines should handle any valid UTF-8 input without panicking
        let _ = parse_config_lines(s);
    }
});
