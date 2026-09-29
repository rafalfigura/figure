//! Imports as references: `import`, `export ... from`, `require()` and `import()` with a
//! string argument. Relative specifiers become `crate::<module path>` so the language-neutral
//! resolver can follow them; bare specifiers become the package name.

use tree_sitter::Node;

use super::syntax::{line, text};
use crate::model::{FileIndex, PathRef};

/// Extensions that name a source file and are dropped from a specifier.
pub const SOURCE_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Files a specifier can name that are not modules.
const ASSET_EXTENSIONS: [&str; 12] = [
    "css", "scss", "sass", "less", "json", "svg", "png", "jpg", "jpeg", "gif", "webp", "html",
];

/// Collects every import of the file. `dir` is the file's directory below the source root.
pub fn walk(root: Node, src: &str, dir: &[String], out: &mut FileIndex) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "import_statement" => import(node, src, dir, out),
            "export_statement" => {
                if let Some(source) = node.child_by_field_name("source") {
                    push(out, node, src, dir, source, None);
                }
            }
            "call_expression" => dynamic(node, src, dir, out),
            _ => {}
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    out.refs.sort_by_key(|r| r.line);
}

fn import(node: Node, src: &str, dir: &[String], out: &mut FileIndex) {
    let Some(source) = node.child_by_field_name("source") else {
        return;
    };
    let mut names = Vec::new();
    let mut whole_module = false;
    let mut cursor = node.walk();
    for clause in node.named_children(&mut cursor) {
        if clause.kind() != "import_clause" {
            continue;
        }
        let mut inner = clause.walk();
        for part in clause.named_children(&mut inner) {
            if part.kind() != "named_imports" {
                whole_module = true;
                continue;
            }
            let mut specs = part.walk();
            for spec in part.named_children(&mut specs) {
                if let Some(name) = spec.child_by_field_name("name") {
                    names.push(text(name, src).to_string());
                }
            }
        }
    }
    if names.is_empty() || whole_module {
        push(out, node, src, dir, source, None);
    }
    for name in names {
        push(out, node, src, dir, source, Some(name));
    }
}

fn dynamic(node: Node, src: &str, dir: &[String], out: &mut FileIndex) {
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };
    if func.kind() != "import" && text(func, src) != "require" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    if let Some(first) = args.named_child(0).filter(|a| a.kind() == "string") {
        push(out, node, src, dir, first, None);
    }
}

fn push(
    out: &mut FileIndex,
    at: Node,
    src: &str,
    dir: &[String],
    literal: Node,
    name: Option<String>,
) {
    let spec = text(literal, src).trim_matches(['"', '\'', '`']);
    let Some(mut segments) = specifier(spec, dir) else {
        return;
    };
    segments.extend(name);
    out.refs.push(PathRef {
        segments,
        glob: false,
        line: line(at),
        in_test: false,
        owner: None,
    });
}

/// The path segments a module specifier stands for, `None` when it points nowhere useful.
/// `./x` in `a/b.ts` -> `crate::a::x`; `@scope/pkg/sub` -> `@scope/pkg`, `sub`.
pub fn specifier(spec: &str, dir: &[String]) -> Option<Vec<String>> {
    if !spec.starts_with('.') {
        let spec = spec.strip_prefix("node:").unwrap_or(spec);
        let mut parts: Vec<String> = spec
            .split('/')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect();
        // `@scope/pkg` is one name; `@/x` (an alias) is not.
        if parts
            .first()
            .is_some_and(|f| f.len() > 1 && f.starts_with('@'))
            && parts.len() > 1
        {
            let name = format!("{}/{}", parts[0], parts[1]);
            parts.splice(..2, [name]);
        }
        return (!parts.is_empty()).then_some(parts);
    }
    let mut path: Vec<String> = dir.to_vec();
    for part in spec.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                path.pop()?;
            }
            p => path.push(p.to_string()),
        }
    }
    if let Some(last) = path.last_mut()
        && let Some((stem, ext)) = last.rsplit_once('.')
    {
        if ASSET_EXTENSIONS.contains(&ext) {
            return None;
        }
        if SOURCE_EXTENSIONS.contains(&ext) {
            *last = stem.to_string();
        }
    }
    if path.last().is_some_and(|l| l == "index") {
        path.pop();
    }
    let mut segments = vec!["crate".to_string()];
    segments.extend(path);
    Some(segments)
}
