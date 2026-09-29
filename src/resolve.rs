//! Resolution over the index: which module (and item) a path points at, and which items a
//! user's query names. How a path is read is the language adapter's job.

use crate::index::Index;
use crate::model::{FileIndex, ModPath, PathRef};

/// Where a path points.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// A module of this crate, and the item named right after it, if any.
    Internal {
        module: ModPath,
        symbol: Option<String>,
    },
    /// A dependency package (or the language's standard library).
    External(String),
    /// Paths that are meant to be internal but point nowhere.
    Unresolved,
    /// Not a module path: `Self::X`, `Vec::new`, enum variants, local names.
    Other,
}

/// Resolves `segs` as written in `file`.
pub fn resolve(index: &Index, file: &FileIndex, segs: &[String]) -> Target {
    index.adapter().resolve(index, file, segs, false)
}

/// Resolves a reference the adapter recorded.
pub fn resolve_ref(index: &Index, file: &FileIndex, r: &PathRef) -> Target {
    index
        .adapter()
        .resolve(index, file, &r.segments, r.anchored)
}

/// True when `name` is a package the language ships with, not a dependency.
pub fn is_builtin(index: &Index, name: &str) -> bool {
    index.adapter().is_builtin(name)
}

/// The module `abs` names, or its deepest existing ancestor plus the item right after it.
pub fn internal(index: &Index, abs: &[String]) -> Target {
    let depth = (0..=abs.len())
        .rev()
        .find(|&n| index.modules.contains(&abs[..n]))
        .unwrap_or(0);
    Target::Internal {
        module: abs[..depth].to_vec(),
        symbol: abs.get(depth).cloned(),
    }
}

/// Something a query names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Module(ModPath),
    Item { file: usize, item: usize },
}

/// Finds modules and items matching `query`: `Harm`, `Harm::new`, `shared::hazard`,
/// `crate::traps::register`.
pub fn find(index: &Index, query: &str) -> Vec<Found> {
    let mut segs = index.split_query(query.trim().trim_end_matches("()"));
    let anchored = segs.first().is_some_and(|s| s == "crate");
    if anchored {
        segs.remove(0);
    }
    let fits = |module: &[String], prefix: &[String]| {
        if anchored {
            module == prefix
        } else {
            module.ends_with(prefix)
        }
    };
    let mut modules: Vec<Found> = index
        .modules
        .iter()
        .filter(|m| !segs.is_empty() && fits(m, &segs) || segs.is_empty() && m.is_empty())
        .map(|m| Found::Module(m.clone()))
        .collect();
    if modules.len() > 1
        && let Some(exact) = modules
            .iter()
            .position(|f| matches!(f, Found::Module(m) if *m == segs))
    {
        modules = vec![modules.swap_remove(exact)];
    }
    let Some((name, prefix)) = segs.split_last() else {
        return modules;
    };
    let mut items = Vec::new();
    let mut reexports = Vec::new();
    for (fi, file) in index.files.iter().enumerate() {
        for (ii, item) in file.items.iter().enumerate() {
            if item.name != *name {
                continue;
            }
            let matches = match (&item.owner, prefix.split_last()) {
                (_, None) => true,
                (Some(owner), Some((last, rest))) if owner == last => fits(&file.module, rest),
                _ => fits(&file.module, prefix),
            };
            if matches {
                let found = Found::Item { file: fi, item: ii };
                if item.kind == crate::model::ItemKind::Use {
                    reexports.push(found);
                } else {
                    items.push(found);
                }
            }
        }
    }
    if items.is_empty() {
        items = reexports;
    }
    modules.extend(items);
    modules
}

/// True when `module` defines an item or submodule called `name`.
pub fn module_has(index: &Index, module: &[String], name: &str) -> bool {
    let mut child = module.to_vec();
    child.push(name.to_string());
    index.modules.contains(&child)
        || index.files.iter().filter(|f| f.module == module).any(|f| {
            f.items
                .iter()
                .any(|i| i.name == name || i.owner.as_deref() == Some(name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::testing::index_of;

    fn segs(s: &str) -> Vec<String> {
        s.split("::").map(String::from).collect()
    }

    /// Queries match modules by suffix; `crate::` anchors them.
    #[test]
    fn finds_modules() {
        let idx = index_of(
            &[
                "",
                "traps",
                "traps/spikes",
                "traps/shared",
                "traps/shared/hazard",
                "core",
            ],
            &[],
        );
        assert_eq!(
            find(&idx, "hazard"),
            vec![Found::Module(segs("traps::shared::hazard"))]
        );
        assert_eq!(
            find(&idx, "crate::traps"),
            vec![Found::Module(segs("traps"))]
        );
        assert!(find(&idx, "crate::hazard").is_empty());
    }
}
