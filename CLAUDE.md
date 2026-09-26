# CLAUDE.md

figure: a Rust CLI that prints compact, extracted code maps for AI agents. See `README.md`.

## Exploring this code: use figure on itself

```
cargo run -q -- map src            # purpose, tree, public API of every module
cargo run -q -- howto              # recipes: add a command, add a language
cargo run -q -- show <symbol>      # one item; --body for its source
```

## Commands

```
cargo test                                  # unit, CLI and golden tests
FIGURE_BLESS=1 cargo test --test golden     # regenerate examples/rust/expected, then review the diff
cargo run -q -- check --strict              # must stay clean: every file and pub item documented
cargo fmt && cargo clippy --all-targets     # no warnings
```

## Rules

- Only `src/lang/` knows a language's syntax; everything else reads `model::FileIndex`.
- Output is plain text, complete (no truncation), stable order: golden tests diff it byte for byte.
- Any output change updates `examples/rust/expected/` in the same commit.
- Keep files under ~300 lines; document every file (`//!`) and pub item (`///`).
