//! `figure mcp`: figure as an MCP server on stdin/stdout.
//!
//! Agents get figure's commands as tools next to Read and Grep, which they prefer over shell
//! commands, and the reading order in the server's instructions, which clients put in the
//! system prompt. Each call parses the crate afresh, so answers follow the agent's edits.
//!
//! # Gotchas
//! - stdout carries only JSON-RPC messages, one per line; nothing else may print there.
//! - A tool call becomes a command line and goes through the same parser and [`crate::run`]
//!   as the CLI, so a tool can never drift from its command.

use std::io::{self, BufRead, Write};
use std::path::Path;

use clap::Parser;
use serde_json::{Value, json};

use crate::Cli;

/// Protocol version answered when the client does not ask for one.
const PROTOCOL: &str = "2025-06-18";

/// The reading order, sent as the server's instructions.
pub const INSTRUCTIONS: &str = "\
figure maps this Rust crate from its source and docs. Use its tools instead of Read, Grep, \
cat, sed or grep to explore .rs files. Work top down and go one level deeper only when the \
level above cannot answer your question:
1. Connections. `map` a directory or module: purpose, tree, public API, relations. `deps`: \
what it uses; with reverse, who uses it. Answers: where does this live, what talks to what, \
what does a change touch.
2. Contracts. `show` one item or module: signature, doc, fields, methods, who wires, uses \
and calls it, and its file:start-end. `howto` before adding anything; follow the recipe.
3. Code. Only when the doc does not answer your question, or to edit the item: Read that \
file:start-end range (offset and limit), never the whole file.
Know the question before each call and stop when it is answered. Reading the code of more \
than ~3 items for one task means you are reading, not navigating: go back to map, deps and \
show. Grep only for text that is not a symbol (a string literal, a log message). After a \
change, run `check` with changed.";

/// Serves JSON-RPC requests from stdin until it closes.
pub fn serve(root: Option<&Path>) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg, root),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The answer to one message; `None` for notifications, which get no answer.
pub fn handle(msg: &Value, root: Option<&Path>) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let params = &msg["params"];
    let result = match msg["method"].as_str().unwrap_or_default() {
        "initialize" => json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "figure", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => call(params, root),
        other => return Some(error(id, -32601, &format!("unknown method {other}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools() -> Value {
    let string = |d: &str| json!({ "type": "string", "description": d });
    let flag = |d: &str| json!({ "type": "boolean", "description": d });
    let depth = json!({ "type": "integer", "minimum": 1, "description": "Levels of submodules to include (default 1)." });
    let tool = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
            "annotations": { "readOnlyHint": true },
        })
    };
    json!([
        tool(
            "map",
            "Step 1, connections. Module manifest of a directory, .rs file or module path: \
             purpose, shape, tree, public API with docs, relations, dependencies, contracts, \
             recipes. Start here in any module you have not mapped yet.",
            json!({
                "path": string("A directory (src/traps), a .rs file, or a module path (crate::traps)."),
                "depth": depth,
                "fields": flag("Also list struct fields, enum variants and trait members."),
                "private": flag("Also list private items."),
            }),
            &["path"],
        ),
        tool(
            "deps",
            "Step 1, connections. What a module depends on, with file counts and symbols; with \
             reverse, who depends on it, with file:line. Use it to see what a change touches.",
            json!({
                "path": string("A directory, a .rs file, or a module path."),
                "depth": depth,
                "reverse": flag("Who depends on the module instead of what it depends on."),
            }),
            &["path"],
        ),
        tool(
            "show",
            "Step 2, contracts. One item or module: file:start-end, signature, doc, fields, \
             methods, relations, who wires, uses and calls it. To see or edit its code, Read \
             only that line range; figure prints no source.",
            json!({ "symbol": string("Harm, Harm::new, traps::register, crate::traps, or a path on disk.") }),
            &["symbol"],
        ),
        tool(
            "howto",
            "Step 2, contracts. Without a topic, lists the crate's `How to` recipes; with one, \
             prints it with every link resolved to file:line. Check it before adding anything.",
            json!({ "topic": string("Words of the recipe topic, e.g. `add a trap`.") }),
            &[],
        ),
        tool(
            "check",
            "After a change. Files without a module doc, undocumented public items, broken doc \
             links. Pass changed to check only what changed since a git revision.",
            json!({
                "path": string("Limit the check to one module (default: the whole crate)."),
                "changed": string("Only what changed since this git revision, e.g. HEAD or origin/main."),
            }),
            &[],
        ),
    ])
}

/// Runs one tool as the command line it stands for.
fn call(params: &Value, root: Option<&Path>) -> Value {
    let text = |t: String, is_error: bool| json!({ "content": [{ "type": "text", "text": t }], "isError": is_error });
    let Some(argv) = command_line(params, root) else {
        return text(format!("unknown tool {}", params["name"]), true);
    };
    match Cli::try_parse_from(&argv) {
        Err(e) => text(e.to_string(), true),
        Ok(cli) => match crate::run(cli) {
            Ok((out, _)) => text(out, false),
            Err(e) => text(e, true),
        },
    }
}

/// `{"name": "show", "arguments": {"symbol": "Harm"}}` -> `figure show Harm`.
fn command_line(params: &Value, root: Option<&Path>) -> Option<Vec<String>> {
    let args = &params["arguments"];
    let s = |key: &str| args[key].as_str().map(str::to_string);
    let on = |key: &str| args[key].as_bool().unwrap_or(false);
    let name = params["name"].as_str()?;
    let mut argv = vec!["figure".to_string(), name.to_string()];
    match name {
        "map" | "deps" => {
            argv.extend(s("path"));
            if let Some(d) = args["depth"].as_u64() {
                argv.extend(["--depth".into(), d.to_string()]);
            }
            for flag in ["fields", "private", "reverse"] {
                if on(flag) {
                    argv.push(format!("--{flag}"));
                }
            }
        }
        "show" => argv.extend(s("symbol")),
        "howto" => argv.extend(
            s("topic")
                .iter()
                .flat_map(|t| t.split_whitespace().map(str::to_string)),
        ),
        "check" => {
            argv.extend(s("path"));
            if let Some(base) = s("changed") {
                argv.push(format!("--changed={base}"));
            }
        }
        _ => return None,
    }
    if let Some(root) = root {
        argv.extend(["--root".into(), root.display().to_string()]);
    }
    Some(argv)
}
