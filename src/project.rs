//! The project being mapped: what every language adapter reports about its package, and the
//! optional `figure.toml`. Finding and reading the package manifest is the adapter's job.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::aliases::Aliases;
use crate::lang;

/// A package (a Cargo crate, an npm package, ...) and its figure settings.
pub struct Project {
    /// Directory holding the package manifest.
    pub root: PathBuf,
    pub name: String,
    /// Names of the packages this one depends on, spelled the way source refers to them.
    pub externals: BTreeSet<String>,
    /// Id of the language adapter that reads the sources.
    pub language: &'static str,
    /// Directory below `root` that holds the sources (`src`); empty when they start at `root`.
    pub source_dir: String,
    /// Import aliases the language configures (`tsconfig.json` `paths`).
    pub aliases: Aliases,
    /// Relation packs the manifest switches on (`bevy`).
    pub packs: Vec<&'static str>,
    /// Advice appended to the "no source directory" error (a monorepo root).
    pub hint: String,
    pub config: Config,
}

impl Project {
    /// A project with the defaults most adapters share; the adapter sets what differs.
    pub fn new(root: &Path, name: String, language: &'static str, config: Config) -> Project {
        Project {
            root: root.to_path_buf(),
            name,
            externals: BTreeSet::new(),
            language,
            source_dir: "src".to_string(),
            aliases: Aliases::default(),
            packs: Vec::new(),
            hint: String::new(),
            config,
        }
    }

    /// True when the manifest switched the relation pack `name` on.
    pub fn has_pack(&self, name: &str) -> bool {
        self.packs.contains(&name)
    }
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

/// The nearest directory at or above `start` that some language adapter recognises as a package.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start
    };
    loop {
        if lang::all().iter().any(|a| a.detect(&dir)) {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
}

/// Names of the manifests figure looks for: `Cargo.toml or package.json`.
pub fn manifest_names() -> String {
    let names: Vec<&str> = lang::all().iter().map(|a| a.manifest()).collect();
    names.join(" or ")
}

/// Reads the manifest of the first adapter that recognises `root`, and the optional `figure.toml`.
pub fn load(root: &Path) -> Result<Project, String> {
    let adapters = lang::all();
    let adapter = adapters
        .iter()
        .find(|a| a.detect(root))
        .ok_or_else(|| format!("no {} in {}", manifest_names(), root.display()))?;
    adapter.load_project(root, load_config(root)?)
}

/// Reads `figure.toml` in `root`; the defaults when there is none.
pub fn load_config(root: &Path) -> Result<Config, String> {
    let config_path = root.join("figure.toml");
    if !config_path.is_file() {
        return Ok(Config::default());
    }
    toml::from_str(&fs::read_to_string(&config_path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("invalid {}: {e}", config_path.display()))
}
