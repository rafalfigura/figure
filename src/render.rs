//! Plain-text building blocks shared by the commands.

use crate::index::Index;
use crate::model::Item;
use crate::relations;

/// Output under construction.
#[derive(Default)]
pub struct Out {
    lines: Vec<String>,
}

impl Out {
    /// Adds one line.
    pub fn line(&mut self, s: impl Into<String>) {
        self.lines.push(s.into());
    }

    /// Adds an empty line, never two in a row.
    pub fn blank(&mut self) {
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.lines.push(String::new());
        }
    }

    /// A `LABEL    value` line with the label padded to a fixed width.
    pub fn field(&mut self, label: &str, value: impl AsRef<str>) {
        self.line(format!(
            "{label:<width$}{}",
            value.as_ref(),
            width = LABEL_WIDTH
        ));
    }

    /// The text, ending in exactly one newline.
    pub fn finish(mut self) -> String {
        while self.lines.last().is_some_and(String::is_empty) {
            self.lines.pop();
        }
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }
}

/// Width of the label column in [`Out::field`] lines.
pub const LABEL_WIDTH: usize = 14;

/// Rows of `(name, rest)` with names padded to one column.
pub fn aligned(rows: &[(String, String)], indent: &str) -> Vec<String> {
    let width = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(0);
    rows.iter()
        .map(|(a, b)| {
            if b.is_empty() {
                format!("{indent}{a}")
            } else {
                format!("{indent}{a:<width$}  {b}")
            }
        })
        .collect()
}

/// A node of a drawn tree.
pub struct TreeNode {
    pub label: String,
    pub detail: String,
    pub notes: Vec<String>,
    pub children: Vec<TreeNode>,
}

/// Draws `nodes` with `├──`/`└──` connectors under a root line.
pub fn tree(nodes: &[TreeNode]) -> Vec<String> {
    let mut out = Vec::new();
    draw(nodes, "", &mut out);
    out
}

fn draw(nodes: &[TreeNode], prefix: &str, out: &mut Vec<String>) {
    let width = nodes
        .iter()
        .map(|n| n.label.chars().count())
        .max()
        .unwrap_or(0);
    for (i, node) in nodes.iter().enumerate() {
        let last = i + 1 == nodes.len();
        let (branch, cont) = if last {
            ("└── ", "    ")
        } else {
            ("├── ", "│   ")
        };
        let line = if node.detail.is_empty() {
            format!("{prefix}{branch}{}", node.label)
        } else {
            format!("{prefix}{branch}{:<width$}  {}", node.label, node.detail)
        };
        out.push(line);
        let child_prefix = format!("{prefix}{cont}");
        for note in &node.notes {
            out.push(format!("{child_prefix}  {note}"));
        }
        draw(&node.children, &child_prefix, out);
    }
}

/// `3 files`, `1 file`.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// An item's one-line form: signature, labels, and `[no doc]` when undocumented.
pub fn item_line(index: &Index, file: &crate::model::FileIndex, item: &Item) -> String {
    let mut sig = item.signature.clone();
    if let Some(owner) = &item.owner {
        if index.language == "typescript" {
            let at = [format!("{}(", item.name), format!("{}<", item.name)]
                .iter()
                .find_map(|needle| sig.find(needle.as_str()));
            if let Some(at) = at {
                sig.replace_range(at..at + item.name.len(), &index.qualified(item));
            }
        } else {
            let needle = format!("fn {}", item.name);
            sig = sig.replacen(&needle, &format!("fn {owner}::{}", item.name), 1);
        }
    }
    let mut extras = relations::labels(index, file, item);
    if item.kind != crate::model::ItemKind::Use && !item.is_documented() {
        extras.push("[no doc]".into());
    }
    if extras.is_empty() {
        sig
    } else {
        format!("{sig}   {}", extras.join(" · "))
    }
}
