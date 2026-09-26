//! figure: compact, extracted code maps for AI agents.
//!
//! Each command finds the crate, parses its sources live, prints plain text and exits.

mod commands;
mod docs;
mod graph;
mod index;
mod lang;
mod model;
mod project;
mod relations;
mod render;
mod resolve;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::index::Index;
use crate::model::{ModPath, mod_display};
use crate::resolve::Found;

#[derive(Parser)]
#[command(
    name = "figure",
    version,
    about = "Compact code maps for AI agents: modules, symbols, recipes, dependencies."
)]
struct Cli {
    /// Crate root (the directory with Cargo.toml). Default: found from the path argument or the current directory.
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Module manifest: purpose, tree, public API, relations, dependencies, contracts, recipes.
    Map {
        /// A directory, a .rs file, or a module path (crate::traps).
        path: String,
        /// How many levels of submodules to include.
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// Show struct fields, enum variants and trait members.
        #[arg(long)]
        fields: bool,
        /// Include private items.
        #[arg(long)]
        private: bool,
    },
    /// One symbol or module: doc, signature, fields, relations, users.
    Show {
        /// `Harm`, `Harm::new`, `traps::register`, `crate::traps`, or a path on disk.
        symbol: String,
        /// Print the item's exact source with line numbers.
        #[arg(long)]
        body: bool,
    },
    /// List `# How to ...` recipes, or print one with its links checked.
    Howto {
        /// Words of the recipe topic, e.g. `trap` or `add a trap`.
        topic: Vec<String>,
    },
    /// What a module depends on, or with --reverse, who depends on it.
    Deps {
        path: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        #[arg(long)]
        reverse: bool,
    },
    /// Files without a module doc, undocumented public items, broken doc links.
    Check {
        /// Limit the check to one module (default: the whole crate).
        path: Option<String>,
        /// Exit with status 1 when anything is found.
        #[arg(long)]
        strict: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok((text, code)) => {
            print!("{text}");
            ExitCode::from(code)
        }
        Err(e) => {
            eprintln!("figure: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(String, u8), String> {
    let hint = match &cli.command {
        Command::Map { path, .. } | Command::Deps { path, .. } => Some(path.as_str()),
        Command::Show { symbol, .. } => Some(symbol.as_str()),
        Command::Check { path, .. } => path.as_deref(),
        Command::Howto { .. } => None,
    };
    let index = open(cli.root.as_deref(), hint)?;
    Ok(match cli.command {
        Command::Map {
            path,
            depth,
            fields,
            private,
        } => {
            let scope = scope(&index, &path)?;
            let opts = commands::map::Options {
                depth,
                fields,
                private,
            };
            (commands::map::run(&index, &scope, &opts), 0)
        }
        Command::Show { symbol, body } => {
            let found = match index.module_for_path(Path::new(&symbol)) {
                Some(m) if Path::new(&symbol).exists() => Found::Module(m),
                _ => pick(&index, &symbol)?,
            };
            (commands::show::run(&index, &found, body)?, 0)
        }
        Command::Howto { topic } => {
            let topic = (!topic.is_empty()).then(|| topic.join(" "));
            (commands::howto::run(&index, topic.as_deref())?, 0)
        }
        Command::Deps {
            path,
            depth,
            reverse,
        } => {
            let scope = scope(&index, &path)?;
            (commands::deps::run(&index, &scope, depth, reverse), 0)
        }
        Command::Check { path, strict } => {
            let scope = match path {
                Some(p) => scope(&index, &p)?,
                None => Vec::new(),
            };
            let report = commands::check::run(&index, &scope);
            (report.text, u8::from(strict && report.findings))
        }
    })
}

/// Finds the crate (from --root, the path argument, or the current directory) and indexes it.
fn open(root: Option<&Path>, hint: Option<&str>) -> Result<Index, String> {
    let root = match root {
        Some(r) => project::find_root(r),
        None => hint
            .map(Path::new)
            .filter(|p| p.exists())
            .and_then(project::find_root)
            .or_else(|| project::find_root(Path::new("."))),
    }
    .ok_or("no Cargo.toml with a [package] found (use --root)")?;
    Index::build(project::load(&root)?)
}

/// A path on disk or a module path -> a module of the index.
fn scope(index: &Index, arg: &str) -> Result<ModPath, String> {
    let path = Path::new(arg);
    if path.exists() {
        return index
            .module_for_path(path)
            .ok_or_else(|| format!("{arg} is not a module of crate {}", index.project.name));
    }
    index
        .module_named(arg)
        .ok_or_else(|| format!("no module or path named {arg}"))
}

/// Exactly one match for a symbol query, or an error listing the candidates.
fn pick(index: &Index, query: &str) -> Result<Found, String> {
    let found = resolve::find(index, query);
    match found.as_slice() {
        [] => Err(format!("nothing named {query} (try figure map)")),
        [one] => Ok(one.clone()),
        many => {
            let list: Vec<String> = many
                .iter()
                .map(|f| match f {
                    Found::Module(m) => format!("  {}  module", mod_display(m)),
                    Found::Item { file, item } => {
                        let file = &index.files[*file];
                        let item = &file.items[*item];
                        format!(
                            "  {}::{}  {}:{}",
                            mod_display(&file.module),
                            item.qualified_name(),
                            index::slash(&file.path),
                            item.decl_line
                        )
                    }
                })
                .collect();
            Err(format!(
                "{query} is ambiguous; use a longer path:\n{}",
                list.join("\n")
            ))
        }
    }
}
