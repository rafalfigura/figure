//! Items, doc comments, tests and trait impls: a walk over item containers
//! (the file, inline `mod` bodies, `impl` and `trait` bodies).

use tree_sitter::Node;

use super::syntax::{
    Attrs, Comment, attr_inner, classify_comment, collapse, line, path_segments, text,
};
use crate::model::{FileIndex, Item, ItemKind, TestFn, TraitImpl, Vis};

#[derive(Clone, Default)]
struct Ctx {
    /// Type of the enclosing `impl`.
    owner: Option<String>,
    /// Inside `impl Trait for Type`: methods are documented by the trait, not listed.
    trait_impl: bool,
    /// Inside a `#[cfg(test)]` module: only test functions are collected.
    in_test: bool,
    /// The file's top level, where `//!` documents the module.
    top: bool,
}

/// Doc comments and attributes seen since the last item.
#[derive(Default)]
struct Pending {
    docs: Vec<String>,
    attrs: Vec<String>,
    start_row: Option<usize>,
}

impl Pending {
    fn note(&mut self, node: Node) {
        self.start_row.get_or_insert(node.start_position().row);
    }
}

/// Collects items, docs, tests and trait impls of the file rooted at `root`.
pub fn walk(root: Node, src: &str, out: &mut FileIndex) {
    let ctx = Ctx {
        top: true,
        ..Ctx::default()
    };
    walk_container(root, src, out, &ctx);
}

fn walk_container(node: Node, src: &str, out: &mut FileIndex, ctx: &Ctx) {
    let mut pending = Pending::default();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "line_comment" | "block_comment" => match classify_comment(text(child, src)) {
                Comment::Inner(lines) if ctx.top => {
                    if out.module_doc_line == 0 {
                        out.module_doc_line = line(child);
                    }
                    out.module_doc.extend(lines);
                }
                Comment::Outer(lines) => {
                    pending.note(child);
                    pending.docs.extend(lines);
                }
                _ => {}
            },
            "attribute_item" => {
                pending.note(child);
                pending.attrs.push(attr_inner(text(child, src)));
            }
            "inner_attribute_item" => {}
            _ => {
                item(child, src, out, ctx, &pending);
                pending = Pending::default();
            }
        }
    }
}

fn item(node: Node, src: &str, out: &mut FileIndex, ctx: &Ctx, pending: &Pending) {
    let attrs = Attrs::from_inner(&pending.attrs);
    let in_test = ctx.in_test || attrs.cfg_test;
    let kind = match node.kind() {
        "function_item" | "function_signature_item" => {
            if attrs.is_test {
                out.tests.push(TestFn {
                    name: name_of(node, src),
                    doc: pending.docs.clone(),
                });
                return;
            }
            if ctx.trait_impl {
                return;
            }
            ItemKind::Fn
        }
        "struct_item" => ItemKind::Struct,
        "enum_item" => ItemKind::Enum,
        "union_item" => ItemKind::Union,
        "trait_item" => ItemKind::Trait,
        "type_item" => ItemKind::Type,
        "const_item" => ItemKind::Const,
        "static_item" => ItemKind::Static,
        "macro_definition" => ItemKind::Macro,
        "use_declaration" => ItemKind::Use,
        "mod_item" => {
            if let Some(body) = node.child_by_field_name("body") {
                let inner = Ctx {
                    in_test,
                    ..Ctx::default()
                };
                walk_container(body, src, out, &inner);
            }
            return;
        }
        "impl_item" => {
            impl_block(node, src, out, ctx, in_test);
            return;
        }
        _ => return,
    };
    if ctx.in_test {
        return;
    }
    let vis = visibility(node, src);
    if kind == ItemKind::Use && vis == Vis::Private {
        return;
    }
    let start_row = pending.start_row.unwrap_or(node.start_position().row);
    let base = Item {
        kind,
        name: name_of(node, src),
        vis,
        signature: signature(node, src, kind),
        doc: pending.docs.clone(),
        start_line: start_row + 1,
        decl_line: line(node),
        end_line: node.end_position().row + 1,
        fields: members(node, src),
        derives: attrs.derives,
        owner: ctx.owner.clone(),
        hidden: attrs.hidden,
        test_only: attrs.cfg_test,
    };
    if kind != ItemKind::Use {
        out.items.push(base);
        return;
    }
    // One item per re-exported name: `pub use a::{B, C}` exports `B` and `C`.
    let mut paths = Vec::new();
    if let Some(arg) = node.child_by_field_name("argument") {
        super::refs::flatten_use(arg, src, &[], &mut paths);
    }
    for (segs, glob, alias) in paths {
        let name = if glob {
            "*".to_string()
        } else {
            alias.or_else(|| segs.last().cloned()).unwrap_or_default()
        };
        let star = if glob { "::*" } else { "" };
        out.items.push(Item {
            name,
            signature: format!("use {}{star}", segs.join("::")),
            ..base.clone()
        });
    }
}

fn impl_block(node: Node, src: &str, out: &mut FileIndex, ctx: &Ctx, in_test: bool) {
    let Some(ty) = node.child_by_field_name("type") else {
        return;
    };
    let ty = base_name(text(ty, src));
    let trait_name = node
        .child_by_field_name("trait")
        .map(|t| base_name(text(t, src)));
    if let Some(trait_name) = &trait_name
        && !in_test
    {
        out.trait_impls.push(TraitImpl {
            ty: ty.clone(),
            trait_name: trait_name.clone(),
        });
    }
    if let Some(body) = node.child_by_field_name("body") {
        let inner = Ctx {
            owner: Some(ty),
            trait_impl: trait_name.is_some(),
            in_test: ctx.in_test || in_test,
            top: false,
        };
        walk_container(body, src, out, &inner);
    }
}

/// `bevy::app::Plugin` -> `Plugin`, `Foo<T>` -> `Foo`.
fn base_name(s: &str) -> String {
    path_segments(s).pop().unwrap_or_default()
}

fn name_of(node: Node, src: &str) -> String {
    node.child_by_field_name("name")
        .map(|n| text(n, src).to_string())
        .unwrap_or_default()
}

fn visibility(node: Node, src: &str) -> Vis {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.kind() == "visibility_modifier");
    match found.map(|v| text(v, src).replace(char::is_whitespace, "")) {
        None => Vis::Private,
        Some(v) if v == "pub" => Vis::Pub,
        Some(_) => Vis::Restricted,
    }
}

/// Declaration text without its body: `fn run(time: Res<Time>)`, `struct Harm`.
pub fn signature(node: Node, src: &str, kind: ItemKind) -> String {
    let end = match node.child_by_field_name("body") {
        Some(body) if body.kind() != "ordered_field_declaration_list" => body.start_byte(),
        _ => node.end_byte(),
    };
    let mut sig = collapse(&src[node.start_byte()..end]);
    while sig.ends_with(';') || sig.ends_with('{') || sig.ends_with(' ') {
        sig.pop();
    }
    if matches!(kind, ItemKind::Const | ItemKind::Static)
        && sig.len() > 100
        && let Some((head, _)) = sig.split_once(" = ")
    {
        sig = head.to_string();
    }
    if kind == ItemKind::Macro {
        sig = format!("macro_rules! {}", name_of(node, src));
    }
    match sig.strip_prefix("pub ") {
        Some(rest) => rest.to_string(),
        None => sig,
    }
}

/// Struct fields, enum variants, or trait method signatures.
fn members(node: Node, src: &str) -> Vec<String> {
    let Some(body) = node.child_by_field_name("body") else {
        return Vec::new();
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter_map(|c| match c.kind() {
            "field_declaration" | "enum_variant" => Some(collapse(text(c, src))),
            "function_signature_item" | "function_item" => Some(signature(c, src, ItemKind::Fn)),
            "associated_type" => Some(collapse(text(c, src)).trim_end_matches(';').to_string()),
            _ => None,
        })
        .collect()
}
