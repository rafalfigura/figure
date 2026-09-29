//! Items and doc comments: functions, classes (with their methods), interfaces, enums, type
//! aliases, constants and re-exports. `export` makes an item public.

use tree_sitter::Node;

use super::class;
use super::exports::Exports;
use super::syntax::{JsDoc, declaration_text, jsdoc, line, shorten, start_after_decorators, text};
use crate::model::{FileIndex, Item, ItemKind, Vis};

/// Where a declaration sits: the statement that spans it and how it is exported.
pub struct Site<'a> {
    /// The `export` statement, or the declaration itself.
    pub span: Node<'a>,
    pub vis: Vis,
    pub doc: Option<&'a JsDoc>,
}

/// Collects items, docs and `implements` clauses of the file rooted at `root`.
pub fn walk(root: Node, src: &str, out: &mut FileIndex) {
    let mut pending: Option<JsDoc> = None;
    let mut seen_statement = false;
    let mut exports = Exports::default();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "hash_bang_line" => {}
            "comment" => {
                let Some(doc) = jsdoc(child, src) else {
                    continue;
                };
                if !seen_statement
                    && let Some(prev) = pending.take_if(|p| is_module_doc(p, doc.start_row))
                {
                    set_module_doc(out, prev);
                }
                pending = Some(doc);
            }
            _ => {
                if !seen_statement
                    && let Some(prev) =
                        pending.take_if(|p| is_module_doc(p, child.start_position().row))
                {
                    set_module_doc(out, prev);
                }
                seen_statement = true;
                let doc = pending
                    .take()
                    .filter(|d| child.start_position().row <= d.end_row + 1);
                statement(child, src, out, doc.as_ref(), &mut exports);
            }
        }
    }
    exports.apply(&mut out.items);
}

/// A leading comment documents the file when tagged so or when a blank line follows it.
fn is_module_doc(doc: &JsDoc, next_row: usize) -> bool {
    doc.module || next_row > doc.end_row + 1
}

fn set_module_doc(out: &mut FileIndex, doc: JsDoc) {
    if out.module_doc_line == 0 {
        out.module_doc_line = doc.start_row + 1;
        out.module_doc = doc.lines;
    }
}

fn statement(
    node: Node,
    src: &str,
    out: &mut FileIndex,
    doc: Option<&JsDoc>,
    exports: &mut Exports,
) {
    if node.kind() == "expression_statement" {
        let site = Site {
            span: node,
            vis: Vis::Pub,
            doc,
        };
        return super::commonjs::statement(node, src, out, &site, exports);
    }
    if node.kind() != "export_statement" {
        if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
            super::commonjs::declaration(node, src, exports);
        }
        let site = Site {
            span: node,
            vis: Vis::Private,
            doc,
        };
        return declaration(node, src, out, &site);
    }
    let site = Site {
        span: node,
        vis: Vis::Pub,
        doc,
    };
    if let Some(decl) = node.child_by_field_name("declaration") {
        return declaration(decl, src, out, &site);
    }
    if node.child_by_field_name("source").is_some() {
        return super::exports::re_export(node, src, out, &site);
    }
    let mut cursor = node.walk();
    for clause in node.named_children(&mut cursor) {
        if clause.kind() != "export_clause" {
            continue;
        }
        exports.add_clause(clause, src);
    }
}

fn declaration(node: Node, src: &str, out: &mut FileIndex, site: &Site) {
    let name = name_of(node, src);
    match node.kind() {
        "ambient_declaration" => {
            let mut cursor = node.walk();
            if let Some(inner) = node
                .named_children(&mut cursor)
                .find(|c| c.kind() != "comment")
            {
                declaration(inner, src, out, site);
            }
        }
        "function_declaration" | "generator_function_declaration" | "function_signature" => {
            let sig = signature(node, src);
            push(out, site, node, ItemKind::Fn, name, sig, Vec::new(), None);
        }
        "class_declaration" | "abstract_class_declaration" => {
            class::class(node, src, out, site, name)
        }
        "interface_declaration" => {
            let fields = class::members(
                node,
                src,
                &[
                    "property_signature",
                    "method_signature",
                    "call_signature",
                    "construct_signature",
                    "index_signature",
                ],
            );
            let sig = signature(node, src);
            push(
                out,
                site,
                node,
                ItemKind::Interface,
                name,
                sig,
                fields,
                None,
            );
        }
        "type_alias_declaration" => {
            let sig = shorten(declaration_text(src, node.start_byte(), node.end_byte()));
            push(out, site, node, ItemKind::Type, name, sig, Vec::new(), None);
        }
        "enum_declaration" => {
            let fields = class::members(
                node,
                src,
                &["property_identifier", "string", "enum_assignment"],
            );
            let sig = signature(node, src);
            push(out, site, node, ItemKind::Enum, name, sig, fields, None);
        }
        "lexical_declaration" | "variable_declaration" => variables(node, src, out, site),
        _ => {}
    }
}

/// `const a = 1, f = () => {}`: a function for arrow and function values, a constant otherwise.
fn variables(node: Node, src: &str, out: &mut FileIndex, site: &Site) {
    let keyword = text(node, src).split_whitespace().next().unwrap_or("const");
    let mut cursor = node.walk();
    for decl in node.named_children(&mut cursor) {
        let (Some(name), "variable_declarator") = (decl.child_by_field_name("name"), decl.kind())
        else {
            continue;
        };
        if name.kind() != "identifier" {
            continue; // destructuring: `const { a } = require(..)`
        }
        let value = decl.child_by_field_name("value");
        let is_fn = value.is_some_and(|v| {
            matches!(
                v.kind(),
                "arrow_function" | "function_expression" | "function"
            )
        });
        let (kind, end) = match value.and_then(|v| v.child_by_field_name("body")) {
            Some(body) if is_fn => (ItemKind::Fn, body.start_byte()),
            _ => (ItemKind::Const, decl.end_byte()),
        };
        let sig = declaration_text(src, decl.start_byte(), end);
        let sig = shorten(format!("{keyword} {sig}"));
        let name = text(name, src).to_string();
        push(out, site, node, kind, name, sig, Vec::new(), None);
    }
}

/// The declared name, `default` for anonymous default exports.
pub fn name_of(node: Node, src: &str) -> String {
    node.child_by_field_name("name")
        .map_or_else(|| "default".to_string(), |n| text(n, src).to_string())
}

/// Declaration text without its body: `function add(a: number): number`, `class A extends B`.
pub fn signature(node: Node, src: &str) -> String {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    declaration_text(src, start_after_decorators(node), end)
}

/// Appends one item spanning `site`, documented by the site's JSDoc.
#[allow(clippy::too_many_arguments)]
pub fn push(
    out: &mut FileIndex,
    site: &Site,
    decl: Node,
    kind: ItemKind,
    name: String,
    signature: String,
    fields: Vec<String>,
    owner: Option<String>,
) {
    let start_row = site
        .doc
        .map_or(site.span.start_position().row, |d| d.start_row);
    out.items.push(Item {
        kind,
        name,
        vis: site.vis,
        signature,
        doc: site.doc.map(|d| d.lines.clone()).unwrap_or_default(),
        start_line: start_row + 1,
        decl_line: line(decl),
        end_line: decl.end_position().row.max(site.span.end_position().row) + 1,
        fields,
        derives: Vec::new(),
        owner,
        hidden: site.doc.is_some_and(|d| d.hidden),
        test_only: false,
    });
}
