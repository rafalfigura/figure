//! Relations: what a function wires up (systems, observers, registrations, plugins).
//! Framework packs and `figure.toml` patterns turn raw calls into named relations.

use crate::index::Index;
use crate::model::{Call, FileIndex, Item, Owner};
use crate::project::RelationPattern;

/// One wiring fact: `owner` does `kind` to `target`, e.g. `spikes::plugin registers Spikes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub file: usize,
    pub line: usize,
    /// `spikes::plugin`, `TrapsPlugin.build`.
    pub owner: String,
    /// `system`, `observer`, `registers`, ...
    pub kind: String,
    pub target: String,
}

/// All relations in the crate, in file and line order. Test code is skipped.
pub fn extract(index: &Index) -> Vec<Relation> {
    let mut out = Vec::new();
    for (fi, file) in index.files.iter().enumerate() {
        let mut found: Vec<Relation> = Vec::new();
        for call in file.calls.iter().filter(|c| !c.in_test) {
            let owner = owner_name(file, call.owner.as_ref());
            let mut push = |kind: &str, target: String| {
                found.push(Relation {
                    file: fi,
                    line: call.line,
                    owner: owner.clone(),
                    kind: kind.to_string(),
                    target,
                });
            };
            if index.project.has_pack("bevy")
                && let Some((kind, target)) = bevy_call(call)
            {
                push(kind, target);
            }
            for pattern in &index.project.config.relations {
                if let Some(target) = match_pattern(pattern, call) {
                    push(&pattern.name, target);
                }
            }
        }
        if index.project.has_pack("bevy") {
            for r in file.refs.iter().filter(|r| !r.in_test && r.owner.is_some()) {
                if r.segments.len() > 1 && r.segments.last().is_some_and(|s| s == "plugin") {
                    let target: Vec<&str> = r
                        .segments
                        .iter()
                        .map(String::as_str)
                        .skip_while(|s| *s == "super" || *s == "self" || *s == "crate")
                        .collect();
                    found.push(Relation {
                        file: fi,
                        line: r.line,
                        owner: owner_name(file, r.owner.as_ref()),
                        kind: "plugin".into(),
                        target: target.join("::"),
                    });
                }
            }
        }
        found.sort_by_key(|r| r.line);
        for r in found {
            let dup = out.iter().any(|o: &Relation| {
                o.file == r.file && o.owner == r.owner && o.kind == r.kind && o.target == r.target
            });
            if !dup {
                out.push(r);
            }
        }
    }
    out
}

/// `spikes::plugin` for free functions, `TrapsPlugin.build` for methods.
pub fn owner_name(file: &FileIndex, owner: Option<&Owner>) -> String {
    match owner {
        Some(Owner { ty: Some(ty), name }) => format!("{ty}.{name}"),
        Some(Owner { ty: None, name }) => match file.module.last() {
            Some(m) => format!("{m}::{name}"),
            None => name.clone(),
        },
        None => "(module)".into(),
    }
}

/// A call argument as shown in a relation: closures and struct literals by their shape.
fn brief(arg: &str) -> String {
    let t = arg.trim();
    if t.starts_with('|') || t.starts_with("move |") || t.starts_with("move|") {
        return "<closure>".into();
    }
    match t.find('{') {
        Some(at) if at > 0 && !t.starts_with('(') => format!("{} {{..}}", t[..at].trim()),
        _ => t.to_string(),
    }
}

fn bevy_call(call: &Call) -> Option<(&'static str, String)> {
    let method = call.method.as_deref()?;
    let args: Vec<String> = call.args.iter().map(|a| brief(a)).collect();
    let call = &Call {
        args,
        ..call.clone()
    };
    let first = call.args.first().cloned();
    let generic_or_first = || call.generics.clone().or_else(|| first.clone());
    match method {
        "add_systems" => {
            let (schedule, systems) = call.args.split_first()?;
            Some(("system", format!("{} ({schedule})", systems.join(", "))))
        }
        "add_observer" => Some(("observer", first?)),
        "init_resource" | "insert_resource" => Some(("resource", generic_or_first()?)),
        "add_plugins" => Some(("plugin", first?)),
        "add_event" | "add_message" => Some(("event", generic_or_first()?)),
        "init_state" | "insert_state" => Some(("state", generic_or_first()?)),
        _ => None,
    }
}

/// Matches `call = "register::<$T>"` against a call; returns what `$T` captured
/// (or the first argument for patterns without `$T`).
pub fn match_pattern(pattern: &RelationPattern, call: &Call) -> Option<String> {
    let full = match (&call.method, &call.generics) {
        (Some(m), Some(g)) => format!("{m}::<{g}>"),
        (Some(m), None) => m.clone(),
        (None, _) => call.callee.clone(),
    };
    let pat: String = pattern
        .call
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let ends_on_segment = |text: &str, at: usize| at == 0 || text[..at].ends_with("::");
    match pat.split_once("$T") {
        Some((pre, post)) => {
            let body = full.strip_suffix(post)?;
            let at = body.rfind(pre)?;
            let captured = &body[at + pre.len()..];
            (ends_on_segment(body, at) && !captured.is_empty()).then(|| captured.to_string())
        }
        None => {
            let at = full.len().checked_sub(pat.len())?;
            (full.ends_with(&pat) && ends_on_segment(&full, at))
                .then(|| call.args.first().cloned().unwrap_or_default())
        }
    }
}

/// Framework labels for an item: `component` from `#[derive(Component)]`, `impl Plugin`.
pub fn labels(index: &Index, file: &FileIndex, item: &Item) -> Vec<String> {
    let mut out = Vec::new();
    if index.project.has_pack("bevy") {
        for d in &item.derives {
            let label = match d.as_str() {
                "Component" => "component",
                "Resource" => "resource",
                "Event" | "Message" | "EntityEvent" => "event",
                "SystemParam" => "system param",
                "States" | "SubStates" => "state",
                "Bundle" => "bundle",
                "Asset" => "asset",
                _ => continue,
            };
            out.push(label.to_string());
        }
    }
    if item.owner.is_none() {
        for f in index.files.iter().filter(|f| f.module == file.module) {
            for t in f.trait_impls.iter().filter(|t| t.ty == item.name) {
                out.push(format!("impl {}", t.trait_name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(callee: &str, method: Option<&str>, generics: Option<&str>) -> Call {
        Call {
            callee: callee.into(),
            method: method.map(String::from),
            generics: generics.map(String::from),
            args: vec!["app".into()],
            line: 1,
            in_test: false,
            owner: None,
        }
    }

    fn pattern(call: &str) -> RelationPattern {
        RelationPattern {
            call: call.into(),
            name: "registers".into(),
        }
    }

    /// `$T` captures the turbofish type on path and method calls, whole segments only.
    #[test]
    fn patterns_capture_types() {
        let p = pattern("register::<$T>");
        assert_eq!(
            match_pattern(&p, &call("super::register::<Spikes>", None, None)).as_deref(),
            Some("Spikes")
        );
        assert_eq!(
            match_pattern(&p, &call("register", Some("register"), Some("Saw"))).as_deref(),
            Some("Saw")
        );
        assert_eq!(
            match_pattern(&p, &call("preregister::<X>", None, None)),
            None
        );
        assert_eq!(match_pattern(&p, &call("register", None, None)), None);
    }

    /// Closures and struct literals are named by shape.
    #[test]
    fn brief_args() {
        assert_eq!(brief("|e: On<Add, P>| { x(); }"), "<closure>");
        assert_eq!(brief("TrapAssets { disc: 1 }"), "TrapAssets {..}");
        assert_eq!(brief("(a, b).run_if(c)"), "(a, b).run_if(c)");
    }

    /// Patterns without `$T` report the first argument.
    #[test]
    fn plain_patterns() {
        let p = pattern("spawn_trap");
        assert_eq!(
            match_pattern(&p, &call("world::spawn_trap", None, None)).as_deref(),
            Some("app")
        );
    }
}
