//! The TypeScript / JavaScript adapter: tree-sitter-typescript, module paths from the
//! `src/` layout (`index.ts` defines its directory).
//!
//! JavaScript and JSX are parsed with the TSX grammar; plain TypeScript that TSX rejects
//! (`<T>x` casts) falls back to the TypeScript grammar. Documentation is JSDoc.

mod class;
mod commonjs;
mod exports;
mod imports;
mod items;
mod syntax;

use std::path::{Component, Path};

use tree_sitter::{Language, Node, Parser, Tree};

use super::LanguageAdapter;
use crate::model::{FileIndex, ModPath};

/// Directories that hold dependencies, build output or tests rather than the package's code.
const SKIPPED_DIRS: [&str; 7] = [
    "node_modules",
    "dist",
    "coverage",
    "__tests__",
    "__mocks__",
    "test",
    "tests",
];

/// The TypeScript and JavaScript language adapter.
pub struct TypeScript;

impl LanguageAdapter for TypeScript {
    fn id(&self) -> &'static str {
        "typescript"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &imports::SOURCE_EXTENSIONS
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "export",
            "default",
            "function",
            "class",
            "interface",
            "type",
            "const",
            "let",
            "var",
            "enum",
            "async",
            "abstract",
            "declare",
            "namespace",
            "public",
            "private",
            "protected",
            "static",
            "readonly",
            "import",
        ]
    }

    fn module_of(&self, rel: &Path) -> Option<ModPath> {
        let parts = components(rel);
        let (file, dirs) = parts.split_last()?;
        if dirs.iter().any(|d| SKIPPED_DIRS.contains(&d.as_str())) {
            return None;
        }
        let (stem, ext) = file.rsplit_once('.')?;
        let is_test = stem.ends_with(".test") || stem.ends_with(".spec");
        if !imports::SOURCE_EXTENSIONS.contains(&ext) || stem.ends_with(".d") || is_test {
            return None;
        }
        let mut module = dirs.to_vec();
        if stem != "index" {
            module.push(stem.to_string());
        }
        Some(module)
    }

    fn parse(&self, source: &str, rel: &Path) -> FileIndex {
        let mut out = FileIndex {
            lines: source.lines().count(),
            ..FileIndex::default()
        };
        let Some(tree) = parse_tree(source) else {
            out.parse_errors = true;
            return out;
        };
        let root = tree.root_node();
        out.parse_errors = root.has_error();
        let mut dir = components(rel);
        dir.pop();
        items::walk(root, source, &mut out);
        imports::walk(root, source, &dir, &mut out);
        out
    }
}

fn components(rel: &Path) -> Vec<String> {
    rel.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

/// TSX first (it also reads JS and JSX), then plain TypeScript when TSX reports errors, then
/// the source with unsupported type-parameter modifiers blanked out.
fn parse_tree(source: &str) -> Option<Tree> {
    let tree = parse_with_fallback(source)?;
    if !tree.root_node().has_error() {
        return Some(tree);
    }
    blank_modifiers(source, &tree)
        .and_then(|fixed| parse_with_fallback(&fixed))
        .or(Some(tree))
}

fn parse_with_fallback(source: &str) -> Option<Tree> {
    let languages: [Language; 2] = [
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    ];
    let mut best: Option<Tree> = None;
    for language in languages {
        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .expect("tree-sitter-typescript grammar matches the tree-sitter version");
        let Some(tree) = parser.parse(source, None) else {
            continue;
        };
        if !tree.root_node().has_error() {
            return Some(tree);
        }
        best = best.or(Some(tree));
    }
    best
}

/// The grammar does not know `export type *` and variance modifiers on type parameters
/// (`<in out T>`): it reads
/// `in` as the parameter's name and stumbles on the rest. Replaces the modifiers with spaces
/// of the same length, so every offset in the tree still points into the original source.
fn blank_modifiers(source: &str, tree: &Tree) -> Option<String> {
    let is_modifier = |n: Node| matches!(&source[n.byte_range()], "in" | "out");
    let mut bytes = source.as_bytes().to_vec();
    let mut blanked = false;
    let mut blank = |n: Node| {
        bytes[n.byte_range()].fill(b' ');
        blanked = true;
    };
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if !node.has_error() {
            continue;
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for (i, child) in children.iter().enumerate() {
            // `export type * from './x'`: `type` is a stray word before the `*`.
            if node.kind() == "export_statement"
                && child.is_error()
                && &source[child.byte_range()] == "type"
            {
                blank(*child);
            }
            let strays: Vec<Node> = {
                let mut inner = child.walk();
                child.children(&mut inner).collect()
            };
            let in_parameters = matches!(node.kind(), "type_parameter" | "type_parameters");
            if !(in_parameters
                && child.is_error()
                && strays.iter().all(|w| w.kind() == "identifier"))
            {
                continue;
            }
            // A leading `out` before a parameter, comments in between: all modifiers.
            let next = children[i + 1..].iter().find(|n| n.kind() != "comment");
            if node.kind() == "type_parameters"
                && strays.iter().all(|w| is_modifier(*w))
                && next.is_some_and(|n| n.kind() == "type_parameter")
            {
                strays.iter().for_each(|w| blank(*w));
            }
            // `in out T`: the words before the last one are modifiers.
            strays
                .iter()
                .rev()
                .skip(1)
                .filter(|w| is_modifier(**w))
                .for_each(|w| blank(*w));
            // The parameter the parser named `in` or `out` is a modifier too.
            let named = match node.kind() {
                "type_parameter" => node.child_by_field_name("name"),
                _ => i
                    .checked_sub(1)
                    .map(|p| children[p])
                    .filter(|p| p.kind() == "type_parameter"),
            };
            if let Some(name) = named.filter(|n| is_modifier(*n)) {
                blank(name);
            }
        }
        stack.extend(children);
    }
    blanked.then(|| String::from_utf8(bytes).ok()).flatten()
}

#[cfg(test)]
mod tests;
