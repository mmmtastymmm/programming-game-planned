*Part of [03-resources](../03-resources.md).*

# Harvest Tool Tiers

Harvesting reads the **grade of the drill the bot has installed** ([02-agents.md](../02-agents.md), Q111/Q118): grade N works every resource of tier ≤ N. **Grade 1 is free with the chassis**, so a fresh print works its whole start zone and the opening program never stalls; each resource declares its required tier (data-driven; numbers below are made-up tuning values):

| Resource | Required tool tier |
|---|---|
| Wood, Stone, Sand | 0 |
| Iron, Coal | 1 |
| Copper, Tin | 2 |
| Silver, Gold | 3 |
| Crystal | 4 |
| Water | — (pumped by a structure, not mined) |

The tier ladder is the arc of the colony: chop, dig, electrify, get rich, get brave.

**Only the DRILL has a reach ladder, and it tops out one grade early.** Grade 4
already reaches Crystal, the deepest resource tier, so **grade 5 is a quality
step** — more yield per swing, faster swings — rather than a new reach. That is
not a special case so much as the normal one: the other nine tools never had a
reach dimension at all, so every grade of theirs is a quality step. The drill
is simply the tool where the first three grades happen to buy *access* as well.
Grades 2–5 are purchasable for every tool, which is what keeps the catalog
dense enough that no level is dead (Q118, assertion 3).

**The ladder rule (Q72, narrowed by Q118): no tool may be priced in a material
that its own ladder unlocks at or above the grade being bought** — no tier's
key is ever locked behind its own door. Because only the drill unlocks
materials, this binds on the drill alone today; it is written generally so a
future unlocking tool (a Pump gating Water, say) is covered without an
amendment. Refined goods resolve through their recipes, so the effective
mining tier of a price is the deepest raw input it needs: Glass and Lens 0,
Steel 1, Bronze and Wire 2, Chips and Gold Chip 4.

**The seller-side corollary (P1 ruling, 2026-08-01): the rule binds on
sellers too.** A structure priced above the ladder it *exclusively* sells is
the same deadlock one step removed — the Upgrade Station prices in Chips
(effective tier 4): Crystal needs a **grade-4** drill and even the Foundry's
Bronze needs grade 2 — and every drill grade above the free first is sold
only at a Station.
Such a seller must have a **granted instance in the starting state**: hence
the ruined Upgrade Station in every start base, repairable for tier-0/1
materials ([structures-and-start.md](structures-and-start.md)).

Tools are bought at the **Upgrade Station** (Q105 folded the Printer's
tool-making role into the one pad flow) and are **licensed by level** — a bot
may buy a grade-N tool once *either* that track's level *or* its total level
reaches N. **Buying costs no XP and resets nothing** (Q111): the licence is
what the level bought, and the level stays.

**The drill ladder** is the one the anti-circularity rule actually binds, because
the drill is the only tool that unlocks materials:

| Drill grade | Priced in | Made from what you already mine | Reach |
|---|---|---|---|
| 1 | free with the chassis | — | tiers 0–1 (the start zone) |
| 2 | Steel | Iron + Coal (tier 1) | + tier 2 |
| 3 | Bronze | Copper + Tin (tier 2) | + tier 3 |
| 4 | Bronze + **Gold** | tier-2 alloy + tier-3 wealth — *get rich to get brave* | + tier 4 |
| 5 | Bronze + Gold | — | no new reach: a **quality** step (yield, swing speed) |

**Every other tool prices by resource ROLE, not by rung** — nothing stops a
weapon costing Bronze or a CPU costing Chips, because neither unlocks the
material it is priced in. *Bronze arms, Chips think*: weapons and civil kit in
Bronze, sensing in the Sand → Glass → Lens chain, and compute starting cheap
and escalating — **CPU 2 in Wire, 3 in Silver + Wire, 4 in Chips, 5 in Gold
Chips**, with the flat capacity buys (memory, stack, log buffer) starting on
Wire too, so program size is never the last thing a colony unlocks (Q118).

All ten tools carry grades 2–5, which is what makes the catalog dense enough
that no reachable level is dead. The full catalog is [06-progression.md](../06-progression.md)'s.

Each rung is bought with the previous rung's ore, so reaching Crystal is a wealth investment on top of a territorial risk ([05-terrain.md](../05-terrain.md)): the bot that can mine it is expensive, and it's working next to Corruption. Escort it.

**Build tool grade 2 is the ladder's one exception** (Q84, restated for Q111's tool model): the civil-kit grade prices in **Steel**, so the first heavy builder is a free print plus a Steel upgrade, and the Smelter is never locked behind the Bronze it would produce. (Every print already holds build tool grade 1, so ordinary `build()` never waits on anything — only the heavy verbs need grade 2: field repair, `hijack`, and nest claim/raze, per Q105-R2.)

