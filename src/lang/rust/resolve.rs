//! Rust path resolution: `crate::`, `self::`, `super::`, child modules and `use` imports.

use crate::index::Index;
use crate::model::{FileIndex, ModPath};
use crate::resolve::{Target, internal};

const STD: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];

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
    internal(index, &abs)
}

/// The full path a file imported under `name` (`use crate::traps;` -> `traps`).
fn imported(file: &FileIndex, name: &str) -> Option<Vec<String>> {
    file.refs
        .iter()
        .find(|r| !r.glob && r.segments.len() > 1 && r.segments.last().is_some_and(|l| l == name))
        .map(|r| r.segments.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::testing::index_of;
    use crate::model::PathRef;

    fn segs(s: &str) -> Vec<String> {
        s.split("::").map(String::from).collect()
    }

    fn internal_at(m: &str, s: Option<&str>) -> Target {
        Target::Internal {
            module: segs(m).into_iter().filter(|x| !x.is_empty()).collect(),
            symbol: s.map(String::from),
        }
    }

    /// `crate::`, `super::`, `self::` and child-module paths land on the right module.
    #[test]
    fn resolves_relative_paths() {
        let idx = index_of(
            &[
                "",
                "traps",
                "traps/spikes",
                "traps/shared",
                "traps/shared/hazard",
                "core",
            ],
            &["bevy"],
        );
        let spikes = &idx.files[2];
        assert_eq!(
            resolve(&idx, spikes, &segs("super::shared::hazard::Harm")),
            internal_at("traps::shared::hazard", Some("Harm"))
        );
        assert_eq!(
            resolve(&idx, spikes, &segs("super::register")),
            internal_at("traps", Some("register"))
        );
        assert_eq!(
            resolve(&idx, spikes, &segs("crate::core::*")),
            internal_at("core", Some("*"))
        );
        assert_eq!(
            resolve(&idx, &idx.files[1], &segs("shared::hazard")),
            internal_at("traps::shared::hazard", None)
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
        let mut idx = index_of(
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
        idx.files[0].refs.push(PathRef {
            segments: segs("crate::traps::shared"),
            glob: false,
            anchored: false,
            line: 1,
            in_test: false,
            owner: None,
        });
        let root = &idx.files[0];
        assert_eq!(
            resolve(&idx, root, &segs("shared::hazard::Harm")),
            internal_at("traps::shared::hazard", Some("Harm"))
        );
    }
}
