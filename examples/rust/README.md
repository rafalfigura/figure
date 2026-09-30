# Example crate

A fixture for figure's golden tests: a small Bevy arcade game (`arcade`) with hazards
("traps") that hurt whoever steps in. It is parsed, never built.

Its shape exercises what figure extracts:

| Path | Exercises |
| --- | --- |
| `src/traps/` | a star-shaped module: `saw` and `spikes` both use `shared/`; Bevy relations (systems, observers, plugins) |
| `src/traps/mod.rs` | a `# How to add a trap` recipe and the `register` call from `figure.toml` |
| `src/registry.rs`, `src/core.rs`, `src/app.rs` | cross-module references for `deps`, `deps --reverse` and `show` |
| `figure.toml` | a project relation (`registers`) and `YAML` doc facts |

`expected/` holds the output of every command on this crate, one file per invocation.
`tests/golden.rs` compares them byte for byte:

```sh
cargo test --test golden
FIGURE_BLESS=1 cargo test --test golden   # regenerate, then review the diff
```

Try it by hand:

```sh
figure map src/traps --root examples/rust
figure show traps::register --root examples/rust
figure howto "add a trap" --root examples/rust
figure out trap --root examples/rust      # alias of howto; any words of the topic
```
