#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_server::format::strftime_expand;

fuzz_target!(|data: &[u8]| {
    if let Ok(template) = std::str::from_utf8(data) {
        // strftime_expand should handle any valid UTF-8 string without panicking
        let _ = strftime_expand(template);
    }
});
