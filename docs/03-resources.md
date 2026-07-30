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

- **Every resource gates a different verb.** This is the load-bearing rule: a
  material that gates nothing, or gates what another already gates, does not
  ship. Adding one means naming the verb it gates.
- **The ladder must not be circular** (Q118). No tool may be priced in a material
  its own ladder unlocks at or above the grade being bought. Today that binds on
  Mining alone, but it is stated generally so a future unlocking tool needs no
  amendment. Three load-time assertions enforce it: anti-circularity, no orphan
  materials, no gaps.
- **Energy is a rate and Data is a currency.** Neither is a pile in a depot, so
  neither obeys hauling, cargo, or the Request Box.
- **Only the compute family draws coolant** (Q119), declared per catalog entry
  rather than per code branch — the failure that made every mechanical tool cost
  Water.
- **Compute does not sit behind maxed mining** (Q118). The compute ladder starts
  on Wire and escalates to Chips; program capacity grows with it.
- **All numbers here are tuning constants** bound for data files, never code.
