//! Small helpers over tree-sitter nodes and Rust comment/attribute text.

use tree_sitter::Node;

/// Source text of a node.
pub fn text<'a>(node: Node, src: &'a str) -> &'a str {
    &src[node.byte_range()]
}

/// 1-based line of the node's first byte.
pub fn line(node: Node) -> usize {
    node.start_position().row + 1
}

/// Collapses every whitespace run to one space.
pub fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Removes all whitespace: `super :: a` -> `super::a`.
pub fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Splits a path into segments, dropping turbofish and generic arguments:
/// `super::register::<T>` -> `["super", "register"]`, `Vec<u8>` -> `["Vec"]`.
pub fn path_segments(path: &str) -> Vec<String> {
    let path = strip_generics(&squeeze(path));
    path.split("::")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Removes every `<...>` group, nesting included.
pub fn strip_generics(s: &str) -> String {
    let mut depth = 0usize;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// What kind of comment a comment node is.
pub enum Comment {
    /// `//!` or `/*! */`: documents the enclosing module.
    Inner(Vec<String>),
    /// `///` or `/** */`: documents the next item.
    Outer(Vec<String>),
    Plain,
}

/// Classifies a comment and strips its markers.
pub fn classify_comment(raw: &str) -> Comment {
    let raw = raw.trim_end_matches(['\n', '\r']);
    if let Some(rest) = raw.strip_prefix("//!") {
        return Comment::Inner(vec![strip_one_space(rest)]);
    }
    if raw.starts_with("///") && !raw.starts_with("////") {
        return Comment::Outer(vec![strip_one_space(&raw[3..])]);
    }
    if let Some(body) = raw.strip_prefix("/*!") {
        return Comment::Inner(block_lines(body));
    }
    if raw.starts_with("/**") && !raw.starts_with("/***") && raw != "/**/" {
        return Comment::Outer(block_lines(&raw[3..]));
    }
    Comment::Plain
}

fn strip_one_space(s: &str) -> String {
    s.strip_prefix(' ').unwrap_or(s).trim_end().to_string()
}

fn block_lines(body: &str) -> Vec<String> {
    let body = body.strip_suffix("*/").unwrap_or(body);
    body.lines()
        .map(|l| {
            let t = l.trim_start();
            strip_one_space(t.strip_prefix('*').unwrap_or(t))
        })
        .skip_while(|l| l.is_empty())
        .collect()
}

/// What a run of attributes says about the item after them.
#[derive(Default, Debug)]
pub struct Attrs {
    pub cfg_test: bool,
    pub is_test: bool,
    pub hidden: bool,
    pub derives: Vec<String>,
}

impl Attrs {
    /// `inner` is the text inside `#[...]`.
    pub fn from_inner(list: &[String]) -> Attrs {
        let mut attrs = Attrs::default();
        for raw in list {
            let a = squeeze(raw);
            if is_cfg_test(&a) {
                attrs.cfg_test = true;
            }
            if a == "test" || a.ends_with("::test") || a.starts_with("test(") {
                attrs.is_test = true;
            }
            if a.contains("doc(hidden)") {
                attrs.hidden = true;
            }
            if let Some(list) = a.strip_prefix("derive(").and_then(|r| r.strip_suffix(')')) {
                attrs.derives.extend(
                    list.split(',')
                        .filter(|d| !d.is_empty())
                        .map(|d| d.rsplit("::").next().unwrap_or(d).to_string()),
                );
            }
        }
        attrs
    }
}

/// `#[...]` -> the text inside the brackets.
pub fn attr_inner(raw: &str) -> String {
    let t = raw.trim();
    let t = t.strip_prefix("#[").unwrap_or(t);
    t.strip_suffix(']').unwrap_or(t).to_string()
}

/// True for `cfg(test)` and `cfg(all(test, ..))`, given the text with whitespace removed.
pub fn is_cfg_test(squeezed_inner: &str) -> bool {
    squeezed_inner == "cfg(test)" || squeezed_inner.starts_with("cfg(all(test")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turbofish and generic arguments never become path segments.
    #[test]
    fn segments_drop_generics() {
        assert_eq!(
            path_segments("super::register::<Spikes>"),
            ["super", "register"]
        );
        assert_eq!(path_segments("crate::a::B<Vec<u8>>"), ["crate", "a", "B"]);
        assert_eq!(path_segments("crate :: a\n :: b"), ["crate", "a", "b"]);
    }

    /// Doc comments are told apart from plain comments by their marker.
    #[test]
    fn comments_classified() {
        assert!(matches!(classify_comment("//! Module."), Comment::Inner(l) if l == ["Module."]));
        assert!(matches!(classify_comment("/// Item.\n"), Comment::Outer(l) if l == ["Item."]));
        assert!(matches!(classify_comment("//// divider"), Comment::Plain));
        assert!(matches!(classify_comment("// note"), Comment::Plain));
    }

    /// Derives keep only their last path segment.
    #[test]
    fn derives_parsed() {
        let a = Attrs::from_inner(&["derive(Component, serde::Deserialize)".into()]);
        assert_eq!(a.derives, ["Component", "Deserialize"]);
        assert!(Attrs::from_inner(&["cfg(test)".into()]).cfg_test);
        assert!(!Attrs::from_inner(&["cfg(not(test))".into()]).cfg_test);
    }
}
