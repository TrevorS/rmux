#![deny(clippy::all, clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    clippy::doc_markdown,
    clippy::match_same_arms,
    clippy::many_single_char_names,
    clippy::unused_self,
    clippy::cast_possible_wrap
)]

//! # rmux-terminal
//!
//! Terminal I/O for rmux: VT100/xterm escape sequence parsing, terminal output
//! generation, PTY management, key sequence parsing.

pub mod input;
pub mod keys;
pub mod mouse;
pub mod output;
pub mod pty;
