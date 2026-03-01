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
    clippy::wildcard_imports,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::cast_possible_wrap
)]

//! # rmux-protocol
//!
//! Wire protocol for rmux, compatible with tmux's imsg-based protocol.
//! Protocol version 8.

pub mod codec;
pub mod identify;
pub mod message;
