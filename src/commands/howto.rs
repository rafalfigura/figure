//! `figure howto [topic]`: list recipes, or print one with its links resolved.

use crate::commands::links::{Resolved, resolve_link};
use crate::docs::{self, parse_module_doc};
use crate::index::{Index, slash};
use crate::model::FileIndex;
use crate::render::{Out, aligned};

/// A `# How to ...` section of a module doc.
pub struct Recipe {
    pub topic: String,
    pub title: String,
    /// `src/traps/mod.rs:5`.
    pub source: String,
    pub lines: Vec<String>,
    /// The file the recipe lives in, for resolving relative links.
    pub file: usize,
}

/// Every `# How to ...` section in the crate's module docs.
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
                    file: fi,
                });
            }
        }
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
    let file: Option<&FileIndex> = Some(&index.files[recipe.file]);
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
