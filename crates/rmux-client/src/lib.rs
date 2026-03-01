#![deny(unsafe_code)]
#![deny(clippy::all, clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::wildcard_imports,
    clippy::doc_markdown
)]

//! # rmux-client
//!
//! Client process for rmux. Handles CLI parsing, terminal setup, and
//! communication with the server.

pub mod connect;
pub mod dispatch;
pub mod terminal;
