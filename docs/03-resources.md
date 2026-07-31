# Resources

**Eleven raw materials → seven refined products**, plus **Energy** (a rate, not a pile) and **Data** (a currency, not a mineral). Each exists to gate a *different verb*, so shortages push players toward different behavior instead of "more of everything." (All recipes/rates are tuning values.)

## The parts

| File | Owns |
|---|---|
| [the-tree.md](03-resources/the-tree.md) | The raw → refined tree, what each resource's role is, and the design rules the tree has to satisfy. |
| [harvest-tiers.md](03-resources/harvest-tiers.md) | Which tool grade works which material tier. |
| [structures-and-start.md](03-resources/structures-and-start.md) | The resource-relevant buildings, the ally Request Box, and what each player begins with. |
| [decided.md](03-resources/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **Every resource gates a different verb** — canonical in
  [the-tree.md](03-resources/the-tree.md). This is the load-bearing rule: a
  material that gates nothing, or gates what another already gates, does not
  ship. Adding one means naming the verb it gates.
- **The ladder must not be circular** (Q118) — canonical in
  [harvest-tiers.md](03-resources/harvest-tiers.md) and
  [decided.md](03-resources/decided.md). No tool may be priced in a material
  its own ladder unlocks at or above the grade being bought. Three load-time
  assertions enforce it: anti-circularity, no orphan materials, no gaps.
  **This rule is known to be too narrow.** Q118 scoped it to bind on the drill
  alone, so it does not catch a *structure* priced above the ladder it sells —
  which is exactly how [PROBLEMS.md](PROBLEMS.md) P1 (the colony cannot bootstrap
  at all) got through. Treat the rule as necessary but not sufficient until P1 is
  ruled on; the fix is expected to restate it to cover the class.
- **Energy is a rate and Data is a currency** — canonical in
  [the-tree.md](03-resources/the-tree.md). Neither is a pile in a depot, so
  neither obeys hauling, cargo, or the Request Box.
- **Only the compute family draws coolant** (Q119) — canonical in
  [06-progression.md](06-progression.md), *not* in this doc. Declared per catalog entry
  rather than per code branch — the failure that made every mechanical tool cost
  Water.
- **Compute does not sit behind maxed mining** (Q118) — canonical in
  [06-progression.md](06-progression.md). The compute ladder starts
  on Wire and escalates to Chips; program capacity grows with it.
- **All numbers here are tuning constants** bound for data files, never code —
  canonical in CLAUDE.md's doc conventions.
