#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_core::options::Options;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Split input into key/value at the first null byte or midpoint
        let (key, value) = if let Some(pos) = s.find('\0') {
            (&s[..pos], &s[pos + 1..])
        } else if s.len() >= 2 {
            // Find a char-boundary-safe split point near the midpoint
            let mid = s.len() / 2;
            let split = s.ceil_char_boundary(mid);
            if split == 0 || split >= s.len() {
                return;
            }
            (&s[..split], &s[split..])
        } else {
            return;
        };
        // parse_and_set should handle any key/value pair without panicking
        let mut opts = Options::new();
        opts.parse_and_set(key, value);
    }
});
