//! The TypeScript adapter's project: `package.json`, where the sources are, and the aliases.

use std::fs;
use std::path::Path;

use super::aliases;
use crate::project::{Config, Project};

/// True when `dir` holds a `package.json`.
pub fn detect(dir: &Path) -> bool {
    dir.join("package.json").is_file()
}

/// Reads the package name, every dependency table, the source directory and the aliases.
pub fn load(root: &Path, config: Config) -> Result<Project, String> {
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
    let source_dir = config
        .source
        .clone()
        .unwrap_or_else(|| detect_source_dir(root));
    let mut project = Project::new(root, name, "typescript", config);
    project.externals = [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .filter_map(|t| manifest[*t].as_object())
    .flat_map(|t| t.keys().cloned())
    .collect();
    project.aliases = aliases::load(root, &manifest, &source_dir);
    project.source_dir = source_dir;
    if !manifest["workspaces"].is_null() {
        project.hint =
            " (a workspace root: point --root at a package, e.g. packages/<name>)".to_string();
    }
    Ok(project)
}

/// `src`, `source` or `lib` when one exists, else the project root itself.
fn detect_source_dir(root: &Path) -> String {
    ["src", "source", "lib"]
        .iter()
        .find(|d| root.join(d).is_dir())
        .map_or_else(String::new, |d| d.to_string())
}
