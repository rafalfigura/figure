---
name: monster-model-redesign
description: Redesigns a LiteRogueCrawler monster's 3D look and animation (src/monsters/kinds/*.rs) to match the game's established low-poly visual style (the golem_bloom redesign, full rules in src/monsters/kinds/DESIGN.md), using a structured multi-version feedback loop with the user rather than jumping straight to one final file. Use this whenever the user wants a monster's model redesigned, restyled, given a facelift, brought up to the new look, or created fresh for a monster that doesn't have a model yet — even if they don't say "redesign" explicitly, e.g. "the boar model looks dated", "let's give the drake the golem treatment", "make a model for the new monster we added". Also use it to judge or review whether an existing kind file already matches the established style.
---

# Monster model redesign

This skill is a **process wrapper**, not a style reference. The actual visual grammar, the
`ModelCtx` API, the detail budgets and the hard "never do this" rules all live in
[`src/monsters/kinds/DESIGN.md`](../../../src/monsters/kinds/DESIGN.md) — **read that file in full before
writing or judging any model code.** Don't restate its rules from memory or duplicate them here;
that doc is the single source of truth and this skill exists to keep drifting from it.

What this skill adds on top of that doc is the *workflow*: when a person is actually driving a
specific monster's redesign, going straight to one finished file skips the part where they get to
react to real options, and a first guess is rarely the best one. Run the loop below instead.

## Before starting

1. **Know which monster.** If the user hasn't named one, ask — don't guess from context.
2. **Read the target's current state**: its kind file in `src/monsters/kinds/` (if any), whose
   `types()` holds the family's `shape`, ids, colours, sizes, abilities and traits. If there's no
   model yet, gather the same info from the user's description.
3. **Read `src/monsters/kinds/DESIGN.md`** end to end if you haven't already this session. It has the
   look rules, the animation-style rules, the part budgets per tier, and the `ModelCtx` cheat
   sheet you'll need for every step below.

## The loop

This mirrors `DESIGN.md` §6, as concrete actions:

1. **First pass: 5 versions, genuinely different approaches.**
   - For *each* version, first write a short Markdown plan of its Primary/Secondary/Tertiary
     masses and target tier budget (`DESIGN.md` §5 step 2) — before any Rust. This
     is where a weak silhouette or an unbalanced budget gets caught cheaply; don't skip straight
     to code because the plan feels like overhead.
   - Write each version as its own `kinds/<shape>_<name>_a.rs` … `_e.rs`. Vary primary silhouette
     and ornamentation, not just colour — but all five still obey the "Never" list and the look
     rules in §1-§3 of the style guide; "different approach" never means "different style".
   - Give each version at least one small per-spawn cosmetic roll (`roll(ctx, n)`, style guide
     §1) — the user should be judging the finished feel, not a static base.
   - Register all 5 as rows in `designs()` in `src/modes/lab/scenes.rs`, with the
     family's monster ids as `variants`.
2. **Get the user into the game, not into the diff.** Tell them to run
   `cargo run --bin lab -- --scene gallery` (every version side by side) and walk through the keys in style
   guide §5 step 7. Do not evaluate the versions yourself and report back a verdict —
   the whole point of this step is the user's own reaction to seeing them move in-game.
3. **Collect explicit per-part feedback.** Ask which parts of which versions they liked (e.g.
   "version 2's arms, version 4's head"). Wait for their actual answer; don't assume which parts
   "must" be the good ones.
4. **Merge into 3 new versions.** Discard every version not mentioned. Build 3 versions that
   recombine the liked parts across the *kept* versions — not a re-show of the same 5, and not a
   single average that blends everything together. Re-register these in `designs()` the same
   way, replacing the discarded rows.
5. **User picks a winner and gives final requests** (colour, a missing detail, a proportion
   nudge). Apply them directly to the winning file.
6. **Finalize**, per style guide §5 steps 6-8:
   - Switch `shape` in the family's `types()` and move `types()` into the winning file.
   - Delete the old kind file and every sketch that didn't win.
   - Remove the abandoned sketches' rows from `designs()`, keeping only the shipped one.

## Guardrails

- **Never skip the per-version plan before writing Rust.** Applies to all 5 first-pass versions
  and all 3 merged versions, not just a single final design.
- **Never present a version as code only.** The user judges it in the lab's gallery, in
  motion, from the game's camera — that's the actual acceptance test, not a code review.
- **Never finalize (step 6) without the user explicitly picking a winner.** If they haven't
  responded yet, the loop is paused at step 3 or 5, not skippable.
- **Never wire an in-progress sketch into a real monster type's `shape`.** Everything before step 6 lives only
  in the lab's gallery, disposable and rerollable with `F5`, so nothing mid-loop touches real
  spawns.
- **Never let "different approach" become "different style."** Every version, at every pass,
  still has to pass the "Never" checklist at the top of `DESIGN.md`.

## Reference

- [`src/monsters/kinds/DESIGN.md`](../../../src/monsters/kinds/DESIGN.md) — binding style guide: look rules,
  animation style, detail budgets, `ModelCtx` API, the single-file refactor steps this loop
  builds on.
- [`src/monsters/kinds/golem_bloom.rs`](../../../src/monsters/kinds/golem_bloom.rs) — the
  canonical example model.
