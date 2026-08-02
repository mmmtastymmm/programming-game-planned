*Part of [02-agents](../02-agents.md).*

# XP & Specialization

Bots earn XP **per task track**, by doing:

| Track | Earned by | Tool it licenses | Level perks |
|---|---|---|---|
| Mining | units harvested, any resource kind (yields are typed, [01-language.md](../01-language.md)) | **drill** | mine yield (bounded), at L3: `mine()` action time −25% |
| Hauling | cargo-distance delivered | **cargo rack** | cargo capacity (bounded), at L3: +10% move speed while loaded |
| Combat | damage dealt / kills | **weapon** | damage (bounded), at L3: +1 **hearing range** vs enemies (Q74) |
| Building | build/repair progress | **build tool** | build speed (bounded), at L3: repairs restore +25% more |
| Scouting | nodes discovered + surveys completed (Q83 — sim events, read off the phase-5 per-faction known-tiles writes — Q94) | **optics** | sensor range (bounded), at L3: immune to Corruption's cycle tax ([05-terrain.md](../05-terrain.md)) |

**Perks take three shapes, and none of them is linear-per-level** (Q121). The
ladder is uncapped, so a `+10% per level` perk would run away — a `+1 sensor
per level` scout would see the entire map somewhere around level 30 and switch
fog of war off for its faction, which is precisely what this rule exists to
prevent. Instead:

1. **Tools carry the step changes.** This is the load-bearing one: because the
   power lives in a purchase rather than in the level count, an uncapped ladder
   is harmless by construction.
2. **Milestones** are named levels that grant something qualitative — the `L3`
   entries above are exactly this shape, and they are the model for any new one.
   Most levels grant no automatic perk at all.
3. **Bounded continuous** perks, where growth genuinely reads as continuous
   (a hull toughening with age, bearings wearing in, optics sharpening), use an
   integer **hyperbolic**:

   ```
   bonus = max_bonus × level / (level + K)
   ```

   No floats, deterministic, asymptotic: half of `max_bonus` at level `K`, 80%
   at `4K`, and never more than `max_bonus` however far the ladder runs. Both
   constants live per-perk in `xp.ron` alongside the track's `curve_base`;
   magnitudes are tuning and deliberately not fixed here.

This is why every level still matters without any level being large: **a level
is a licence** (Q118 — the catalog is dense to grade 5, so no reachable level is
dead), and only some levels are also a perk.

These five are the **task tracks** — earned by what the program chooses to do. A second family levels by what merely *happens* to the machine:

## Body tracks (use-based)

| Track | Earned by | Improves |
|---|---|---|
| Age | every tick survived | max HP **and self-repair rate** — the machine that lasts, lasts (and mends) |
| Mileage | every tile traveled | move rate — worn-in bearings |
| Processing | every operation the VM executes (Q100) | cycles per tick — the CPU tool's track; the one body stat that is also a compute stat |
| Hiding | per **detection episode**: seen *or heard* by an enemy faction, re-armed only after escaping both (edge-triggered, like the hurt line) | signature — the more it's *caught*, the better it hides (−1/level, tuning) |
| Flinch | every flinch endured **from a hostile source** — enemy damage, enemy rams; self-inflicted signals grant nothing | flinch duration |

Same quadratic *shape* for every track, but **each track carries its own `curve_base`** and there is **no level cap** (Q111/Q123) — all tuning. The theme is scar tissue: **the machine gets good at whatever keeps happening to it** — a bot that has flinched a hundred times flinches fast, a bot that keeps getting spotted learns to be unseen, and a bot that has simply *survived* is harder to kill. Age is the pillar-3 stat distilled: its XP is literally time, so what death costs you is unrecoverable by definition — you can reprint the program in seconds, but the replacement is *young*. **Farming is legal, but every event must be real** (Q68, decided): grinding is allowed play — walking laps for Mileage is fine, since walking is what bots do. The test for whether a track needs a guard is **whether farming it is FREE** (Q116): guard it when a bot can farm it *alongside* its job or at no opportunity cost, leave it alone when farming costs the bot its whole output. So Flinch counts only from hostile sources (a two-bot mosh pit in your base earns nothing), Hauling excludes stock withdrawn and cycled back (a withdraw→lap→deposit loop uses the depot the bot was already at), and detection is per-episode with an escape re-arm (parking beside a passive harvester earns one XP, ever — slipping in and out of enemy coverage is what levels Hiding). **Mileage and Processing get no guard**: a bot pacing two tiles or spinning `x = 1` is a bot not mining, not hauling and not building, still drawing upkeep against the fleet cap — an exploit is something that beats playing properly, and these lose to it. Tracks cut over the milestone's life: **Regen** and **Print** (unfixable or unlevelable), **Boot** (a perk and a documented income, never once awarded), and **Learning** (Q121 — it measured 10% of every other award, which the mean-across-tracks total level now measures from the same data without a stored copy).

**XP stores CENTI-points** in an `i64` (Q111 — every table in these docs still reads in whole XP, the human unit; 1 whole XP = 100 centi). The finer unit is what lets a sub-100% XP-gain multiplier (a quirk like Tech Debt) reduce a small award instead of flooring it to zero. **Storage never decreases and is never capped**: buying a tool costs materials, never XP, and nothing resets — the curve saturates on its own, so no clamp is needed.

**Total level is the MEAN across all ten tracks** (Q111), which makes it a seniority-and-breadth measure rather than a clock: a bot that has done many things scores on the mean, a specialist scores on its own track, and **a tool is licensed by whichever of the two is higher**. (Quirk manifestation reads the Age **level**, not raw Age XP — [09-quirks.md](../09-quirks.md) — so it re-derives itself from whatever pace Age currently carries instead of drifting every time the income is retuned.)

**Income constants (Q83/Q123 — first-pass, all tuning):** Mining 1/unit · Hauling 1 per unit-per-10-tiles · Combat 1 per 10 damage + 25/kill · Building 1 per 10 progress · Scouting 5/node + 10/survey · **Age 1 per 50 ticks** (0.2 deci = 2 centi per tick — cut 5× by Q123 so that simply existing no longer out-earns working) · Mileage 1/tile · Hiding 25/episode · Flinch 10/hostile flinch · **Processing 1 per 10 operations executed** (Q100 — the track behind cycles-per-tick).

**Pace is set per track, not by the income alone** (Q123). Because each track has its own `curve_base`, an event's payout keeps its fiction while the ladder normalises how fast the track climbs:

```
curve_base = dedicated_rate × target_ticks_to_L5 / 15
```

with a deliberate **two-tier target**: a bot doing nothing else reaches **L5 in ~10 minutes on a JOB track** (Mining, Building, Scouting, Combat, Hauling) and **~50 minutes on an AMBIENT one** (Age, Mileage, Processing, Hiding, Flinch). That gap is the whole point — it is what lets a specialist's own track outrun the seniority clock, so specialising means something. First-pass bases, in centi:

| Track | Dedicated rate | `curve_base` | | Track | Dedicated rate | `curve_base` |
|---|---|---|---|---|---|---|
| Mining | ~80 /tick | 32,000 | | Processing | ~15 /tick | 30,000 |
| Building | 10 /tick | 4,000 | | Mileage | ~7 /tick | 14,000 |
| Scouting | ~20 /tick | 8,000 | | Hiding | ~5 /tick | 10,000 |
| Combat | ~10 /tick effective | 4,000 | | Age | 2 /tick | 4,000 |
| Hauling | ~1.4 /tick | 600 | | Flinch | ~1 /tick | 2,000 |

Three to watch in playtest: **Combat**'s effective rate is a placeholder (its in-fight rate is ~100 centi/tick and its duty cycle is whatever the match gives it) and its 2,500-centi kill bonus is 60% of a first level; **Hauling**'s base is the lowest by far, so hauling levels are cheap for everyone and a dedicated hauler out-levels a part-timer by only ~1.8× where mining's margin is far wider; **Processing**'s rate scales with cycles, which its own tool buys.

The dichotomy that organizes all growth, restated by Q111/Q121: **levels are earned, tools are bought, and the level is what licenses the tool.** Every track — body and task alike — earns levels by doing its thing; every track has a tool sold at the Upgrade Station; and a bot may buy a grade-N tool once *either* that track's level or its total level reaches N. Levels rarely change a stat by themselves (see the perk table); **the tools carry the power**, which is what keeps an uncapped ladder from running away.

Design intent:

- **XP follows behavior, not assignment.** There's no class picker; a bot whose program mines becomes a good miner. The program *is* the specialization mechanism — reinforcing pillar 1.
- **The tracks are the body plan.** With chassis classes gone, growth carries *all* physical differentiation: task tracks license the working tools (drill, cargo rack, optics, build tool, weapon), body tracks license the chassis ones (hull plating by Age, drivetrain by Mileage, gyros by Flinch, signature dampener by Hiding, CPU by Processing). Nothing physical is chosen at print time; everything physical is a biography **plus what that biography entitled you to buy**.
- **Perks are task-relevant** (requirement 7): a veteran miner mines faster/more, a veteran fighter hits harder. Cross-track XP is tracked independently; hybrid programs produce hybrid veterans, but slower.
- **Total loss on destruction** (requirement 8) makes veterans strategic assets, and it is **unconditional** — nothing in the game preserves XP across a death. The pressure valves are all things you do *before* the bot dies: hurt-handler retreat programs, Repair Bays, escorts for your best miners, and field-repair rescue during the self-destruct countdown. Targeting enemy veterans — and double-handling or salvage-sniping them to deny rescue — becomes PvP strategy.

## XP curve (quadratic increments)

Each level costs `curve_base × n` more than the last, so the cumulative cost of
level *n* is `curve_base × n(n+1) / 2`. **`curve_base` is per track** (Q123, table
above) — the shape is shared, the pace is not. At a base of 100 whole XP:

| Level | XP for this level | Cumulative |
|---|---|---|
| 1 | 100 | 100 |
| 2 | 200 | 300 |
| 3 | 300 | 600 |
| 4 | 400 | 1000 |
| 5 | 500 | 1500 |

**There is no level cap** (Q111). The ladder runs until `i64` does — which at any
sane base is tens of millions of levels, i.e. never. Most levels grant nothing
automatically; what every level does grant is a **licence to buy that track's
next tool grade** (Q118), and the catalog is dense to grade 5 so no reachable
level is dead. Past grade 5 a level is pure score, and that is deliberate: it
is fun to let it ride.

Early levels come fast (new bots feel like they're growing immediately); a high
level represents real accumulated play — which is exactly what makes losing one
hurt. All values are tuning constants like everything else.

## XP visibility

Levels are visible to **everyone** (pillar 2: transparency) — a veteran bot has visible wear/decals. In PvP, your shiny L5 hauler is a target. This is intentional.

