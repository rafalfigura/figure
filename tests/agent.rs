//! `figure mcp` and `figure hook`: JSON in on stdin, answers and exit codes out, on the
//! example project and on small crates written to a temporary directory.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

/// Runs figure in `dir` with `stdin`: (exit code, stdout, stderr).
fn piped(dir: &Path, args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_figure"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("figure runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A crate with one 120-line file `src/big.rs`: `fn first` on lines 1-3, filler, `fn last`.
fn big_crate(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figure-agent-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
    )
    .unwrap();
    let mut big = String::from("/// First.\npub fn first() {\n}\n");
    big.push_str(&"// filler\n".repeat(114));
    big.push_str("/// Last.\npub fn last() {\n    first();\n}\n");
    fs::write(dir.join("src/big.rs"), big).unwrap();
    fs::write(dir.join("src/lib.rs"), "//! Lib.\npub mod big;\n").unwrap();
    dir
}

/// The guard's verdict on one tool call made in `dir`: (exit code, message).
fn guard(dir: &Path, tool: &str, input: Value) -> (i32, String) {
    let payload = json!({ "tool_name": tool, "cwd": dir, "tool_input": input }).to_string();
    let (code, _, err) = piped(dir, &["hook", "guard"], &payload);
    (code, err)
}

/// A whole-file Read of a long indexed file is blocked with each item's lines; a ranged
/// Read of it passes.
#[test]
fn guard_turns_whole_reads_into_ranges() {
    let dir = big_crate("reads");
    let file = dir.join("src/big.rs");
    let (code, msg) = guard(&dir, "Read", json!({ "file_path": file }));
    assert_eq!(code, 2, "{msg}");
    assert!(
        msg.contains("fn first 1-3") && msg.contains("fn last 118-121"),
        "{msg}"
    );
    let (code, _) = guard(
        &dir,
        "Read",
        json!({ "file_path": file, "offset": 118, "limit": 4 }),
    );
    assert_eq!(code, 0);
    let (code, _) = guard(&dir, "Read", json!({ "file_path": dir.join("src/lib.rs") }));
    assert_eq!(code, 0, "short files may be read whole");
}

/// `cat` and `sed -n` of indexed source are redirected; `sed -n` names the equivalent Read.
#[test]
fn guard_redirects_shell_reads() {
    let dir = big_crate("shell");
    let (code, msg) = guard(
        &dir,
        "Bash",
        json!({ "command": "cat src/big.rs 2>/dev/null" }),
    );
    assert_eq!(code, 2, "{msg}");
    let (code, msg) = guard(
        &dir,
        "Bash",
        json!({ "command": "cd x && sed -n 110,121p src/big.rs" }),
    );
    assert_eq!(code, 2);
    assert!(
        msg.contains("offset 110, limit 12") && msg.contains("fn last"),
        "{msg}"
    );
    let (code, _) = guard(
        &dir,
        "Bash",
        json!({ "command": "sed -i 's/a/b/' src/big.rs && cargo build 2>&1 | tail" }),
    );
    assert_eq!(code, 0, "editing and filtering output are not reads");
    let (code, _) = guard(
        &dir,
        "Bash",
        json!({ "command": "python3 - <<'EOF'\nprint(open('src/big.rs').read())\ncat src/big.rs\nEOF" }),
    );
    assert_eq!(code, 0, "a heredoc body is data");
}

/// A grep for a symbol points at `figure show`; text, case-insensitive and piped greps pass.
#[test]
fn guard_redirects_symbol_greps() {
    let dir = big_crate("greps");
    let (code, msg) = guard(
        &dir,
        "Bash",
        json!({ "command": "grep -rn 'fn last\\|first(' src" }),
    );
    assert_eq!(code, 2);
    assert!(
        msg.contains("figure show last") && msg.contains("figure show first"),
        "{msg}"
    );
    let (code, msg) = guard(&dir, "Grep", json!({ "pattern": "big::", "type": "rust" }));
    assert_eq!(code, 2);
    assert!(msg.contains("figure deps src/big.rs --reverse"), "{msg}");
    for command in [
        "grep -rn 'filler' src",
        "grep -rni last src",
        "cargo test | grep last",
        "grep -rn '\\w+_of' src",
    ] {
        assert_eq!(
            guard(&dir, "Bash", json!({ "command": command })).0,
            0,
            "{command}"
        );
    }
    assert_eq!(
        guard(&dir, "Grep", json!({ "pattern": "last", "glob": "*.md" })).0,
        0
    );
}

/// Outside a crate, or with input that is not a tool call, the guard lets everything run.
#[test]
fn guard_fails_open() {
    let (code, _) = guard(
        &std::env::temp_dir(),
        "Bash",
        json!({ "command": "cat foo.rs" }),
    );
    assert_eq!(code, 0);
    let (code, _, _) = piped(&std::env::temp_dir(), &["hook", "guard"], "not json");
    assert_eq!(code, 0);
}

/// The stop hook blocks once on undocumented new public API, then lets the agent stop.
#[test]
fn stop_hook_blocks_once() {
    let dir = big_crate("stop");
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&dir)
                .status()
                .unwrap()
                .success()
        );
    }
    fs::write(
        dir.join("src/lib.rs"),
        "//! Lib.\npub mod big;\npub fn fresh() {}\n",
    )
    .unwrap();
    let lib = dir.canonicalize().unwrap().join("src/lib.rs");
    let stop = |active: bool, tool: Value| {
        let record = json!({ "type": "assistant", "message": { "content": [tool] } });
        let transcript = dir.join("transcript.jsonl");
        fs::write(&transcript, format!("{record}\n")).unwrap();
        let payload =
            json!({ "cwd": dir, "stop_hook_active": active, "transcript_path": transcript });
        piped(&dir, &["hook", "stop"], &payload.to_string())
    };
    let wrote = json!({ "type": "tool_use", "name": "Write", "input": { "file_path": lib } });
    let (code, _, err) = stop(false, wrote.clone());
    assert_eq!(code, 2);
    assert!(err.contains("fn fresh"), "{err}");
    assert_eq!(stop(true, wrote).0, 0);
    let scripted = json!({ "type": "tool_use", "name": "Bash", "input": { "command": "sed -i 's/x/y/' src/lib.rs" } });
    assert_eq!(
        stop(false, scripted).0,
        2,
        "a file a shell command named counts as touched"
    );
}

/// Uncommitted work from before the session is not the session's to document.
#[test]
fn stop_hook_ignores_untouched_files() {
    let dir = big_crate("untouched");
    for args in [
        &["init", "-q"][..],
        &["add", "."],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&dir)
                .status()
                .unwrap()
                .success()
        );
    }
    fs::write(
        dir.join("src/lib.rs"),
        "//! Lib.\npub mod big;\npub fn fresh() {}\n",
    )
    .unwrap();
    let transcript = dir.join("transcript.jsonl");
    let read = json!({ "type": "assistant", "message": { "content": [{ "type": "tool_use", "name": "Read", "input": { "file_path": "src/big.rs" } }] } });
    fs::write(&transcript, format!("{read}\n")).unwrap();
    let payload = json!({ "cwd": dir, "stop_hook_active": false, "transcript_path": transcript });
    assert_eq!(piped(&dir, &["hook", "stop"], &payload.to_string()).0, 0);
}

/// One MCP session: handshake with instructions, five read-only tools, a call that returns
/// the same text as the command, an error for a bad symbol, silence for notifications.
#[test]
fn mcp_session() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/rust");
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "show", "arguments": { "symbol": "Harm::new" } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "show", "arguments": { "symbol": "Nope" } } }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" }),
    ];
    let stdin: String = requests.iter().map(|r| format!("{r}\n")).collect();
    let (code, out, _) = piped(&example, &["mcp"], &stdin);
    assert_eq!(code, 0);
    let replies: Vec<Value> = out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 5, "no reply to the notification");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert!(
        replies[0]["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("top down")
    );
    let names: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["map", "deps", "show", "howto", "check"]);
    let expected = fs::read_to_string(example.join("expected/show_method.txt")).unwrap();
    assert_eq!(
        replies[2]["result"]["content"][0]["text"],
        expected.as_str()
    );
    assert_eq!(replies[3]["result"]["isError"], true);
    assert_eq!(replies[4]["error"]["code"], -32601);
}
