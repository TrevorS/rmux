#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_terminal::keys::parse_key;

fuzz_target!(|data: &[u8]| {
    // parse_key should return None or Some without panicking
    let _ = parse_key(data);
});
