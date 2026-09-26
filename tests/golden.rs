//! Golden tests per language: every command is run on `examples/<language>/` and its
//! output compared with `examples/<language>/expected/<case>.txt`.
//!
//! After an intended output change, regenerate with `FIGURE_BLESS=1 cargo test --test golden`
//! and review the diff.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUST_CASES: &[(&str, &[&str])] = &[
    ("map_traps", &["map", "src/traps"]),
    ("map_root", &["map", "src"]),
    ("map_file", &["map", "src/core.rs"]),
    (
        "map_shared_fields_private",
        &["map", "src/traps/shared", "--fields", "--private"],
    ),
    ("map_depth2", &["map", "crate::traps", "--depth", "2"]),
    ("show_item", &["show", "Spikes"]),
    ("show_body", &["show", "traps::register", "--body"]),
    ("show_methods_users", &["show", "Harm"]),
    ("show_method", &["show", "Harm::new"]),
    ("show_module", &["show", "crate::traps"]),
    ("howto_list", &["howto"]),
    ("howto_module_recipe", &["howto", "trap"]),
    ("howto_file_recipe", &["howto", "add", "a", "level"]),
    ("deps", &["deps", "src/traps"]),
    ("deps_reverse", &["deps", "src/traps/shared", "--reverse"]),
    ("deps_root", &["deps", "src"]),
    ("check", &["check"]),
    ("check_module", &["check", "src/traps/shared"]),
];

fn example(language: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(language)
}

fn run_cases(language: &str, cases: &[(&str, &[&str])]) {
    let dir = example(language);
    let bless = std::env::var_os("FIGURE_BLESS").is_some();
    let mut failures = Vec::new();
    for (name, args) in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_figure"))
            .args(*args)
            .current_dir(&dir)
            .output()
            .expect("figure runs");
        let mut actual = String::from_utf8(output.stdout).expect("utf-8 output");
        if !output.stderr.is_empty() {
            actual.push_str("--- stderr\n");
            actual.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        let expected_path = dir.join("expected").join(format!("{name}.txt"));
        if bless {
            fs::create_dir_all(expected_path.parent().unwrap()).unwrap();
            fs::write(&expected_path, &actual).unwrap();
            continue;
        }
        let expected = fs::read_to_string(&expected_path).unwrap_or_default();
        if expected != actual {
            failures.push(format!(
                "case {name} (figure {}):\n--- expected\n{expected}--- actual\n{actual}",
                args.join(" ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} golden case(s) differ:\n\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Rust is supported: every command matches its golden output on `examples/rust`.
#[test]
fn rust() {
    run_cases("rust", RUST_CASES);
}
