#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_core::options::Options;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Split input into key/value at the first null byte or midpoint
        let (key, value) = if let Some(pos) = s.find('\0') {
            (&s[..pos], &s[pos + 1..])
        } else if s.len() >= 2 {
            let mid = s.len() / 2;
            (&s[..mid], &s[mid..])
        } else {
            return;
        };
        // parse_and_set should handle any key/value pair without panicking
        let mut opts = Options::new();
        opts.parse_and_set(key, value);
    }
});
