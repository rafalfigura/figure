//! The project being mapped: its `Cargo.toml` and the optional `figure.toml`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A Cargo package and its figure settings.
pub struct Project {
    /// Directory holding `Cargo.toml`.
    pub root: PathBuf,
    pub name: String,
    /// Crate names usable in paths (`-` replaced by `_`), from every dependency table.
    pub externals: BTreeSet<String>,
    /// The bevy relation pack is on when bevy is a dependency.
    pub bevy: bool,
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

/// The nearest directory at or above `start` whose `Cargo.toml` has a `[package]`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start
    };
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && fs::read_to_string(&manifest)
                .ok()
                .and_then(|s| s.parse::<toml::Table>().ok())
                .is_some_and(|t| t.contains_key("package"))
        {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
}

/// Reads `Cargo.toml` and the optional `figure.toml` in `root`.
pub fn load(root: &Path) -> Result<Project, String> {
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
    let config_path = root.join("figure.toml");
    let config = if config_path.is_file() {
        toml::from_str(&fs::read_to_string(&config_path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("invalid {}: {e}", config_path.display()))?
    } else {
        Config::default()
    };
    Ok(Project {
        root: root.to_path_buf(),
        name,
        externals,
        bevy,
        config,
    })
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
