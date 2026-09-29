//! Structure rules that keep languages divided: shared code never names a language.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Outside `src/lang/`, no source file compares a language id or names a manifest file: the
/// language adapter answers those questions.
#[test]
fn shared_code_names_no_language() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let mut offenders = Vec::new();
    for file in files.iter().filter(|f| !f.starts_with(src.join("lang"))) {
        let text = fs::read_to_string(file).unwrap();
        // Test modules build fixtures for one language.
        let code = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for (n, line) in code.lines().enumerate() {
            let is_comment = line.trim_start().starts_with("//");
            let names = [
                "\"rust\"",
                "\"typescript\"",
                "\"Cargo.toml\"",
                "\"package.json\"",
            ];
            if !is_comment && names.iter().any(|name| line.contains(name)) {
                offenders.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "language named outside src/lang/:\n{}",
        offenders.join("\n")
    );
}
