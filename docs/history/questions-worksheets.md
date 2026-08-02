# Questions — answered worksheet entries (archive)

The full worksheet bodies of answered questions, moved out of
[QUESTIONS.md](../QUESTIONS.md) on 2026-08-01 so the open-questions file holds
only open questions. The condensed per-question rulings are in
[questions-answered.md](questions-answered.md); the authoritative text is
always the owning doc's **Decided** section.

This file is history, not spec — an entry records the question *as it was
worked*, including drafts and corrections superseded by later rulings.

---

*Q111–Q119 opened 2026-07-27 by the M16 rethink. Three review passes over
the capability-slots milestone confirmed 45 defects; sorting them by root
cause showed they were not 45 independent slips but a handful of design
decisions that were never actually made, each patched three times by
implementation guesswork. M16's two fix commits are reverted (tag
`m16-fix-attempts`) and the underlying choices are recorded here first.*

**Q111 — how does a capability's earned LEVEL reset when its TIER is bought?
ANSWERED 2026-07-27: it doesn't — TIERS ARE REMOVED ENTIRELY.** The question
dissolves rather than resolving: both halves of Q105's tier/level split were
a mistake. A bot now has **levels and nothing else**, and XP is strictly
monotonic — buying never costs XP, nothing ever resets. The model:

  - **Ten tracks, structurally identical**: Mining, Hauling, Combat,
    Building, Scouting, Processing, Age, Mileage, Hiding, Flinch. (Boot was
    deleted here; Learning followed under Q121.) No capability/body split,
    no `Capability::track()` pairing deciding which tracks are special. One
    struct, one rule, one code path.
  - **Centi-points** (`i64`), replacing deci. The `gain_carry` and
    `learning_carry` fields exist today because deci was too coarse for a
    10% Learning cut of a 1-deci drip, and they hold hundredths-of-a-deci —
    i.e. centi. `learning_carry` dies with the Learning track (Q121).
    `gain_carry` is subtler than first written here: a carry buys one
    decimal place *below* the storage unit, so deci→centi moves the problem
    down rather than removing it. What is true is weaker — at centi
    magnitudes the shipped awards are large enough (Age's 1 deci/tick
    becomes 10 centi/tick, ×90% = 9 exactly) that truncation stops biting,
    so the carry becomes unnecessary in practice rather than impossible in
    principle.
  - **One quadratic curve**, applied uniformly, with **no level cap** — the
    ladder runs until `i64` does (~43 million levels at any sane base, so
    never in practice). Most levels grant nothing; specific ones do.
  - **Total level = the mean across all ten tracks, FLOORED** — docs/02
    already rules that rounding is pessimistic (gains floor, penalties
    ceil), and a level is a gain, so no new convention is needed. Passive
    tracks
    included. Seniority is a legitimate route to capability and staying
    alive is how it is earned — deliberately rewarding careful play.
  - **Tools are BOUGHT, and level licenses the purchase.** Every track has
    exactly one, and the mapping is fixed here so it is not guesswork:
    **Mining → drill · Building → build tool · Combat → weapon · Scouting →
    optics · Processing → CPU · Hauling → cargo rack · Age → hull plating ·
    Mileage → drivetrain · Hiding → signature dampener · Flinch → gyros.**
    (An earlier draft listed eleven names for ten tracks, the extra being a
    "training module" for the Learning track Q121 had already deleted — it
    would have been a tool licensed by a track that does not exist, so
    either the no-gaps assertion rejects the catalog or the entry is
    permanently unbuyable.) A bot may buy a
    tool whose requirement is met by **either** that skill's level **or** its
    total level. Because XP never decreases and the gate sits at purchase, a
    separate use-gate is redundant — a bot can never hold a tool it is not
    licensed for. Quirks may grant tools outright (e.g. a machine-learning
    quirk upgrading the processor); that is the deliberate exception.
  - **`XpTrack::Boot` is deleted** — an odd track that had a perk, a
    documented income, and no award site anywhere in the sim.

  Deleted outright by this ruling: `Capability`, `tiers[5]`, `tier()`,
  `tier_value()`, `TIER_INVESTMENT_WEIGHT`, `TierSpec` and the tier catalog,
  `tier_sensors`/`tier_damage_pct`/`tier_build_pct`/`tier_cpu_centi`/
  `tier_xp_scale_pct`, `StatCtx::track_scale`, `capability_level`,
  `track_cap_deci`/`track_cap_deci_scaled` and the settle-time clamp,
  `UpgradeOrder::Tier`, the Q105-R1 load validation, and the Q105-R3
  investment weighting. This is the root cause of roughly 26 of M16's 45
  findings, removed rather than repaired. ⚠HASH, and a units migration.

**Q112 — what does the energy bill read: EFFECTIVE or LIFETIME levels?
ANSWERED 2026-07-27: moot** — there is only one kind of level now. What the
question was really carrying survives as **Q122** (upkeep has lost its
ceiling, and `draw_per_module` has lost the `tier_value()` it multiplied).

**Q113 — what does `SelectKey::Xp(track)` rank by? ANSWERED 2026-07-27:**
the track's centi-points, directly. With one unscaled unit everywhere the
key is unambiguous and comparable across bots — the question only existed
because tier-scaled storage made "XP" mean different things on different bots.

**Q114 — how does a tier reset present in the inspector? ANSWERED
2026-07-27: moot** — nothing resets. The inspector shows level and
centi-points per track, plus the total level (the mean).

**Q115 — what does `investment()` measure? ANSWERED 2026-07-27; AMENDED
2026-07-28: the Backup Core is CUT, and `investment()` is XP plus installed
tools.** Two parts:

  **The Backup Core is removed from the game.** Its whole definition was
  "preserves every capability tier and wipes XP", and Q111 deleted tiers, so
  it had nothing left to preserve. The inversion first written here — a
  cloud backup keeping all XP and losing all tools — collided immediately
  with Q118's "grade 1 free with the chassis": taken literally the reprint
  had *no optics*, hence sensor range 0, contradicting docs/02's ratified
  "a bot is never blind, so the Tier-0 starter works on every print
  forever", and no drill, so the most expensive item in the game produced a
  blind bot that could never mine its way back and wandered until upkeep
  scrapped it. Rather than patch a third definition onto an item whose
  original purpose no longer exists, it is cut. **Total loss on destruction
  is now unconditional** — nothing in the game preserves XP across a death,
  which is a cleaner statement of pillar 3 than any softening item was.
  Gold Chips keep a sink: the highest CPU tool grades.

  **`investment()` is earned XP plus the value of installed tools.** With
  one unscaled unit (Q111), `xp_total()` is meaningful again and plain
  addition works. This is the "what would I lose" number, and it is what the
  scrap valve and `SelectKey::TotalXp` rank on. **Salvage deliberately reads
  something different** — the *build receipt*, what was actually spent —
  because a refund must not return value that was never paid: a
  quirk-granted tool (Q111 lets quirks grant tools outright) counts toward
  investment, since losing the bot loses it, but refunds nothing, since
  nobody bought it. Three formulas were live across the docs before this
  ruling; these are the two that survive, and the distinction between them
  is the point rather than an inconsistency.

**Q105-R2 RESTATED 2026-07-28.** Its gate on field repair, `hijack` and
nest claim/raze read "Building tier ≥ 2", and Q111 deleted tiers — orphaning
the rule in docs/01, docs/02 and docs/03 while TASKS.md still marked it done.
The direct translation is **a build tool of grade ≥ 2**: the grade is licensed
by the Building track, so the gate still means "an experienced builder", and
the heavy verbs stay the heavy verbs. Without it a zero-XP rookie print can
hijack a veteran's wreck, which collapses the wreck race that made `hijack`
the slowest and most gated verb of the four.

**Q116 — does the Processing track survive? ANSWERED 2026-07-27: yes, and
neither it nor Mileage gets an anti-farm guard.** Processing is one of the
ten, with the CPU as its tool. The objection was that it and Mileage are the
only tracks whose income counts an *action* rather than an *outcome*, so a
bot spinning `x = 1` in a bare loop, or pacing two tiles, farms them having
delivered nothing. The ruling is that this sorts itself out, and two later
decisions are why it now clearly does: **Q121** moved the power into tools
and made continuous perks hyperbolic-bounded, so the prize is slow and
asymptotic rather than compounding; **Q123** put both tracks in the ambient
bucket at ~50 minutes to L5. Meanwhile the cost never shrank — a farming bot
is not mining, hauling or building, still draws upkeep, still counts against
the fleet cap, and a level only *licenses* a tool it has earned no materials
to buy. **An exploit is something that beats playing properly; these lose to
it**, so they are bad strategies rather than exploits, and the opponent who
mined does the policing.

  **The distinction that makes this a principle rather than permissiveness**
  — and which explains why the guards that already exist must stay. Hauling
  excludes `withdrawn_aboard` (a withdraw→lap→deposit loop farms XP at the
  depot the bot was standing at anyway); Flinch pays only for hostile
  sources (a friendly ram costs the colony nothing); Hiding needs its
  detection episode to re-arm (or the same enemy pays out repeatedly for
  free). In all three, farming was **free** — available *alongside* the job,
  or at no opportunity cost at all. So the rule for any future track is:
  **guard it when farming is free; leave it alone when farming costs the
  work.** Pacing and spinning cost the bot's entire output, so they need
  nothing.

  Note also that Q105's specialisation ruling makes legitimate play dominate
  the "exploit": a hauler earns Mileage constantly just by doing its job,
  and one that pads its route by wandering simply delivers less.

**Q117 — how do tier-blind queries and a failing `mine()` coexist without
killing the fleet? ANSWERED 2026-07-27: NAMED minable queries, plus shipped
programs that handle the miss — and docs/01 needs no amendment.** The
failure was a fleet-killer: `move_to(closest(ore).expect())` then `mine()`
walks every bot to a seam its drill cannot work, and because `move_to`
returns `Ok` immediately at chebyshev ≤ 1 the loop collapses to
`closest → move_to (0 ticks) → mine → fault → restart`, about 3–4 ticks per
iteration. At `fault_damage: 2` a 40 HP chassis dies in ~20 faults ≈ **80
ticks, eight seconds** — and every bot on the same program does it at the
same vein together. (An earlier draft of this question guessed the
mine→haul round trip spread the faults out. It does not: the travel happens
once, then the tight loop is identical for specialist and generalist.)

  It is really **two** failures, and only one is a targeting problem:
  *(1)* the nearest ore is unworkable while workable ore exists; *(2)* no
  workable ore exists anywhere known — the mid-game transition, where
  `closest` returns `Err` and there is genuinely nothing to hand back. Any
  answer addressing only (1) moves the death from "when Copper is nearest"
  to "when Iron runs out."

  **The ruling, both halves:**

  - **Named queries, not a redefinition.** `closest(ore)` and `exists(ore)`
    are untouched and stay tier-blind, so docs/01's ratified rule —
    *"Queries return nodes regardless of tool tier — sensing isn't
    harvesting"* — remains literally true and needs no edit. Alongside them
    sit **`closest_minable(kind)`** and **`exists_minable(kind)`**, which
    answer the other question explicitly. The name carries the meaning
    instead of `ore` silently behaving unlike the union of its members.
    Both take a kind, so `closest_minable(copper)` means "the copper, if my
    drill reaches it".

    **`minable` means WORKABLE RIGHT NOW** (settled 2026-07-28), which is two
    conditions, not one: the node's tier is within the **grade of the drill
    this bot has installed**, *and* it has **ore remaining**. Grade alone is
    not enough — node existence is permanent map knowledge while amounts are
    live (docs/03), so a grade-only predicate keeps returning an Iron seam
    the colony emptied an hour ago, and the fleet walks to it and grinds
    itself down exactly as it would on an over-grade seam. That is the same
    failure the ruling exists to close, re-entered through depletion instead
    of tier. It is the **installed drill**, not the licence: a bot licensed
    for a grade-3 drill that has not bought one still cannot mine Silver. `scan_resources()` stays complete — it is the
    survey/planning list.
    **Doing this as a query rather than an entity attribute also closes a
    hole by construction**: M16's `workable` attribute read `world.nodes`
    directly and disclosed a node's grade for ground the faction had never
    scouted, whereas the query family is already scoped to `known_nodes`.
  - **Shipped programs handle the miss.** This is what covers failure (2),
    and Q108 already set the precedent that a shipped source must not
    crash-loop, because the first program a player reads must not teach a
    bug they will copy. **Q110 additionally requires binding once rather
    than check-then-act**, which rules out the `exists`-then-`closest`
    guard (the node can vanish between them) and makes `match` the
    consistent form:

    **AMENDED TWICE, 2026-07-28.** *(Second amendment, after the follow-up
    review.)* The `try_*`-eats-a-failed-query rule below was itself wrong:
    it named `Result.Err` but not `Option.None`, the language's *other*
    absence value, leaving `try_move_to(try_receive("orders"))` undefined —
    and since `try_move_to` is signal-safe, one reading makes that line a
    fault inside a running handler, i.e. a double-handle that wrecks the
    wounded bot it was meant to save. **The rule is deleted rather than
    extended.** `try_*` verbs take a **concrete target**; handing one a
    `Result` or an `Option` is an ordinary type fault. That also removes a
    hole the extension would have left, where `try_send(ch, None)` becomes a
    silent no-op and a legitimate message vanishes.

    What replaces it: **`if` / `elif` / `else` is granted at game start**
    instead of costing 20 Data, so the starter can guard its fallible
    queries:

    ```python
    if exists_minable(ore):
        move_to(closest_minable(ore).expect())
        try_mine()
    if exists(depot):
        move_to(closest(depot).expect())
        try_deposit()
    ```

    **The guard-then-query race is accepted deliberately** — the two calls
    are adjacent ops, so the window is a tick or two rather than the tens of
    ticks a blocking verb opens (which is what made Q110's Feral race a
    systematic bug), and it faults occasionally rather than every iteration,
    costing 2 HP that passive repair heals. Binding once would need
    Variables, and the starter is deliberately a Tier-0 program.

    *(First amendment, below, for the record.)* The `match` form first
    written here was
    impossible: `enum + match` is a **Tier-6** construct (70 Data) and the
    shipped starter is defined by docs/01 as **Tier-0 straight-line code**
    with no branching at all (`if` alone costs 20). A starter written in
    locked syntax cannot be edited or redeployed by the player who owns it.
    The fault-free family was always the Tier-0 answer — `try_deposit` and
    `try_withdraw` are *already* in the start kit — so the starter is:

    ```python
    try_move_to(closest_minable(ore))
    try_mine()
    try_move_to(closest(depot))
    try_deposit()
    ```

    — superseded by the second amendment above, which replaced the
    unwrap-inside-`try_*` rule with granting `if` at game start.
    `try_move_to` and `try_mine` do join the start kit, alongside the
    `try_*` verbs already there.

    GREEN/RED (`crates/game/src/editor/mod.rs`) and the Feral Harvester
    (`crates/sim/src/feral.rs`) both take this form, and docs/04's verbatim
    sources need re-syncing — they were already found stale against Q110.

  **`try_mine()` joins the `try_*` family** rather than being a one-off: a
  fault-free swing for the case where the node empties between arriving and
  mining. It lands with `try_move_to` and `try_attack` (backlogged from
  Q109/Q110) so the family grows in one coherent pass with one convention.

  A fault class ("a failing `mine()` does not chip HP") was considered and
  rejected. The resolving distinction is avoidable versus unavoidable, and
  every fault here is avoidable — the language already ships `match` and
  `on error:`, so Q109's punishment stays fair and needs no exceptions,
  which would only have raised "which other faults?".

  Implementation notes: the two new builtins need cycle costs in the cost
  table, and both sort `(distance, id)` like `closest` for determinism.

**Q118 — should the tool catalog be validated against Q72's ladder rule at
load? ANSWERED 2026-07-27: yes — but the rule is narrower than docs/03
states, and the catalog gets three assertions.** Checking the shipped
catalog first revealed that the *rule* was the problem, not the data:

  | Tool | Grade | Cost | Max material tier | Allowed (≤N−1) | |
  |---|---|---|---|---|---|
  | Mining | 2/3/4 | Steel · Bronze · Bronze+Gold | 1 / 2 / 3 | ≤1/2/3 | ✓ |
  | Building | 2/3 | Steel · Bronze | 1 / 2 | ≤1/2 | ✓ |
  | Combat | 2/3 | Bronze · Bronze+Gold | 2 / 3 | ≤1/2 | ✗ |
  | Optics | 2/3 | Lens+Bronze · Lens+Chips | 2 / 4 | ≤1/2 | ✗ |
  | Processor | 2/3/4 | Chips · Chips · Chips+GoldChip | 4 / 4 / 4 | ≤1/2/3 | ✗ |

  **Mining and Building comply; every other ladder violates, and Processor
  violates at every grade.** But the catalog is not sloppy — it follows the
  *resource-role* rules immaculately (Bronze arms, Chips think, the
  Sand→Glass→Lens seeing chain). The two stated rules are in **direct,
  arithmetic conflict**: Chips require Crystal, Crystal is mining tier 4, so
  under a literal ladder rule *no compute tool below grade 5 could ever be
  priced in Chips*. One of them had to give.

  The ladder rule's own justification — "no tier's key is ever locked behind
  its own door" — is about **circularity**, and circularity is only possible
  on a ladder that *unlocks materials*. A drill priced in what it unlocks is
  a deadlock; an Optics tool priced in Chips is merely late, because Optics
  unlocks nothing. Materials come from colony stock mined by miners, so a
  scout's tool never depended on that scout's own mining. The rule was
  always about drills and was simply written down too broadly.

  **The invariant, generalised so a future unlocking tool is covered
  without an amendment:** *no tool may be priced in a material that its own
  ladder unlocks at or above the grade being bought.* Refined goods resolve
  through their recipes — `mining_tier(resource)` is the max tier over the
  transitive raw inputs (Glass/Lens 0, Steel 1, Bronze/Wire 2, Chips and
  Gold Chip 4). Three load-time assertions, all cheap:
  **(1) anti-circularity** as above; **(2) no orphans** — every material
  named in a price is obtainable at all; **(3) no gaps** — every grade from
  2 to a tool's ceiling has an entry, so no level is dead.

  **Compute is NOT meant to sit behind maxed mining** (ruled here). Chips →
  Crystal → Mining 4 meant every compute purchase, *and with it
  `memory_bank` and `stack_ext` — program lines, variable slots and stack
  depth* — was gated on the entire mining ladder. In a game about
  programming, the size of program a player may write was the last thing
  unlocked. The fix keeps "Chips think" by letting the compute ladder
  **start cheap and escalate**: CPU 2 in **Wire** (Copper, tier 2, right
  after the first drill upgrade), 3 in Silver+Wire, 4 in Chips, 5 in Gold
  Chips — and program-capacity upgrades start on Wire for the same reason.

  **Every level licenses a purchase** (ruled here). This reads as a
  contradiction with Q121 and is not: Q121 says most levels grant no
  *automatic* perk, this says every level opens a *shopping option*. A level
  is a licence, and there is always something newly licensed. Assertion (3)
  enforces it. **Ceiling: GRADE 5**, grade 1 free with the
  chassis, 2–5 purchasable, and levels past 5 are pure score. **CORRECTED
  2026-07-28:** the original wording ("five rungs, matching the resource
  ladder 0–4") assumed grade N unlocks resource tier N−1, which would have
  left the free grade-1 drill reaching tier 0 only — both starting bots
  fault on their first `mine()` against an Iron/Coal start zone and the
  colony soft-locks at tick 0. docs/03's mapping is the correct one:
  **grade N works resource tier ≤ N**, so the free drill covers the start
  zone. That makes grade 4 the deepest *reach* (Crystal), so **the drill's
  grade 5 is a quality step** — more yield, faster swings — which keeps
  four purchasable grades per tool and leaves no level dead. The other nine
  tools never had a reach dimension, so all of their grades are quality
  steps anyway; the drill is the only tool where grades buy access too. That sizes the catalog at **10 tools × 4 grades ≈
  40 entries** against today's 12; every one needs a price obeying the
  invariant and the resource roles, which is a tuning pass.

  Doc-sync regardless: docs/03's ladder paragraph still says buying a tier
  "resets that capability's earned level", which Q111 deleted.

**Q119 — which tool purchases draw coolant? ANSWERED 2026-07-27: the
compute family only, and it becomes a property of the CATALOG ENTRY rather
than of a code branch.** docs/06 already ruled the principle — "module work
draws no coolant (mechanical, not thermal — coolant is for compute)" — and
`coolant_water_deci`'s own doc comment in `stats.rs` says the same. So the
answer was never in doubt; the *mechanism* was the bug. M16 attached the
charge to the **Compute branch of the purchase code**, and when
`UpgradeOrder::Tier` was added it inherited that branch, silently making
every mechanical tool cost Water.

  The ruling therefore has two halves. **Substance:** only the compute
  family pays coolant — the **CPU tool** and the flat program-capacity buys
  (memory bank, stack extension, log buffer), which are silicon and
  genuinely thermal. The mechanical tools pay none: drill, build tool,
  weapon, optics (a lens is glass), hull plating, drivetrain, cargo rack,
  gyros, signature dampener. **Mechanism:** coolant is **declared per
  catalog entry in data**, not inferred from which code path handles the
  purchase, so a future entry cannot acquire it by inheriting the wrong
  branch — which is exactly how this shipped.

  Consequence worth remembering: the failure was invisible. A colony with
  no Water reaching its Station could not buy Mining grade 2 — the gate on
  every Copper and Tin seam — and the order simply re-armed forever with no
  message, no charge and no log line. Re-arming is correct behaviour for a
  *temporary* shortfall, but a requirement the colony cannot satisfy at all
  is a soft-lock. The Station should surface what an order is waiting on
  (flagged for the HUD pass, not a design question).

**Q120 — what happens when a structure completes on an occupied tile? ANSWERED
2026-07-27: DISPLACE the occupant. AMENDED 2026-07-28: nothing dies.** A
pending designation is *not* solid and never becomes so: making blueprints
block pathing was rejected because it hands every player a free terrain
weapon — designate, never build, and wall off any ground you like. So bots
walk over designations, and the completing build **displaces** whatever
stands on the site.

  The destination is found by **breadth-first search outward from the site**
  over passable tiles, first free tile wins, ties on lowest `(x, y)`; free
  means in bounds, walkable, no structure/printer/depot/nest and no other
  bot, with paint filters excluded (they are per-call route preferences, not
  terrain). BFS rather than a chebyshev-radius sweep because the destination
  must be **somewhere the bot could have walked** — a radius scan can push a
  bot through a mountain into a sealed pocket. The displaced bot's path is
  invalidated (a `move_to` re-paths next tick, anything else fails its own
  range check); no fault, no HP chip, and no terrain trigger on landing,
  since a displacement is not a walk.

  **The amendment:** as first written, an occupant with no free *adjacent*
  tile was destroyed and dropped a black box, skipping the wreck stage. The
  doc-spec review found that broke three things at once — docs/02 states
  three separate times that "there is no instant-destruction path"; the
  black box's landing tile was undefined once the tile became a solid
  building, so the one death in the game whose forensics silently vanish;
  and it handed every faction a cross-faction kill primitive that had to be
  separately justified. **Widening the search from adjacent to the whole map
  removes all three**, because the case the death rule existed for cannot
  arise: a colony's fleet cap sits far below the map's tile count, so a legal
  state always has a free tile somewhere. Simpler rule, fewer interactions,
  and docs/02's invariant survives untouched.

  What the ruling forbids matters as much as what it allows: the build must
  **never hold** (a silent stall mints XP and progress for no output; one
  that faults grinds the builder to a wreck under Q109) and must **never
  delete the designation**, destroying the player's up-front materials. Both
  were tried during M16 and both were wrong.

**Q121 — what shape do per-level perks take now the ladder is uncapped?
ANSWERED 2026-07-27: TOOLS carry the power; LEVELS license, with sparse
MILESTONES and bounded CONTINUOUS perks.** The premise of the question was
that every number in the perk table was authored against a cap of 5 —
`+10% mine yield per level` and `+1 sensor per level` were chosen knowing
the maximum multiplier was 5×. Uncapping the ladder does not merely risk
runaway; it leaves the table with no authored intent past L5 at all. The
ruling has three parts:

  - **Tools carry the step changes.** Now that tools are bought and
    licensed by level, the tool holds the big numbers. This is the piece
    that makes an uncapped ladder harmless *by construction*: levels barely
    multiply anything, so there is nothing to run away.
  - **Qualitative growth arrives as sparse milestones** at named levels.
    The shape already exists and is proven — five perks are L3 thresholds
    today (Mining swing −25%, Building repair +25%, Hauling loaded speed,
    Combat hearing, Scouting corruption immunity). Most levels grant
    nothing; they are licence and score.
  - **Where a perk genuinely reads as continuous** — hull toughening with
    age, bearings wearing in, optics sharpening — it uses a **bounded
    hyperbolic**: `bonus = max_bonus × level / (level + K)`. Pure integer,
    deterministic, no floats: half of `max_bonus` at level K, 80% at 4K,
    asymptotic and never exceeding it.

  The hyperbolic settles the sharpest case without a special-case clamp.
  `+1 sensor/level` on a 34×20 map put a Scouting-L10 bot's vision at 15
  tiles — a 31×31 square, most of the board — and L15 saw everything, so
  **fog of war switched off at a reachable level**. Under an asymptotic
  bonus vision approaches a ceiling instead, and fog cannot be ground away.
  The self-saturating perks (Flinch −10%/lvl floors at L10, Mileage at L25,
  Hiding signature once below any hearing radius) already behaved and need
  no change beyond restatement in the new form.

  **Learning is retired entirely — both the perk and the track.** It was
  the lifetime-achievement track, defined as 10% of every other award; the
  mean-across-tracks total level now measures exactly that, derived from
  the same data without a stored copy. Deletes `XpTrack::Learning`,
  `learning_feed_pct`, `learning_gain_pct_per_level`, `learning_carry`, the
  `feeds` map and `settle_xp`'s entire second pass, the Learning term in
  `xp_gain_pct` (quirk `XpPct` effects — 10x Developer, Tech Debt — stay),
  and the Learning award site. Ten tracks remain.

  Per-perk magnitudes (`max_bonus`, `K`, which levels carry milestones) are
  tuning and deferred; the *shape* is what this ruling fixes.

**Q122 — what does energy upkeep scale on now? ANSWERED 2026-07-27: the
hyperbolic, and tools replace the module term.** Two problems, one answer.
The per-bot draw was `base + per_upgrade × upgrades + per_module ×
tier_value() + per_track_level × Σ levels`. `tier_value()` no longer exists,
so that term's basis becomes **installed tools** — and note M16 had already
silently changed it from a 3-slot cap to a 12-tier sum, quadrupling the
ceiling with no retune and no mention in any commit. The level term had a
worse problem: `Σ levels` was bounded at 5 × 12 = 60 and is now
**unbounded**, so an ancient fleet would brown out a colony purely by being
old. It takes **the same bounded hyperbolic Q121 gives the perks** —
`max × Σlevels / (Σlevels + K)` — so veteran upkeep approaches a ceiling
instead of growing without limit. Using one shape for both is deliberate:
the two questions are the same question (an uncapped ladder feeding a
linear term), and a single mechanism means a future reader has one thing to
understand rather than two. Magnitudes are tuning.

**Q123 — how are track incomes rebalanced so specialisation beats
seniority? ANSWERED 2026-07-27: per-track CURVE BASES, plus one income
change — Age drops to 0.2 deci/tick (2 centi).** Two corrections to how this
question was first written, because the original framing was half wrong:

  - It claimed the passive tracks out-earn the active ones so badly that
    the skill route to a tool licence was "dead on arrival". True in raw
    deci/tick, **false in levels**: levels go as √XP, and total level is the
    *mean over ten tracks* with several sitting at zero for any given bot.
    That dilution very nearly cancels the passive lead — worked forward,
    a pure miner at 50,000 ticks has Mining L3 against a total level of 3.
    Tied, not dead. The sharper true statement is that the skill route
    only *beats* the clock for tracks whose rate substantially exceeds
    Age's, which was **Combat and nothing else** — so the skill route
    worked for fighters and was denied to workers, backwards for a game
    about programming an economy.
  - **Specialisation dissolves most of the problem by itself**, which is
    why the "pay the loop, not the verb" option was rejected as
    unnecessary. The 1.4% mining duty cycle came from one bot doing the
    whole mine→walk→deposit→walk loop. A bot that parks at a vein and
    lets a hauler carry runs ~80% duty and earns ~8 deci/tick. The travel
    that ate everything now belongs to the hauler, whose travel *is* its
    job.

  **The ruling: each track carries its own `curve_base`**, so income rate
  and progression pace are tuned independently — an event's payout keeps
  its fiction while the ladder normalises the pace. The pacing intent is
  deliberately **two-tier**, because normalising everything to one pace
  would silently undo the Age slowdown:

  - **Job tracks** (Mining, Building, Scouting, Combat, Hauling): a
    dedicated specialist reaches **L5 in ~10 minutes** (6,000 ticks).
  - **Ambient tracks** (Age, Mileage, Processing, Hiding, Flinch): **L5 in
    ~50 minutes** (30,000 ticks) — these are seniority, not skill.

  That gap is what makes the specialist route beat the clock, which is the
  entire point of the question. Sanity check: a dedicated miner at ten
  minutes holds Mining **L5** against a **total level of 0** (Age L2,
  Processing L2, Mileage 0 because it never walks, the rest zero).

  First-pass values, in centi (1 whole XP = 100 centi):

  | Track | Income | Dedicated rate | `curve_base` | L5 at that rate |
  |---|---|---|---|---|
  | Mining | 100/unit | ~80 /tick | 32,000 | 10 min |
  | Building | 10/tick building | 10 /tick | 4,000 | 10 min |
  | Scouting | 500/node, 1,000/survey | ~20 /tick | 8,000 | 10 min |
  | Combat | 10/HP, 2,500/kill | ~10 /tick effective | 4,000 | 10 min |
  | Hauling | 10/unit-tile | ~1.4 /tick | 600 | 11 min |
  | Processing | 10/op | ~15 /tick | 30,000 | 50 min |
  | Mileage | 100/tile | ~7 /tick | 14,000 | 50 min |
  | Hiding | 2,500/episode | ~5 /tick | 10,000 | 50 min |
  | **Age** | **2/tick** | 2 /tick | 4,000 | 50 min |
  | Flinch | 1,000/flinch | ~1 /tick | 2,000 | 50 min |

  Cumulative cost to level N is `curve_base × N(N+1)/2`, so every value
  above is one substitution into **`curve_base = dedicated_rate ×
  target_ticks_to_L5 / 15`**. That formula and the two-tier intent are the
  durable part; the numbers are guesses and belong to the playtest bucket.

  **Three to watch.** *Combat* is a guess wearing a number — its in-fight
  rate is ~100 centi/tick and its duty cycle is whatever the match gives
  it, so "10 effective" is a placeholder, and the 2,500 kill bonus is 60%
  of a first level. *Hauling's 600* is the lowest base by far, so hauling
  levels are cheap for everyone, not just haulers — a dedicated hauler
  out-levels a part-timer by only ~1.8× (mining's margin is far wider),
  and if that feels wrong the honest fix is raising Hauling's income
  rather than cutting its base further, which would mean revisiting the
  B-only decision for that one track. *Processing at 30,000* assumes ~1.5
  ops/tick, which scales with cycles — so a better CPU levels Processing
  faster, which buys more cycles through its tool. Bounded by Q121's
  hyperbolic, but a loop worth watching.

---

*(Q124–Q126, answered 2026-08-02 — worksheet bodies moved from
QUESTIONS.md the same day:)*

**Q124 — can opponents see a color's *version counter* tick? OPEN (opened
2026-08-01, docs/08).** Swept in from an unnumbered note in
[08-multiplayer/code-visibility.md](../08-multiplayer/code-visibility.md). A
visible counter is decryption-free intel ("they redeployed Blue 30 seconds
after our salvage"). Lean **yes** — it rewards attention.

**Q125 — is structural whitespace always visible in masked views? OPEN (opened
2026-08-01, docs/08).** Same origin. Should line breaks and indentation be
exempt from the reveal mask at every decryption level? Lean **yes** —
silhouettes read as "shape of the program," which is good partial-intel
texture.

**Q126 — should programs be able to query foreign structures at all? OPEN
(opened 2026-08-02, docs/05 / docs/01).** P22's final form removed foreign
structures from the query domain entirely: the fog display shows
last-observed foreign structures to the *player*, but no builtin reaches
them from Pyrite. Opened when the `faction=` selector design was retired
(see PROBLEMS.md, P22's amendments). If a use case appears — raid targeting,
espionage programs — the surface must solve what the retired design did not:
a value domain that doesn't collide with kind constants, staleness semantics
(as-last-observed is remembered intel, not current state), and a hash story.
