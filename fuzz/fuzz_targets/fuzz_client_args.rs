#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_client::parse_args;

fuzz_target!(|data: &[u8]| {
    // Convert fuzz bytes into a vector of argument strings.
    // Split on null bytes to get multiple arguments.
    let input = String::from_utf8_lossy(data);
    let mut argv: Vec<String> = vec!["rmux".to_string()];
    argv.extend(input.split('\0').filter(|s| !s.is_empty()).map(String::from));

    // parse_args should never panic, only return Ok or Err
    let _ = parse_args(&argv);
});
