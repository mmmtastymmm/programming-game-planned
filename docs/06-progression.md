# Progression

Progression runs on three axes — **research**, **hardware**, and **territory** — plus per-match **function blocks**. The parts below own all the mechanics; the *What holds* list is the cross-part contract.

## The parts

| File | Owns |
|---|---|
| [scopes.md](06-progression/scopes.md) | The permanent-vs-per-match split, the PvP gate, and the three per-match tracks. |
| [template-caches.md](06-progression/template-caches.md) | The Cache mechanic: non-consumable study sites, depth ordering, territorial contest. |
| [unlock-tree.md](06-progression/unlock-tree.md) | The construct + function tree, the start kit, and the design rules every unlock must pass. |
| [upgrade-station.md](06-progression/upgrade-station.md) | Tools (grades, licensing, pricing invariants) and flat capacity buys. |
| [pacing.md](06-progression/pacing.md) | The target learning arc of a new player's first session. |
| [decided.md](06-progression/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **Constructs are permanent; everything else is per-match** — canonical in
  [scopes.md](06-progression/scopes.md). The language is knowledge you keep;
  the match is how well you use it. The PvP gate (full construct knowledge)
  rides on this split — [08-multiplayer.md](08-multiplayer.md) enforces it at
  the server door.
- **Every unlock changes what programs *can say*, immediately** — canonical in
  [unlock-tree.md](06-progression/unlock-tree.md) (Design Rules). No passive
  "+5%" research anywhere in these parts; stat growth belongs to XP
  ([02-agents.md](02-agents.md)) and hardware.
- **Function blocks are learned, not researched** — canonical in
  [template-caches.md](06-progression/template-caches.md). Caches are
  non-consumable and contested territorially, never exclusively; the tree's
  function numbers read as cache depth, not Data cost.
- **Colors are not in the tree** — canonical in
  [unlock-tree.md](06-progression/unlock-tree.md). Beyond the starting Green
  and the Data-repaired Red printer ([01-language.md](01-language.md)),
  program slots are gated by controlled nests on the quadratic curve
  ([04-enemies.md](04-enemies.md)) — territory is the third axis, and no part
  may re-price a nest-gated slot in Data.
- **Tools carry the power; levels license** (Q111/Q121) — canonical in
  [02-agents/xp-and-specialization.md](02-agents/xp-and-specialization.md)
  (the same owner the 02-agents doorway names).
  [upgrade-station.md](06-progression/upgrade-station.md) owns the
  station-side rules: three load-time pricing invariants (anti-circularity,
  no orphan materials, no gaps — Q118) and the compute-only coolant rule
  (Q119); a change to the tool model must keep all four.
- **Progression is per-player, always** — canonical in
  [decided.md](06-progression/decided.md). Allies share work products
  (libraries, intel), never capability.
- **All numbers here are tuning constants** bound for data files, never code —
  canonical in CLAUDE.md's doc conventions.
