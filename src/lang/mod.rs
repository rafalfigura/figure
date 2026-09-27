//! Language adapters: the only code that knows a language's syntax.
//!
//! An adapter turns one source file into a [`FileIndex`]; everything after that is
//! language-neutral. The MVP ships the Rust adapter only.
//!
//! # How to add a language
//! 1. Add `src/lang/<language>/` implementing [`LanguageAdapter`] on top of a tree-sitter
//!    grammar; fill [`crate::model::FileIndex`] the way [`crate::lang::rust::Rust`] does.
//! 2. Teach [`crate::resolve`] the language's import rules.
//! 3. Add `examples/<language>/` with its own golden cases in `tests/golden.rs`; the
//!    language is supported only when that suite passes.

pub mod rust;

use std::path::Path;

use crate::model::{FileIndex, ModPath};

/// Turns the source files of one language into [`FileIndex`] records.
pub trait LanguageAdapter: Sync {
    /// Short id, e.g. `rust`.
    fn id(&self) -> &'static str;
    /// File extensions this adapter parses, without the dot.
    fn extensions(&self) -> &'static [&'static str];
    /// The module a file defines, from its path relative to the source root.
    /// `None` for files that are not part of the module tree.
    fn module_of(&self, rel: &Path) -> Option<ModPath>;
    /// Parses one file. `path` and `module` are filled in by the caller.
    fn parse(&self, source: &str) -> FileIndex;
    /// Words that can stand before a name in a code search (`fn`, `struct`, `impl`): the
    /// agent guard skips them when it decides whether a grep pattern names a symbol.
    fn keywords(&self) -> &'static [&'static str];
}

/// The adapter whose [`LanguageAdapter::id`] is `id`.
pub fn adapter(id: &str) -> Option<&'static dyn LanguageAdapter> {
    match id {
        "rust" => Some(&rust::Rust),
        _ => None,
    }
}
