#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_core::style::parse_style;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // parse_style should handle any string without panicking
        let _ = parse_style(s);
    }
});
