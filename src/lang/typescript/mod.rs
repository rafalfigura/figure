//! The TypeScript / JavaScript adapter: tree-sitter-typescript, module paths from the
//! `src/` layout (`index.ts` defines its directory).
//!
//! JavaScript and JSX are parsed with the TSX grammar; plain TypeScript that TSX rejects
//! (`<T>x` casts) falls back to the TypeScript grammar. Documentation is JSDoc.

mod aliases;
mod class;
mod commonjs;
mod exports;
mod grammar;
mod imports;
mod items;
mod jsonc;
mod project;
mod resolve;
mod syntax;

use std::path::{Component, Path};

use super::LanguageAdapter;
use crate::index::Index;
use crate::model::{FileIndex, ModPath};
use crate::project::{Config, Project};
use crate::resolve::Target;

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

    fn manifest(&self) -> &'static str {
        "package.json"
    }

    fn detect(&self, dir: &Path) -> bool {
        project::detect(dir)
    }

    fn load_project(&self, root: &Path, config: Config) -> Result<Project, String> {
        project::load(root, config)
    }

    fn defines_dir(&self, file_name: &str) -> bool {
        file_name
            .rsplit_once('.')
            .is_some_and(|(stem, _)| stem == "index")
    }

    fn resolve(&self, index: &Index, file: &FileIndex, segs: &[String], anchored: bool) -> Target {
        resolve::resolve(index, file, segs, anchored)
    }

    fn is_builtin(&self, name: &str) -> bool {
        resolve::is_builtin(name)
    }

    fn module_name(&self, module: &[String]) -> String {
        if module.is_empty() {
            "(root)".to_string()
        } else {
            module.join("/")
        }
    }

    fn group_name(&self, group: &[String]) -> String {
        self.module_name(group)
    }

    fn qualify(&self, owner: &str, name: &str) -> String {
        format!("{owner}.{name}")
    }

    fn full_name(&self, module: &[String], qualified: &str) -> String {
        match module.is_empty() {
            true => qualified.to_string(),
            false => format!("{}/{qualified}", module.join("/")),
        }
    }

    fn split_query(&self, query: &str) -> Vec<String> {
        query
            .replace("::", "/")
            .split(['/', '.'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn qualify_signature(&self, signature: &str, owner: &str, name: &str) -> String {
        let qualified = self.qualify(owner, name);
        let at = [format!("{name}("), format!("{name}<")]
            .iter()
            .find_map(|needle| signature.find(needle.as_str()));
        match at {
            Some(at) => format!(
                "{}{qualified}{}",
                &signature[..at],
                &signature[at + name.len()..]
            ),
            None => signature.to_string(),
        }
    }

    fn grep_types(&self) -> &'static [&'static str] {
        &["ts", "js", "typescript", "javascript", "tsx", "jsx"]
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
        let Some(tree) = grammar::parse_tree(source) else {
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

#[cfg(test)]
mod tests;
