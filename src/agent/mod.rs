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

use std::io::{self, Read};
use std::path::PathBuf;

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

/// `figure check --changed --strict` for the agent about to stop. Lets it stop the second
/// time (`stop_hook_active`), so a gap it cannot fix never traps it.
fn stop(input: &Value) -> Option<String> {
    if input["stop_hook_active"].as_bool() == Some(true) {
        return None;
    }
    let cwd = input["cwd"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())?;
    let index = Index::build(project::load(&project::find_root(&cwd)?).ok()?).ok()?;
    let changes = changes::since(&index, "HEAD").ok()?;
    let report = commands::check::run(&index, &Vec::new(), Some(&changes));
    report.findings.then(|| {
        format!(
            "figure check: document what this change added before you stop \
             (a `//!` line per new file, a `///` line per new or re-signed public item).\n{}",
            report.text
        )
    })
}
