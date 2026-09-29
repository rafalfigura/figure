//! The project being mapped: its `Cargo.toml` or `package.json` and the optional `figure.toml`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A Cargo or npm package and its figure settings.
pub struct Project {
    /// Directory holding `Cargo.toml` or `package.json`.
    pub root: PathBuf,
    pub name: String,
    /// Crate names usable in paths (`-` replaced by `_`), from every dependency table.
    pub externals: BTreeSet<String>,
    /// The bevy relation pack is on when bevy is a dependency.
    pub bevy: bool,
    /// Id of the language adapter that reads the sources: `rust` or `typescript`.
    pub language: &'static str,
    /// Directory below `root` that holds the sources (`src`); empty when they start at `root`.
    pub source_dir: String,
    /// A `package.json` with `workspaces`: a monorepo root, whose packages are separate projects.
    pub workspaces: bool,
    /// Import aliases (`tsconfig.json` `paths`, `package.json` `imports`).
    pub aliases: crate::aliases::Aliases,
    pub config: Config,
}

/// `figure.toml`: only what cannot be inferred from the code.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub relations: Vec<RelationPattern>,
    #[serde(default)]
    pub docs: DocsConfig,
    /// Directory with the sources when it is not `src`, `source` or `lib`.
    pub source: Option<String>,
}

/// A project's own call convention, e.g. `call = "register::<$T>"`, `name = "registers"`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationPattern {
    pub call: String,
    pub name: String,
}

/// `[docs]` in `figure.toml`.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DocsConfig {
    /// `/// YAML: ...` lines are shown under items when `YAML` is listed here.
    #[serde(default)]
    pub fact_prefixes: Vec<String>,
}

/// The nearest directory at or above `start` whose `Cargo.toml` has a `[package]` or that
/// has a `package.json`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start
    };
    loop {
        if is_cargo_package(&dir) || dir.join("package.json").is_file() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
}

fn is_cargo_package(dir: &Path) -> bool {
    fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .is_some_and(|t| t.contains_key("package"))
}

/// Reads `Cargo.toml` (or else `package.json`) and the optional `figure.toml` in `root`.
pub fn load(root: &Path) -> Result<Project, String> {
    if !is_cargo_package(root) && root.join("package.json").is_file() {
        return load_npm(root);
    }
    let manifest_path = root.join("Cargo.toml");
    let manifest: toml::Table = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("cannot read {}: {e}", manifest_path.display()))?
        .parse()
        .map_err(|e| format!("cannot parse {}: {e}", manifest_path.display()))?;
    let name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("crate")
        .to_string();
    let externals = dependency_names(&manifest);
    let bevy = externals
        .iter()
        .any(|d| d == "bevy" || d.starts_with("bevy_"));
    Ok(Project {
        root: root.to_path_buf(),
        name,
        externals,
        bevy,
        language: "rust",
        source_dir: "src".into(),
        aliases: Default::default(),
        workspaces: false,
        config: load_config(root)?,
    })
}

/// Reads `package.json`: the package name and every dependency table.
fn load_npm(root: &Path) -> Result<Project, String> {
    let path = root.join("package.json");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?,
    )
    .map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
    let name = manifest["name"]
        .as_str()
        .or_else(|| root.file_name().and_then(|n| n.to_str()))
        .unwrap_or("package")
        .to_string();
    let externals = [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .filter_map(|t| manifest[*t].as_object())
    .flat_map(|t| t.keys().cloned())
    .collect();
    let config = load_config(root)?;
    let source_dir = config
        .source
        .clone()
        .unwrap_or_else(|| detect_source_dir(root));
    Ok(Project {
        root: root.to_path_buf(),
        name,
        externals,
        bevy: false,
        language: "typescript",
        aliases: crate::aliases::Aliases::load(root, &manifest, &source_dir),
        source_dir,
        workspaces: !manifest["workspaces"].is_null(),
        config,
    })
}

/// `src`, `source` or `lib` when one exists, else the project root itself.
fn detect_source_dir(root: &Path) -> String {
    ["src", "source", "lib"]
        .iter()
        .find(|d| root.join(d).is_dir())
        .map_or_else(String::new, |d| d.to_string())
}

fn load_config(root: &Path) -> Result<Config, String> {
    let config_path = root.join("figure.toml");
    if !config_path.is_file() {
        return Ok(Config::default());
    }
    toml::from_str(&fs::read_to_string(&config_path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("invalid {}: {e}", config_path.display()))
}

fn dependency_names(manifest: &toml::Table) -> BTreeSet<String> {
    const TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut tables: Vec<&toml::Table> = TABLES
        .iter()
        .filter_map(|t| manifest.get(*t).and_then(|v| v.as_table()))
        .collect();
    if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
        for target in targets.values().filter_map(|t| t.as_table()) {
            tables.extend(
                TABLES
                    .iter()
                    .filter_map(|t| target.get(*t).and_then(|v| v.as_table())),
            );
        }
    }
    tables
        .into_iter()
        .flat_map(|t| t.keys())
        .map(|k| k.replace('-', "_"))
        .collect()
}
