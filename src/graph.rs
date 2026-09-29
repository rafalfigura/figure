//! The dependency graph: resolved references between modules, grouped for display.

use std::collections::{BTreeMap, BTreeSet};

use crate::index::Index;
use crate::model::ModPath;
use crate::resolve::{Target, is_builtin, resolve_ref};

/// One resolved reference from a file into a module of this crate.
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: usize,
    pub line: usize,
    pub target: ModPath,
    pub symbol: Option<String>,
}

/// Resolved references of the whole crate.
pub struct Graph {
    pub edges: Vec<Edge>,
    /// `crate::`/`super::`/`self::` paths that point nowhere: (file, line, path).
    pub unresolved: Vec<(usize, usize, String)>,
    /// Dependency crate -> files using it outside tests, and files using it in tests.
    pub externals: BTreeMap<String, (BTreeSet<usize>, BTreeSet<usize>)>,
}

impl Graph {
    /// Resolves every reference in the index.
    pub fn build(index: &Index) -> Graph {
        let mut graph = Graph {
            edges: Vec::new(),
            unresolved: Vec::new(),
            externals: BTreeMap::new(),
        };
        for (fi, file) in index.files.iter().enumerate() {
            for r in &file.refs {
                match resolve_ref(index, file, r) {
                    Target::Internal { module, symbol } if !r.in_test => graph.edges.push(Edge {
                        from: fi,
                        line: r.line,
                        target: module,
                        symbol: if r.glob { Some("*".into()) } else { symbol },
                    }),
                    Target::External(name) if !is_builtin(index, &name) => {
                        let entry = graph.externals.entry(name).or_default();
                        if r.in_test {
                            entry.1.insert(fi);
                        } else {
                            entry.0.insert(fi);
                        }
                    }
                    Target::Unresolved => {
                        graph.unresolved.push((fi, r.line, r.segments.join("::")))
                    }
                    _ => {}
                }
            }
        }
        graph
    }

    /// References leaving `scope`: target group -> the edges.
    pub fn outgoing(&self, index: &Index, scope: &[String]) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|e| {
                index.files[e.from].module.starts_with(scope) && !e.target.starts_with(scope)
            })
            .collect()
    }

    /// References into `scope` from outside it.
    pub fn incoming(&self, index: &Index, scope: &[String]) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|e| {
                e.target.starts_with(scope) && !index.files[e.from].module.starts_with(scope)
            })
            .collect()
    }

    /// External crates used by files in `scope`: (name, files, test-only).
    pub fn externals_in(&self, index: &Index, scope: &[String]) -> Vec<(String, usize, bool)> {
        let inside = |set: &BTreeSet<usize>| {
            set.iter()
                .filter(|f| index.files[**f].module.starts_with(scope))
                .count()
        };
        let mut out: Vec<(String, usize, bool)> = self
            .externals
            .iter()
            .filter_map(|(name, (main, tests))| {
                let n = inside(main);
                let t = inside(tests);
                match (n, t) {
                    (0, 0) => None,
                    (0, t) => Some((name.clone(), t, true)),
                    (n, _) => Some((name.clone(), n, false)),
                }
            })
            .collect();
        out.sort_by(|a, b| a.2.cmp(&b.2).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
        out
    }
}

/// Cuts `module` to one level below its common ancestor with `scope`:
/// scope `traps`, target `registry::x` -> `registry`; scope `traps::a`, target `traps::b::c` -> `traps::b`.
pub fn group_key(module: &[String], scope: &[String], extra_depth: usize) -> ModPath {
    let common = module.iter().zip(scope).take_while(|(a, b)| a == b).count();
    module[..module.len().min(common + 1 + extra_depth)].to_vec()
}

/// How the direct children of `scope` depend on each other.
pub fn shape(index: &Index, graph: &Graph, scope: &[String]) -> Option<String> {
    let child_of = |m: &[String]| {
        (m.len() > scope.len() && m.starts_with(scope)).then(|| m[scope.len()].clone())
    };
    let children: BTreeSet<String> = index
        .files
        .iter()
        .filter_map(|f| child_of(&f.module))
        .collect();
    if children.len() < 2 {
        return None;
    }
    let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in &graph.edges {
        if let (Some(a), Some(b)) = (child_of(&index.files[e.from].module), child_of(&e.target))
            && a != b
        {
            adj.entry(a).or_default().insert(b);
        }
    }
    let cycles = cycles(&children, &adj);
    if !cycles.is_empty() {
        let list: Vec<String> = cycles.iter().map(|c| c.join(" <-> ")).collect();
        return Some(format!("mesh: cycles {}", list.join("; ")));
    }
    let edge_count: usize = adj.values().map(BTreeSet::len).sum();
    if edge_count == 0 {
        return Some(format!(
            "flat: {} parts, no imports between them",
            children.len()
        ));
    }
    let hubs: BTreeSet<&String> = adj.values().flatten().collect();
    let leaves = children.len() - hubs.len();
    let leaf_edges = adj
        .iter()
        .filter(|(from, _)| !hubs.contains(from))
        .flat_map(|(_, to)| to)
        .filter(|to| !hubs.contains(to))
        .count();
    if hubs.len() <= 2 && leaves >= 2 && leaf_edges == 0 {
        let names: Vec<String> = hubs
            .iter()
            .map(|h| display_child(index, scope, h))
            .collect();
        return Some(format!(
            "star: {leaves} parts -> {}, no other sibling imports, no cycles",
            names.join(", ")
        ));
    }
    Some(format!("layered: {edge_count} sibling imports, no cycles"))
}

fn display_child(index: &Index, scope: &[String], child: &str) -> String {
    let mut m = scope.to_vec();
    m.push(child.to_string());
    let is_dir = index
        .files
        .iter()
        .any(|f| f.module.len() > m.len() && f.module.starts_with(&m));
    if is_dir {
        format!("{child}/")
    } else {
        child.to_string()
    }
}

/// Groups of nodes that reach each other (strongly connected, size > 1).
fn cycles(nodes: &BTreeSet<String>, adj: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    let reach = |from: &String| {
        let mut seen = BTreeSet::new();
        let mut stack = vec![from];
        while let Some(n) = stack.pop() {
            for m in adj.get(n).into_iter().flatten() {
                if seen.insert(m) {
                    stack.push(m);
                }
            }
        }
        seen
    };
    let reach: BTreeMap<&String, BTreeSet<&String>> = nodes.iter().map(|n| (n, reach(n))).collect();
    let mut done = BTreeSet::new();
    let mut out = Vec::new();
    for n in nodes {
        if done.contains(n) || !reach[n].contains(n) {
            continue;
        }
        let group: Vec<String> = nodes
            .iter()
            .filter(|m| reach[n].contains(m) && reach[*m].contains(n))
            .cloned()
            .collect();
        done.extend(group.iter().cloned());
        out.push(group);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> ModPath {
        s.split("::")
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect()
    }

    /// Groups sit one level below the common ancestor.
    #[test]
    fn group_keys() {
        assert_eq!(group_key(&m("registry::x"), &m("traps"), 0), m("registry"));
        assert_eq!(
            group_key(&m("traps::b::c"), &m("traps::a"), 0),
            m("traps::b")
        );
        assert_eq!(group_key(&m("traps"), &m("traps::a"), 0), m("traps"));
        assert_eq!(
            group_key(&m("traps::b::c"), &m("traps::a"), 1),
            m("traps::b::c")
        );
    }

    /// Mutual reachability finds cycles, and only them.
    #[test]
    fn finds_cycles() {
        let nodes: BTreeSet<String> = ["a", "b", "c"].map(String::from).into();
        let mut adj = BTreeMap::new();
        adj.insert("a".to_string(), BTreeSet::from(["b".to_string()]));
        adj.insert(
            "b".to_string(),
            BTreeSet::from(["a".to_string(), "c".to_string()]),
        );
        assert_eq!(
            cycles(&nodes, &adj),
            vec![vec!["a".to_string(), "b".to_string()]]
        );
    }
}
