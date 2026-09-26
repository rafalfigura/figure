//! `figure check [path] [--strict] [--changed [REF]]`: documentation gaps and broken links,
//! for the whole crate or only for what changed since a git revision.

use std::path::Path;

use crate::changes::ChangeSet;
use crate::commands::howto;
use crate::commands::links::{Resolved, resolve_link};
use crate::docs::{self, Link};
use crate::index::{Index, slash};
use crate::model::{ItemKind, ModPath, Vis};
use crate::render::{Out, count};

/// What `check` found.
pub struct Report {
    pub text: String,
    /// True when `--strict` should fail.
    pub findings: bool,
}

/// Checks module docs, public item docs and doc links under `scope`. With `changes`, only
/// changed files are checked: module docs on new files, docs on new or re-signed public
/// items, and every link in those files.
pub fn run(index: &Index, scope: &ModPath, changes: Option<&ChangeSet>) -> Report {
    let mut out = Out::default();
    let files: Vec<_> = index
        .files_in(scope)
        .filter(|(_, f)| changes.is_none_or(|c| c.files.contains_key(&f.path)))
        .collect();
    if let Some(c) = changes {
        let (changed, new) = c.counts();
        if changed == 0 {
            out.field(&format!("since {}", c.base), "no source files changed");
            return Report {
                text: out.finish(),
                findings: false,
            };
        }
        out.field(
            &format!("since {}", c.base),
            format!("{} changed ({new} new)", count(changed, "file")),
        );
    }
    let needs_module_doc: Vec<_> = files
        .iter()
        .filter(|(_, f)| changes.is_none_or(|c| c.is_new_file(&f.path)))
        .collect();

    let no_module_doc: Vec<String> = needs_module_doc
        .iter()
        .filter(|(_, f)| docs::summary(&f.module_doc).is_none())
        .map(|(_, f)| slash(&f.path))
        .collect();
    out.field(
        "module docs",
        format!(
            "{} of {} {}",
            needs_module_doc.len() - no_module_doc.len(),
            needs_module_doc.len(),
            if changes.is_some() {
                "new files"
            } else {
                "files"
            }
        ),
    );
    list(&mut out, "missing", &no_module_doc);

    let public: Vec<(String, bool)> = files
        .iter()
        .flat_map(|(_, f)| {
            f.visible_items()
                .filter(|i| i.vis >= Vis::Restricted && i.kind != ItemKind::Use)
                .filter(move |i| changes.is_none_or(|c| c.is_new_or_changed(&f.path, i)))
                .map(move |i| {
                    let name = format!(
                        "{}:{} {} {}",
                        slash(&f.path),
                        i.decl_line,
                        i.kind.keyword(),
                        i.qualified_name()
                    );
                    (name, i.is_documented())
                })
        })
        .collect();
    let undocumented: Vec<String> = public
        .iter()
        .filter(|(_, d)| !d)
        .map(|(n, _)| n.clone())
        .collect();
    out.field(
        "public items",
        format!(
            "{} of {} {}documented",
            public.len() - undocumented.len(),
            public.len(),
            if changes.is_some() {
                "new or changed "
            } else {
                ""
            }
        ),
    );
    list(&mut out, "missing", &undocumented);

    let tests: Vec<_> = files.iter().flat_map(|(_, f)| &f.tests).collect();
    let documented_tests = tests
        .iter()
        .filter(|t| docs::summary(&t.doc).is_some())
        .count();
    out.field(
        "tests",
        format!("{documented_tests} of {} with a doc line", tests.len()),
    );

    let recipes = howto::collect(index);
    let topics: Vec<String> = recipes.iter().map(|r| r.topic.clone()).collect();
    let mut checked = 0;
    let mut broken = Vec::new();
    let mut note = |res: Resolved, place: String, link: &Link| match res {
        Resolved::Broken => {
            checked += 1;
            broken.push(format!("{place} {}", link_text(link)));
        }
        Resolved::External => {}
        _ => checked += 1,
    };
    for (_, file) in &files {
        let recipe_lines = recipe_line_set(file);
        for (n, line) in file.module_doc.iter().enumerate() {
            for link in docs::links(line) {
                let strict = recipe_lines.contains(&n);
                let res = resolve_link(index, Some(file), &link, &topics, strict);
                note(
                    res,
                    format!("{}:{}", slash(&file.path), file.module_doc_line + n),
                    &link,
                );
            }
        }
        for item in &file.items {
            for (n, line) in item.doc.iter().enumerate() {
                for link in docs::links(line) {
                    let res = resolve_link(index, Some(file), &link, &topics, false);
                    note(
                        res,
                        format!("{}:{}", slash(&file.path), item.start_line + n),
                        &link,
                    );
                }
            }
        }
    }
    if scope.is_empty() {
        let md_in_scope =
            |source: &str| changes.is_none_or(|c| c.paths.contains(Path::new(source)));
        for recipe in recipes
            .iter()
            .filter(|r| r.file.is_none() && md_in_scope(&r.source))
        {
            for line in &recipe.lines {
                for link in docs::links(line) {
                    let res = resolve_link(index, None, &link, &topics, true);
                    note(res, recipe.source.clone(), &link);
                }
            }
        }
    }
    out.field(
        "links",
        format!("{checked} checked, {} broken", broken.len()),
    );
    list(&mut out, "broken", &broken);

    let in_scope: Vec<String> = recipes
        .iter()
        .filter(|r| {
            r.file.map_or(scope.is_empty(), |f| {
                index.files[f].module.starts_with(scope)
            })
        })
        .map(|r| format!("{} ({})", r.topic, r.source))
        .collect();
    out.field(
        "recipes",
        if in_scope.is_empty() {
            "none".to_string()
        } else {
            in_scope.join(" · ")
        },
    );

    let syntax: Vec<String> = files
        .iter()
        .filter(|(_, f)| f.parse_errors)
        .map(|(_, f)| slash(&f.path))
        .collect();
    out.field(
        "syntax errors",
        if syntax.is_empty() {
            "none".to_string()
        } else {
            count(syntax.len(), "file")
        },
    );
    list(&mut out, "in", &syntax);

    let findings = !no_module_doc.is_empty()
        || !undocumented.is_empty()
        || !broken.is_empty()
        || !syntax.is_empty();
    if findings && changes.is_some() {
        out.blank();
        out.line(
            "Document each item above with a `///` line saying what it does, and each new file",
        );
        out.line("with a `//!` line saying what it is; then run figure check --changed again.");
    }
    Report {
        text: out.finish(),
        findings,
    }
}

fn list(out: &mut Out, label: &str, entries: &[String]) {
    for (i, e) in entries.iter().enumerate() {
        let head = if i == 0 {
            format!("  {label}: ")
        } else {
            " ".repeat(label.len() + 4)
        };
        out.line(format!("{head}{e}"));
    }
}

fn link_text(link: &Link) -> String {
    match link.kind {
        docs::LinkKind::Symbol => format!("[`{}`]", link.label),
        docs::LinkKind::File => format!("[[{}]]", link.target),
        docs::LinkKind::Howto => format!("[[howto:{}]]", link.target),
    }
}

/// Indexes of module doc lines that belong to a recipe section.
fn recipe_line_set(file: &crate::model::FileIndex) -> Vec<usize> {
    docs::parse_module_doc(&file.module_doc)
        .sections
        .iter()
        .filter(|s| s.recipe_topic().is_some())
        .flat_map(|s| s.at..s.end)
        .collect()
}
