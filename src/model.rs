//! The language-neutral index: what one parsed source file contributes.
//!
//! A language adapter fills these structs; every command reads only them, never syntax trees.

use std::path::PathBuf;

/// A module path without the `crate` prefix: `["traps", "shared"]`. The crate root is empty.
pub type ModPath = Vec<String>;

/// Renders a module path the way Rust spells it: `crate::traps::shared`.
pub fn mod_display(path: &[String]) -> String {
    if path.is_empty() {
        "crate".to_string()
    } else {
        format!("crate::{}", path.join("::"))
    }
}

/// How far an item is visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vis {
    Private,
    /// `pub(crate)`, `pub(super)`, `pub(in ...)`.
    Restricted,
    Pub,
}

/// What kind of item an [`Item`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Fn,
    Struct,
    /// A class (TypeScript / JavaScript).
    Class,
    /// An interface (TypeScript).
    Interface,
    Enum,
    Union,
    Trait,
    Type,
    Const,
    Static,
    Macro,
    /// A re-export (`pub use`).
    Use,
}

impl ItemKind {
    /// The keyword for the kind: `fn`, `struct`, `class`, ...
    pub fn keyword(self) -> &'static str {
        match self {
            ItemKind::Fn => "fn",
            ItemKind::Struct => "struct",
            ItemKind::Class => "class",
            ItemKind::Interface => "interface",
            ItemKind::Enum => "enum",
            ItemKind::Union => "union",
            ItemKind::Trait => "trait",
            ItemKind::Type => "type",
            ItemKind::Const => "const",
            ItemKind::Static => "static",
            ItemKind::Macro => "macro",
            ItemKind::Use => "use",
        }
    }
}

/// One declared item: a function, type, constant, method or re-export.
#[derive(Clone, Debug)]
pub struct Item {
    pub kind: ItemKind,
    pub name: String,
    pub vis: Vis,
    /// Declaration text without the body, whitespace collapsed; plain `pub ` removed.
    pub signature: String,
    /// Doc comment lines, markers stripped.
    pub doc: Vec<String>,
    /// First line of the item including its docs and attributes (1-based).
    pub start_line: usize,
    /// Line of the declaration keyword (1-based).
    pub decl_line: usize,
    pub end_line: usize,
    /// Struct fields, enum variants or trait method signatures.
    pub fields: Vec<String>,
    /// Derive macro names, last path segment only: `Component`, `Deserialize`.
    pub derives: Vec<String>,
    /// For methods: the type of the enclosing `impl`.
    pub owner: Option<String>,
    /// `#[doc(hidden)]`: skipped by every command.
    pub hidden: bool,
    /// Only compiled for tests (`#[cfg(test)]`).
    pub test_only: bool,
}

impl Item {
    /// True when the item has a non-empty doc comment.
    pub fn is_documented(&self) -> bool {
        self.doc.iter().any(|l| !l.trim().is_empty())
    }

    /// `Type::method` for methods, the plain name otherwise.
    pub fn qualified_name(&self) -> String {
        match &self.owner {
            Some(owner) => format!("{owner}::{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// `impl Trait for Type`.
#[derive(Clone, Debug)]
pub struct TraitImpl {
    pub ty: String,
    pub trait_name: String,
}

/// A test function; its doc states the rule it enforces.
#[derive(Clone, Debug)]
pub struct TestFn {
    pub name: String,
    pub doc: Vec<String>,
}

/// A path the file refers to: a `use` or an inline path like `crate::a::b()`.
#[derive(Clone, Debug)]
pub struct PathRef {
    pub segments: Vec<String>,
    pub glob: bool,
    /// `segments` are a module path below the source root (an adapter resolved a relative
    /// import while parsing); otherwise they are as written in the source.
    pub anchored: bool,
    pub line: usize,
    pub in_test: bool,
    /// The function the path appears in, if any.
    pub owner: Option<Owner>,
}

/// The function a call or path appears in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    pub ty: Option<String>,
    pub name: String,
}

/// A call expression, kept raw; relation packs decide what it means.
#[derive(Clone, Debug)]
pub struct Call {
    /// Callee text, whitespace removed: `super::register::<Spikes>` or the method name.
    pub callee: String,
    /// For method calls (`x.add_systems(..)`): the method name.
    pub method: Option<String>,
    /// Turbofish arguments of a method call, `init_resource::<T>` -> `T`.
    pub generics: Option<String>,
    pub args: Vec<String>,
    pub line: usize,
    pub in_test: bool,
    pub owner: Option<Owner>,
}

/// Everything one source file contributes to the index.
#[derive(Clone, Debug, Default)]
pub struct FileIndex {
    /// Relative to the project root.
    pub path: PathBuf,
    /// The module this file defines (`src/traps/mod.rs` -> `["traps"]`).
    pub module: ModPath,
    pub lines: usize,
    pub module_doc: Vec<String>,
    /// Line of the first `//!` line (1-based), 0 when there is no module doc.
    pub module_doc_line: usize,
    pub items: Vec<Item>,
    pub trait_impls: Vec<TraitImpl>,
    pub tests: Vec<TestFn>,
    pub refs: Vec<PathRef>,
    pub calls: Vec<Call>,
    pub parse_errors: bool,
}

impl FileIndex {
    /// Items that commands show: not hidden and not test-only.
    pub fn visible_items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| !i.hidden && !i.test_only)
    }
}
