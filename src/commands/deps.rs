//! `figure deps <path> [--depth N] [--reverse]`: what a module depends on, or who
//! depends on it.

use std::collections::{BTreeMap, BTreeSet};

use crate::graph::{self, Edge, Graph};
use crate::index::{Index, slash};
use crate::model::ModPath;
use crate::render::{Out, TreeNode, count, tree};

/// Prints what `scope` depends on, or with `reverse`, who depends on it.
pub fn run(index: &Index, scope: &ModPath, depth: usize, reverse: bool) -> String {
    let graph = Graph::build(index);
    let outgoing = graph.outgoing(index, scope);
    let incoming = graph.incoming(index, scope);
    let fan_out = groups(&outgoing, |e| e.target.clone(), scope).len();
    let fan_in = groups(&incoming, |e| index.files[e.from].module.clone(), scope).len();
    let mut out = Out::default();
    out.line(format!(
        "{}  ({})   fan-out {fan_out} · fan-in {fan_in}",
        index.mod_name(scope),
        count(index.files_in(scope).count(), "file")
    ));
    if reverse {
        let nodes = build(
            index,
            &incoming,
            |e| index.files[e.from].module.clone(),
            scope,
            depth,
            true,
        );
        if nodes.is_empty() {
            out.line("  nothing outside this module uses it");
        }
        for l in tree(&nodes) {
            out.line(l);
        }
        return out.finish();
    }
    let nodes = if outgoing.is_empty()
        && index
            .modules
            .iter()
            .any(|m| m.len() > scope.len() && m.starts_with(scope))
    {
        children_overview(index, &graph, scope)
    } else {
        build(index, &outgoing, |e| e.target.clone(), scope, depth, false)
    };
    if nodes.is_empty() {
        out.line("  no internal dependencies");
    }
    for l in tree(&nodes) {
        out.line(l);
    }
    let externals = graph.externals_in(index, scope);
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
        out.blank();
        out.line(format!("external    {}", list.join(" · ")));
    }
    let unresolved: Vec<String> = graph
        .unresolved
        .iter()
        .filter(|(f, _, _)| index.files[*f].module.starts_with(scope))
        .map(|(f, line, path)| format!("{}:{line} {path}", slash(&index.files[*f].path)))
        .collect();
    if !unresolved.is_empty() {
        out.line(format!("unresolved  {}", unresolved.join(" · ")));
    }
    out.finish()
}

/// For a scope with no outside dependencies (the crate root): each child module and
/// what it depends on.
fn children_overview(index: &Index, graph: &Graph, scope: &[String]) -> Vec<TreeNode> {
    let children: Vec<&ModPath> = index
        .modules
        .iter()
        .filter(|m| m.len() == scope.len() + 1 && m.starts_with(scope))
        .collect();
    children
        .into_iter()
        .map(|child| {
            let outgoing = graph.outgoing(index, child);
            let mut counts: BTreeMap<ModPath, BTreeSet<usize>> = BTreeMap::new();
            for e in &outgoing {
                counts
                    .entry(graph::group_key(&e.target, child, 0))
                    .or_default()
                    .insert(e.from);
            }
            let mut list: Vec<(String, usize)> = counts
                .into_iter()
                .map(|(k, f)| (index.group_name(&k), f.len()))
                .collect();
            list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let fan_in = groups(
                &graph.incoming(index, child),
                |e| index.files[e.from].module.clone(),
                child,
            )
            .len();
            let deps: Vec<String> = list.iter().map(|(k, n)| format!("{k} {n}")).collect();
            TreeNode {
                label: child.last().cloned().unwrap_or_default(),
                detail: format!("fan-out {} · fan-in {fan_in}", list.len()),
                notes: if deps.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("-> {}", deps.join(" · "))]
                },
                children: Vec::new(),
            }
        })
        .collect()
}

fn groups(
    edges: &[&Edge],
    module: impl Fn(&Edge) -> ModPath,
    scope: &[String],
) -> BTreeSet<ModPath> {
    edges
        .iter()
        .map(|e| graph::group_key(&module(e), scope, 0))
        .collect()
}

/// Tree nodes for `edges`, grouped by `module(edge)` down to `depth` levels.
fn build(
    index: &Index,
    edges: &[&Edge],
    module: impl Fn(&Edge) -> ModPath + Copy,
    scope: &[String],
    depth: usize,
    locations: bool,
) -> Vec<TreeNode> {
    level(index, edges, module, scope, 0, depth.max(1), locations, &[])
}

#[allow(clippy::too_many_arguments)]
fn level(
    index: &Index,
    edges: &[&Edge],
    module: impl Fn(&Edge) -> ModPath + Copy,
    scope: &[String],
    extra: usize,
    depth: usize,
    locations: bool,
    parent: &[String],
) -> Vec<TreeNode> {
    let mut by_key: BTreeMap<ModPath, Vec<&Edge>> = BTreeMap::new();
    for e in edges {
        let key = graph::group_key(&module(e), scope, extra);
        if key.len() > parent.len() || parent.is_empty() {
            by_key.entry(key).or_default().push(e);
        }
    }
    let mut nodes: Vec<(usize, TreeNode)> = Vec::new();
    for (key, group) in by_key {
        let files: BTreeSet<usize> = group.iter().map(|e| e.from).collect();
        let label = if parent.is_empty() {
            index.group_name(&key)
        } else {
            key.last().cloned().unwrap_or_default()
        };
        let mut detail = count(files.len(), "file");
        if !locations {
            let symbols: BTreeSet<&str> =
                group.iter().filter_map(|e| e.symbol.as_deref()).collect();
            if !symbols.is_empty() {
                detail = format!(
                    "{detail}   {}",
                    symbols.into_iter().collect::<Vec<_>>().join(", ")
                );
            }
        }
        let notes = if locations && extra + 1 >= depth {
            usage_lines(index, &group)
        } else {
            Vec::new()
        };
        let deeper: Vec<&Edge> = group
            .iter()
            .copied()
            .filter(|e| module(e).len() > key.len())
            .collect();
        let children = if extra + 1 < depth && !deeper.is_empty() {
            level(
                index,
                &deeper,
                module,
                scope,
                extra + 1,
                depth,
                locations,
                &key,
            )
        } else {
            Vec::new()
        };
        nodes.push((
            files.len(),
            TreeNode {
                label,
                detail,
                notes,
                children,
            },
        ));
    }
    nodes.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.label.cmp(&b.1.label)));
    nodes.into_iter().map(|(_, n)| n).collect()
}

/// `src/app.rs:5  TrapsPlugin, register` per using file.
fn usage_lines(index: &Index, group: &[&Edge]) -> Vec<String> {
    let mut by_file: BTreeMap<usize, (usize, BTreeSet<&str>)> = BTreeMap::new();
    for e in group {
        let entry = by_file.entry(e.from).or_insert((e.line, BTreeSet::new()));
        entry.0 = entry.0.min(e.line);
        if let Some(s) = &e.symbol {
            entry.1.insert(s);
        }
    }
    by_file
        .into_iter()
        .map(|(f, (line, symbols))| {
            let list: Vec<&str> = symbols.into_iter().collect();
            format!(
                "{}:{line}  {}",
                slash(&index.files[f].path),
                list.join(", ")
            )
            .trim_end()
            .to_string()
        })
        .collect()
}
