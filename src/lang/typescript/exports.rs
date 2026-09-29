//! What a file exports. ESM `export` on a declaration is read where the declaration is; this
//! module collects the other forms (`export { a }`, `module.exports = { a }`, `exports.a = a`)
//! and decides, once, how visible every item and method ends up. Re-exports
//! (`export { a } from './x'`, `export * from './x'`) become items of their own.

use std::collections::HashMap;

use tree_sitter::Node;

use super::items::{Site, push};
use super::syntax::{collapse, text};
use crate::model::{FileIndex, Item, ItemKind, Vis};

/// Names a file exports somewhere other than at their declaration.
#[derive(Default)]
pub struct Exports {
    names: Vec<String>,
}

impl Exports {
    /// `name` is exported.
    pub fn add(&mut self, name: &str) {
        self.names.push(name.to_string());
    }

    /// The names of an `export { a, b as c }` clause: the local names are what get exported.
    pub fn add_clause(&mut self, clause: Node, src: &str) {
        let mut cursor = clause.walk();
        for spec in clause.named_children(&mut cursor) {
            if let Some(name) = spec.child_by_field_name("name") {
                self.add(text(name, src));
            }
        }
    }

    /// Sets the final visibility: exported names are public, and a method is as visible as the
    /// least visible of itself and its owner.
    pub fn apply(&self, items: &mut [Item]) {
        for item in items.iter_mut().filter(|i| i.owner.is_none()) {
            if self.names.contains(&item.name) {
                item.vis = Vis::Pub;
            }
        }
        let owners: HashMap<String, Vis> = items
            .iter()
            .filter(|i| i.owner.is_none())
            .map(|i| (i.name.clone(), i.vis))
            .collect();
        for item in items.iter_mut() {
            let Some(owner) = &item.owner else { continue };
            let owner_vis =
                owners
                    .get(owner)
                    .copied()
                    .unwrap_or(match self.names.contains(owner) {
                        true => Vis::Pub,
                        false => Vis::Private,
                    });
            item.vis = item.vis.min(owner_vis);
        }
    }
}

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
