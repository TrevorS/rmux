#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_terminal::mouse::try_parse_mouse_csi;

fuzz_target!(|data: &[u8]| {
    // Mouse parser should handle any input without panicking
    let _ = try_parse_mouse_csi(data);
});
