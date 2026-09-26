//! Checking doc links against the index. Shared by `howto` (rendering) and `check`.

use std::path::Path;

use crate::docs::{Link, LinkKind};
use crate::index::{Index, slash};
use crate::model::FileIndex;
use crate::resolve::{Found, Target, find, is_std, module_has, resolve};

/// Where a link points, when it resolves.
#[derive(Debug)]
pub enum Resolved {
    /// `src/traps/mod.rs:12`, with the file's line count.
    At {
        location: String,
        file_lines: usize,
    },
    /// A file or recipe that exists.
    Exists,
    /// Probably outside this crate (a std or dependency type); not checked.
    External,
    Broken,
}

/// Resolves a link written in `file` (or in a `.figure/howto` file when `file` is `None`).
/// `strict` treats unknown bare names as broken; used for recipes.
pub fn resolve_link(
    index: &Index,
    file: Option<&FileIndex>,
    link: &Link,
    topics: &[String],
    strict: bool,
) -> Resolved {
    match link.kind {
        LinkKind::File => {
            if index.project.root.join(Path::new(&link.target)).exists() {
                Resolved::Exists
            } else {
                Resolved::Broken
            }
        }
        LinkKind::Howto => {
            if topics.iter().any(|t| *t == link.target.to_lowercase()) {
                Resolved::Exists
            } else {
                Resolved::Broken
            }
        }
        LinkKind::Symbol => symbol(index, file, &link.target, strict),
    }
}

fn symbol(index: &Index, file: Option<&FileIndex>, target: &str, strict: bool) -> Resolved {
    let segs: Vec<String> = target.split("::").map(str::to_string).collect();
    let Some(first) = segs.first() else {
        return Resolved::Broken;
    };
    if let Some(file) = file
        && matches!(first.as_str(), "crate" | "self" | "super")
    {
        return match resolve(index, file, &segs) {
            Target::Internal {
                module,
                symbol: None,
            } => at_module(index, &module),
            Target::Internal {
                module,
                symbol: Some(s),
            } if module_has(index, &module, &s) => at_found(
                index,
                &find(index, &format!("crate::{}::{s}", module.join("::"))),
            )
            .unwrap_or_else(|| at_module(index, &module)),
            Target::External(_) => Resolved::External,
            _ => Resolved::Broken,
        };
    }
    if is_std(first) || index.project.externals.contains(first) {
        return Resolved::External;
    }
    if let Some(found) = at_found(index, &find(index, target)) {
        return found;
    }
    let imported = file.is_some_and(|f| {
        f.refs
            .iter()
            .any(|r| !r.glob && r.segments.last() == Some(first))
    });
    let local_root = index.modules.iter().any(|m| m.first() == Some(first));
    if imported || (!strict && (segs.len() == 1 || !local_root)) {
        Resolved::External
    } else {
        Resolved::Broken
    }
}

fn at_found(index: &Index, found: &[Found]) -> Option<Resolved> {
    match found.first()? {
        Found::Module(m) => Some(at_module(index, m)),
        Found::Item { file, item } => {
            let f = &index.files[*file];
            Some(Resolved::At {
                location: format!("{}:{}", slash(&f.path), f.items[*item].decl_line),
                file_lines: f.lines,
            })
        }
    }
}

fn at_module(index: &Index, module: &[String]) -> Resolved {
    match index.root_file(module) {
        Some(f) => Resolved::At {
            location: slash(&f.path),
            file_lines: f.lines,
        },
        None => Resolved::Exists,
    }
}
