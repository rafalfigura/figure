//! Reading the documentation convention: module doc sections, recipes, facts and links.

/// A `# Heading` inside a module doc and the lines under it.
#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub lines: Vec<String>,
    /// Index of the heading line in the doc.
    pub at: usize,
    /// Index just past the section's last line.
    pub end: usize,
}

impl Section {
    /// `How to add a trap` -> `add a trap`.
    pub fn recipe_topic(&self) -> Option<String> {
        let lower = self.title.to_lowercase();
        lower
            .strip_prefix("how to ")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    }
}

/// A module doc split into its purpose and its `#` sections.
#[derive(Debug, Default)]
pub struct ModuleDoc {
    /// The first sentence: what the module is.
    pub purpose: Option<String>,
    pub sections: Vec<Section>,
}

/// Splits `//!` lines into purpose and sections; headings inside code fences are ignored.
pub fn parse_module_doc(lines: &[String]) -> ModuleDoc {
    let mut doc = ModuleDoc::default();
    let mut headings: Vec<(usize, usize, String)> = Vec::new();
    let mut fenced = false;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced && let Some((level, title)) = heading(t) {
            headings.push((i, level, title));
        }
    }
    let first_heading = headings.first().map_or(lines.len(), |h| h.0);
    doc.purpose = summary(&lines[..first_heading]);
    for (n, (at, level, title)) in headings.iter().enumerate() {
        let end = headings[n + 1..]
            .iter()
            .find(|h| h.1 <= *level)
            .map_or(lines.len(), |h| h.0);
        doc.sections.push(Section {
            title: title.clone(),
            lines: trim_blank(&lines[at + 1..end]),
            at: *at,
            end,
        });
    }
    doc
}

fn heading(line: &str) -> Option<(usize, String)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    let rest = &line[level..];
    (level > 0 && level <= 6 && rest.starts_with(' ')).then(|| (level, rest.trim().to_string()))
}

fn trim_blank(lines: &[String]) -> Vec<String> {
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(start, |e| e + 1);
    lines[start..end].to_vec()
}

/// The first sentence of the first paragraph, joined across wrapped lines.
pub fn summary(doc: &[String]) -> Option<String> {
    let paragraph: Vec<&str> = doc
        .iter()
        .map(|l| l.trim())
        .skip_while(|l| l.is_empty())
        .take_while(|l| !l.is_empty() && heading(l).is_none())
        .collect();
    if paragraph.is_empty() {
        return None;
    }
    let text = paragraph.join(" ");
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        let at_end = i + 1 == text.len() || bytes.get(i + 1) == Some(&b' ');
        if c == '.' && at_end && !text[..i].ends_with("e.g") && !text[..i].ends_with("i.e") {
            return Some(text[..=i].to_string());
        }
    }
    Some(text)
}

/// Lines like `YAML: {...}` whose prefix is configured as a fact.
pub fn facts<'a>(doc: &'a [String], prefixes: &[String]) -> Vec<&'a str> {
    doc.iter()
        .map(|l| l.trim())
        .filter(|l| {
            prefixes.iter().any(|p| {
                l.strip_prefix(p.as_str())
                    .is_some_and(|r| r.starts_with(':'))
            })
        })
        .collect()
}

/// What a link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    /// ``[`Harm`]`` or ``[`text`](crate::a::B)``: a rustdoc intra-doc link.
    Symbol,
    /// `[[src/a.rs]]`.
    File,
    /// `[[howto:add a trap]]`.
    Howto,
}

/// A link found in a doc line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub kind: LinkKind,
    /// What the link points at, cleaned: `crate::traps::register`.
    pub target: String,
    /// Text shown in place of the link.
    pub label: String,
    /// Byte range of the link in the line.
    pub start: usize,
    pub end: usize,
}

/// Finds ``[`X`]``, ``[`X`](path)``, `[[file]]` and `[[howto:topic]]` links in a line.
pub fn links(line: &str) -> Vec<Link> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        if let Some(inner) = rest.strip_prefix("[[") {
            if let Some(close) = inner.find("]]") {
                let body = inner[..close].trim();
                let (kind, target) = match body.strip_prefix("howto:") {
                    Some(t) => (LinkKind::Howto, t.trim().to_string()),
                    None => (LinkKind::File, body.to_string()),
                };
                let end = i + 2 + close + 2;
                out.push(Link {
                    kind,
                    label: target.clone(),
                    target,
                    start: i,
                    end,
                });
                i = end;
                continue;
            }
        } else if let Some(inner) = rest.strip_prefix("[`")
            && let Some(close) = inner.find("`]")
        {
            let label = inner[..close].to_string();
            let mut end = i + 2 + close + 2;
            let mut target = label.clone();
            if line[end..].starts_with('(')
                && let Some(paren) = closing_paren(&line[end..])
            {
                target = line[end + 1..end + paren].trim_matches('`').to_string();
                end += paren + 1;
            }
            out.push(Link {
                kind: LinkKind::Symbol,
                target: clean_target(&target),
                label,
                start: i,
                end,
            });
            i = end;
            continue;
        } else if rest.starts_with('`') {
            // An inline code span shows syntax; nothing inside it is a link.
            let ticks = rest.chars().take_while(|c| *c == '`').count();
            let fence = &rest[..ticks];
            match rest[ticks..].find(fence) {
                Some(close) => {
                    i += ticks + close + ticks;
                    continue;
                }
                None => {
                    i += ticks;
                    continue;
                }
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// Byte index of the `)` closing the `(` that `s` starts with.
fn closing_paren(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Drops rustdoc disambiguators and call syntax: `fn@run()` -> `run`, `vec!` -> `vec`.
fn clean_target(t: &str) -> String {
    let t = t.split_once('@').map_or(t, |(_, r)| r);
    let t = t.trim().trim_end_matches("()").trim_end_matches('!');
    let mut depth = 0;
    t.chars()
        .filter(|c| match c {
            '<' => {
                depth += 1;
                false
            }
            '>' => {
                depth -= 1;
                false
            }
            _ => depth == 0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    /// The first line is the purpose; headings split sections; fenced code is not a heading.
    #[test]
    fn module_doc_sections() {
        let doc = parse_module_doc(&lines(
            "Traps: hazards.\n\nMore text.\n\n# How to add a trap\n1. Step\n```\n# not a heading\n```\n## Detail\nx\n# Invariants\nRule.",
        ));
        assert_eq!(doc.purpose.as_deref(), Some("Traps: hazards."));
        let titles: Vec<_> = doc.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["How to add a trap", "Detail", "Invariants"]);
        assert_eq!(
            doc.sections[0].recipe_topic().as_deref(),
            Some("add a trap")
        );
        assert_eq!(doc.sections[0].lines.len(), 6);
        assert_eq!(doc.sections[2].lines, ["Rule."]);
    }

    /// All link forms are found with their targets cleaned.
    #[test]
    fn link_forms() {
        let found = links(
            "Use [`Harm`], [`run`](fn@crate::a::run()), [[src/a.rs]] and [[howto:add a trap]].",
        );
        let targets: Vec<_> = found
            .iter()
            .map(|l| (l.kind.clone(), l.target.as_str()))
            .collect();
        assert_eq!(
            targets,
            [
                (LinkKind::Symbol, "Harm"),
                (LinkKind::Symbol, "crate::a::run"),
                (LinkKind::File, "src/a.rs"),
                (LinkKind::Howto, "add a trap"),
            ]
        );
    }

    /// A summary is the first sentence, even when the line wraps; no period keeps the paragraph.
    #[test]
    fn summaries() {
        assert_eq!(
            summary(&lines(
                "Arrow Wall: a wall with\nthree holes. Stepping on it"
            ))
            .as_deref(),
            Some("Arrow Wall: a wall with three holes.")
        );
        assert_eq!(
            summary(&lines("Uses e.g. arrows.")).as_deref(),
            Some("Uses e.g. arrows.")
        );
        assert_eq!(
            summary(&lines("YAML: `{ a: 1,\n b: 2 }`\n\nMore.")).as_deref(),
            Some("YAML: `{ a: 1, b: 2 }`")
        );
        assert_eq!(summary(&lines("")), None);
    }

    /// Link syntax quoted in a code span is not a link.
    #[test]
    fn code_spans_skipped() {
        assert!(links("Write `[[file]]` or ``[`X`]``.").is_empty());
        assert_eq!(links("`code` then [`Real`]")[0].target, "Real");
    }

    /// Only configured prefixes count as facts.
    #[test]
    fn fact_lines() {
        let doc = lines("Spikes.\nYAML: { type: Spikes }\nNote: x");
        assert_eq!(facts(&doc, &["YAML".into()]), ["YAML: { type: Spikes }"]);
    }
}
