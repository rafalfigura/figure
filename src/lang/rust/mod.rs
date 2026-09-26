//! The Rust adapter: tree-sitter-rust, module paths from the `src/` layout.

mod items;
mod refs;
mod syntax;

use std::path::{Component, Path};

use tree_sitter::Parser;

use super::LanguageAdapter;
use crate::model::{FileIndex, ModPath};

/// The Rust language adapter.
pub struct Rust;

impl LanguageAdapter for Rust {
    fn id(&self) -> &'static str {
        "rust"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn module_of(&self, rel: &Path) -> Option<ModPath> {
        if rel.extension()? != "rs" {
            return None;
        }
        let parts: Vec<String> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let (file, dirs) = parts.split_last()?;
        if dirs.first().is_some_and(|d| d == "bin") {
            return None;
        }
        let stem = file.strip_suffix(".rs")?;
        let mut module: ModPath = dirs.to_vec();
        match stem {
            "mod" => {}
            "main" | "lib" if dirs.is_empty() => {}
            _ => module.push(stem.to_string()),
        }
        Some(module)
    }

    fn parse(&self, source: &str) -> FileIndex {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("tree-sitter-rust grammar matches the tree-sitter version");
        let mut out = FileIndex {
            lines: source.lines().count(),
            ..FileIndex::default()
        };
        let Some(tree) = parser.parse(source, None) else {
            out.parse_errors = true;
            return out;
        };
        let root = tree.root_node();
        out.parse_errors = root.has_error();
        items::walk(root, source, &mut out);
        refs::walk(root, source, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemKind, Vis};

    fn parse(src: &str) -> FileIndex {
        Rust.parse(src)
    }

    /// `mod.rs`, `lib.rs`/`main.rs` and plain files map to the module they define.
    #[test]
    fn module_paths() {
        let m = |p: &str| Rust.module_of(Path::new(p));
        assert_eq!(m("main.rs"), Some(vec![]));
        assert_eq!(m("traps/mod.rs"), Some(vec!["traps".to_string()]));
        assert_eq!(
            m("traps/spikes.rs"),
            Some(vec!["traps".into(), "spikes".into()])
        );
        assert_eq!(m("bin/tool.rs"), None);
    }

    /// Signatures stop at the body; plain `pub` is dropped, restricted visibility kept.
    #[test]
    fn signatures_without_bodies() {
        let f = parse(
            "/// Adds.\npub fn add<T: Copy>(a: T) -> T where T: Clone { a }\n\
             pub(super) fn plugin(app: &mut App) {}\n\
             pub struct Unit;\npub struct Tuple(pub u8);\n",
        );
        let sigs: Vec<_> = f.items.iter().map(|i| i.signature.as_str()).collect();
        assert_eq!(
            sigs,
            [
                "fn add<T: Copy>(a: T) -> T where T: Clone",
                "pub(super) fn plugin(app: &mut App)",
                "struct Unit",
                "struct Tuple(pub u8)",
            ]
        );
        assert_eq!(f.items[0].doc, ["Adds."]);
        assert_eq!(f.items[1].vis, Vis::Restricted);
    }

    /// Nested use groups over several lines expand to one path each.
    #[test]
    fn use_groups_flatten() {
        let f = parse(
            "use super::shared::{\n    hazard::{Harm, Plate},\n    fx,\n};\nuse crate::core::*;\n",
        );
        let paths: Vec<String> = f.refs.iter().map(|r| r.segments.join("::")).collect();
        assert_eq!(
            paths,
            [
                "super::shared::hazard::Harm",
                "super::shared::hazard::Plate",
                "super::shared::fx",
                "crate::core"
            ]
        );
        assert!(f.refs[3].glob);
    }

    /// Inline qualified paths inside function bodies are references too.
    #[test]
    fn inline_paths_found() {
        let f = parse("fn f() { crate::core::running(); }");
        assert!(
            f.refs
                .iter()
                .any(|r| r.segments == ["crate", "core", "running"])
        );
        let owner = f.refs[0].owner.as_ref().unwrap();
        assert_eq!(owner.name, "f");
    }

    /// Items in `#[cfg(test)]` modules are not listed; `#[test]` functions become tests.
    #[test]
    fn test_module_items() {
        let f = parse(
            "#[cfg(test)]\nmod tests {\n    pub fn helper() {}\n    /// Rule.\n    #[test]\n    fn rule() {}\n}\n",
        );
        assert!(f.items.is_empty());
        assert_eq!(f.tests.len(), 1);
        assert_eq!(f.tests[0].doc, ["Rule."]);
        assert!(f.refs.is_empty() || f.refs.iter().all(|r| r.in_test));
    }

    /// Methods of inherent impls are items with an owner; trait impl methods are not.
    #[test]
    fn impl_methods() {
        let f = parse(
            "pub struct Harm;\nimpl Harm { pub fn new() -> Self { Harm } }\n\
             impl Plugin for Harm { fn build(&self, app: &mut App) {} }\n",
        );
        let names: Vec<_> = f.items.iter().map(|i| i.qualified_name()).collect();
        assert_eq!(names, ["Harm", "Harm::new"]);
        assert_eq!(f.trait_impls[0].trait_name, "Plugin");
        assert_eq!(f.items[1].kind, ItemKind::Fn);
    }

    /// Method calls keep the method name and turbofish; path calls keep the path.
    #[test]
    fn calls_recorded() {
        let f = parse(
            "fn plugin(app: &mut App) { super::register::<Spikes>(app, \"Spikes\"); \
             app.init_resource::<Reg>().add_systems(Update, run.run_if(on)); }",
        );
        let callees: Vec<_> = f.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"super::register::<Spikes>"));
        let systems = f.calls.iter().find(|c| c.callee == "add_systems").unwrap();
        assert_eq!(systems.args, ["Update", "run.run_if(on)"]);
        let res = f
            .calls
            .iter()
            .find(|c| c.callee == "init_resource")
            .unwrap();
        assert_eq!(res.generics.as_deref(), Some("Reg"));
    }

    /// `//!` lines document the module; `#[doc(hidden)]` and `#[cfg(test)]` are flagged.
    #[test]
    fn module_doc_and_flags() {
        let f = parse(
            "//! Traps.\n//!\n//! More.\n#[doc(hidden)]\npub fn h() {}\n#[cfg(test)]\npub use a::B;\n",
        );
        assert_eq!(f.module_doc, ["Traps.", "", "More."]);
        assert!(f.items[0].hidden);
        assert!(f.items[1].test_only);
        assert_eq!(f.items[1].name, "B");
        let g = parse("pub use a::{B, c::D as E};\n");
        let names: Vec<_> = g
            .items
            .iter()
            .map(|i| (i.name.as_str(), i.signature.as_str()))
            .collect();
        assert_eq!(names, [("B", "use a::B"), ("E", "use a::c::D")]);
    }
}
