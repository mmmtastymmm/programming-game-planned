# Enemies — The Feral

The PvE faction: **Feral machines**, corrupted bots left over from whatever wrecked this world. The core conceit (requirement 5) — Ferals run real Pyrite on the player's VM, and their programs can be decrypted and read like anyone else's — is owned by [inspection-and-decryption.md](04-enemies/inspection-and-decryption.md).

## The parts

| File | Owns |
|---|---|
| [inspection-and-decryption.md](04-enemies/inspection-and-decryption.md) | Why Ferals run the player's VM, and how their code is read: the decryption methods, per-arcanum rates, the comm-key rule. |
| [archetypes.md](04-enemies/archetypes.md) | The initial archetype set (Drone, Stinger, Harvester, Warden) and their shipped source. |
| [nests-and-claims.md](04-enemies/nests-and-claims.md) | The Nest structure, claiming vs. razing, reclaim pressure, and what losing a claim does. |
| [allegiance.md](04-enemies/allegiance.md) | The Major Arcana 0–21: the difficulty-and-personality axis and its mechanical flags. |
| [capturing-wrecks.md](04-enemies/capturing-wrecks.md) | `hijack()`: stealing wrecks into your fleet. |
| [escalation.md](04-enemies/escalation.md) | Threat escalation, program variants, and the Feral role in co-op vs. PvP. |
| [decided.md](04-enemies/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **One interpreter, one truth** — canonical in
  [inspection-and-decryption.md](04-enemies/inspection-and-decryption.md).
  Ferals run legal Pyrite on the player's VM against the same function registry
  (the registry clause is canonical in
  [07-architecture/vm.md](07-architecture/vm.md)); there is no separate AI
  system.
  Every archetype program, variant, and mutation in these parts must parse and
  run under the current language spec ([01-language.md](01-language.md)).
- **Programs are read on murder — one universal rule** — the mechanic is
  canonical in
  [08-multiplayer/code-visibility.md](08-multiplayer/code-visibility.md);
  [inspection-and-decryption.md](04-enemies/inspection-and-decryption.md)
  owns its Feral-side application: the *same* salvage-attrition mechanic, at
  per-arcanum rates; channels are never included (comm keys via `analyze()`).
  No part may invent a free read.
- **Shipped Feral source must not teach bugs** (Q108) — canonical in
  [archetypes.md](04-enemies/archetypes.md) for authored code and in
  [decided.md](04-enemies/decided.md) (*Mutated programs stay functional*) for
  generated code. The Fool's intentional crash-loops are an authored exception,
  never a mutation accident. Part of the same bar: **bind once, never
  check-then-act** (Q110, ruled inside Q117's answer) — also canonical in
  [archetypes.md](04-enemies/archetypes.md).
- **Allegiance is who a nest is; escalation is how awake it is** — the axes are
  orthogonal: [allegiance.md](04-enemies/allegiance.md) owns *who*,
  [escalation.md](04-enemies/escalation.md) owns *awake*. A change to either
  file must not couple them.
- **Controlled nests gate printers/colors on the quadratic curve** — canonical
  in [nests-and-claims.md](04-enemies/nests-and-claims.md) and the dormancy
  rulings in [decided.md](04-enemies/decided.md); the color/printer side lives
  in [01-language.md](01-language.md). Territory is the third progression axis
  ([06-progression.md](06-progression.md)).
- **All numbers here are tuning constants** bound for data files, never code —
  canonical in CLAUDE.md's doc conventions.
