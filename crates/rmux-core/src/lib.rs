#![forbid(unsafe_code)]
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
    clippy::return_self_not_must_use,
    clippy::bool_to_int_with_if,
    clippy::struct_excessive_bools,
    clippy::trivially_copy_pass_by_ref,
    clippy::doc_markdown,
    clippy::match_same_arms
)]

//! # rmux-core
//!
//! Core data structures for rmux: grid, screen, layout, options, styles.
//! This crate has zero I/O dependencies - it is purely data structures and algorithms.

pub mod error;
pub mod grid;
pub mod key;
pub mod layout;
pub mod options;
pub mod screen;
pub mod style;
pub mod utf8;
