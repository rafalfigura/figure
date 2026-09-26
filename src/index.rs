//! Builds the in-memory index: discover source files, parse them in parallel, keep the
//! module tree. Nothing is stored between commands.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::lang::LanguageAdapter;
use crate::lang::rust::Rust;
use crate::model::{FileIndex, ModPath};
use crate::project::Project;

/// The parsed crate: files, module tree and project settings.
pub struct Index {
    pub project: Project,
    /// Sorted by path.
    pub files: Vec<FileIndex>,
    /// Every module that has a file, plus their ancestors.
    pub modules: BTreeSet<ModPath>,
    /// Id of the language adapter that parsed the files.
    pub language: &'static str,
}

impl Index {
    /// Finds and parses every source file of `project`.
    pub fn build(project: Project) -> Result<Index, String> {
        let adapter = Rust;
        let src = project.root.join("src");
        if !src.is_dir() {
            return Err(format!("no src/ directory in {}", project.root.display()));
        }
        let mut found: Vec<(PathBuf, ModPath)> = Vec::new();
        for entry in ignore::WalkBuilder::new(&src).build().flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Ok(rel) = path.strip_prefix(&src) else {
                continue;
            };
            let parsed_here = path
                .extension()
                .is_some_and(|e| adapter.extensions().iter().any(|x| e == *x));
            if let Some(module) = adapter.module_of(rel).filter(|_| parsed_here) {
                found.push((path.to_path_buf(), module));
            }
        }
        let root = project.root.clone();
        let mut files: Vec<FileIndex> = found
            .par_iter()
            .map(|(path, module)| {
                let source = fs::read_to_string(path).unwrap_or_default();
                let mut file = adapter.parse(&source);
                file.path = path.strip_prefix(&root).unwrap_or(path).to_path_buf();
                file.module = module.clone();
                file
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut modules = BTreeSet::new();
        for file in &files {
            for n in 0..=file.module.len() {
                modules.insert(file.module[..n].to_vec());
            }
        }
        Ok(Index {
            project,
            files,
            modules,
            language: adapter.id(),
        })
    }

    /// Files that belong to `module` or any module below it.
    pub fn files_in<'a>(
        &'a self,
        module: &'a [String],
    ) -> impl Iterator<Item = (usize, &'a FileIndex)> {
        self.files
            .iter()
            .enumerate()
            .filter(move |(_, f)| f.module.starts_with(module))
    }

    /// The file that defines `module` itself (`mod.rs`, `foo.rs`, `lib.rs`).
    pub fn root_file(&self, module: &[String]) -> Option<&FileIndex> {
        self.files.iter().find(|f| f.module == module)
    }

    /// Display location of a module: `src/traps/` for a directory module, the file otherwise.
    pub fn module_location(&self, module: &[String]) -> String {
        match self.root_file(module) {
            Some(f) if module.is_empty() => {
                format!("{}/", slash(f.path.parent().unwrap_or(Path::new(""))))
            }
            Some(f) if f.path.file_name().is_some_and(|n| n == "mod.rs") => {
                format!("{}/", slash(f.path.parent().unwrap_or(Path::new(""))))
            }
            Some(f) => slash(&f.path),
            None => format!("src/{}/", module.join("/")),
        }
    }

    /// What a user types to name the module on the command line: `src/traps`, `src`,
    /// `src/core.rs`.
    pub fn module_arg(&self, module: &[String]) -> String {
        self.module_location(module)
            .trim_end_matches('/')
            .to_string()
    }

    /// Directory the module's files live in (`src/traps`, `src`), when it has one.
    pub fn module_dir(&self, module: &[String]) -> Option<PathBuf> {
        let file = self.root_file(module)?;
        let name = file.path.file_name()?.to_string_lossy();
        matches!(name.as_ref(), "mod.rs" | "main.rs" | "lib.rs")
            .then(|| file.path.parent().map(Path::to_path_buf))?
    }

    /// Maps a file or directory on disk to its module.
    pub fn module_for_path(&self, path: &Path) -> Option<ModPath> {
        let abs = path.canonicalize().ok()?;
        let root = self.project.root.canonicalize().ok()?;
        let rel = abs.strip_prefix(&root).ok()?;
        if abs.is_file() {
            return self
                .files
                .iter()
                .find(|f| f.path == rel)
                .map(|f| f.module.clone());
        }
        let under_src = rel.strip_prefix("src").ok()?;
        let module: ModPath = under_src
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        self.modules.contains(&module).then_some(module)
    }

    /// Parses `crate::a::b`, `a::b` or `crate` into a known module.
    pub fn module_named(&self, name: &str) -> Option<ModPath> {
        let mut segs: Vec<String> = name
            .split("::")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if segs.first().is_some_and(|s| s == "crate") {
            segs.remove(0);
        }
        self.modules.contains(&segs).then_some(segs)
    }
}

/// A path with `/` separators, for output.
pub fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
