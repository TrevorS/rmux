//! Terminal input parsing: VT100/xterm escape sequence state machine.
//!
//! The input parser processes raw bytes from a PTY and converts them into
//! screen operations (character writes, cursor moves, style changes, etc.).

pub mod params;
pub mod parser;

pub use parser::InputParser;
