//! CommonJS exports: `module.exports = ...` and `exports.name = ...` make items public.

use tree_sitter::Node;

use super::exports::Exports;
use super::items::{Site, push};
use super::syntax::{declaration_text, shorten, text};
use crate::model::{FileIndex, ItemKind, Vis};

/// Handles one top-level statement if it assigns to `module.exports` or `exports`. Names of
/// local declarations exported by reference (`module.exports = { a, b }`) go to `exports`.
pub fn statement(node: Node, src: &str, out: &mut FileIndex, site: &Site, exports: &mut Exports) {
    let Some(assign) = node
        .named_child(0)
        .filter(|a| a.kind() == "assignment_expression")
    else {
        return;
    };
    let Some(right) = assign.child_by_field_name("right") else {
        return;
    };
    if right.kind() == "assignment_expression" {
        // `exports = module.exports = create;`
        let mut inner = right;
        while let Some(next) = inner
            .child_by_field_name("right")
            .filter(|n| n.kind() == "assignment_expression")
        {
            inner = next;
        }
        return assignment(node, inner, src, out, site, exports);
    }
    assignment(node, assign, src, out, site, exports)
}

/// One `left = right` assignment, `node` being the statement that holds it.
fn assignment(
    node: Node,
    assign: Node,
    src: &str,
    out: &mut FileIndex,
    site: &Site,
    exports: &mut Exports,
) {
    let (Some(left), Some(right)) = (
        assign.child_by_field_name("left"),
        assign.child_by_field_name("right"),
    ) else {
        return;
    };
    let target: String = text(left, src).split_whitespace().collect();
    match target.as_str() {
        "module.exports" => match right.kind() {
            "object" => {
                let mut cursor = right.walk();
                for member in right.named_children(&mut cursor) {
                    let name = match member.kind() {
                        "shorthand_property_identifier" => Some(member),
                        "pair" => member
                            .child_by_field_name("value")
                            .filter(|v| v.kind() == "identifier"),
                        _ => None,
                    };
                    if let Some(n) = name {
                        exports.add(text(n, src));
                    }
                }
            }
            "identifier" => exports.add(text(right, src)),
            _ => define(node, right, src, out, site, "default", "module.exports"),
        },
        t => {
            let name = t
                .strip_prefix("module.exports.")
                .or_else(|| t.strip_prefix("exports."))
                .filter(|n| !n.is_empty() && !n.contains('.'));
            match (name, right.kind()) {
                (Some(_), "identifier") if text(right, src) == name.unwrap_or_default() => {
                    exports.add(text(right, src));
                }
                (Some(name), _) => define(node, right, src, out, site, name, t),
                _ => method(node, right, left, src, out, site),
            }
        }
    }
}

/// One public item for `target = value`.
fn define(
    node: Node,
    value: Node,
    src: &str,
    out: &mut FileIndex,
    site: &Site,
    name: &str,
    target: &str,
) {
    let (kind, end) = match value.kind() {
        "arrow_function" | "function_expression" | "function" | "generator_function" => (
            ItemKind::Fn,
            value.child_by_field_name("body").map(|b| b.start_byte()),
        ),
        "class" => (
            ItemKind::Class,
            value.child_by_field_name("body").map(|b| b.start_byte()),
        ),
        _ => (ItemKind::Const, None),
    };
    let sig = declaration_text(src, value.start_byte(), end.unwrap_or(value.end_byte()));
    let sig = shorten(format!("{target} = {sig}"));
    push(
        out,
        site,
        node,
        kind,
        name.to_string(),
        sig,
        Vec::new(),
        None,
    );
}

/// `res.send = function send(..)` and `Route.prototype.dispatch = function(..)`: a method of the
/// top-level object `res` / `Route`, public once that object is exported.
fn method(node: Node, value: Node, left: Node, src: &str, out: &mut FileIndex, site: &Site) {
    if !matches!(
        value.kind(),
        "arrow_function" | "function_expression" | "function"
    ) {
        return;
    }
    let target: String = text(left, src).split_whitespace().collect();
    let parts: Vec<&str> = target.split('.').collect();
    let (owner, name) = match parts.as_slice() {
        [owner, name] | [owner, "prototype", name] if *owner != "exports" && *owner != "module" => {
            (*owner, *name)
        }
        _ => return,
    };
    let params = value
        .child_by_field_name("parameters")
        .or_else(|| value.child_by_field_name("parameter"))
        .map_or(value.start_byte(), |p| p.start_byte());
    let end = value
        .child_by_field_name("body")
        .map_or(value.end_byte(), |b| b.start_byte());
    let asynchronous = if text(value, src).starts_with("async") {
        "async "
    } else {
        ""
    };
    let sig = format!("{asynchronous}{name}{}", declaration_text(src, params, end));
    let public = Site {
        span: site.span,
        vis: Vis::Pub,
        doc: site.doc,
    };
    push(
        out,
        &public,
        node,
        ItemKind::Fn,
        name.to_string(),
        sig,
        Vec::new(),
        Some(owner.to_string()),
    );
}

/// `var app = exports = module.exports = {};`: the declared name is what the module exports.
pub fn declaration(node: Node, src: &str, exports: &mut Exports) {
    let mut cursor = node.walk();
    for decl in node.named_children(&mut cursor) {
        let (Some(name), Some(value)) = (
            decl.child_by_field_name("name"),
            decl.child_by_field_name("value"),
        ) else {
            continue;
        };
        let mut chain = value;
        while chain.kind() == "assignment_expression" {
            let left: String = chain
                .child_by_field_name("left")
                .map(|l| text(l, src).split_whitespace().collect())
                .unwrap_or_default();
            if left == "exports" || left == "module.exports" {
                exports.add(text(name, src));
                break;
            }
            let Some(next) = chain.child_by_field_name("right") else {
                break;
            };
            chain = next;
        }
    }
}
