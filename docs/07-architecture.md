# Architecture (Bevy)

Constraint that shapes everything: the simulation must be **deterministic and fixed-tick** for lockstep multiplayer ([08-multiplayer.md](08-multiplayer.md)). Rendering is decoupled and free-running.

## The parts

| File | Owns |
|---|---|
| [layering.md](07-architecture/layering.md) | The sim/game split, the two layering rules, and the crate layout. |
| [tick-model.md](07-architecture/tick-model.md) | The 9-phase tick, sub-pass assignment, RNG stream inventory, and phase notes. |
| [vm.md](07-architecture/vm.md) | The Pyrite VM: run states, fault path, signal dispatch, templates, recall, cost resolution, the function registry. |
| [world-state.md](07-architecture/world-state.md) | The world-state struct sketch and the complete `Command` inventory. |
| [ui-notes.md](07-architecture/ui-notes.md) | Editor, inspector, and Codex implementation notes. |
| [testing.md](07-architecture/testing.md) | Golden replays, CI determinism checks, the headless balance harness. |
| [decided.md](07-architecture/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **The `sim` crate is plain Rust — no Bevy, `BTreeMap` world** — canonical in
  [layering.md](07-architecture/layering.md) (Rule 1) and
  [decided.md](07-architecture/decided.md); the enforcement rule lives in
  CLAUDE.md. Deterministic iteration is by construction, not discipline; ECS
  feeding sim state is the #1 violation to flag.
- **All external input is ordered `Command`s** — canonical in
  [layering.md](07-architecture/layering.md) (Rule 2);
  [world-state.md](07-architecture/world-state.md) owns the complete inventory
  (Q77 — it grows only when a decided system adds a player input). Even
  single-player is lockstep with one peer.
- **Phase order, sub-pass assignment, and RNG streams are spec, because they
  are hash-affecting** — canonical in
  [tick-model.md](07-architecture/tick-model.md). No part (and no
  implementation) may treat them as internals; a change here is a replay-hash
  change and must say so.
- **One registry, one code path** — canonical in [vm.md](07-architecture/vm.md).
  Engine-forced behaviors reuse builtin registry entries (`become_disabled`
  is registry-shared but engine-only — the player's scuttle verb is
  `abort()`), Ferals
  ([04-enemies.md](04-enemies.md)) call the same function registry, and cost
  resolution runs through the one layered pipeline
  (`floor₁(region(tile(base + Σ per-bot deltas)))`, bounded by `bank_cap` at
  load — the player-facing side is owned by
  [01-language/execution-model.md](01-language/execution-model.md)).
- **Inspectable secrets are sim state** — canonical in
  [vm.md](07-architecture/vm.md): the per-`(color, faction)` decryption
  levels are hashed lockstep state — the *only* decryption state — and the
  reveal mask is *derived* deterministically from
  `(color, version, faction, level)`, identical across peers without being
  stored or hashed; ruled by [08-multiplayer.md](08-multiplayer.md).
  Anything a player can inspect that differs per faction must live sim-side,
  never as a UI overlay.
- **Programs are byte-exact plain text, versioned by source hash** — canonical
  in [decided.md](07-architecture/decided.md); also a CLAUDE.md determinism
  rule. The AST is always a derived cache.
- **Golden replays guard all of it** — canonical in
  [testing.md](07-architecture/testing.md); a PR that changes a replay hash
  must explain why (CLAUDE.md).
