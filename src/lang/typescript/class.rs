//! Classes: the class item with its fields, `implements` clauses and one item per method.

use tree_sitter::Node;

use super::items::{Site, name_of, push, signature};
use super::syntax::{JsDoc, collapse, declaration_text, jsdoc, start_after_decorators, text};
use crate::model::{FileIndex, ItemKind, TraitImpl, Vis};

/// Adds the class, its `implements` clauses and its methods to `out`.
pub fn class(node: Node, src: &str, out: &mut FileIndex, site: &Site, name: String) {
    let body = node.child_by_field_name("body");
    let fields = body.map(|b| class_fields(b, src)).unwrap_or_default();
    push(
        out,
        site,
        node,
        ItemKind::Class,
        name.clone(),
        signature(node, src),
        fields,
        None,
    );
    let mut cursor = node.walk();
    for heritage in node
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "class_heritage")
    {
        let mut inner = heritage.walk();
        for clause in heritage
            .named_children(&mut inner)
            .filter(|c| c.kind() == "implements_clause")
        {
            let mut names = clause.walk();
            for ty in clause.named_children(&mut names) {
                out.trait_impls.push(TraitImpl {
                    ty: name.clone(),
                    trait_name: collapse(text(ty, src))
                        .split('<')
                        .next()
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
    }
    let Some(body) = body else { return };
    let mut pending: Option<JsDoc> = None;
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() == "comment" {
            pending = jsdoc(member, src).or(pending);
            continue;
        }
        let doc = pending
            .take()
            .filter(|d| member.start_position().row <= d.end_row + 1);
        if !matches!(
            member.kind(),
            "method_definition" | "abstract_method_signature" | "method_signature"
        ) {
            continue;
        }
        let vis = match modifier(member, src) {
            "private" => Vis::Private,
            "protected" => Vis::Restricted,
            _ if member
                .child_by_field_name("name")
                .is_some_and(|n| n.kind() == "private_property_identifier") =>
            {
                Vis::Private
            }
            _ => Vis::Pub,
        };
        let inner = Site {
            span: member,
            vis,
            doc: doc.as_ref(),
        };
        let sig = signature(member, src);
        let sig = sig.strip_prefix("public ").unwrap_or(&sig).to_string();
        let method = name_of(member, src);
        push(
            out,
            &inner,
            member,
            ItemKind::Fn,
            method,
            sig,
            Vec::new(),
            Some(name.clone()),
        );
    }
}

fn modifier<'a>(member: Node, src: &'a str) -> &'a str {
    let mut cursor = member.walk();
    member
        .children(&mut cursor)
        .find(|c| c.kind() == "accessibility_modifier")
        .map_or("", |m| text(m, src))
}

fn class_fields(body: Node, src: &str) -> Vec<String> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|c| c.kind() == "public_field_definition")
        .map(|c| {
            let end = c
                .child_by_field_name("value")
                .map_or(c.end_byte(), |v| v.start_byte());
            declaration_text(src, start_after_decorators(c), end)
                .trim_end_matches('=')
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Interface members or enum variants, one collapsed line each.
pub fn members(node: Node, src: &str, kinds: &[&str]) -> Vec<String> {
    let Some(body) = node.child_by_field_name("body") else {
        return Vec::new();
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|c| kinds.contains(&c.kind()))
        .map(|c| declaration_text(src, c.start_byte(), c.end_byte()))
        .collect()
}
