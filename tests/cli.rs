//! Exit codes and failure paths, on the example project and on small crates written
//! to a temporary directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn figure(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_figure"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("figure runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/rust")
}

/// A throwaway crate: `files` are (path under the crate, contents).
fn temp_crate(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("figure-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
    )
    .unwrap();
    for (path, body) in files {
        let path = dir.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    dir
}

/// `check --strict` fails when anything is undocumented; plain `check` never fails.
#[test]
fn strict_check_fails_on_gaps() {
    assert_eq!(figure(&example(), &["check"]).status.code(), Some(0));
    assert_eq!(
        figure(&example(), &["check", "--strict"]).status.code(),
        Some(1)
    );
}

/// A renamed symbol breaks the recipe link that named it, and `check --strict` fails.
#[test]
fn renamed_symbol_breaks_recipe() {
    let lib = "//! Shapes.\n//!\n//! # How to add a shape\n//! Copy [`Circle`].\n\n/// A circle.\npub struct Circle;\n";
    let dir = temp_crate("renamed", &[("src/lib.rs", lib)]);
    let ok = figure(&dir, &["check", "--strict"]);
    assert_eq!(ok.status.code(), Some(0), "{}", stdout(&ok));

    fs::write(
        dir.join("src/lib.rs"),
        lib.replace("pub struct Circle", "pub struct Round"),
    )
    .unwrap();
    let broken = figure(&dir, &["check", "--strict"]);
    assert_eq!(broken.status.code(), Some(1));
    assert!(
        stdout(&broken).contains("broken: src/lib.rs:4 [`Circle`]"),
        "{}",
        stdout(&broken)
    );
    let howto = figure(&dir, &["howto", "shape"]);
    assert!(
        stdout(&howto).contains("Circle (BROKEN)"),
        "{}",
        stdout(&howto)
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Unknown and ambiguous names exit 2 with the candidates listed.
#[test]
fn lookup_errors() {
    let unknown = figure(&example(), &["show", "Nope"]);
    assert_eq!(unknown.status.code(), Some(2));
    let ambiguous = figure(&example(), &["show", "plugin"]);
    assert_eq!(ambiguous.status.code(), Some(2));
    let err = String::from_utf8_lossy(&ambiguous.stderr);
    assert!(err.contains("crate::traps::saw::plugin"), "{err}");
    assert_eq!(
        figure(&example(), &["map", "src/nope"]).status.code(),
        Some(2)
    );
    assert_eq!(
        figure(&example(), &["show", "crate::traps", "--body"])
            .status
            .code(),
        Some(2)
    );
}

/// `--root` finds the crate from any working directory; paths are then module paths.
#[test]
fn root_flag() {
    let root = example();
    let out = figure(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &["--root", root.to_str().unwrap(), "map", "crate::core"],
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout(&out).starts_with("MODULE        crate::core"),
        "{}",
        stdout(&out)
    );
}

/// A file with syntax errors is still indexed and reported.
#[test]
fn syntax_errors_reported() {
    let dir = temp_crate(
        "broken",
        &[(
            "src/lib.rs",
            "//! Broken.\n\n/// Fine.\npub fn ok() {}\n\npub fn bad( {\n",
        )],
    );
    let out = figure(&dir, &["check"]);
    assert!(
        stdout(&out).contains("syntax errors 1 file"),
        "{}",
        stdout(&out)
    );
    let map = figure(&dir, &["map", "src"]);
    assert!(stdout(&map).contains("fn ok()"), "{}", stdout(&map));
    fs::remove_dir_all(dir).unwrap();
}

/// Without figure.toml or bevy, there is no relation section and nothing else breaks.
#[test]
fn works_without_config() {
    let dir = temp_crate(
        "plain",
        &[
            (
                "src/main.rs",
                "//! Plain.\n\nmod a;\n\nfn main() { a::f(); }\n",
            ),
            ("src/a.rs", "//! A.\n\n/// F.\npub fn f() {}\n"),
        ],
    );
    let out = figure(&dir, &["map", "src"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!stdout(&out).contains("RELATIONS"), "{}", stdout(&out));
    let deps = figure(&dir, &["deps", "src/a.rs", "--reverse"]);
    assert!(
        stdout(&deps).contains("src/main.rs:5  f"),
        "{}",
        stdout(&deps)
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Runs git in `dir` with a throwaway identity.
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=figure",
            "-c",
            "user.email=figure@example.com",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs")
        .status
        .success();
    assert!(ok, "git {}", args.join(" "));
}

/// `--changed` reports only new or re-signed public items and new files; old gaps are ignored.
#[test]
fn changed_checks_only_new_api() {
    let old = "//! Shapes.\n\npub fn legacy() {}\n\n/// Area.\npub fn area(r: f32) -> f32 { r }\n";
    let dir = temp_crate("changed", &[("src/lib.rs", old)]);
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "base"]);

    let clean = figure(&dir, &["check", "--changed", "--strict"]);
    assert_eq!(clean.status.code(), Some(0), "{}", stdout(&clean));
    assert!(stdout(&clean).contains("no source files changed"));

    let edited =
        old.replace("area(r: f32)", "area(r: f64)") + "pub mod extra;\npub fn fresh() {}\n";
    fs::write(dir.join("src/lib.rs"), edited).unwrap();
    fs::write(dir.join("src/extra.rs"), "pub struct Extra;\n").unwrap();
    let out = figure(&dir, &["check", "--changed", "--strict"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("2 files changed (1 new)"), "{text}");
    assert!(text.contains("missing: src/extra.rs"), "{text}");
    assert!(text.contains("src/lib.rs:8 fn fresh"), "{text}");
    assert!(text.contains("src/extra.rs:1 struct Extra"), "{text}");
    assert!(text.contains("1 of 3 new or changed documented"), "{text}");
    assert!(
        !text.contains("legacy"),
        "old gaps are not reported: {text}"
    );

    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "more"]);
    let against_first = figure(&dir, &["check", "--changed", "HEAD~1"]);
    assert!(
        stdout(&against_first).contains("since HEAD~1"),
        "{}",
        stdout(&against_first)
    );
    fs::remove_dir_all(dir).unwrap();
}

/// `--changed` outside a git repository, or with an unknown revision, is a usage error.
#[test]
fn changed_needs_git() {
    let dir = temp_crate("nogit", &[("src/lib.rs", "//! X.\n")]);
    let out = figure(&dir, &["check", "--changed"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("git"));
    fs::remove_dir_all(dir).unwrap();
    let bad = figure(&example(), &["check", "--changed", "no-such-ref"]);
    assert_eq!(bad.status.code(), Some(2));
}
