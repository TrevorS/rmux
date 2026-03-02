#![no_main]
use libfuzzer_sys::fuzz_target;
use rmux_core::screen::Screen;
use rmux_terminal::input::InputParser;

fuzz_target!(|data: &[u8]| {
    let mut screen = Screen::new(80, 24, 100);
    let mut parser = InputParser::new();
    parser.parse(data, &mut screen);
    // The parser should never panic regardless of input
});
