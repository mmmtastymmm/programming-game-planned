# Multiplayer

Multiplayer is a **day-one constraint**, not a feature (per project decision: retrofitting it later was judged too costly). There are no hard modes: every player owns a colony, and co-op vs. PvP is how players choose to interact on a given server.

## The parts

| File | Owns |
|---|---|
| [lockstep.md](08-multiplayer/lockstep.md) | The lockstep model: input delay, relay topology, desync handling, late join. |
| [determinism-contract.md](08-multiplayer/determinism-contract.md) | The eight rules every system must obey (the CI-enforced contract). |
| [modes.md](08-multiplayer/modes.md) | Server harm settings, the PvP gate, and allied-colony scaffolding. |
| [code-visibility.md](08-multiplayer/code-visibility.md) | Shared libraries, per-color decryption by salvage attrition, the reveal-mask rules, spectating. |
| [match-settings.md](08-multiplayer/match-settings.md) | The owning inventory of every match dial (Q77). |
| [decided.md](08-multiplayer/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **Every player owns their own colony; interaction is a server setting** —
  canonical in [modes.md](08-multiplayer/modes.md) and
  [decided.md](08-multiplayer/decided.md). No part may introduce a
  shared-colony mechanic or a hard co-op/PvP mode split.
- **The determinism contract binds every system in every part** — canonical in
  [determinism-contract.md](08-multiplayer/determinism-contract.md), with the
  same rules restated as law in CLAUDE.md and implemented per
  [07-architecture.md](07-architecture.md). Anything added here (a new dial, a
  new sharing rule) must be expressible as lockstep-shared state plus ordered
  `Command`s.
- **Programs are read on murder — permanent, monotonic, per-color attrition** —
  canonical in [code-visibility.md](08-multiplayer/code-visibility.md). One
  rule for players and Ferals alike ([04-enemies.md](04-enemies.md) applies it
  at per-arcanum rates); decryption state is hashed sim state
  ([07-architecture.md](07-architecture.md)); alliance pooling is forward-only
  (Q107 — no retroactive merge, ever — canonical in
  [modes.md](08-multiplayer/modes.md)).
- **Allies share work products, never capability** — canonical in
  [modes.md](08-multiplayer/modes.md); the progression side is owned by
  [06-progression.md](06-progression.md). Libraries, intel, vision, and
  channels are shareable; unlocks are not.
- **Every match dial lives in the match-settings inventory** — canonical in
  [match-settings.md](08-multiplayer/match-settings.md) (Q77). A part (or any
  other doc) adding a configurable constant must register it there.
- **All numbers here are tuning constants** bound for data files, never code —
  canonical in CLAUDE.md's doc conventions.
