//! `figure map <path>`: the L0 module manifest.

use crate::docs::{self, parse_module_doc};
use crate::graph::{self, Graph};
use crate::index::{Index, slash};
use crate::model::{ItemKind, ModPath, Vis};
use crate::relations;
use crate::render::{Out, aligned, count, item_line};

/// Flags of `figure map`.
pub struct Options {
    pub depth: usize,
    pub fields: bool,
    pub private: bool,
}

/// Renders the L0 manifest of `scope`.
pub fn run(index: &Index, scope: &ModPath, opts: &Options) -> String {
    let graph = Graph::build(index);
    let files: Vec<_> = index.files_in(scope).collect();
    let lines: usize = files.iter().map(|(_, f)| f.lines).sum();
    let root = index.root_file(scope);
    let doc = root
        .map(|f| parse_module_doc(&f.module_doc))
        .unwrap_or_default();
    let mut out = Out::default();

    out.field(
        "MODULE",
        format!(
            "{}   {}  {} · {lines} lines · {}",
            index.mod_name(scope),
            index.module_location(scope),
            count(files.len(), "file"),
            index.language
        ),
    );
    out.field(
        "PURPOSE",
        doc.purpose.as_deref().unwrap_or("[no module doc]"),
    );
    if let Some(shape) = graph::shape(index, &graph, scope) {
        out.field("SHAPE", shape);
    }
    let broken: Vec<String> = files
        .iter()
        .filter(|(_, f)| f.parse_errors)
        .map(|(_, f)| slash(&f.path))
        .collect();
    if !broken.is_empty() {
        out.field("WARNING", format!("syntax errors in {}", broken.join(", ")));
    }

    tree_section(index, scope, opts.depth, &mut out);
    api_section(index, scope, opts, &mut out);
    relations_section(index, scope, &mut out);
    deps_section(index, &graph, scope, &mut out);
    contracts_section(index, scope, &doc, &mut out);

    out.blank();
    let other: Vec<&str> = doc
        .sections
        .iter()
        .filter(|s| s.recipe_topic().is_none() && s.title != "Invariants")
        .map(|s| s.title.as_str())
        .collect();
    if !other.is_empty() {
        out.field(
            "DOC SECTIONS",
            format!(
                "{}   (figure show {})",
                other.join(" · "),
                index.mod_name(scope)
            ),
        );
    }
    let recipes: Vec<String> = files
        .iter()
        .flat_map(|(_, f)| parse_module_doc(&f.module_doc).sections)
        .filter_map(|s| s.recipe_topic())
        .map(|t| format!("howto {t}"))
        .collect();
    if !recipes.is_empty() {
        out.field("RECIPES", recipes.join(" · "));
    }
    let (documented, total) = doc_counts(index, scope);
    let undocumented = total - documented;
    if undocumented == 0 {
        out.field(
            "UNDOCUMENTED",
            format!("none ({})", count(total, "public item")),
        );
    } else {
        out.field(
            "UNDOCUMENTED",
            format!(
                "{undocumented} of {} in this module (list: figure check {})",
                count(total, "public item"),
                index.module_arg(scope)
            ),
        );
    }
    out.finish()
}

fn tree_section(index: &Index, scope: &[String], depth: usize, out: &mut Out) {
    let mut rows = Vec::new();
    let children: Vec<ModPath> = index
        .modules
        .iter()
        .filter(|m| m.len() > scope.len() && m.len() <= scope.len() + depth && m.starts_with(scope))
        .cloned()
        .collect();
    for m in children {
        let indent = "  ".repeat(m.len() - scope.len() - 1);
        let below = index.files_in(&m).count();
        let is_dir = index
            .files
            .iter()
            .any(|f| f.module.len() > m.len() && f.module.starts_with(&m));
        let name = format!(
            "{indent}{}{}",
            m.last().map_or("", String::as_str),
            if is_dir { "/" } else { "" }
        );
        let purpose = index
            .root_file(&m)
            .and_then(|f| parse_module_doc(&f.module_doc).purpose)
            .unwrap_or_else(|| "[no module doc]".into());
        let detail = if is_dir {
            format!("{purpose} ({})", count(below, "file"))
        } else {
            purpose
        };
        rows.push((name, detail));
    }
    if !rows.is_empty() {
        out.blank();
        out.line("TREE");
        for l in aligned(&rows, "  ") {
            out.line(l);
        }
    }
}

fn api_section(index: &Index, scope: &[String], opts: &Options, out: &mut Out) {
    out.blank();
    out.line("PUBLIC API");
    let min_vis = if opts.private {
        Vis::Private
    } else {
        Vis::Restricted
    };
    let mut test_only = Vec::new();
    let files = index
        .files_in(scope)
        .filter(|(_, f)| f.module.len() <= scope.len() + opts.depth);
    for (_, file) in files {
        let items: Vec<_> = file.visible_items().filter(|i| i.vis >= min_vis).collect();
        test_only.extend(
            file.items
                .iter()
                .filter(|i| i.test_only && !i.hidden)
                .map(|i| {
                    format!(
                        "{} ({})",
                        i.name,
                        i.signature.trim_start_matches("use ").trim_end_matches(';')
                    )
                }),
        );
        if items.is_empty() {
            continue;
        }
        let rel = match index.module_dir(scope) {
            Some(dir) => file
                .path
                .strip_prefix(dir)
                .unwrap_or(&file.path)
                .to_path_buf(),
            None => file.path.file_name().map(Into::into).unwrap_or_default(),
        };
        out.line(format!("## {}", slash(&rel)));
        for item in items {
            out.line(format!("  {}", item_line(index, file, item)));
            if item.kind != ItemKind::Use
                && let Some(summary) = docs::summary(&item.doc)
            {
                out.line(format!("      {summary}"));
                for fact in docs::facts(&item.doc, &index.project.config.docs.fact_prefixes) {
                    if !summary.contains(fact) {
                        out.line(format!("      {fact}"));
                    }
                }
            }
            if opts.fields {
                for f in &item.fields {
                    out.line(format!("      · {f}"));
                }
            }
        }
    }
    if !test_only.is_empty() {
        out.line(format!("  test-only: {}", test_only.join(" · ")));
    }
}

fn relations_section(index: &Index, scope: &[String], out: &mut Out) {
    let rels: Vec<_> = relations::extract(index)
        .into_iter()
        .filter(|r| index.files[r.file].module.starts_with(scope))
        .collect();
    if rels.is_empty() {
        return;
    }
    out.blank();
    out.line(if index.project.bevy {
        "RELATIONS  (bevy pack)"
    } else {
        "RELATIONS"
    });
    let mut rows: Vec<(String, String)> = Vec::new();
    for r in rels {
        let text = format!("{} {}", r.kind, r.target);
        match rows.last_mut() {
            Some((owner, list)) if *owner == r.owner => {
                list.push_str(" · ");
                list.push_str(&text);
            }
            _ => rows.push((r.owner, text)),
        }
    }
    for l in aligned(&rows, "  ") {
        out.line(l);
    }
}

fn deps_section(index: &Index, graph: &Graph, scope: &[String], out: &mut Out) {
    let mut outgoing: Vec<(String, usize)> = group_counts(
        index,
        graph
            .outgoing(index, scope)
            .iter()
            .map(|e| (graph::group_key(&e.target, scope, 0), e.from)),
    );
    outgoing.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let externals = graph.externals_in(index, scope);
    let users: Vec<(String, usize)> = group_counts(
        index,
        graph.incoming(index, scope).iter().map(|e| {
            (
                graph::group_key(&index.files[e.from].module, scope, 0),
                e.from,
            )
        }),
    );
    if outgoing.is_empty() && externals.is_empty() && users.is_empty() {
        return;
    }
    out.blank();
    out.line("DEPENDENCIES");
    if !outgoing.is_empty() {
        let list: Vec<String> = outgoing.iter().map(|(k, n)| format!("{k} {n}")).collect();
        out.line(format!("  internal  {}", list.join(" · ")));
    }
    if !externals.is_empty() {
        let list: Vec<String> = externals
            .iter()
            .map(|(name, n, tests)| {
                if *tests {
                    format!("(tests) {name}")
                } else {
                    format!("{name} {n}")
                }
            })
            .collect();
        out.line(format!("  external  {}", list.join(" · ")));
    }
    if !users.is_empty() {
        let list: Vec<String> = users.iter().map(|(k, _)| k.clone()).collect();
        out.line(format!("  used by   {}", list.join(" · ")));
    }
}

/// (group, file) pairs -> (group name, distinct files).
fn group_counts(
    index: &Index,
    pairs: impl Iterator<Item = (ModPath, usize)>,
) -> Vec<(String, usize)> {
    let mut map: std::collections::BTreeMap<ModPath, std::collections::BTreeSet<usize>> =
        Default::default();
    for (k, f) in pairs {
        map.entry(k).or_default().insert(f);
    }
    map.into_iter()
        .map(|(k, s)| (index.group_name(&k), s.len()))
        .collect()
}

fn contracts_section(index: &Index, scope: &[String], doc: &docs::ModuleDoc, out: &mut Out) {
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut undocumented = 0;
    for (_, file) in index.files_in(scope) {
        for t in &file.tests {
            match docs::summary(&t.doc) {
                Some(s) => rows.push((t.name.clone(), s)),
                None => undocumented += 1,
            }
        }
    }
    let invariants: Vec<&String> = doc
        .sections
        .iter()
        .filter(|s| s.title == "Invariants")
        .flat_map(|s| &s.lines)
        .filter(|l| !l.trim().is_empty())
        .collect();
    if rows.is_empty() && invariants.is_empty() && undocumented == 0 {
        return;
    }
    out.blank();
    out.line("CONTRACTS  (invariants and tests)");
    for l in invariants {
        out.line(format!(
            "  invariant  {}",
            l.trim().trim_start_matches("- ")
        ));
    }
    for l in aligned(&rows, "  ") {
        out.line(l);
    }
    if undocumented > 0 {
        out.line(format!(
            "  (+ {} without a doc line)",
            count(undocumented, "test")
        ));
    }
}

/// (documented, total) public items under `scope`; re-exports are not counted.
pub fn doc_counts(index: &Index, scope: &[String]) -> (usize, usize) {
    let items: Vec<_> = index
        .files_in(scope)
        .flat_map(|(_, f)| f.visible_items())
        .filter(|i| i.vis >= Vis::Restricted && i.kind != ItemKind::Use)
        .collect();
    (
        items.iter().filter(|i| i.is_documented()).count(),
        items.len(),
    )
}
