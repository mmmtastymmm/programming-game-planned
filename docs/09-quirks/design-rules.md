*Part of [09-quirks](../09-quirks.md).*

# Design Rules (what keeps quirks on-pillar)

1. **Quirks bend the hardware, never the program** (pillar 1). A quirk may change stats, costs, timings, ranges, or *where a signal line sits* — it never makes a decision, overrides an instruction, or moves the bot on its own. The player's code stays the only brain.
2. **Deterministic by construction** (pillar 4). Quirks are rolled from a named, seeded RNG stream (`quirk_roll`) advanced only by print events. "Every Nth" in any quirk is a deterministic counter, never a random check.
3. **Data-driven.** Quirk ids, rarity weights, and effect magnitudes live in `quirks.ron`. Every number in the [catalog](catalog.md) is a tuning constant.
4. **Programs can read them.** Builtins `my_quirks()` / `has_quirk(q)` — quirk names are pre-bound constants like kind constants, no enum (Q79) — let one color program adapt per-bot (`if has_quirk(deprecated_drivers): stay closer to escort`). This is the payoff: quirks turn "one program, many bots" into a real programming problem instead of cosmetic noise.
5. **Losses hurt more** (pillar 3). A veteran isn't just XP — it's *this body*, with *these quirks*. Reprinting rerolls quirks: destruction can cost you a lucky roll on top of the levels.
6. **Quirks touch runtime stats only.** Every quirk modifies a row of the stat sheet ([02-agents.md](../02-agents.md)) or lays a per-bot overlay on the cost table — never a *deploy-time* validation stat (program memory, variable slots, handler window caps). A color program that deploys must deploy to **every** bot of its color; a quirk that could make one bot reject the colony's code would break that (Q52).
