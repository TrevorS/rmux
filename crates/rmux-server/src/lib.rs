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
    clippy::doc_markdown
)]

//! # rmux-server
//!
//! Server process for rmux. Manages sessions, windows, panes, and clients.
//! Implements the tmux-compatible command set.

pub mod client;
pub mod command;
pub mod config;
pub mod copymode;
pub mod format;
pub mod keybind;
pub mod navigate;
pub mod notify;
pub mod pane;
pub mod paste;
pub mod render;
pub mod server;
pub mod session;
pub mod window;
