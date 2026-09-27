//! Agent integration: figure as an MCP server, and as Claude Code hooks that steer an agent
//! from raw source reads to figure.
//!
//! The Claude Code plugin in `plugin/` wires all of it up with one install; `figure mcp` and
//! `figure hook <guard|stop>` are what it runs.
//!
//! # Invariants
//! - A hook fails open: bad input, no crate, a parse error or a panic lets the agent go on.
//! - A hook blocks with exit code 2 and says why on stderr; Claude Code shows that to the agent.

pub mod guard;
pub mod mcp;
mod pattern;
mod shell;

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::changes;
use crate::commands;
use crate::index::Index;
use crate::project;

/// A Claude Code hook figure answers.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum Hook {
    /// `PreToolUse` on Read, Grep and Bash: block raw reads of indexed source.
    Guard,
    /// `Stop`: block stopping while new or changed public API is undocumented.
    Stop,
}

/// Runs `hook` on the JSON Claude Code sends on stdin: the exit code (2 blocks) and the
/// message for the agent.
pub fn run(hook: Hook) -> (u8, String) {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return (0, String::new());
    }
    let Ok(input) = serde_json::from_str::<Value>(&input) else {
        return (0, String::new());
    };
    std::panic::set_hook(Box::new(|_| {}));
    let verdict = std::panic::catch_unwind(|| match hook {
        Hook::Guard => guard::judge(&input),
        Hook::Stop => stop(&input),
    });
    match verdict {
        Ok(Some(message)) => (2, message),
        _ => (0, String::new()),
    }
}

/// `figure check --changed --strict` for the agent about to stop, on the files this session
/// touched: work left uncommitted before it started is not its to document. Lets it stop the
/// second time (`stop_hook_active`), so a gap it cannot fix never traps it.
fn stop(input: &Value) -> Option<String> {
    if input["stop_hook_active"].as_bool() == Some(true) {
        return None;
    }
    let cwd = input["cwd"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    let root = project::find_root(&cwd)?;
    let session = Session::read(Path::new(input["transcript_path"].as_str()?))?;
    let index = Index::build(project::load(&root).ok()?).ok()?;
    let mut changes = changes::since(&index, "HEAD").ok()?;
    changes.paths.retain(|p| session.touched(&root, p));
    changes.files.retain(|p, _| session.touched(&root, p));
    if changes.paths.is_empty() {
        return None;
    }
    let report = commands::check::run(&index, &Vec::new(), Some(&changes));
    report.findings.then(|| {
        format!(
            "figure check: document what this change added before you stop \
             (a `//!` line per new file, a `///` line per new or re-signed public item).\n{}",
            report.text
        )
    })
}

/// What a session wrote, from its Claude Code transcript.
struct Session {
    /// `file_path` of every Edit, MultiEdit, Write and NotebookEdit call.
    edited: BTreeSet<PathBuf>,
    /// Every Bash command, joined: scripted edits name the files they write.
    commands: String,
}

impl Session {
    fn read(transcript: &Path) -> Option<Session> {
        let text = fs::read_to_string(transcript).ok()?;
        let mut session = Session {
            edited: BTreeSet::new(),
            commands: String::new(),
        };
        for record in text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        {
            let Some(content) = record["message"]["content"].as_array() else {
                continue;
            };
            for tool in content.iter().filter(|c| c["type"] == "tool_use") {
                let input = &tool["input"];
                match tool["name"].as_str().unwrap_or_default() {
                    "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
                        session
                            .edited
                            .extend(input["file_path"].as_str().map(PathBuf::from));
                    }
                    "Bash" => {
                        session
                            .commands
                            .push_str(input["command"].as_str().unwrap_or_default());
                        session.commands.push('\n');
                    }
                    _ => {}
                }
            }
        }
        Some(session)
    }

    /// True when `rel` (relative to `root`) was edited, or named by a shell command.
    fn touched(&self, root: &Path, rel: &Path) -> bool {
        self.edited.contains(&root.join(rel)) || self.commands.contains(&*rel.to_string_lossy())
    }
}
