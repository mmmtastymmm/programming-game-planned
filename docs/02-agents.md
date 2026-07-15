# Agents (Bots)

A **bot** is a printed machine that runs exactly one [Pyrite](01-language.md) program. Bots are the only actors the player owns; everything the colony does, a bot does.

## Anatomy

Every bot is a **chassis + modules + program** — and every chassis is **identical**:

| Part | What it determines |
|---|---|
| **Chassis** | The universal printed body ([03-resources.md](03-resources.md)) — **no classes**. Every bot prints with the same floor statline (below); everything above the floor is earned (XP), slotted (modules), or rolled (quirks). |
| **Compute upgrades** | Cycles per tick, program memory, variable slots, stack depth, log buffer, Coprocessor — bought **per-bot at an Upgrade Station** ([03-resources.md](03-resources.md)), a player-placed structure the bot must physically walk to. Compute never occupies a module slot. |
| **Tool modules** | Which function blocks the bot can *physically* execute (a mining drill enables `mine()`, a blaster enables `attack()`). Harvest tools are **tiered** — level N mines resources of tier ≤ N ([03-resources.md](03-resources.md)). Language unlocks are colony-wide; tools are per-bot. |
| **Program** | One of the colony's **colored program slots** ([01-language.md](01-language.md)) — one color per Printer, printer count gated by controlled nests. The bot is visibly tinted by its color. Redeploying a color updates all its bots at their next loop boundary; printer count sets the colony's **fleet cap**, and each printer after the first carries a **target share + selection key** choosing which bots wear its color (the first takes the remainder), enforced via the recall interrupt ([01-language.md](01-language.md)). |

The universal base statline (the floor — roughly the worst of every option from the old class table; all tuning):

| Stat | Base |
|---|---|
| HP | 40 |
| Move rate | 14 ticks/tile (slow) |
| Cargo | 4 |
| Sensors | 5 tiles |
| Module slots | 1 |

**Identity is earned, not printed.** A fresh print is slow, fragile, dim-eyed, and nearly empty-handed — the same sorry machine every time. What it becomes is written by what it does — and by simply lasting: XP tracks grow the body (HP by Age, speed by Mileage), total XP builds out the frame (slots), modules extend it, quirks bend it. The old sensing/carrying/surviving triangle didn't disappear — it moved from a print-time class picker to a lifetime of behavior. Identical rookies are the point: divergence starts at the first tick, and the print-time identity choice relocated to the first module + the color.

## The Stat Sheet

The canonical list of every number a bot owns. Anything anywhere in the design that makes one bot better or worse than another — hardware ([06-progression.md](06-progression.md)), XP perks, quirks ([09-quirks.md](09-quirks.md)), terrain and colony state — modifies a row on this sheet; if an effect can't name its row, it isn't a stat effect. All values are integers and tuning constants (`stats.ron`).

Five sources feed the sheet, in modifier-pipeline order: **universal base** (the identical print floor) → **hardware** (Chips: Upgrade Station purchases + slotted modules) → **XP perks** (earned) → **quirks** (rolled/acquired) → **state** (temporary: Damaged, brownout, Corruption, loaded, terrain).

The sheet lists only two things per stat: the **universal base** every bot prints with, and **what grows it** — an XP track, an Upgrade Station purchase, or a derived formula. Everything else declares itself at its source — modules in [06-progression.md](06-progression.md), quirks in [09-quirks.md](09-quirks.md), states in their own sections — so there is exactly one place to look up any modifier. The "Grown by" column *is* deliberate duplication of the growth systems, and that's the point: **a — in that column means nothing grows the stat today.** Some are engine constants on purpose; the rest are open gaps (Q66).

Domains: **Compute** (how well it thinks), **Body** (survives and carries), **Senses** (perceives and is perceived), **Tools** (acts), **Survival** (fails).

| Domain | Stat | Base | Grown by | What it buys you |
|---|---|---|---|---|
| Compute | **Cycles per tick** | 1 | **Upgrade Station** (walk there, pay Chips) | The speed of thought. Every Pyrite operation costs cycles, so this is the exchange rate between *smart* and *slow*: a 1-cycle bot can run your scan-and-coordinate masterpiece — it'll just stand there thinking between moves. The single most contested stat in the game. |
| Compute | **Program memory** | 32 lines | Upgrade Station | The longest color program this bot can receive at deploy. Each color's deployed artifact sets a **hardware bar** its printer enforces — under-spec bots simply aren't claimed ([01-language.md](01-language.md), Q52 answered). |
| Compute | **Variable slots** | 8 names | Upgrade Station | How many distinct names the program may bind — checked at deploy, like program memory. |
| Compute | **Stack depth** | 4 frames | Upgrade Station | How deep `def` calls nest; what makes recursion viable. Overflow is an ordinary fault — chip damage and restart. |
| Compute | **Log ring buffer** | 8 entries | Upgrade Station | How much story survives: richer `upload_log()` telemetry, richer Black Boxes, richer forced boot uploads. The forensics stat. |
| Compute | **Coprocessor** | absent | Upgrade Station | Think *while* an action resolves. Without it, a blocked bot is a paused bot; with it, travel time is compute time. |
| Body | **Max HP** | 40 | **Age track**: every tick survived — old bots are tough bots | Abuse before the wreck. Doubly load-bearing: the Damaged line and the hurt signal are *percentages of this* — more HP also widens the healthy band. |
| Body | **Damage taken** | 100% of incoming | — (by design) | Deliberately **no armor stat** — HP is the whole defense, so combat math stays legible and a bigger gun is always a bigger gun. Only quirks scale incoming damage, and only from specific sources. |
| Body | **Self-repair** | 1 HP / 1000 ticks | **Age track**: seniority mends (a Regen track was cut — self-hurt-then-heal farmed it for free) | Old scars fade on their own — and fade faster on old machines. Never combat-relevant; Repair Bays and field-repair are the real medicine. |
| Body | **Move rate** | 14 ticks per tile | **Mileage track**: every tile traveled; Hauling L3: +10% while loaded | Response time and hauling throughput. Terrain multiplies it ([05-terrain.md](05-terrain.md)), so the map decides how much earned speed is worth. |
| Body | **Cargo capacity** | 4 | Hauling: +10%/level | Fewer round trips. Since travel is time and time is cycles not spent working, cargo is secretly a compute stat. |
| Body | **Module slots** | 1 | **total-XP milestones**: +1 slot at thresholds (tuning), cap 3 | Every bot starts with one slot, so *which* module fills it is the identity decision. Veterancy literally builds out the frame: total XP across all tracks unlocks the 2nd and 3rd slots — the old chassis range, now earned. |
| Senses | **Sensor range** | 5 tiles | Scouting: +1/level; Combat L3: +1 vs enemies | **One stat, two radii** (Q57): eyes-only fog reveal = this value; the query radius (`closest()`, `exists()`, `scan_*()`, `search()`) is derived larger — × `query_factor` (tuning ~150%: base 5 eyes / 7 queries) — so sensing outranges seeing and every point here widens both. Per-kind bonuses extend queries only; beyond-eyes queries return entities on fogged tiles ([05-terrain.md](05-terrain.md)). A blind bot isn't dumb — it's uninformed, and no cycle budget fixes that. (Q53 answered: base sight is innate — no bot is ever blind; the **Optics tool module**, built from Lenses, extends it — [06-progression.md](06-progression.md).) |
| Senses | **Signature** | 0 | **Hiding track**: per **detection episode** — XP on being detected by an enemy, re-armed only after fully escaping detection (−1 signature/level, tuning) | How far away this bot is *sensed* (Q54, decided): enemy queries return it at `their query radius + this signature`, floored at 1 — adjacency always detects. Default 0 = sensed at the normal rate; noisy (+, Loud Fans) is sensed *beyond* the normal radius; quiet (−) must be approached. Being sensed ≠ being seen — fog reveal is untouched (Q57). The formal home for every "enemies sense this bot at ±N" effect. |
| Tools | **Damage** | weapon module | Combat: +5%/level | Kill speed — which in the wreck economy is also *rescue-denial* speed. |
| Tools | **Action time** | per tool (e.g. one `mine()` swing ~20 ticks) | Mining L3: −25% mine time | Ticks per swing once the decision is made. Cycles govern *deciding*; action time governs *doing* — a maxed CPU on slow tools is a philosopher with a shovel. |
| Tools | **Harvest tier** | tool level N | — (tool upgrades only) | Which resources `mine()` may touch at all (tier ≤ N) — a hard gate, not a rate. |
| Tools | **Build/repair rate** | tool (1 progress/tick) | Building: +10%/level; L3: repairs +25% | Blueprint and field-repair throughput — including how fast a wreck rescue completes. |
| Survival | **Damaged line** | 50% of max HP, fixed | — (engine constant) | Below it the bot is **Damaged**: visible sparks, speed and cycle budget −25%. An engine state, not a policy — nothing moves it. |
| Survival | **Hurt line** | = Damaged line (50%) | — (policy, not progress: the `hurt_line` env variable, [01-language.md](01-language.md)) | When the hurt handler fires (edge-triggered, re-arms above the line). Defaults to the Damaged line but decouples via `setenv` or quirks — the Damaged penalties stay at 50% regardless. Earlier = safer and less productive; later = the opposite. Not better or worse — a *policy* knob your handler must agree with; the idiomatic place to set it is the `on boot:` window. |
| Survival | **Flinch** | `handler_init()` ≈ 15 ticks | **Flinch track**: every flinch endured **from a hostile source** (enemy damage, enemy rams) — self-inflicted signals grant nothing | The forced prologue on most signals — time spent locked and vulnerable before your window runs. The shorter it is, the less every problem costs. |
| Survival | **Boot ritual** | ~20 ticks of engine time | **Boot track**: every **rescue boot** (field-repair revival) — print and re-color boots grant nothing | The fixed engine portion of every print, rescue, and re-coloring; the forced `upload_log()` and any `on boot:` window then run at normal cycle costs on top. All of it is double-handle exposure — boot time is rescue risk. |
| Survival | **Countdown** | ~30 s + per-XP bonus | total XP (built into the base formula) | The wreck's self-destruct timer: your rescue window, their salvage/hijack window — one number, three parties racing it. |
| Survival | **XP gain** | 100% | **Learning track**: XP earned in any other track raises the multiplier | Multiplier on every track's earn rate — how fast this body becomes a veteran. Deliberately compounding: veterancy begets veterancy. |
| Survival | **XP preserved** | 0% on destruction | — | What a reprint inherits. The one stat that softens pillar 3 — which is why its module (Backup Core) is late and expensive ([06-progression.md](06-progression.md)). |
| Survival | **Salvage profile** | +5% color decryption to the salvager; a cut of the **build receipt**; reprint at full cost | *derived* (Q68): the build receipt — a fraction of every material invested in the bot (Steel chassis, module materials, Chips-worth of compute) | What your corpse is worth to the enemy — and what the replacement costs you. Scales with the bot: the more it has become, the juicier the kill — and it *looks* juicy (enemy-visible, pillar 2). |
| Survival | **Print time** | ~10 s | — (engine fact) | Reprint turnaround. Fleet resilience is really countdown + print time: how long a hole in the line stays a hole. |
| Survival | **Upkeep draw** | per-bot Energy + Steel (`upkeep.ron`) | *derived* (Q68): base draw + an increment per station upgrade, slotted module, and track level (`upkeep.ron` factors) | What the colony pays per tick to keep it. Veterans cost more to run — power is priced. Individually invisible, collectively the real population cap: sustained excess → brownout → scrap recalls. |

Reading the column: **compute is bought, not practiced** — walked to an Upgrade Station and paid for in Chips, never occupying a module slot ([06-progression.md](06-progression.md), [03-resources.md](03-resources.md)). The passive Survival stats level **by happening** — surviving, traveling, flinching under fire, rebooting from a rescue, getting caught all train the machine they happen to (each source-filtered so the bot can't stage its own XP — see Body tracks). **Salvage profile and upkeep are derived sums** — they scale with everything else, so a veteran is simultaneously pricier to run and juicier to kill. The remaining — are engine facts (Damaged line), policy knobs (hurt line), design refusals (no armor), or module territory (XP preserved). Every body stat now has an earned path: **brains are bought, the body is earned** (Q66, answered).

Interpreter *costs* aren't bot stats — they live in `costs.ron` — but they support **per-bot overlays**: a quirk that makes `send()` cheaper or `if` pricier is a bot-local overlay on the cost table, the exact mechanism biome overlays already use per-map ([01-language.md](01-language.md)). Resolution: base table → biome overlay → per-bot quirk overlay, floored at 0.

### The ledger — per-bot facts that aren't dials

Riding alongside the stats: **color** (program slot), **program version**, **XP per track** (5 task + 6 body), **quirk list**, **local log contents**, **cargo held**. XP and quirks are the two ledger entries that reach back and modify the sheet above.

### Modifier pipeline (deterministic)

One fixed order, integer math:

> **base (universal print / tool) → hardware → XP perks → quirks → state (Damaged, brownout, Corruption, loaded, terrain)** → clamp (cycles/tick and move rate never below 1 stored unit — see Granularity below).

Percentages are integer percents of the running subtotal; within a layer, effects apply in stable list order (purchase order then slot order for hardware, acquisition order for quirks). Every stat's data entry declares its **improvement direction** — cargo up is better, move rate *down* is better; "Grown by" in the table above always means *improved* — and rounding is **pessimistic**: fractional results round toward worse-for-the-bot (gains floor, penalties ceil), so the tiebreak is uniform and replay-stable. The pipeline is sim state — same inputs, same sheet, every machine ([08-multiplayer.md](08-multiplayer.md)).

**Granularity (Q56, answered):** stats that take percent modifiers store **fine-grained integer units**. The cycle budget is stored in **centicycles** (stock bot = 100/tick — brownout's −50% leaves 50, so the over-extension penalty finally reaches stock CPUs, accumulating to one op every two ticks); cargo, build/repair progress, and move rate store **deci-units** (the millitile precedent — Hauling's +10% on base-4 cargo is a real +0.4 that compounds across levels). Each stat's `stats.ron` entry declares its **`unit_scale`**; the pipeline runs entirely in stored units with the same pessimistic rounding, and the UI displays human units. Flat-only stats (HP, slots, sensor tiles) stay whole. No percent perk needs converting to flat — the bases just stopped being small. Implementation note: the VM's cycle accounting migrates to centicycles, a replay-hash change that lands before any stat modifiers ship.

Design intent: **stats scale execution, never decisions.** XP, hardware, and quirks make the numbers bigger or smaller; only the program decides what the bot *does* — a stat sheet never rescues bad code (pillar 1).

## Damage, Faults, and Death

```mermaid
stateDiagram-v2
    [*] --> Printing: Fabricator job queued
    Printing --> Boot: print complete
    Boot --> Active: log uploaded (if any),<br/>program starts at line 1
    Active --> Damaged: HP below 50%
    Damaged --> Active: repaired
    Damaged --> Disabled: HP hits 0 → ABORT#colon;<br/>fully reserved — forced<br/>upload_log() + become_disabled()<br/>starts SELF-DESTRUCT COUNTDOWN
    Disabled --> Boot: field-repaired before<br/>countdown ends — XP PRESERVED
    Disabled --> Destroyed: countdown expires —<br/>the ONLY explosion —<br/>or wreck salvaged/destroyed
    Active --> Disabled: double-handle<br/>(second signal mid-handler)<br/>→ ABORT
    Damaged --> Disabled: double-handle<br/>→ ABORT
    Boot --> Disabled: any signal mid-boot<br/>= double-handle → ABORT
    Active --> Recalling: recall interrupt<br/>(engine-fixed, unwritable)
    Recalling --> Boot: reaches printer →<br/>re-colored, XP KEPT
    Recalling --> [*]: over-capacity scrap<br/>(partial Steel refund)
    Recalling --> Disabled: any signal mid-recall<br/>= double-handle → ABORT
    Destroyed --> Printing: reprint (program kept,<br/>XP LOST)
    note right of Destroyed
        EVERY destruction drops a
        BLACK BOX on the tile:
        local logs + cause of death
    end note
```

- **HP sources of loss**: combat damage and **unhandled faults** (each crash chips the chassis, [01-language.md](01-language.md)) — a buggy program is a slow suicide.
- **Passive self-repair**: chassis regenerate a trickle (tuning: ~1 hp / 1000 ticks — minutes per point). Enough that old scars eventually fade; not enough to matter in a fight — Repair Bays and field-repair are the real medicine. The hurt signal **re-arms** once health climbs back above its threshold, so a bot that recovers and is wounded again fires `hurt` again.
- **Damaged** (< 50% HP): visible sparks; speed and cycle budget reduced 25%. The Damaged line is fixed; the **hurt line** defaults to it but is a separate, movable stat (the `hurt_line` env variable, quirks — see the stat sheet). Crossing the hurt line fires the `hurt` signal — the amber-cloud handler template ([01-language.md](01-language.md)) — whose canonical `on hurt:` window drops cargo and retreats to a Repair Bay ([03-resources.md](03-resources.md)); the Damaged penalties apply at 50% regardless of where the hurt line sits. Pre-handler-unlock, polling `if health_low():` does the same job, worse.
- **Handler states are visible**: entering any handler template puts that signal's fixed icon and color in the bot's **thought cloud** ([01-language.md](01-language.md)) — friend and foe alike read a bot's crisis at a glance (pillar 2).
- **Disabled** (0 HP, or a double-handle): the bot **aborts** — a fully engine-reserved sequence, no player code: the engine force-calls `upload_log()` + `become_disabled()` — the same forced-ordinary-function pattern as `upload_crash_dump()` on unhandled errors; every death exits through those calls, so the logs always reach the cloud. There are no last words: the black box is whatever the bot logged while alive. It puts the bot into an inert wreck state with a **self-destruct countdown that scales with total XP** (base ~30s + per-XP bonus, tuning constants): rookies pop fast, veterans linger — the more a bot was worth, the longer the window to save it (and the longer the enemy has to salvage-snipe it; the race gets richer exactly when the stakes are highest). Before it ends, the wreck can be:
  - **field-repaired** (any bot with a build/repair tool module — the gate the Artisan class used to hold) → enters **Boot Sequence**, **XP preserved** — rescue missions are a real play;
  - **`salvage()`d** — by anyone, allies *or enemies* — for a cut of its **build receipt** (a fraction of every material invested in it — see the stat sheet's salvage profile) **plus +5% permanent decryption of the bot's program color** ([08-multiplayer.md](08-multiplayer.md)) — programs are read on murder, a few percent per murder, and the percentage never goes back down. Salvage destroys the wreck;
  - **`hijack()`ed by the enemy** (harm-enabled servers, [04-enemies.md](04-enemies.md)) — the wreck boots under *their* color, **XP intact**: your veteran now works for them (no code leaks — it runs their program). Hijacked bots are **not reprintable** by their new owner: a stolen veteran is a unique prize.

  The wreck race is three-way: your rescue vs. their salvage (intel + materials) vs. their hijack (the bot itself). The countdown scaling with XP means the richest prizes give everyone the most time to fight over them.
- **Destroyed**: the countdown expires (the wreck explodes — the *only* explosion in the game, [01-language.md](01-language.md)), or the wreck is salvaged/destroyed. **The blast is real** (Q55, decided): area damage to adjacent tiles (radius tuning), **scaling with the wreck's max HP** — the toughest veterans pop hardest, and since countdown also scales with XP, the most dangerous wrecks come with the longest warnings. It hits **friend and foe, on every server type** — no harm-setting carve-out; the 30+ second public countdown is the mechanical warning, and an "ally" who walks ticking wrecks into your base is answered socially, not by rules. Cargo stays inert (a loaded coal hauler is not a bomb — revisit as a match option, maybe). Deliberate scuttles are therefore a legal play, and clearing enemy wrecks near your structures is urgent. There is no instant-destruction path: a **double-handle** (any second handler firing while one runs, any combination) *aborts* the bot into Disabled like any other death. Reprinting at a Fabricator costs full resources; the program redeploys automatically; **all XP is lost**.
- **Black Box**: *every* destruction, by any path, drops a Black Box on the tile — the bot's local log ring buffer plus id, tick, cause of death, and an **env snapshot** (its runtime policy values — how env leaks to enemies, and forensics for you: "why did this one retreat early?" — Q58, [01-language.md](01-language.md)). Click to read it (with vision); `recover_black_box()` banks it permanently to the colony cloud (its printers, [03-resources.md](03-resources.md)). Enemies can grab it first — logs are battlefield intel. **Information always survives; XP is the only thing gambled.** What double-handling a veteran buys the enemy is an *early wreck on their terms* — downed deep in their territory, where the rescue race is theirs to lose.
- **Boot Sequence** (entered from Printing, a rescue, or a recall re-coloring): step 1 — if the local log buffer is non-empty, the engine force-calls `upload_log()` (a rescued bot files its own incident report); step 2 — the optional `on boot:` window runs ([01-language.md](01-language.md)); step 3 — the program starts from line 1 with fresh state, and the bot is Active. **Boot is an interrupt context**: any signal arriving mid-boot is a double-handle → abort, dropping the bot straight back into a wreck with the countdown running again. Rescuing under fire burns the rescue — time your field-repairs to secured ground. (Fresh prints boot too, but inside your base that's rarely dangerous — until someone raids the Fabricator.)
- **Recalling** ([01-language.md](01-language.md)): the engine-fixed, un-writable recall interrupt — the bot suspends its program and walks home. Fired when the target-share allocation assigns the bot a new color (re-colored at the claiming printer, **XP kept** — XP lives on the bot, not the color) or by colony over-capacity (**lowest-total-XP bot is scrapped** for a partial Steel refund). Recall is an interrupt context: double-handle applies for the whole trip, so rebalancing bots that are deep in hostile territory is a gamble — turn the dials when your bots are somewhere safe.

## XP & Specialization

Bots earn XP **per task track**, by doing:

| Track | Earned by | Level perks (per level, cap L5) |
|---|---|---|
| Mining | units of ore extracted | +10% mine yield, at L3: `mine()` action time −25% |
| Hauling | cargo-distance delivered | +10% cargo capacity, at L3: +10% move speed while loaded |
| Combat | damage dealt / kills | +5% damage, at L3: +1 sensor range vs enemies |
| Building | build/repair progress | +10% build speed, at L3: repairs restore +25% more |
| Scouting | new tiles revealed, resource nodes prospected via `search()` ([05-terrain.md](05-terrain.md)) | +1 sensor range, at L3: immune to Corruption's cycle tax ([05-terrain.md](05-terrain.md)) |

These five are the **task tracks** — earned by what the program chooses to do. A second family levels by what merely *happens* to the machine:

### Body tracks (use-based)

| Track | Earned by | Improves |
|---|---|---|
| Age | every tick survived | max HP **and self-repair rate** — the machine that lasts, lasts (and mends) |
| Mileage | every tile traveled | move rate — worn-in bearings |
| Hiding | per **detection episode**: detected by an enemy, re-armed only after fully escaping detection (edge-triggered, like the hurt line) | signature — the more it's *caught*, the better it hides (−1/level, tuning) |
| Flinch | every flinch endured **from a hostile source** — enemy damage, enemy rams; self-inflicted signals grant nothing | flinch duration |
| Boot | every **rescue boot** (field-repair revival) — print and re-color boots grant nothing | boot ritual time |
| Learning | XP earned in any other track | XP gain multiplier |

Same quadratic curve, same L5 cap, all tuning. The theme is scar tissue: **the machine gets good at whatever keeps happening to it** — a bot that has flinched a hundred times flinches fast, a bot that keeps getting spotted learns to be unseen, and a bot that has simply *survived* is harder to kill. Age is the pillar-3 stat distilled: its XP is literally time, so what death costs you is unrecoverable by definition — you can reprint the program in seconds, but the replacement is *young*. **Farming is legal, but every event must be real** (Q68, decided): grinding is allowed play — walking laps for Mileage is fine, since walking is what bots do — but each track's earn condition is **source-filtered so the bot can't stage its own XP**: flinches count only from hostile sources (a two-bot mosh pit in your base earns nothing), boots count only from genuine rescues (printer-dial toggling reboots nobody into XP), and detection is per-episode with an escape re-arm (parking beside a passive harvester earns one XP, ever — slipping in and out of enemy coverage is what levels Hiding). Two tracks were cut entirely as unfixable or unlevelable: **Regen** (self-inflicted chip damage plus passive healing was a free XP machine — self-repair growth folded into Age) and **Print** (no bot is ever printed twice; print time is a fixed engine stat).

The dichotomy that organizes all growth: **brains are bought, the body is earned.** Compute comes from the Upgrade Station for Chips; every body stat comes from the bot's lived history — tracks for the stats, total-XP milestones for the frame itself (module slots).

Design intent:

- **XP follows behavior, not assignment.** There's no class picker; a bot whose program mines becomes a good miner. The program *is* the specialization mechanism — reinforcing pillar 1.
- **The tracks are the body plan.** With chassis classes gone, leveling carries *all* physical differentiation: task tracks grow the working stats (cargo, sensors, work rates), body tracks grow the machine itself (HP by Age, speed by Mileage), and total XP builds out the frame (slots). Nothing physical is chosen at print time; everything physical is a biography.
- **Perks are task-relevant** (requirement 7): a veteran miner mines faster/more, a veteran fighter hits harder. Cross-track XP is tracked independently; hybrid programs produce hybrid veterans, but slower.
- **Total loss on destruction** (requirement 8) makes veterans strategic assets. The pressure valves: hurt-handler retreat programs, Repair Bays, escorts for L5 miners, field-repair rescue during the self-destruct countdown, and (late) the Backup Core module. Targeting enemy veterans — and double-handling or salvage-sniping them to deny rescue — becomes PvP strategy.

### XP curve (quadratic increments)

Each level costs `100 × n` more XP than the last, per track:

| Level | XP for this level | Cumulative |
|---|---|---|
| 1 | 100 | 100 |
| 2 | 200 | 300 |
| 3 | 300 | 600 |
| 4 | 400 | 1000 |
| 5 (cap) | 500 | 1500 |

Early levels come fast (new bots feel like they're growing immediately); an L5 represents real accumulated play — which is exactly what makes losing one hurt. All values are tuning constants like everything else.

### XP visibility

Levels are visible to **everyone** (pillar 2: transparency) — a veteran bot has visible wear/decals. In PvP, your shiny L5 hauler is a target. This is intentional.

## Reprinting Economics

- Reprint cost = original print cost (no discount) — the *sting* is XP, not extra resources.
- **Print cost is a match setting, default FREE** — a colony must never be soft-locked (no resources + no bots = no way to gather resources). Population stays bounded by printer dials and colony capacity; when a map does price prints, scrap refunds may be nonzero too (never exceeding print cost, or scrapping mints resources).
- Fabricators keep a **blueprint registry**: destroyed bots appear in a "reprint queue" UI with one-click requeue.
- Possible later unlock: *Backup Core* module — expensive, preserves 50% XP on destruction. Gated late so early losses stay meaningful ([06-progression.md](06-progression.md)).

## Decided

- **One universal chassis — no classes** (2026-07-13, supersedes the Scamp/Drudge/Bulwark/Artisan table). Every print is identical, starting at the floor of the old class options (HP 40, move 14 ticks/tile, cargo 4, sensors 5, slots 1 — all tuning). Specialization is earned, never printed: XP tracks, modules, and quirks are the only differentiation. Anything that was class-gated re-gates on tool modules (field repair needs a build/repair tool, not "an Artisan").
- **Signature ships** (2026-07-14, answers Q54). A bot is sensed at `perceiver's query radius + its signature`, floored at 1 (adjacency always detects): default 0 is sensed at the normal rate; noisy (+, Loud Fans) is sensed beyond the normal radius; quiet (−, Hiding levels) must be approached. Sensing only — fog reveal is untouched (Q57's sensing ≠ seeing), so a loud bot is *heard* past the eyes, never seen. Implementation is one asymmetric term in the perception check; replay-hash change when it lands.
- **Base sight is innate; Optics extends it** (2026-07-14, answers Q53 as the hybrid). The universal base (5 tiles) is part of the floor statline — a bot is never blind, so the Tier-0 starter program works on every print forever. The **Optics tool module** (2 Lens + 1 Bronze, tuning — the seeing chain's payoff, [03-resources.md](03-resources.md)) adds +2 sensor range (tuning), raising the one stat so *both* radii widen (Q57). On a one-slot rookie, Optics is the whole build — a dedicated prospector that gave up its ability to work; slot growth (Q66 milestones) relaxes that tax with seniority. Scouting levels remain the earned path to range; Optics is the bought one.
- **Percent-modified stats store fine-grained units** (2026-07-14, answers Q56). Cycle budget in centicycles, cargo/progress/move in deci-units; per-stat `unit_scale` in `stats.ron`; pipeline math in stored units, human units in the UI; flat-only stats stay whole. Brownout finally bites stock CPUs; small percent perks stop rounding to +0. VM cycle accounting migrates → replay-hash change, before stat modifiers ship.
- **A color's code sets its printer's hardware bar** (2026-07-14, answers Q52). Deploy computes the artifact's program-memory/variable-slot requirements; the printer claims only bots whose bought hardware fits (a filter before the selection key). A deploy is a rule edit — immediate re-allocation, under-spec members drop to the remainder, the editor warns and proceeds. The remainder color is capped at stock hardware (32 lines, 8 names) so it can receive anyone. Quirks never enter deploy-time stats.
- **The body grows by living** (2026-07-14, answers Q66). **Max HP — and, since Regen's cut, self-repair rate — grow with the Age body track** — its XP is ticks survived, so toughness is seniority and death's cost is unrecoverable time. **Move rate grows with the Mileage track** (tiles traveled). **Module slots unlock at total-XP milestones** (+1 at thresholds, cap 3 — the old chassis range, now earned). The organizing dichotomy: **brains are bought** (Upgrade Station, Chips), **the body is earned** (lived history). Identical rookies stay by design — divergence begins at tick one.
- **Compute is bought at the Upgrade Station; passive stats level by happening** (2026-07-13). All six compute stats upgrade per-bot at a player-placed **Upgrade Station** the bot must physically walk to (Chips — [03-resources.md](03-resources.md), catalog in [06-progression.md](06-progression.md)); compute never occupies a module slot, so slots are pure tool territory. Use-based **body tracks** (Hiding, Flinch, Boot, Learning, plus Q66's Age and Mileage) level the passive stats by the event itself happening — **source-filtered** (2026-07-14) so the bot can't stage its own XP: hostile-source flinches only, rescue boots only, detection per-episode with escape re-arm. Two tracks were cut the same day: **Print** (no bot is printed twice — could never level) and **Regen** (self-hurt-then-heal was a free XP machine; self-repair growth folded into Age).
- **Q68 closed** (2026-07-14): **no per-bot cost curve at the Upgrade Station** — the catalog's tier ladder (Mk2 → Mk3) is the whole curve, flat prices per entry. Both derived sums read the **build receipt**: `salvage()` returns a fraction of every material invested in the bot (which also settles Q69's salvage-composition edge — the mix, not just Steel) plus the decryption constant; upkeep charges a base draw plus an increment per station upgrade, module, and track level (`upkeep.ron` factors). Both sums are **enemy-visible for free** — they're physical (bolted-on gear, veterancy decals), and pillar 2 wants the juicy corpse to look juicy.
- **Perks apply to the bot only.** No colony-wide or program-attached XP effects — the veteran *is* the asset, which is what gives death its sting.
- **Quadratic XP increments** — level *n* costs 100×*n* additional XP (see XP curve table).
- **Population is capped by printers, priced by the economy.** Every printer adds a fixed amount to the colony's **fleet cap** (tuning, `printers.ron`) — territory (nests → printers, [04-enemies.md](04-enemies.md)) is the only hard ceiling on fleet size. Below that ceiling, the constraint is economic: upkeep is a **data-driven resource mix** (an `upkeep.ron`-style config, adjustable without code changes — prototype the system, then tune); **v1 config: Energy (primary drain) + Steel (chassis maintenance)** per [03-resources.md](03-resources.md). Over-extending doesn't block printing — it degrades the colony (brownout halves cycle budgets) and, if sustained, triggers **scrap recalls**: the colony recalls its lowest-total-XP bot for a partial Steel refund. The fleet's real operating point is an economic equilibrium the player feels; the printer-derived cap is only the ceiling above it.
- **Wreck countdown scales with XP** — base + per-XP bonus (tuning): veterans get longer rescue windows; rookie wrecks barely exist.
- **Fleet composition is target shares + selection keys** (2026-07-13, supersedes the per-printer desired-max dial). Each printer after the first sets a **target** — an absolute count or a percentage of the fleet cap (rounded down) — and a **selection key** naming which bots it claims (e.g. Red: 20 bots, keyed on highest Combat XP) — **any stat can be keyed**: every stat-sheet row and ledger number is a legal sort. The **first printer is the remainder bucket** — no target, no key, not editable: it holds every unclaimed bot, so shares always sum to the fleet. Editing printer rules has the player set the **priority order across all printers** (remainder implicitly last). Re-allocation — each printer claiming down the priority list by its key, entity-ID tie-breaks, oversubscribed targets clamping to what remains — runs on every rule edit and on a player-set **check interval** (every X ticks, default 1000, tuning); reassigned bots are recalled then and there, and between checks nothing reshuffles. Enforced by recall re-coloring (XP kept). Q64 answered 2026-07-14: keys carry a **best-first / worst-first direction toggle**, percentages read against the **cap** (stable targets), no composite keys in v1. Q65 answered the same day: a dormant printer's bots are **ghost machines** — off the allocation, frozen code, still paying upkeep; retaking the nest **uploads them again** into the fleet. Full mechanics in [01-language.md](01-language.md).
- **Bots are solid — one per tile.** When the next tile is occupied, the mover first tries a **random sidestep** (seeded sim RNG) among free neighbors that lose no ground toward its goal, then re-plans from where it lands. Only when **boxed in** does it **bump** — and collisions are **signals** ([01-language.md](01-language.md)): the rammer gets `bump`, the victim `bumped`, then both take chassis damage. The factory windows apply asymmetric-blame stuns — rammer ~5 s, victim a ~1.5 s stagger (clearing the scene before the at-fault bot re-plans). Your `on bump:` / `on bumped:` window *replaces* the stun with your own response. Double-handle applies: colliding with (or as) a bot mid-handler/boot/recall aborts it into a wreck (tuning; routed through the normal damage pipeline, so hurt/abort signals — and the double-handle rule during boots/recalls — apply). Dodges keep traffic flowing; a true head-on corridor deadlock now grinds both bots to mutual destruction — the deadlock self-clears, the expensive way. Channels remain the cheap way. Traffic jams are therefore *visible program bugs* (write better routing). Printed and re-colored bots emerge on the first free tile beside their printer; a fully walled-in printer holds finished prints until space opens.

## Open Questions

- Upkeep mix tuning: does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? (System is data-driven — answer via playtest, not redesign.)
