# Agents (Bots)

A **bot** is a printed machine that runs exactly one [Pyrite](01-language.md) program. Bots are the only actors the player owns; everything the colony does, a bot does.

## The parts

| File | Owns |
|---|---|
| [anatomy.md](02-agents/anatomy.md) | What a bot is physically made of — chassis, tools, the printed object. |
| [stat-sheet.md](02-agents/stat-sheet.md) | Every stat row, its units, and what reads it. |
| [damage-faults-death.md](02-agents/damage-faults-death.md) | HP, damage sources, fault consequences, death, wrecks. |
| [xp-and-specialization.md](02-agents/xp-and-specialization.md) | The task tracks, XP curves, levels and perks. |
| [reprinting.md](02-agents/reprinting.md) | What a replacement bot costs. |
| [decided.md](02-agents/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **Tools are bought, levels are earned** — canonical in
  [xp-and-specialization.md](02-agents/xp-and-specialization.md). Tools come from
  the Upgrade Station ([06-progression.md](06-progression.md)); levels come from
  doing the work.
  Identical rookies diverge from tick one — that is the design, not a gap.
- **Tools carry the power; levels license** (Q121) — canonical in
  [xp-and-specialization.md](02-agents/xp-and-specialization.md).
  **There are no tiers** — Q111
  removed `Capability`, the tier catalog, the scale factor and the level cap
  outright. Ten structurally identical tracks, centi-points, one uncapped
  quadratic *shape* — each track carrying its own `curve_base` (Q123). A tool is bought, then licensed by *either* its specific
  track's level *or* the floored mean across all ten; quirks can grant a licence
  outright.
- **XP is strictly monotonic** — canonical in
  [xp-and-specialization.md](02-agents/xp-and-specialization.md). Buying never
  costs XP and nothing resets it (Q111). The one item that would have wiped it — the Backup Core — is **cut**
  (Q115); the stat sheet's `XP preserved` row is 0% on destruction with no item
  softening it. `investment()` is earned XP plus installed tool value (Q115),
  which is what keeps the scrap valve ranking by investment rather than raw XP
  (Q105-R3). *(P8's stale-formula carriers are all swept — closed and amended
  in [PROBLEMS.md](PROBLEMS.md).)*
- **The stat sheet owns every row's unit.** `unit_scale` — centicycles for the
  cycle budget, deci-units for cargo/progress/move, **centi-points for XP** — is
  canonical in [stat-sheet.md](02-agents/stat-sheet.md). Any other part that
  names a unit is a carrier, not an authority, and a carrier that disagrees is
  the bug. Units are rounding inputs to the modifier pipeline and divisors on the
  XP curve, so a drift here is hash-affecting and silent (P34: the owning
  *Decided* entry stored XP in deci for a month after Q111 moved it to centi).
- **Every stat row is keyable.** Any row of the stat sheet and any ledger number
  can serve as a selection key, which is why
  [stat-sheet.md](02-agents/stat-sheet.md) is a contract and not just a table —
  adding a row adds a key.
- **Upkeep re-bases on installed tools** (Q122/Q123) — canonical in
  [stat-sheet.md](02-agents/stat-sheet.md) (the Upkeep-draw row; the ruling in
  [decided.md](02-agents/decided.md)). A change to the tool
  model in [06-progression.md](06-progression.md) moves upkeep here.
- **All numbers here are tuning constants** bound for data files (`xp.ron`,
  `upkeep.ron`, …), never code — canonical in CLAUDE.md's doc conventions.
