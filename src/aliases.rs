//! Import aliases of a JavaScript / TypeScript project: `paths` and `baseUrl` from
//! `tsconfig.json`, `imports` from `package.json`. They turn `@/store/cart` into the module
//! it names, so the dependency graph sees the edge.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use serde_json::Value;

use crate::jsonc::strip_jsonc;
use crate::model::ModPath;

const SOURCE_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// One alias: specifiers starting with `prefix` (or equal to it, when `exact`) live in `target`.
#[derive(Debug)]
struct Rule {
    prefix: String,
    exact: bool,
    target: ModPath,
}

/// All aliases of a project.
#[derive(Debug, Default)]
pub struct Aliases {
    /// Longest prefix first.
    rules: Vec<Rule>,
    /// `baseUrl` inside `src/`: bare specifiers are looked up below it.
    base: Option<ModPath>,
}

impl Aliases {
    /// Reads `tsconfig.json` (or `jsconfig.json`, following `extends`) and the `imports` of the parsed `package.json`.
    pub fn load(root: &Path, package: &Value, source_dir: &str) -> Aliases {
        let mut out = Aliases::default();
        let config = ["tsconfig.json", "jsconfig.json"]
            .iter()
            .find_map(|name| options(root, Path::new(name), 0));
        if let Some(config) = config {
            out.base = config
                .base
                .as_deref()
                .and_then(|b| under_src(b, "", source_dir));
            for (pattern, targets) in config.paths.iter().flat_map(|p| p.entries.iter()) {
                let target = targets.as_array().and_then(|t| t.first()?.as_str());
                out.add(
                    pattern,
                    target,
                    config.paths.as_ref().map_or(".", |p| &p.dir),
                    source_dir,
                );
            }
        }
        if let Some(imports) = package["imports"].as_object() {
            for (pattern, target) in imports {
                out.add(pattern, first_string(target), ".", source_dir);
            }
        }
        out.rules.sort_by(|a, b| {
            b.prefix
                .len()
                .cmp(&a.prefix.len())
                .then(a.prefix.cmp(&b.prefix))
        });
        out
    }

    fn add(&mut self, pattern: &str, target: Option<&str>, dir: &str, source_dir: &str) {
        let Some(target) = target else { return };
        let (prefix, exact) = match pattern.strip_suffix('*') {
            Some(p) => (p, false),
            None => (pattern, true),
        };
        if let Some(target) = under_src(dir, target.split('*').next().unwrap_or(""), source_dir) {
            self.rules.push(Rule {
                prefix: prefix.to_string(),
                exact,
                target,
            });
        }
    }

    /// The `crate::...` segments a bare specifier stands for, if an alias or `baseUrl` claims it.
    pub fn expand(&self, segs: &[String], modules: &BTreeSet<ModPath>) -> Option<Vec<String>> {
        let joined = segs.join("/");
        for rule in &self.rules {
            let rest = if rule.exact {
                (joined == rule.prefix).then_some("")
            } else {
                joined.strip_prefix(&rule.prefix)
            };
            if let Some(rest) = rest {
                let mut path = rule.target.clone();
                path.extend(rest.split('/').filter(|p| !p.is_empty()).map(String::from));
                return Some(crate_path(path));
            }
        }
        let base = self.base.as_ref()?;
        let mut path = base.clone();
        path.push(segs.first()?.clone());
        if !modules.contains(&path) {
            return None;
        }
        path.truncate(base.len());
        path.extend(segs.iter().cloned());
        Some(crate_path(path))
    }
}

/// The `paths` of one config and the directory (below the project root) they are relative to.
struct PathMap {
    entries: serde_json::Map<String, Value>,
    dir: String,
}

/// The compiler options that matter here, after following `extends`.
struct Options {
    /// `baseUrl`, relative to the project root.
    base: Option<String>,
    paths: Option<PathMap>,
}

/// Reads the config at `file` (relative to `root`); `extends` chains of relative files are
/// followed, and a field set in the child replaces the parent's.
fn options(root: &Path, file: &Path, depth: u32) -> Option<Options> {
    let text = fs::read_to_string(root.join(file)).ok()?;
    let config: Value = serde_json::from_str(&strip_jsonc(&text)).ok()?;
    let dir = file.parent().unwrap_or(Path::new(""));
    let parent = config["extends"]
        .as_str()
        .filter(|e| e.starts_with('.') && depth < 5)
        .and_then(|e| {
            let mut path = dir.join(e);
            if path.extension().is_none() {
                path.set_extension("json");
            }
            options(root, &path, depth + 1)
        });
    let (mut base, mut paths) = parent.map_or((None, None), |p| (p.base, p.paths));
    let opts = &config["compilerOptions"];
    if let Some(b) = opts["baseUrl"].as_str() {
        base = Some(dir.join(b).to_string_lossy().into_owned());
    }
    if let Some(entries) = opts["paths"].as_object() {
        let at = base
            .clone()
            .unwrap_or_else(|| dir.to_string_lossy().into_owned());
        paths = Some(PathMap {
            entries: entries.clone(),
            dir: at,
        });
    }
    Some(Options { base, paths })
}

fn first_string(value: &Value) -> Option<&str> {
    match value {
        Value::String(s) => Some(s),
        Value::Object(map) => map.values().find_map(first_string),
        _ => None,
    }
}

/// `crate` plus the module path, without a source extension or trailing `index`.
fn crate_path(mut path: ModPath) -> Vec<String> {
    if let Some(last) = path.last_mut()
        && let Some((stem, ext)) = last.rsplit_once('.')
        && SOURCE_EXTENSIONS.contains(&ext)
    {
        *last = stem.to_string();
    }
    if path.last().is_some_and(|l| l == "index") {
        path.pop();
    }
    std::iter::once("crate".to_string()).chain(path).collect()
}

/// `dir/target` (relative to the project root) as a module path below `src/`, if it is there.
fn under_src(dir: &str, target: &str, source_dir: &str) -> Option<ModPath> {
    let mut parts: Vec<String> = Vec::new();
    for c in Path::new(dir).join(target).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            _ => {}
        }
    }
    let below = Path::new(source_dir).components().count();
    parts
        .starts_with(&parts_of(source_dir))
        .then(|| parts[below..].to_vec())
}

fn parts_of(dir: &str) -> Vec<String> {
    Path::new(dir)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(s: &str) -> Vec<String> {
        s.split('/').map(String::from).collect()
    }

    /// tsconfig comments and trailing commas are tolerated; `paths` map onto `src/` modules.
    #[test]
    fn paths_and_base_url() {
        let dir = std::env::temp_dir().join(format!("figure-aliases-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("tsconfig.json"),
            "{ // c\n \"compilerOptions\": { \"baseUrl\": \"src\", /* x */\n \"paths\": { \"@/*\": [\"./*\"], \"@ui\": [\"ui/index.ts\"], }, }, }",
        )
        .unwrap();
        let package = serde_json::json!({ "imports": { "#lib/*": "./src/lib/*.js" } });
        let a = Aliases::load(&dir, &package, "src");
        let modules: BTreeSet<ModPath> = BTreeSet::from([segs("store")]);
        assert_eq!(
            a.expand(&segs("@/store/cart"), &modules),
            Some(segs("crate/store/cart"))
        );
        assert_eq!(a.expand(&segs("@ui"), &modules), Some(segs("crate/ui")));
        assert_eq!(
            a.expand(&segs("#lib/x"), &modules),
            Some(segs("crate/lib/x"))
        );
        assert_eq!(
            a.expand(&segs("store/cart"), &modules),
            Some(segs("crate/store/cart"))
        );
        assert_eq!(a.expand(&segs("react"), &modules), None);
        fs::remove_dir_all(dir).ok();
    }

    /// `extends` supplies `paths` relative to the extended file's directory.
    #[test]
    fn follows_extends() {
        let dir = std::env::temp_dir().join(format!("figure-extends-{}", std::process::id()));
        fs::create_dir_all(dir.join("config")).unwrap();
        fs::write(
            dir.join("config/base.json"),
            "{ \"compilerOptions\": { \"paths\": { \"~/*\": [\"../src/*\"] } } }",
        )
        .unwrap();
        fs::write(
            dir.join("tsconfig.json"),
            "{ \"extends\": \"./config/base\" }",
        )
        .unwrap();
        let a = Aliases::load(&dir, &serde_json::json!({}), "src");
        let modules = BTreeSet::new();
        assert_eq!(
            a.expand(&segs("~/store"), &modules),
            Some(segs("crate/store"))
        );
        fs::remove_dir_all(dir).ok();
    }
}
