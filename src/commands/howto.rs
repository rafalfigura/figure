//! `figure howto [topic]`: list recipes, or print one with its links resolved.

use std::fs;

use crate::commands::links::{Resolved, resolve_link};
use crate::docs::{self, parse_module_doc};
use crate::index::{Index, slash};
use crate::model::FileIndex;
use crate::render::{Out, aligned};

/// A `# How to ...` section or a `.figure/howto/*.md` file.
pub struct Recipe {
    pub topic: String,
    pub title: String,
    /// `src/traps/mod.rs:5` or `.figure/howto/x.md`.
    pub source: String,
    pub lines: Vec<String>,
    /// The file the recipe lives in, for resolving relative links.
    pub file: Option<usize>,
}

/// Every recipe in the crate: module doc sections first, then `.figure/howto` files.
pub fn collect(index: &Index) -> Vec<Recipe> {
    let mut out = Vec::new();
    for (fi, file) in index.files.iter().enumerate() {
        for s in parse_module_doc(&file.module_doc).sections {
            if let Some(topic) = s.recipe_topic() {
                let line = file.module_doc_line + s.at;
                out.push(Recipe {
                    topic,
                    title: s.title.clone(),
                    source: format!("{}:{line}", slash(&file.path)),
                    lines: s.lines,
                    file: Some(fi),
                });
            }
        }
    }
    let dir = index.project.root.join(".figure").join("howto");
    let mut md: Vec<_> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    md.sort();
    for path in md
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
    {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let doc = parse_module_doc(&lines);
        let rel = slash(path.strip_prefix(&index.project.root).unwrap_or(&path));
        let (title, topic, body) = match doc
            .sections
            .iter()
            .find_map(|s| s.recipe_topic().map(|t| (s, t)))
        {
            Some((s, t)) => (s.title.clone(), t, s.lines.clone()),
            None => {
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().replace('-', " "))
                    .unwrap_or_default();
                (format!("How to {stem}"), stem, lines.clone())
            }
        };
        out.push(Recipe {
            topic,
            title,
            source: rel,
            lines: body,
            file: None,
        });
    }
    out
}

/// Lists the recipes, or prints the one matching `topic`.
pub fn run(index: &Index, topic: Option<&str>) -> Result<String, String> {
    let recipes = collect(index);
    let Some(topic) = topic else {
        return Ok(list(&recipes));
    };
    let query = topic.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let exact: Vec<&Recipe> = recipes.iter().filter(|r| r.topic == query).collect();
    let matches: Vec<&Recipe> = if exact.is_empty() {
        recipes
            .iter()
            .filter(|r| words.iter().all(|w| r.topic.contains(w)))
            .collect()
    } else {
        exact
    };
    match matches.as_slice() {
        [] => Err(format!(
            "no recipe matches \"{topic}\" (figure howto lists them)"
        )),
        [one] => Ok(render(index, one, &recipes)),
        many => {
            let names: Vec<String> = many
                .iter()
                .map(|r| format!("  {}  ({})", r.topic, r.source))
                .collect();
            Err(format!(
                "\"{topic}\" matches several recipes:\n{}",
                names.join("\n")
            ))
        }
    }
}

fn list(recipes: &[Recipe]) -> String {
    let mut out = Out::default();
    out.line("RECIPES");
    if recipes.is_empty() {
        out.line("  none (add a `# How to <task>` section to a module doc)");
    }
    let rows: Vec<(String, String)> = recipes
        .iter()
        .map(|r| {
            let first = r
                .lines
                .iter()
                .map(|l| l.trim())
                .find(|l| !l.is_empty())
                .unwrap_or("");
            (r.topic.clone(), format!("{}   {first}", r.source))
        })
        .collect();
    for l in aligned(&rows, "  ") {
        out.line(l);
    }
    out.finish()
}

fn render(index: &Index, recipe: &Recipe, all: &[Recipe]) -> String {
    let topics: Vec<String> = all.iter().map(|r| r.topic.clone()).collect();
    let file: Option<&FileIndex> = recipe.file.map(|f| &index.files[f]);
    let mut ok = 0;
    let mut broken = 0;
    let mut body = Vec::new();
    for line in &recipe.lines {
        let links = docs::links(line);
        let mut rendered = String::new();
        let mut at = 0;
        for link in &links {
            rendered.push_str(&line[at..link.start]);
            let res = resolve_link(index, file, link, &topics, true);
            let text = match &res {
                Resolved::At {
                    location,
                    file_lines,
                } if line.contains("Example to copy") => {
                    format!("{} ({location}, {} lines)", link.label, file_lines)
                }
                Resolved::At { location, .. } => format!("{} ({location})", link.label),
                Resolved::Exists | Resolved::External => link.label.clone(),
                Resolved::Broken => format!("{} (BROKEN)", link.label),
            };
            match res {
                Resolved::Broken => broken += 1,
                _ => ok += 1,
            }
            rendered.push_str(&text);
            at = link.end;
        }
        rendered.push_str(&line[at..]);
        body.push(rendered);
    }
    let mut status = format!("{ok} links ok");
    if broken > 0 {
        status = format!("{status}, {broken} BROKEN");
    }
    let mut out = Out::default();
    out.line(format!("{}   ({}, {status})", recipe.title, recipe.source));
    for l in body {
        out.line(l);
    }
    out.finish()
}
