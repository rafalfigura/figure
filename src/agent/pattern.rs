//! Grep patterns: whether a search names only symbols figure knows, and which figure call
//! answers it instead.
//!
//! A pattern names a symbol when, in every `|` alternative, one identifier path is left after
//! regex escapes, character classes and the language's keywords (`fn`, `struct`) are dropped:
//! `fn run\b`, `Harm::new(`, `mesh\|Shape`. `\w+_of` or `"Survive until"` are text.

use crate::index::Index;
use crate::lang;

use super::guard::Place;

/// A block message when every alternative of `pattern` names a symbol of the crate.
pub fn symbol_search(at: &Place, pattern: &str, how: &str) -> Option<String> {
    let keywords = lang::adapter(at.index.language).map_or(&[][..], |a| a.keywords());
    let mut hints = Vec::new();
    for alt in pattern.split("\\|").flat_map(|a| a.split('|')) {
        let names: Vec<String> = identifiers(alt)
            .into_iter()
            .filter(|n| !keywords.contains(&n.as_str()))
            .collect();
        let [name] = names.as_slice() else {
            return None;
        };
        hints.push(symbol_hint(at.index, name)?);
    }
    hints.dedup();
    Some(format!(
        "figure guard: `{how} {pattern}` searches for code figure has resolved. Use:\n{}\n\
         grep is for text that is not a symbol (a string literal, a log message).\n",
        hints.join("\n")
    ))
}

/// `figure show X` for an item, `figure deps <module> --reverse` for a module.
fn symbol_hint(index: &Index, name: &str) -> Option<String> {
    let segs: Vec<&str> = name.split("::").filter(|s| !s.is_empty()).collect();
    let last = *segs.last()?;
    let is_item = |n: &str| {
        index
            .files
            .iter()
            .any(|f| f.items.iter().any(|i| i.name == n))
    };
    if let Some(module) = index
        .modules
        .iter()
        .find(|m| m.last().is_some_and(|l| l == last))
    {
        return Some(format!(
            "  figure deps {} --reverse   (who uses module {last})",
            index.module_arg(module)
        ));
    }
    // `Tone::Accent` (a variant) is not an item: figure cannot list its uses, so grep may.
    is_item(last).then(|| {
        format!(
            "  figure show {}   (contract, file:start-end, who uses and calls it)",
            segs.join("::")
        )
    })
}

/// Identifier paths in a regex (`Harm::new`, `fn`), skipping escapes like `\w` and classes.
fn identifiers(pattern: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
                out.push(std::mem::take(&mut cur));
            }
            '[' => {
                out.push(std::mem::take(&mut cur));
                for n in chars.by_ref() {
                    if n == ']' {
                        break;
                    }
                }
            }
            ':' if chars.peek() == Some(&':') => {
                chars.next();
                cur.push_str("::");
            }
            c if c.is_alphanumeric() || c == '_' => cur.push(c),
            _ => out.push(std::mem::take(&mut cur)),
        }
    }
    out.push(cur);
    out.into_iter()
        .map(|s| s.trim_matches(':').to_string())
        .filter(|s| {
            s.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        })
        .collect()
}
