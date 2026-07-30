# Agents (Bots)

A **bot** is a printed machine that runs exactly one [Pyrite](01-language.md) program. Bots are the only actors the player owns; everything the colony does, a bot does.

## The parts

| File | Owns |
|---|---|
| [anatomy.md](02-agents/anatomy.md) | What a bot is physically made of — chassis, slots, the printed object. |
| [stat-sheet.md](02-agents/stat-sheet.md) | Every stat row, its units, and what reads it. |
| [damage-faults-death.md](02-agents/damage-faults-death.md) | HP, damage sources, fault consequences, death, wrecks. |
| [xp-and-specialization.md](02-agents/xp-and-specialization.md) | The task tracks, XP curves, levels, perks and upkeep. |
| [reprinting.md](02-agents/reprinting.md) | What a replacement bot costs. |
| [decided.md](02-agents/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

- **Brains are bought, bodies are earned.** Tiers come from the Upgrade Station
  ([06-progression.md](06-progression.md)); levels come from doing the work.
  Identical rookies diverge from tick one — that is the design, not a gap.
- **Capabilities are tier × level** (Q105, reshaped by Q111/Q121). Tier is what a
  capability *can reach*; level is how *well* it performs. Tools carry the step
  change, levels license. Hauling is the exception — cargo is a body stat, so it
  carries no tier.
- **XP is monotonic and never resets.** The Backup Core preserves tiers and wipes
  XP (Q100); nothing else takes XP away. Ranking by investment rather than raw XP
  is what keeps that honest (Q105-R3).
- **Every stat row is keyable.** Any row of the stat sheet and any ledger number
  can serve as a selection key, which is why
  [stat-sheet.md](02-agents/stat-sheet.md) is a contract and not just a table —
  adding a row adds a key.
- **Upkeep re-bases on installed tools** (Q122/Q123), so a change to the tool
  model in [06-progression.md](06-progression.md) moves upkeep here.
- **All numbers here are tuning constants** bound for data files (`xp.ron`,
  `upkeep.ron`, …), never code.

## Open Questions

- Upkeep mix tuning: does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? (System is data-driven — answer via playtest, not redesign.)
