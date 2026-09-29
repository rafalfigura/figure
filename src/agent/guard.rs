//! The guard: turns an agent's raw reads of indexed source into the figure call or line
//! range that answers the same question.
//!
//! It judges one `PreToolUse` call of Claude Code's `Read`, `Grep` or `Bash` tool and blocks:
//! - a `Read` of a whole indexed file longer than [`WHOLE_FILE_LINES`], listing each item's
//!   lines so the agent can read just one;
//! - `cat`, `head`, `tail`, `less`, `nl` and `bat` of such a file, and `sed -n` of any indexed
//!   file, translated into the Read that does the same;
//! - a `Grep` or `grep`/`rg` over the crate's source whose pattern is only symbols figure
//!   knows, pointing at `figure show` (who uses and calls it) or `figure deps --reverse`.
//!   Case-insensitive searches are text searches and pass.
//!
//! # Invariants
//! - Anything else passes, and so does everything when no crate is found or parsing fails:
//!   the guard only blocks what it can answer better.
//! - A `Read` with `offset` or `limit` always passes; it is the way to the code.

use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::pattern::symbol_search;
use super::shell::{self, Simple};
use crate::index::{Index, slash};
use crate::model::{FileIndex, ItemKind};
use crate::{lang, project};

/// Whole-file reads of indexed files up to this many lines pass.
pub const WHOLE_FILE_LINES: usize = 100;

const PAGERS: [&str; 8] = ["cat", "head", "tail", "less", "more", "nl", "bat", "view"];
const GREPS: [&str; 6] = ["grep", "egrep", "fgrep", "rg", "ag", "ack"];
const GREP_VALUES: [&str; 14] = [
    "-A",
    "-B",
    "-C",
    "-m",
    "-f",
    "-g",
    "-t",
    "-T",
    "-M",
    "-j",
    "--glob",
    "--type",
    "--include",
    "--exclude",
];

/// Why a `PreToolUse` call is blocked, or `None` to let it run.
pub fn judge(input: &Value) -> Option<String> {
    let tool = input["tool_name"].as_str()?;
    let args = &input["tool_input"];
    let cwd = input["cwd"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?
        .canonicalize()
        .ok()?;
    let relevant = match tool {
        "Read" => {
            lang::is_source_file(args["file_path"].as_str()?)
                && args["offset"].is_null()
                && args["limit"].is_null()
        }
        "Grep" => true,
        "Bash" => shell::commands(args["command"].as_str()?).iter().any(|c| {
            let names_rs = c.args.iter().any(|a| lang::is_source_file(a));
            (PAGERS.contains(&c.program.as_str()) || c.program == "sed") && names_rs
                || GREPS.contains(&c.program.as_str()) && !c.piped
        }),
        _ => false,
    };
    if !relevant {
        return None;
    }
    let root = project::find_root(&cwd)?;
    let index = Index::build(project::load(&root).ok()?).ok()?;
    let at = Place {
        index: &index,
        root: &root,
        cwd: &cwd,
    };
    match tool {
        "Read" => {
            let file = at.file(args["file_path"].as_str()?)?;
            (file.lines > WHOLE_FILE_LINES).then(|| whole_file(file, "Read"))
        }
        "Grep" => {
            let path = args["path"].as_str().unwrap_or(".");
            let other_language = args["glob"].as_str().is_some_and(|g| !at.names_language(g))
                || args["type"].as_str().is_some_and(|t| !at.is_type(t));
            (!other_language && at.covers(path))
                .then(|| symbol_search(&at, args["pattern"].as_str()?, "Grep"))?
        }
        _ => shell::commands(args["command"].as_str()?)
            .iter()
            .find_map(|c| bash(&at, c)),
    }
}

/// The crate and where the agent stands.
pub struct Place<'a> {
    pub index: &'a Index,
    root: &'a Path,
    cwd: &'a Path,
}

impl Place<'_> {
    /// `arg` relative to the crate root, when it lies inside it.
    fn rel(&self, arg: &str) -> Option<PathBuf> {
        let mut abs = PathBuf::new();
        for c in self.cwd.join(arg).components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    abs.pop();
                }
                c => abs.push(c),
            }
        }
        abs.strip_prefix(self.root).ok().map(Path::to_path_buf)
    }

    /// The indexed file `arg` names.
    fn file(&self, arg: &str) -> Option<&FileIndex> {
        let rel = self.rel(arg)?;
        self.index.files.iter().find(|f| f.path == rel)
    }

    /// True when `arg` is an indexed file or a directory holding some.
    /// True when a glob or option value names files of the project's language.
    fn names_language(&self, glob: &str) -> bool {
        let adapter = self.index.adapter();
        adapter.extensions().iter().any(|e| glob.contains(e))
    }

    /// True when a ripgrep `--type` covers the project's language.
    fn is_type(&self, name: &str) -> bool {
        self.index.adapter().grep_types().contains(&name)
    }

    fn covers(&self, arg: &str) -> bool {
        self.rel(arg)
            .is_some_and(|rel| self.index.files.iter().any(|f| f.path.starts_with(&rel)))
    }
}

/// The verdict on one simple shell command.
fn bash(at: &Place, cmd: &Simple) -> Option<String> {
    let program = cmd.program.as_str();
    if PAGERS.contains(&program) {
        let file = cmd
            .args
            .iter()
            .filter(|a| !a.starts_with('-'))
            .find_map(|a| at.file(a).filter(|f| f.lines > WHOLE_FILE_LINES))?;
        return Some(whole_file(file, program));
    }
    if program == "sed" {
        if !cmd.args.iter().any(|a| a == "-n") || cmd.args.iter().any(|a| a.starts_with("-i")) {
            return None;
        }
        let file = cmd
            .args
            .iter()
            .filter(|a| !a.starts_with('-'))
            .find_map(|a| at.file(a))?;
        let (from, to) = cmd.args.iter().find_map(|a| sed_range(a))?;
        return Some(range(file, from, to.min(file.lines)));
    }
    if GREPS.contains(&program) {
        let short = |c: char| {
            cmd.args
                .iter()
                .any(|a| a.starts_with('-') && !a.starts_with("--") && a.contains(c))
        };
        if short('i') || cmd.args.iter().any(|a| a == "--ignore-case") {
            return None; // case-insensitive: a text search, not a symbol lookup
        }
        let (pattern, paths) = shell::pattern_and_paths(&cmd.args, &GREP_VALUES);
        let recursive = !program.ends_with("grep")
            || short('r')
            || short('R')
            || cmd.args.iter().any(|a| a == "--recursive");
        let own_language = !cmd.args.iter().any(|a| {
            (a.starts_with("--include") || a.starts_with("--glob") || a.starts_with("-g"))
                && !at.names_language(a)
        });
        let in_crate = if paths.is_empty() {
            recursive && !cmd.piped && at.covers(".")
        } else {
            paths.iter().any(|p| at.covers(p))
        };
        return (in_crate && own_language).then(|| symbol_search(at, &pattern?, program))?;
    }
    None
}

/// `40,80p` -> (40, 80); `12p` -> (12, 12).
fn sed_range(script: &str) -> Option<(usize, usize)> {
    let body = script.strip_suffix('p')?;
    let (a, b) = body.split_once(',').unwrap_or((body, body));
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn whole_file(file: &FileIndex, how: &str) -> String {
    let path = slash(&file.path);
    let mut msg = format!(
        "figure guard: `{how}` of all of {path} ({} lines). Read only the item you need.\n\
         `figure map {path}` says what each item is for, `figure show <item>` its contract.\n\
         Then Read its lines (offset = start, limit = end - start + 1):\n",
        file.lines
    );
    msg.push_str(&items_between(file, 1, file.lines));
    msg.push_str("A Read with offset and limit always passes (e.g. of a whole file you are about to rewrite).\n");
    msg
}

fn range(file: &FileIndex, from: usize, to: usize) -> String {
    let path = slash(&file.path);
    let mut msg = format!(
        "figure guard: read source with the Read tool: file_path {path}, offset {from}, limit {}.\n",
        to + 1 - from.min(to)
    );
    let items = items_between(file, from, to);
    if !items.is_empty() {
        msg.push_str("Items in those lines (`figure show <item>` gives the contract, which may be enough):\n");
        msg.push_str(&items);
    }
    msg
}

/// `  fn Harm::new 13-16` for every item overlapping lines `from..=to`.
fn items_between(file: &FileIndex, from: usize, to: usize) -> String {
    let mut items: Vec<_> = file
        .items
        .iter()
        .filter(|i| !i.test_only && i.kind != ItemKind::Use)
        .filter(|i| i.start_line <= to && i.end_line >= from)
        .collect();
    items.sort_by_key(|i| i.start_line);
    items
        .iter()
        .map(|i| {
            format!(
                "  {} {} {}-{}\n",
                i.kind.keyword(),
                i.qualified_name(),
                i.start_line,
                i.end_line
            )
        })
        .collect()
}
