//! Builds the in-memory index: discover source files, parse them in parallel, keep the
//! module tree. Nothing is stored between commands.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::lang;
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
        let adapter = lang::adapter(project.language)
            .ok_or_else(|| format!("no adapter for {}", project.language))?;
        let src = project.root.join(&project.source_dir);
        if !src.is_dir() {
            let hint = if project.workspaces {
                " (a workspace root: point --root at a package, e.g. packages/<name>)"
            } else {
                ""
            };
            return Err(format!(
                "no {}/ directory in {}{hint}",
                project.source_dir,
                project.root.display()
            ));
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
                let rel = path.strip_prefix(&src).unwrap_or(path);
                let mut file = adapter.parse(&source, rel);
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
                let dir = slash(f.path.parent().unwrap_or(Path::new("")));
                if dir.is_empty() {
                    "./".to_string()
                } else {
                    format!("{dir}/")
                }
            }
            Some(f) if self.defines_dir(f) => {
                format!("{}/", slash(f.path.parent().unwrap_or(Path::new(""))))
            }
            Some(f) => slash(&f.path),
            None => format!("{}/", self.source_path(module)),
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
        (module.is_empty() || self.defines_dir(file))
            .then(|| file.path.parent().map(Path::to_path_buf))?
    }

    /// True for the file that stands for its directory: `mod.rs`, or `index.ts` and friends.
    fn defines_dir(&self, file: &FileIndex) -> bool {
        let Some(name) = file.path.file_name().map(|n| n.to_string_lossy()) else {
            return false;
        };
        match self.language {
            "typescript" => name
                .rsplit_once('.')
                .is_some_and(|(stem, _)| stem == "index"),
            _ => name == "mod.rs",
        }
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
        let under_src = rel.strip_prefix(&self.project.source_dir).ok()?;
        let module: ModPath = under_src
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        self.modules.contains(&module).then_some(module)
    }

    /// Parses `crate::a::b`, `a::b` or `crate` (TypeScript: `a/b`, `a.b`) into a known module.
    pub fn module_named(&self, name: &str) -> Option<ModPath> {
        let mut segs = self.split_query(name);
        if segs.first().is_some_and(|s| s == "crate") {
            segs.remove(0);
        }
        self.modules.contains(&segs).then_some(segs)
    }

    /// `src/a/b`, or `a/b` when the sources start at the project root.
    fn source_path(&self, module: &[String]) -> String {
        let mut parts = vec![self.project.source_dir.clone()];
        parts.extend(module.iter().cloned());
        parts.retain(|p| !p.is_empty());
        parts.join("/")
    }

    fn is_typescript(&self) -> bool {
        self.language == "typescript"
    }

    /// Splits a user's symbol or module query into names: on `::`, and for TypeScript also
    /// on `/` and `.` (`store/cart`, `Cart.add`).
    pub fn split_query(&self, query: &str) -> Vec<String> {
        let query = query.replace("::", "/");
        let separators: &[char] = if self.is_typescript() {
            &['/', '.']
        } else {
            &['/']
        };
        query
            .split(separators)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// How output names a module: `crate::traps::shared` (Rust), `store/cart` (TypeScript).
    pub fn mod_name(&self, module: &[String]) -> String {
        match (self.is_typescript(), module.is_empty()) {
            (true, true) => "(root)".to_string(),
            (true, false) => module.join("/"),
            (false, _) => crate::model::mod_display(module),
        }
    }

    /// How output names a group of modules in a dependency listing.
    pub fn group_name(&self, key: &[String]) -> String {
        if self.is_typescript() {
            self.mod_name(key)
        } else {
            crate::graph::key_name(key)
        }
    }

    /// Unambiguous name of an item, module included: `crate::a::Type::method`, `a/Type.method`.
    pub fn full_name(&self, module: &[String], item: &crate::model::Item) -> String {
        if self.is_typescript() && !module.is_empty() {
            format!("{}/{}", module.join("/"), self.qualified(item))
        } else if self.is_typescript() {
            self.qualified(item)
        } else {
            format!("{}::{}", self.mod_name(module), self.qualified(item))
        }
    }

    /// `Type::method` (Rust), `Type.method` (TypeScript), the plain name for free items.
    pub fn qualified(&self, item: &crate::model::Item) -> String {
        match &item.owner {
            Some(owner) if self.is_typescript() => format!("{owner}.{}", item.name),
            _ => item.qualified_name(),
        }
    }
}

/// A path with `/` separators, for output.
pub fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
