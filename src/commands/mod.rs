//! One module per CLI command; each returns the text to print.
//!
//! # How to add a command
//! 1. Add `src/commands/<name>.rs` with a documented `pub fn run(index: &Index, ...) -> String`
//!    that builds its text with [`crate::render::Out`].
//! 2. Add a variant to `Command` in `src/main.rs` and dispatch to it in `run`.
//! 3. Add golden cases for it to `RUST_CASES` in `tests/golden.rs`, then
//!    `FIGURE_BLESS=1 cargo test --test golden` and review the new files.
//!
//! Example to copy: [`crate::commands::deps::run`]

pub mod check;
pub mod deps;
pub mod howto;
mod links;
pub mod map;
pub mod show;
