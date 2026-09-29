//! Small helpers over tree-sitter nodes and JSDoc comment text.

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

/// A `/** ... */` comment with its markers stripped.
pub struct JsDoc {
    /// Prose lines; everything from the first block tag on is dropped.
    pub lines: Vec<String>,
    /// `@internal`, `@hidden` or `@ignore`: skipped by every command.
    pub hidden: bool,
    /// `@module`, `@fileoverview` or `@packageDocumentation`: documents the file.
    pub module: bool,
    /// Row of the comment's first line (0-based).
    pub start_row: usize,
    /// Row of the comment's last line (0-based).
    pub end_row: usize,
}

/// Parses a JSDoc comment node; `None` for other comments.
pub fn jsdoc(node: Node, src: &str) -> Option<JsDoc> {
    let raw = text(node, src);
    let body = raw.strip_prefix("/**")?;
    if raw.starts_with("/**/") || raw.starts_with("/***") {
        return None;
    }
    let body = body.strip_suffix("*/").unwrap_or(body);
    let mut doc = JsDoc {
        lines: Vec::new(),
        hidden: false,
        module: false,
        start_row: node.start_position().row,
        end_row: node.end_position().row,
    };
    let mut prose = true;
    for raw_line in body.lines() {
        let l = raw_line.trim();
        let l = match l.strip_prefix('*') {
            Some(rest) if !rest.starts_with('*') => rest.strip_prefix(' ').unwrap_or(rest),
            _ => l,
        }
        .trim_end();
        let Some(tag) = l.trim_start().strip_prefix('@') else {
            if prose {
                doc.lines.push(l.to_string());
            }
            continue;
        };
        let (name, rest) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        match name {
            "internal" | "hidden" | "ignore" => doc.hidden = true,
            "module" | "fileoverview" | "file" | "packageDocumentation" => doc.module = true,
            _ => {}
        }
        if matches!(
            name,
            "module" | "fileoverview" | "file" | "packageDocumentation" | "description"
        ) {
            if !rest.trim().is_empty() {
                doc.lines.push(rest.trim().to_string());
            }
        } else {
            prose = false;
        }
    }
    while doc.lines.last().is_some_and(|l| l.is_empty()) {
        doc.lines.pop();
    }
    let lead = doc.lines.iter().take_while(|l| l.is_empty()).count();
    doc.lines.drain(..lead);
    Some(doc)
}

/// Declaration text from `start` to `end`, whitespace collapsed, trailing `;`, `,`, `{`
/// and `=` removed.
pub fn declaration_text(src: &str, start: usize, end: usize) -> String {
    let mut sig = collapse(&src[start..end]);
    while sig.ends_with([';', ',', '{', ' ']) {
        sig.pop();
    }
    sig
}

/// Byte offset where a declaration's own text starts: after its decorators.
pub fn start_after_decorators(node: Node) -> usize {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|c| c.kind() == "decorator")
        .last()
        .map_or(node.start_byte(), |d| d.end_byte())
}

/// Cuts a long `name = value` signature down to `name`.
pub fn shorten(sig: String) -> String {
    match sig.split_once(" = ") {
        Some((head, _)) if sig.len() > 100 => head.to_string(),
        _ => sig,
    }
}
