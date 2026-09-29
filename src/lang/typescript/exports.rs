//! Re-exports: `export { a, b as c } from './x'` and `export * from './x'`.

use tree_sitter::Node;

use super::items::{Site, push};
use super::syntax::{collapse, text};
use crate::model::{FileIndex, ItemKind};

/// `export { a, b as c } from './x'` and `export * from './x'`: one item per exported name.
pub fn re_export(node: Node, src: &str, out: &mut FileIndex, site: &Site) {
    let from = node
        .child_by_field_name("source")
        .map(|s| text(s, src).to_string())
        .unwrap_or_default();
    let mut names: Vec<(String, String)> = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "export_clause" => {
                let mut inner = child.walk();
                for spec in child.named_children(&mut inner) {
                    let Some(name) = spec.child_by_field_name("name") else {
                        continue;
                    };
                    let name = text(name, src);
                    let alias = spec.child_by_field_name("alias").map(|a| text(a, src));
                    let exported = alias.unwrap_or(name);
                    let sig = match alias {
                        Some(a) => format!("export {{ {name} as {a} }} from {from}"),
                        None => format!("export {{ {name} }} from {from}"),
                    };
                    names.push((exported.to_string(), sig));
                }
            }
            "namespace_export" => {
                let alias = collapse(text(child, src));
                let alias = alias
                    .trim_start_matches('*')
                    .trim_start_matches(" as ")
                    .trim();
                names.push((
                    alias.to_string(),
                    format!("export * as {alias} from {from}"),
                ));
            }
            _ => {}
        }
    }
    if names.is_empty() {
        names.push(("*".to_string(), format!("export * from {from}")));
    }
    for (name, signature) in names {
        push(
            out,
            site,
            node,
            ItemKind::Use,
            name,
            signature,
            Vec::new(),
            None,
        );
    }
}
