//! Paths and calls: a walk over the whole tree collecting every `use`, every
//! qualified path (`crate::a::b()`, `super::shared::Harm`) and every call.

use tree_sitter::Node;

use super::syntax::{attr_inner, is_cfg_test, line, path_segments, squeeze, text};
use crate::model::{Call, FileIndex, Owner, PathRef};

#[derive(Clone, Default)]
struct St {
    owner: Option<Owner>,
    impl_ty: Option<String>,
    in_test: bool,
}

/// Collects every use path, qualified path and call in the file.
pub fn walk(root: Node, src: &str, out: &mut FileIndex) {
    visit_children(root, src, out, &St::default());
}

fn visit_children(node: Node, src: &str, out: &mut FileIndex, st: &St) {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    let mut cfg_test = false;
    for child in children {
        match child.kind() {
            "attribute_item" => {
                cfg_test |= is_cfg_test(&squeeze(&attr_inner(text(child, src))));
            }
            "line_comment" | "block_comment" => {}
            _ => {
                if cfg_test {
                    let st = St {
                        in_test: true,
                        ..st.clone()
                    };
                    visit(child, src, out, &st);
                } else {
                    visit(child, src, out, st);
                }
                cfg_test = false;
            }
        }
    }
}

fn visit(node: Node, src: &str, out: &mut FileIndex, st: &St) {
    match node.kind() {
        "use_declaration" => {
            if let Some(arg) = node.child_by_field_name("argument") {
                let mut paths = Vec::new();
                flatten_use(arg, src, &[], &mut paths);
                for (segments, glob, _) in paths {
                    out.refs.push(PathRef {
                        segments,
                        glob,
                        anchored: false,
                        line: line(node),
                        in_test: st.in_test,
                        owner: st.owner.clone(),
                    });
                }
            }
        }
        "function_item" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| text(n, src).to_string())
                .unwrap_or_default();
            let st = St {
                owner: Some(Owner {
                    ty: st.impl_ty.clone(),
                    name,
                }),
                ..st.clone()
            };
            visit_children(node, src, out, &st);
        }
        "impl_item" => {
            let ty = node
                .child_by_field_name("type")
                .and_then(|t| path_segments(text(t, src)).pop());
            let st = St {
                impl_ty: ty,
                ..st.clone()
            };
            visit_children(node, src, out, &st);
        }
        "mod_item" => {
            let st = St {
                owner: None,
                impl_ty: None,
                in_test: st.in_test,
            };
            visit_children(node, src, out, &st);
        }
        "call_expression" => {
            record_call(node, src, out, st);
            visit_children(node, src, out, st);
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let segments = path_segments(text(node, src));
            if segments.len() > 1 {
                out.refs.push(PathRef {
                    segments,
                    glob: false,
                    anchored: false,
                    line: line(node),
                    in_test: st.in_test,
                    owner: st.owner.clone(),
                });
            }
        }
        _ => visit_children(node, src, out, st),
    }
}

fn record_call(node: Node, src: &str, out: &mut FileIndex, st: &St) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let (callee_node, generics) = if function.kind() == "generic_function" {
        let generics = function.child_by_field_name("type_arguments").map(|g| {
            let g = squeeze(text(g, src));
            g.trim_start_matches('<').trim_end_matches('>').to_string()
        });
        (
            function.child_by_field_name("function").unwrap_or(function),
            generics,
        )
    } else {
        (function, None)
    };
    let method = (callee_node.kind() == "field_expression")
        .then(|| callee_node.child_by_field_name("field"))
        .flatten()
        .map(|f| text(f, src).to_string());
    let line = (callee_node.kind() == "field_expression")
        .then(|| callee_node.child_by_field_name("field"))
        .flatten()
        .map_or_else(|| super::syntax::line(node), super::syntax::line);
    let callee = match &method {
        Some(m) => m.clone(),
        None => squeeze(text(function, src)),
    };
    let args = node
        .child_by_field_name("arguments")
        .map(|a| {
            let mut cursor = a.walk();
            a.named_children(&mut cursor)
                .filter(|c| !c.kind().ends_with("comment"))
                .map(|c| squeeze_expr(text(c, src)))
                .collect()
        })
        .unwrap_or_default();
    out.calls.push(Call {
        callee,
        method,
        generics,
        args,
        line,
        in_test: st.in_test,
        owner: st.owner.clone(),
    });
}

/// Collapses an argument expression onto one line without spaces around `.`/`::`.
fn squeeze_expr(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    joined
        .replace(" .", ".")
        .replace(". ", ".")
        .replace(" ::", "::")
}

/// A flattened use path: segments, whether it ends in `*`, and its `as` alias.
pub(super) type UsePath = (Vec<String>, bool, Option<String>);

/// Expands a use tree into full paths: `a::{b, c::{d, e}}` -> `a::b`, `a::c::d`, `a::c::e`.
pub(super) fn flatten_use(node: Node, src: &str, prefix: &[String], out: &mut Vec<UsePath>) {
    let join = |extra: Vec<String>| -> Vec<String> {
        let mut p = prefix.to_vec();
        p.extend(extra);
        p
    };
    match node.kind() {
        "use_as_clause" => {
            if let Some(path) = node.child_by_field_name("path") {
                let alias = node
                    .child_by_field_name("alias")
                    .map(|a| text(a, src).to_string());
                let mut inner = Vec::new();
                flatten_use(path, src, prefix, &mut inner);
                out.extend(
                    inner
                        .into_iter()
                        .map(|(segs, glob, _)| (segs, glob, alias.clone())),
                );
            }
        }
        "use_wildcard" => {
            let t = squeeze(text(node, src));
            let t = t.trim_end_matches('*').trim_end_matches("::");
            out.push((join(path_segments(t)), true, None));
        }
        "scoped_use_list" => {
            let base = node
                .child_by_field_name("path")
                .map(|p| join(path_segments(text(p, src))))
                .unwrap_or_else(|| prefix.to_vec());
            if let Some(list) = node.child_by_field_name("list") {
                flatten_use(list, src, &base, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                flatten_use(child, src, prefix, out);
            }
        }
        "self" if !prefix.is_empty() => out.push((prefix.to_vec(), false, None)),
        "line_comment" | "block_comment" => {}
        _ => out.push((join(path_segments(text(node, src))), false, None)),
    }
}
