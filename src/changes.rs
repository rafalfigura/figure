//! What changed since a git revision: which files, and which public items in them are new
//! or have a new signature. Used by `figure check --changed`, so a project can require docs
//! on new API without first documenting everything that already exists.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::index::Index;
use crate::lang::LanguageAdapter;
use crate::lang::rust::Rust;
use crate::model::{Item, ItemKind, Vis};

/// Files changed since `base` (committed, staged, unstaged or untracked).
pub struct ChangeSet {
    /// The revision compared against, as the user wrote it.
    pub base: String,
    /// Changed source files, relative to the project root.
    pub files: BTreeMap<PathBuf, FileChange>,
    /// Every changed path, source or not, relative to the project root.
    pub paths: BTreeSet<PathBuf>,
}

/// One changed source file and the public items it had at the base revision.
pub struct FileChange {
    /// The file did not exist at the base revision.
    pub new_file: bool,
    /// `fn Harm::new` -> its signature at the base revision.
    base_items: HashMap<String, String>,
}

impl ChangeSet {
    /// True when `item` in `path` is new or its signature changed since the base.
    pub fn is_new_or_changed(&self, path: &Path, item: &Item) -> bool {
        self.files
            .get(path)
            .is_some_and(|f| f.new_file || f.base_items.get(&key(item)) != Some(&item.signature))
    }

    /// True when `path` did not exist at the base revision.
    pub fn is_new_file(&self, path: &Path) -> bool {
        self.files.get(path).is_some_and(|f| f.new_file)
    }

    /// Number of changed source files and how many of them are new.
    pub fn counts(&self) -> (usize, usize) {
        let new = self.files.values().filter(|f| f.new_file).count();
        (self.files.len(), new)
    }
}

fn key(item: &Item) -> String {
    format!("{} {}", item.kind.keyword(), item.qualified_name())
}

/// Collects what changed in the project since `base` (`HEAD`, `origin/main`, a SHA).
pub fn since(index: &Index, base: &str) -> Result<ChangeSet, String> {
    let root = &index.project.root;
    git(root, &["rev-parse", "--is-inside-work-tree"])
        .map_err(|_| "--changed needs a git repository".to_string())?;
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{base}^{{commit}}"),
        ],
    )
    .map_err(|_| format!("unknown git revision {base}"))?;
    let mut paths: BTreeSet<PathBuf> = BTreeSet::new();
    let diff = git(
        root,
        &["diff", "--name-only", "-z", "--relative", base, "--"],
    )?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for p in diff.split('\0').chain(untracked.split('\0')) {
        if !p.is_empty() && root.join(p).is_file() {
            paths.insert(PathBuf::from(p));
        }
    }
    let adapter = Rust;
    let mut files = BTreeMap::new();
    for file in index.files.iter().filter(|f| paths.contains(&f.path)) {
        let spec = format!("{base}:./{}", crate::index::slash(&file.path));
        let change = match git(root, &["show", &spec]) {
            Ok(source) => FileChange {
                new_file: false,
                base_items: adapter
                    .parse(&source)
                    .items
                    .iter()
                    .filter(|i| i.vis >= Vis::Restricted && i.kind != ItemKind::Use)
                    .map(|i| (key(i), i.signature.clone()))
                    .collect(),
            },
            Err(_) => FileChange {
                new_file: true,
                base_items: HashMap::new(),
            },
        };
        files.insert(file.path.clone(), change);
    }
    Ok(ChangeSet {
        base: base.to_string(),
        files,
        paths,
    })
}

/// Runs `git -C root args...`, returning stdout or stderr as the error.
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
