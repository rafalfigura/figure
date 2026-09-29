//! Rust path resolution over the index: which module (and item) a path points at,
//! and which items a user's query names.

use crate::index::Index;
use crate::model::{FileIndex, ModPath};

const STD: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];

/// Where a path points.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    /// A module of this crate, and the item named right after it, if any.
    Internal {
        module: ModPath,
        symbol: Option<String>,
    },
    /// A dependency crate (or `std`).
    External(String),
    /// `crate::`/`super::`/`self::` paths that point nowhere.
    Unresolved,
    /// Not a module path: `Self::X`, `Vec::new`, enum variants, local names.
    Other,
}

/// True for the standard library crates.
pub fn is_std(name: &str) -> bool {
    STD.contains(&name)
}

/// Resolves `segs` as written in `file`.
pub fn resolve(index: &Index, file: &FileIndex, segs: &[String]) -> Target {
    resolve_in(index, file, segs, true)
}

fn resolve_in(index: &Index, file: &FileIndex, segs: &[String], follow_imports: bool) -> Target {
    let Some(first) = segs.first() else {
        return Target::Other;
    };
    if follow_imports && let Some(full) = index.project.aliases.expand(segs, &index.modules) {
        return resolve_in(index, file, &full, false);
    }
    let from = &file.module;
    let abs: ModPath = match first.as_str() {
        "crate" => segs[1..].to_vec(),
        "self" => from.iter().chain(&segs[1..]).cloned().collect(),
        "super" => {
            let ups = segs.iter().take_while(|s| *s == "super").count();
            if ups > from.len() {
                return Target::Unresolved;
            }
            from[..from.len() - ups]
                .iter()
                .chain(&segs[ups..])
                .cloned()
                .collect()
        }
        name if is_std(name) || index.project.externals.contains(name) => {
            return Target::External(name.to_string());
        }
        name => {
            let child: ModPath = from.iter().cloned().chain([name.to_string()]).collect();
            if index.modules.contains(&child) {
                from.iter().chain(segs).cloned().collect()
            } else if follow_imports && let Some(import) = imported(file, name) {
                let mut full = import;
                full.extend(segs[1..].iter().cloned());
                return resolve_in(index, file, &full, false);
            } else {
                return Target::Other;
            }
        }
    };
    let depth = (0..=abs.len())
        .rev()
        .find(|&n| index.modules.contains(&abs[..n]))
        .unwrap_or(0);
    Target::Internal {
        module: abs[..depth].to_vec(),
        symbol: abs.get(depth).cloned(),
    }
}

/// The full path a file imported under `name` (`use crate::traps;` -> `traps`).
fn imported(file: &FileIndex, name: &str) -> Option<Vec<String>> {
    file.refs
        .iter()
        .find(|r| !r.glob && r.segments.len() > 1 && r.segments.last().is_some_and(|l| l == name))
        .map(|r| r.segments.clone())
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
    use crate::model::PathRef;
    use crate::project::{Config, Project};
    use std::collections::BTreeSet;

    fn index() -> Index {
        let module = |s: &str| -> ModPath {
            s.split('/')
                .filter(|p| !p.is_empty())
                .map(String::from)
                .collect()
        };
        let files = [
            "",
            "traps",
            "traps/spikes",
            "traps/shared",
            "traps/shared/hazard",
            "core",
        ]
        .iter()
        .map(|m| FileIndex {
            module: module(m),
            ..FileIndex::default()
        })
        .collect::<Vec<_>>();
        let modules = files.iter().map(|f| f.module.clone()).collect();
        Index {
            project: Project {
                root: ".".into(),
                name: "t".into(),
                externals: BTreeSet::from(["bevy".to_string()]),
                bevy: true,
                language: "rust",
                aliases: Default::default(),
                workspaces: false,
                source_dir: "src".into(),
                config: Config::default(),
            },
            files,
            modules,
            language: "rust",
        }
    }

    fn segs(s: &str) -> Vec<String> {
        s.split("::").map(String::from).collect()
    }

    fn internal(m: &str, s: Option<&str>) -> Target {
        Target::Internal {
            module: segs(m).into_iter().filter(|x| !x.is_empty()).collect(),
            symbol: s.map(String::from),
        }
    }

    /// `crate::`, `super::`, `self::` and child-module paths land on the right module.
    #[test]
    fn resolves_relative_paths() {
        let idx = index();
        let spikes = &idx.files[2];
        assert_eq!(
            resolve(&idx, spikes, &segs("super::shared::hazard::Harm")),
            internal("traps::shared::hazard", Some("Harm"))
        );
        assert_eq!(
            resolve(&idx, spikes, &segs("super::register")),
            internal("traps", Some("register"))
        );
        assert_eq!(
            resolve(&idx, spikes, &segs("crate::core::*")),
            internal("core", Some("*"))
        );
        assert_eq!(
            resolve(&idx, &idx.files[1], &segs("shared::hazard")),
            internal("traps::shared::hazard", None)
        );
        assert_eq!(
            resolve(&idx, spikes, &segs("bevy::prelude::App")),
            Target::External("bevy".into())
        );
        assert_eq!(
            resolve(&idx, spikes, &segs("super::super::super::x")),
            Target::Unresolved
        );
        assert_eq!(resolve(&idx, spikes, &segs("Vec::new")), Target::Other);
    }

    /// A name imported by `use` resolves through that import in inline paths.
    #[test]
    fn follows_imports() {
        let mut idx = index();
        idx.files[0].refs.push(PathRef {
            segments: segs("crate::traps::shared"),
            glob: false,
            line: 1,
            in_test: false,
            owner: None,
        });
        let root = &idx.files[0];
        assert_eq!(
            resolve(&idx, root, &segs("shared::hazard::Harm")),
            internal("traps::shared::hazard", Some("Harm"))
        );
    }

    /// Queries match modules by suffix; `crate::` anchors them.
    #[test]
    fn finds_modules() {
        let idx = index();
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
