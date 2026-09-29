//! Language adapters: the only code that knows a language.
//!
//! An adapter owns everything that differs between languages: finding and reading the package
//! manifest, parsing a file into a [`FileIndex`], how a file maps to a module, how an import
//! resolves, and how names are spelled in output and in queries. Everything else reads
//! [`FileIndex`] and asks the adapter; no other module compares a language id. Rust and
//! TypeScript/JavaScript are supported.
//!
//! # How to add a language
//! 1. Add `src/lang/<language>/` implementing [`LanguageAdapter`] on top of a tree-sitter
//!    grammar, one file per concern like [`crate::lang::rust`]: `mod.rs` (the trait), `items.rs`
//!    and `refs.rs` (parsing into [`crate::model::FileIndex`]), `project.rs` (the manifest) and
//!    `resolve.rs` (imports).
//! 2. List the adapter in [`all`]; its manifest, extensions and naming are then picked up
//!    everywhere.
//! 3. Add `examples/<language>/` with its own golden cases in `tests/golden.rs`; the
//!    language is supported only when that suite passes.

pub mod rust;
pub mod typescript;

use std::path::Path;

use crate::index::Index;
use crate::model::{FileIndex, ModPath};
use crate::project::{Config, Project};
use crate::resolve::Target;

/// Everything figure needs to know about one language.
pub trait LanguageAdapter: Sync {
    /// Short id, e.g. `rust`.
    fn id(&self) -> &'static str;
    /// File extensions this adapter parses, without the dot.
    fn extensions(&self) -> &'static [&'static str];
    /// Words that can stand before a name in a code search (`fn`, `struct`, `impl`): the
    /// agent guard skips them when it decides whether a grep pattern names a symbol.
    fn keywords(&self) -> &'static [&'static str];

    /// File name of the package manifest, e.g. `Cargo.toml`.
    fn manifest(&self) -> &'static str;
    /// True when `dir` is the root of a package of this language.
    fn detect(&self, dir: &Path) -> bool {
        dir.join(self.manifest()).is_file()
    }
    /// Reads the manifest in `root` into a [`Project`].
    fn load_project(&self, root: &Path, config: Config) -> Result<Project, String>;

    /// The module a file defines, from its path relative to the source root.
    /// `None` for files that are not part of the module tree.
    fn module_of(&self, rel: &Path) -> Option<ModPath>;
    /// True for a file name that stands for its directory (`mod.rs`, `index.ts`).
    fn defines_dir(&self, file_name: &str) -> bool;
    /// Parses one file. `rel` is its path below the source root (imports resolve against
    /// it); `path` and `module` are filled in by the caller.
    fn parse(&self, source: &str, rel: &Path) -> FileIndex;

    /// Where `segs`, a path written in `file`, points. `anchored` paths were already made
    /// absolute below the source root by [`Self::parse`].
    fn resolve(&self, index: &Index, file: &FileIndex, segs: &[String], anchored: bool) -> Target;
    /// True for a package the language ships with (`std`, `fs`): it is not a dependency.
    fn is_builtin(&self, _name: &str) -> bool {
        false
    }

    /// How output names a module.
    fn module_name(&self, module: &[String]) -> String;
    /// How a dependency listing names a group of modules.
    fn group_name(&self, group: &[String]) -> String;
    /// How a member is named after its owner.
    fn qualify(&self, owner: &str, name: &str) -> String;
    /// An item's unambiguous name, module included; `qualified` is [`Self::qualify`]'s result.
    fn full_name(&self, module: &[String], qualified: &str) -> String;
    /// Splits a user's symbol or module query into names.
    fn split_query(&self, query: &str) -> Vec<String>;
    /// A member's one-line signature with its owner in the name.
    fn qualify_signature(&self, signature: &str, owner: &str, name: &str) -> String;
    /// Names ripgrep's `--type` gives to this language.
    fn grep_types(&self) -> &'static [&'static str];
}

/// Every adapter, in the order manifests are tried.
pub fn all() -> [&'static dyn LanguageAdapter; 2] {
    [&rust::Rust, &typescript::TypeScript]
}

/// The adapter whose [`LanguageAdapter::id`] is `id`.
pub fn adapter(id: &str) -> Option<&'static dyn LanguageAdapter> {
    all().into_iter().find(|a| a.id() == id)
}

/// True when `name` ends in an extension some adapter parses.
pub fn is_source_file(name: &str) -> bool {
    all().iter().any(|a| {
        a.extensions()
            .iter()
            .any(|e| name.strip_suffix(e).is_some_and(|s| s.ends_with('.')))
    })
}
