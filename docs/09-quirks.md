# Bot Quirks

**Status: DECIDED 2026-07-14** (Q44–Q48, Q60 — see [decided.md](09-quirks/decided.md)).

A **quirk** is a small per-bot deviation from the universal chassis spec ([02-agents.md](02-agents.md)) — a positive or negative "personality" of the individual machine. With chassis classes gone, quirks and XP are the *only* things making two prints differ. Two bots printed from the same Fabricator, running the same color, are no longer perfectly identical. Quirk names are programming jokes on purpose: the audience writes code, and a joke name that *explains its own effect* (Memory Leak, Cold Start) is free documentation.

## The parts

| File | Owns |
|---|---|
| [design-rules.md](09-quirks/design-rules.md) | The six rules that keep quirks on-pillar. |
| [catalog.md](09-quirks/catalog.md) | The tunable quirk catalog: positive, negative, double-edged. |
| [policy-quirks.md](09-quirks/policy-quirks.md) | Quirks that are env-registry entries: temperaments and compulsions. |
| [acquired-quirks.md](09-quirks/acquired-quirks.md) | Post-v1 acquisition: scars, L5 merits, corruption exposure. |
| [visibility-and-manifestation.md](09-quirks/visibility-and-manifestation.md) | Who can see/read quirks, and the latent-roll → Age-level manifestation lifecycle. |
| [decided.md](09-quirks/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **Quirks bend the hardware, never the program** — canonical in
  [design-rules.md](09-quirks/design-rules.md) (rule 1). No catalog entry,
  scar, or merit may make a decision or move a bot; the player's code stays the
  only brain.
- **Runtime stats only, never deploy-time validation** (Q52) — canonical in
  [design-rules.md](09-quirks/design-rules.md) (rule 6). A color program that
  deploys must deploy to every bot of its color; any proposed quirk touching
  program memory or variable slots is invalid by construction.
- **Deterministic by construction** — canonical in
  [design-rules.md](09-quirks/design-rules.md) (rule 2): the seeded
  `quirk_roll` stream, counters never random checks. The companion clause —
  quirk scratch state is hashed sim state — is canonical in
  [07-architecture/tick-model.md](07-architecture/tick-model.md).
- **Rolls are latent until the Age level threshold** — canonical in
  [decided.md](09-quirks/decided.md) (*Quirks manifest with experience*), with
  the lifecycle in
  [visibility-and-manifestation.md](09-quirks/visibility-and-manifestation.md).
  A fresh print is always quirk-free; nothing manifested or latent survives a
  reprint, and everything survives recall and rescue.
- **Policy quirks are env entries** (Q60) — canonical in
  [policy-quirks.md](09-quirks/policy-quirks.md); the env registry itself is
  owned by [01-language.md](01-language.md). Quirk clamps clip quietly; engine
  bounds still fault.
- **Hash-affecting content stays behind the quirk-probability dial** —
  canonical in
  [visibility-and-manifestation.md](09-quirks/visibility-and-manifestation.md);
  the dial is registered in
  [08-multiplayer/match-settings.md](08-multiplayer/match-settings.md).
- **The catalog is tuning data** (`quirks.ron`), never commitments — canonical
  in [design-rules.md](09-quirks/design-rules.md) (rule 3) and CLAUDE.md's doc
  conventions.
