# figure

Compact code maps for AI agents. `figure` parses a Rust crate and prints, as plain text,
what an agent would otherwise learn by opening whole files: a module's purpose, file tree,
public API, what it wires up, what it depends on, the rules its tests guard, and the
hand-written "how to" recipes that live next to the code.

No AI, no network, no stored state: each command parses the files live (the whole
58k-line crate it was tried on takes about 0.2 s) and exits.

## Install

```sh
cargo install --path .
```

## Commands

The agent drills down in three levels: module (`map`) -> symbol (`show`) -> source (`show --body`).

| Command | Answers |
| --- | --- |
| `figure map <path>` | Module manifest: purpose, shape, tree, public API, relations, dependencies, contracts, recipes, undocumented count. `--depth N`, `--fields`, `--private`. |
| `figure show <symbol>` | One item or module: doc, signature, fields, methods, relations, who wires and uses it. `--body` prints its exact source. |
| `figure howto [topic]` | Lists `# How to ...` recipes, or prints one with every link resolved to `file:line`. |
| `figure deps <path>` | What a module depends on (tree with file counts and symbols); `--reverse` for who uses it, with `file:line`. |
| `figure check [path]` | Files without a module doc, undocumented public items, broken doc links. `--strict` exits 1 on any. `--changed [REF]` checks only what changed since a git revision (default `HEAD`). |

`<path>` is a directory, a `.rs` file, or a module path (`crate::traps`, `traps::shared`).
`<symbol>` is `Harm`, `Harm::new`, `traps::register` or `crate::traps::register`.
`--root <dir>` points at the crate when running from elsewhere. Exit codes: 0 ok,
1 findings under `--strict`, 2 usage error.

Example on the fixture crate in `examples/rust`:

```
$ figure map src/traps
MODULE        crate::traps   src/traps/  6 files · 199 lines · rust
PURPOSE       Traps: environmental hazards that hurt whoever steps in.
SHAPE         star: 2 parts -> shared/, no other sibling imports, no cycles

TREE
  saw      Saw Blade: a spinning blade running back and forth along a rail.
  shared/  Building blocks several traps share. (3 files)
  spikes   Pop-up Spikes: a floor plate whose spikes go down, rattle, then up on a cycle.

PUBLIC API
## mod.rs
  struct TrapsPlugin   impl Plugin · [no doc]
  fn register<C: Component + DeserializeOwned>(app: &mut App, name: &'static str)
      Registers a YAML component type whose spec deserializes straight into `C`.
...
RELATIONS  (bevy pack)
  spikes::plugin     registers Spikes · observer build · system run.run_if(gameplay_running) (Update)
...
UNDOCUMENTED  6 of 13 public items in this module (list: figure check src/traps)
```

Every command's output on that crate is in `examples/rust/expected/`.

## Documenting code for figure

figure only extracts; the quality of its output is the quality of your docs. The
convention is plain rustdoc, so the same comments render in `cargo doc`.

Must be documented:

1. Every file: a first `//!` sentence saying what it is.
2. Every `pub` item: a first `///` sentence saying what it does.
3. Every `#[test]` that guards a rule: a `///` line stating the rule.

Optional sections in a `//!` doc:

| Heading | Used for |
| --- | --- |
| `# How to <task>` | a recipe, printed by `figure howto <task>` (`add a trap`, `use the solver`) |
| `# Invariants` | listed under CONTRACTS in `map` |
| `# Data flow`, `# Gotchas`, anything else | listed in `map`, printed by `show <module>` |

Links use rustdoc intra-doc syntax (``[`Harm`]``, ``[`crate::traps::register`]``) and are
checked by `figure check`. In recipes a bare name must resolve inside the crate. The line
`Example to copy: [`Spikes`]` also prints the example's file and size.
`#[doc(hidden)]` marks an item as intentionally undocumented; figure skips it.

Recipes that span several modules can live in `.figure/howto/<topic>.md`, where
`[[src/app.rs]]` links a file and `[[howto:add a trap]]` another recipe.

## Run it automatically

`figure check --changed` checks only what changed since a git revision (default `HEAD`,
including staged, unstaged and untracked files): new files need a `//!` line, and public
items that are new or whose signature changed need a `///` line. Existing gaps are not
reported, so you can turn it on in an old codebase today and document the backlog later.
With `--strict` it exits 1 when anything is missing, which makes it a gate:

```
$ figure check --changed --strict
since HEAD    2 files changed (1 new)
module docs   0 of 1 new files
  missing: src/extra.rs
public items  1 of 3 new or changed documented
  missing: src/extra.rs:1 struct Extra
           src/lib.rs:8 fn fresh
...
```

Pick where it runs:

**On every commit** (git pre-commit hook):

```sh
printf '#!/bin/sh\nexec figure check --changed --strict\n' > .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

**On every `cargo test`**, like any other test (needs `figure` on `PATH`):

```rust
// tests/docs.rs
/// New or changed public API is documented.
#[test]
fn new_api_documented() {
    let out = std::process::Command::new("figure")
        .args(["check", "--changed", "--strict"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("figure is installed");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}
```

**Before an AI agent finishes** (Claude Code `Stop` hook in `.claude/settings.json`): exit
code 2 sends the report back to the agent, which documents the items before it stops.

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "figure check --changed --strict 1>&2 || exit 2" }] }
    ]
  }
}
```

**In CI**, against the branch the PR targets: `figure check --changed origin/main --strict`
(fetch enough history for that revision to exist).

## figure.toml (optional)

Only what cannot be inferred from the code:

```toml
# A project's own registration call, shown as a relation: `spikes::plugin registers Spikes`.
[[relations]]
call = "register::<$T>"
name = "registers"

[docs]
fact_prefixes = ["YAML"]   # `/// YAML: ...` lines are shown under items in `map`
```

The bevy relation pack (systems, observers, resources, plugins, events, states, and
labels such as `component`) switches on when `bevy` is a dependency in `Cargo.toml`.

## For agents

Put this in the project's `CLAUDE.md` / `AGENTS.md`:

```
## Exploring code: use figure first
Before opening source files in an unfamiliar module:
1. `figure map <dir>` for purpose, tree, public API, relations, contracts.
2. `figure howto <topic>` if you are adding something; follow the recipe.
3. `figure show <symbol>` for one item; `--body` for its exact source.
4. `figure deps <dir>` (and `--reverse`) to see what else a change touches.
Open a whole file only to edit it, or when figure output is not enough.
After a change, run `figure check`.
```

## Limits

- Rust only for now; other languages plug in behind `LanguageAdapter` (`figure howto add a language`).
- Resolution is syntactic: items generated by macros are invisible, and paths are resolved
  from `use` declarations and module layout, not by the compiler.
- Files under `src/bin/` and crates of a workspace other than the one found are not indexed.

## Development

```sh
cargo test                                  # unit, CLI and golden tests
FIGURE_BLESS=1 cargo test --test golden     # regenerate expected outputs, then review the diff
cargo run -- check --strict                 # figure checks its own docs
```

A language counts as supported when its golden suite on `examples/<language>/` passes.

## License

MIT
