*Part of [06-progression](../06-progression.md).*

# Tools & Capacity Buys (per-bot, at the Upgrade Station)

Everything above the floor statline is **bought at the Upgrade Station**
([03-resources.md](../03-resources.md)): the bot physically walks to the structure,
mounts the pad, and sits inert for the entry's build time. Two families are sold.

## Tools — one per XP track, grades 1–5

Grade 1 ships free with the chassis; **grades 2–5 are bought, and each is
licensed by level** — a bot may buy grade N once *either* that track's level
*or* its **total level** (the floored mean across the ten tracks) reaches N
(Q111/Q118). Levels rarely change a stat by themselves; **the tools carry the
power**, which is what keeps an uncapped ladder from running away (Q121).

| Track | Tool | What its grades buy |
|---|---|---|
| Mining | **drill** | Harvest reach (grades 2–4 add resource tiers 2/3/4), then quality at 5 |
| Building | **build tool** | Build and repair rate. **Grade ≥ 2 gates the heavy verbs**: field repair, `hijack`, nest claim/raze (Q105-R2) |
| Combat | **weapon** | Damage per hit |
| Scouting | **optics** | Sensor range — one stat, so both circles widen ([05-terrain.md](../05-terrain.md)) |
| Processing | **CPU** | Cycles per tick |
| Hauling | **cargo rack** | Cargo capacity |
| Age | **hull plating** | Max HP |
| Mileage | **drivetrain** | Move rate |
| Hiding | **signature dampener** | Movement signature |
| Flinch | **gyros** | Flinch duration |

**Pricing follows resource ROLE, not a uniform rung** — *Bronze arms, Chips
think*. The one hard constraint is anti-circularity (Q118): **no tool may be
priced in a material its own ladder unlocks at or above the grade being
bought**, which binds on the drill alone, since only the drill unlocks
materials. So the drill climbs Steel → Bronze → Bronze+Gold, weapons and civil
kit price in Bronze, sensing in the Sand → Glass → Lens chain, and compute
starts cheap and escalates: **CPU 2 in Wire, 3 in Silver + Wire, 4 in Chips, 5
in Gold Chips**. Three invariants are checked at load: anti-circularity, no
orphan materials, and **no gaps** — every grade from 2 to a tool's ceiling has
an entry, so no reachable level is dead. (The Station itself prices above the
drill ladder it sells — Chips are effective tier 4 — so the first Station is
a **ruin in the start base**, repairable for tier-0/1 materials: the P1
ruling, [03-resources.md](../03-resources.md), Starting State.)

**Only the compute family draws coolant** (Q119) — the CPU tool and the
capacity buys below. Mechanical tools are not thermal and pay none; the
requirement is declared per catalog entry in data.

## Capacity buys — flat, unlicensed

Not tied to a track and not graded: capacities, not things a bot *does*.

| Upgrade | Cost | Effect |
|---|---|---|
| Memory bank | Wire, escalating | +32 program lines, +4 variables, +8 log ring-buffer entries |
| Stack extension | Wire, escalating | +4 call depth (base cap is 4; recursion is legal but overflows fault — stack is what makes recursive style viable, [01-language.md](../01-language.md)) |
| Log buffer | Wire | More ring-buffer entries — richer `upload_log()`, richer Black Boxes |

These start on **Wire** rather than Chips deliberately: how large a program a
player may write must not be the last thing a colony unlocks (Q118).

Hardware is where the "compute vs. claws" economy bites, as two material
streams (Q72): **Bronze arms, Chips think.**
