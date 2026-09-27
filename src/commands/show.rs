//! `figure show <symbol|module>`: the L1 manifest of one item or module.
//!
//! figure prints no source. The header names the item's whole line range (doc and
//! attributes included), so an agent reads just those lines of the file, after it has seen
//! the contract.

use crate::docs::parse_module_doc;
use crate::graph::Graph;
use crate::index::{Index, slash};
use crate::model::{FileIndex, Item, ItemKind, ModPath, Owner, mod_display};
use crate::relations;
use crate::render::{Out, count, item_line};
use crate::resolve::{Found, Target, resolve};

/// Renders an item or a module.
pub fn run(index: &Index, found: &Found) -> String {
    match found {
        Found::Module(m) => module(index, m),
        Found::Item { file, item } => {
            let file = &index.files[*file];
            item_manifest(index, file, &file.items[*item])
        }
    }
}

fn item_manifest(index: &Index, file: &FileIndex, item: &Item) -> String {
    let mut out = Out::default();
    let mut head = vec![item.kind.keyword().to_string()];
    head.extend(relations::labels(index, file, item));
    out.line(format!(
        "{}  {} · {} · {}:{}-{}",
        item.qualified_name(),
        head.join(" · "),
        mod_display(&file.module),
        slash(&file.path),
        item.start_line,
        item.end_line
    ));
    out.field("signature", &item.signature);
    if item.is_documented() {
        out.line("doc");
        for l in &item.doc {
            out.line(format!("  {l}").trim_end().to_string());
        }
    } else if item.kind != ItemKind::Use {
        out.field("doc", "[no doc]");
    }
    if !item.fields.is_empty() {
        out.line(match item.kind {
            ItemKind::Enum => "variants",
            ItemKind::Trait => "members",
            _ => "fields",
        });
        for f in &item.fields {
            out.line(format!("  {f}"));
        }
    }
    let methods: Vec<String> = index
        .files
        .iter()
        .filter(|f| f.module == file.module)
        .flat_map(|f| {
            f.visible_items()
                .filter(|i| i.owner.as_deref() == Some(item.name.as_str()))
                .map(move |i| (f, i))
        })
        .map(|(f, i)| item_line(index, f, i))
        .collect();
    if !methods.is_empty() {
        out.line("methods");
        for m in methods {
            out.line(format!("  {m}"));
        }
    }
    let rels = relations::extract(index);
    let owner_name = match &item.owner {
        Some(ty) => format!("{ty}.{}", item.name),
        None => relations::owner_name(
            file,
            Some(&crate::model::Owner {
                ty: None,
                name: item.name.clone(),
            }),
        ),
    };
    let own: Vec<String> = rels
        .iter()
        .filter(|r| index.files[r.file].path == file.path && r.owner == owner_name)
        .map(|r| format!("{} {}", r.kind, r.target))
        .collect();
    if item.kind == ItemKind::Fn && !own.is_empty() {
        out.field("relations", own.join(" · "));
    }
    let wired: Vec<String> = rels
        .iter()
        .filter(|r| r.target == item.name || r.target.ends_with(&format!("::{}", item.name)))
        .map(|r| format!("{} {} {}", r.owner, r.kind, r.target))
        .collect();
    if !wired.is_empty() {
        out.field("wired by", wired.join(" · "));
    }
    let users = users_of(index, &file.module, &item.name);
    if !users.is_empty() {
        out.field("used by", users.join(" · "));
    }
    if item.kind == ItemKind::Fn {
        let callers = callers_of(index, &file.module, item);
        if !callers.is_empty() {
            out.field("called by", callers.join(" · "));
        }
    }
    out.finish()
}

/// `caller file:line` of every path call to `item` outside tests: `register::<Saw>(..)`,
/// `Harm::new(..)`, `Self::new(..)`, a plain `helper(..)` in the same module. Calls through
/// a receiver (`harm.push(..)`) are not resolvable without types and are not listed.
fn callers_of(index: &Index, module: &ModPath, item: &Item) -> Vec<String> {
    let mut found = Vec::new();
    for file in &index.files {
        for call in file
            .calls
            .iter()
            .filter(|c| !c.in_test && c.method.is_none())
        {
            let segs = callee_path(&call.callee);
            let Some((last, before)) = segs.split_last() else {
                continue;
            };
            if *last != item.name {
                continue;
            }
            let local = file.module == *module;
            let hit = match (&item.owner, before.split_last()) {
                (Some(ty), Some((t, _))) if t == "Self" => {
                    local && call.owner.as_ref().and_then(|o| o.ty.as_ref()) == Some(ty)
                }
                (Some(ty), Some((t, _))) if t == ty => {
                    points_at(index, file, before, module, local)
                }
                (None, None) => {
                    points_at(index, file, &segs, module, local)
                        || local && is_local(index, file, &segs)
                }
                (None, Some(_)) => points_at(index, file, &segs, module, false),
                _ => false,
            };
            if hit {
                let caller = match &call.owner {
                    Some(Owner { ty: Some(ty), name }) => format!("{ty}::{name}"),
                    Some(Owner { ty: None, name }) => match file.module.last() {
                        Some(m) => format!("{m}::{name}"),
                        None => name.clone(),
                    },
                    None => "(module)".into(),
                };
                found.push(format!("{caller} {}:{}", slash(&file.path), call.line));
            }
        }
    }
    found.dedup();
    found
}

/// True when `segs` resolves to `module` and the item right after it, or, for a type
/// declared in the caller's own module (`local`), does not resolve at all.
fn points_at(
    index: &Index,
    file: &FileIndex,
    segs: &[String],
    module: &ModPath,
    local: bool,
) -> bool {
    match resolve(index, file, segs) {
        Target::Internal { module: m, symbol } => {
            m == *module && symbol.as_deref() == segs.last().map(String::as_str)
        }
        Target::Other => local && segs.len() == 1,
        _ => false,
    }
}

/// A one-segment call that no `use` in `file` brings in from elsewhere.
fn is_local(index: &Index, file: &FileIndex, segs: &[String]) -> bool {
    segs.len() == 1 && resolve(index, file, segs) == Target::Other
}

/// `super::register::<Spikes>` -> `["super", "register"]`; generic arguments dropped.
fn callee_path(callee: &str) -> Vec<String> {
    let mut plain = String::new();
    let mut depth = 0usize;
    for ch in callee.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => plain.push(ch),
            _ => {}
        }
    }
    plain
        .split("::")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// `file:line` of every reference to `module::name` from other files.
fn users_of(index: &Index, module: &ModPath, name: &str) -> Vec<String> {
    let graph = Graph::build(index);
    let mut by_file: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for e in graph
        .edges
        .iter()
        .filter(|e| e.target == *module && e.symbol.as_deref() == Some(name))
    {
        by_file.entry(e.from).or_default().push(e.line);
    }
    by_file
        .into_iter()
        .map(|(f, mut lines)| {
            lines.sort_unstable();
            lines.dedup();
            let lines: Vec<String> = lines.iter().map(usize::to_string).collect();
            format!("{}:{}", slash(&index.files[f].path), lines.join(","))
        })
        .collect()
}

fn module(index: &Index, m: &ModPath) -> String {
    let mut out = Out::default();
    let files = index.files_in(m).count();
    out.line(format!(
        "{}  module · {} · {}",
        mod_display(m),
        index.module_location(m),
        count(files, "file")
    ));
    let Some(file) = index.root_file(m) else {
        return out.finish();
    };
    if file.module_doc.is_empty() {
        out.field("doc", "[no module doc]");
    } else {
        out.line("doc");
        for l in &file.module_doc {
            out.line(format!("  {l}").trim_end().to_string());
        }
    }
    let children: Vec<&str> = index
        .modules
        .iter()
        .filter(|c| c.len() == m.len() + 1 && c.starts_with(m))
        .filter_map(|c| c.last().map(String::as_str))
        .collect();
    if !children.is_empty() {
        out.field("submodules", children.join(" · "));
    }
    let items: Vec<&Item> = file.visible_items().collect();
    if !items.is_empty() {
        out.blank();
        out.line(format!("ITEMS in {}", slash(&file.path)));
        for item in items {
            out.line(format!("  {}", item_line(index, file, item)));
            for l in &item.doc {
                out.line(format!("      {l}").trim_end().to_string());
            }
        }
    }
    let sections = parse_module_doc(&file.module_doc).sections;
    let recipes: Vec<String> = sections.iter().filter_map(|s| s.recipe_topic()).collect();
    if !recipes.is_empty() {
        out.blank();
        out.field(
            "RECIPES",
            recipes
                .iter()
                .map(|r| format!("howto {r}"))
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }
    out.finish()
}
