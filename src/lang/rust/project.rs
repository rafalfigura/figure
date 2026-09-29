//! The Rust adapter's project: `Cargo.toml` and its dependency tables.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::project::{Config, Project};

/// True when `dir` holds a `Cargo.toml` with a `[package]` (a workspace root does not count).
pub fn detect(dir: &Path) -> bool {
    fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .is_some_and(|t| t.contains_key("package"))
}

/// Reads the package name and dependencies; the bevy relation pack is on when bevy is one.
pub fn load(root: &Path, config: Config) -> Result<Project, String> {
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
    let mut project = Project::new(root, name, "rust", config);
    project.externals = dependency_names(&manifest);
    if project
        .externals
        .iter()
        .any(|d| d == "bevy" || d.starts_with("bevy_"))
    {
        project.packs.push("bevy");
    }
    Ok(project)
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
