# Known Problems Register

Defects found in the design docs that are **not open questions** — nobody is
undecided about them. Each is either a decision that was made and then
contradicted, or a number that does not survive arithmetic against the
constants it derives from. They live here until fixed, then move to the
**Fixed** log at the bottom with the commit that closed them.

Numbering is stable — **append new problems, never renumber**. Open design
questions still go in [QUESTIONS.md](QUESTIONS.md); this file is for text that
is already wrong.

Line references are as of the sweep that found them (2026-07-28, `git diff
@{upstream}...HEAD -- docs/` at commit `1f4ffb6`) and will drift as the docs
are edited — the quoted text is the reliable anchor.

**Re-anchored 2026-07-31** for the doc split: `01-language`, `02-agents`,
`03-resources` and `05-terrain` became doorway + directory, so every citation
into those four now names the *part file* that holds the quoted text. Line
numbers were dropped wherever the split invalidated them and kept only where
re-verified against the new file. No finding changed — this is a pointer fix.

**Re-anchored 2026-08-01** for the second doc split: `04-enemies`,
`06-progression`, `07-architecture`, `08-multiplayer` and `09-quirks` became
doorway + directory, so every citation into those five now names the part file
holding the quoted text, with line numbers re-verified against the new files.
No finding changed — pointer fix only. The same day's open-questions sweep
moved the answered worksheet bodies (Q111–Q123) out of `QUESTIONS.md` into
[history/questions-worksheets.md](history/questions-worksheets.md); citations
into those bodies now point there.

**Re-anchored 2026-08-02:** the fix rounds recorded above shifted their own
carriers after the 2026-08-01 pass — eleven Fixed-log line numbers had
drifted (five into `02-agents/xp-and-specialization.md`, two each into
`01-language/builtins.md` and `01-language/syntax-tiers.md`, one into
`07-architecture/world-state.md`, and `08-multiplayer/decided.md`'s Q86 line,
pushed down by the Q124/Q125 closes). Each number re-verified against the
current file. The quoted text remains the reliable anchor. *(A twelfth was
caught later the same day: P22's own citation into
`01-language/signals-and-logging.md`, shifted one line by the guard the P22
fix itself inserted — now :18.)*

**Status 2026-08-02 (latest): 30 opened, 28 fixed — P29 and P30 are open and
need rulings.** Four further high-effort audits followed the third, and their
findings were fixed in the same commit that found them rather than sitting
open as numbered entries: `8e17776` (6 — the last Q125 carrier in
07-architecture/ui-notes.md, eleven drifted Fixed-log anchors, two
false-premise tasks, the "(Q67 open)" marker, a dead intra-file link),
`31b5bb9` (5 — two doorway drifts in 07-architecture.md and 02-agents.md, the
Combat kill-bonus ratio, two stale TASKS.md markers), `2d4818c` (2 — the
02-agents doorway's pre-Q123 single-curve claim, P22's own off-by-one anchor),
`8aef987` (4 — TASKS.md state defects: a stale Q71 cross-reference, duplicated
status markers, an un-annotated Pump note, a verb-index milestone), and
`740539c` (10 — surviving Q111/Q115 carriers in 02-agents, two stale pointers
in 03-resources, and TASKS.md freshness). **The classes recur** — drifted
anchors appeared in three separate rounds — which is the argument for logging
them here rather than only in commit messages. The two findings that could not
be swept, because the docs do not contain the answer, are opened above as
**P29** (barricade query domain) and **P30** (Feral walks vs. P7).

**Status 2026-08-02 (earlier): still 28 opened, 28 fixed — a third audit corrected the
record, not the rulings.** The block below cites P22's close as `09c3e62`
(structure queries answer from faction knowledge); that close was twice
amended the next day — `6686866`, then `95c73c8` — to its final
**knowledge-pool** form (own colony state + granted allies', foreign
structures not query-reachable; see P22's amendment notes). No new problems
opened: the audit's ten findings were residual drift from already-recorded
closes, swept the same day — two stale carriers of P22's unguarded retreat
idiom (`05-terrain/tiles.md`, `05-terrain/map-generation.md`), the
code-visibility DECIDED paragraph never amended for Q124/Q125, a duplicated
clause in `01-language/types-and-env.md`, the Q109/Q110 rulings absent from
`history/questions-answered.md`, the stale `history/README.md` index
(Q123 → Q126), and two relative links broken by the verbatim moves into
`docs/history/`.

**Status 2026-08-01 (final): 28 opened, 28 fixed — the register is clear.**
A post-close xhigh audit (`c87ee66`) corrected ten closes: two spec gaps
(try_ pass assignment, the P22 ownership filter), two arithmetic errors
(P1's grade chain, P2's Hauling base), and stale carriers of P5, P7–P11,
P21, P22; amendments below record each. P1 — the bootstrap
deadlock — ruled and closed in `2c56fdf` (ruined Upgrade Station in the
start base). Earlier the same day the mechanical propagation batch — P8,
P12, P13, P15–P17, P19, P21, P23, P24, P26, P28 — closed in `93d6b25`.
The last six (P9–P11, P14, P18, P25) closed in `d5b561f`. P2
closed in `d90a428` (pacing table recomputed), P3 in `c1b26a7` (component
BFS + non-minting visible hold), P4 in `e913c27` (try_ covers the action,
never the argument), P22 in `09c3e62` (structure queries answer from
faction knowledge), P6+P20 in `3e21e89` (both linear perks converted to
the bounded hyperbolic), P7 in `84e1e68` (starter walks try_, tail
wanders), P5 in `9921848` (hyperbolic grouping/rounding/liveness are spec), P27 in `a2a81ff` (occupancy layer).

**Status 2026-08-01: 28 problems opened (P1–P28), 0 fixed.** P15–P18 were
found by the reviews of the 04–09 doc split; P19–P28 by the same day's
full-corpus consistency audit (post-split, commit `406c837`). Appended below —
P20/P22/P27 under *Needs a ruling*, the rest under *Mechanical*.

**Status 2026-07-28: 14 problems opened (P1–P14), 0 fixed.** All come from one
max-effort doc-coherence review of the Q111–Q123 sweep (76 agents, 84 candidates
verified, 8 refuted) — fifteen verified findings, two of which are the two
halves of P7. They collapse to two failure modes, and both are process problems
rather than authoring mistakes:

1. **A ruling landed in QUESTIONS.md plus one or two docs while the *owning*
   doc kept specifying the superseded mechanism.** Under this repo's own
   convention the owning doc's *Decided* section is the normative record an
   implementer builds from — so for P7 the stalest text in the repo sits in the
   most authoritative place.
2. **A new tuning number was never cross-checked against the constants it
   derives from.** P2 is the flagship: one substituted rate inverts the
   conclusion the ruling that introduced it was written to establish.

The three that change actual game behavior rather than doc clarity are **P1**
(the colony cannot bootstrap at all), **P2** (the XP two-tier is backwards) and
**P3** (a hash-affecting rule specified two incompatible ways).

---

## Needs a ruling

These cannot be swept mechanically — the docs do not contain the answer.

**P29 — the barricade query domain is specified two incompatible ways in one
Decided file.** [05-terrain/decided.md](05-terrain/decided.md) (the Q99
barricade bullet vs. the P22 structure-pool bullet five lines below);
[TASKS.md](TASKS.md) (*Decided-but-unbuilt*: "Barricade HP (Q99)" vs.
"Structure-pool query domain (P22)").

Q99 gives barricades a `barricade` kind constant expressly so an assault force
can find and shoot through a rival's wall, and says finding one "takes eyes" —
i.e. perception suffices. P22's final form removes foreign structures from the
query domain **entirely** — no perception path, no selector. A rival's wall is
therefore both findable (Q99) and unfindable (P22). Both readings are
hash-affecting, so two implementers ship divergent `closest(barricade)` domains
and desync; meanwhile the documented siege idiom faults every loop.
Needs one ruling: are enemy barricades (and any other attackable foreign
placement) a carve-out from P22's own-pool rule, or does breaching become a
pure adjacency/terrain interaction with no query surface?

**P30 — the shipped Feral walks keep the bare blocking `move_to` that P7 ruled
lethal, on a waiver that cites a different fault.**
[04-enemies/archetypes.md](04-enemies/archetypes.md) ("The Drone and Stinger
keep their faulting `move_to`/`attack` deliberately — the Q108
`move_to`-before-swing guard is their lesson").

P7 made the Tier-0 starter's walks `try_` because a no-path fault every loop,
at Q109's `fault_damage` 2 against 40 base HP, kills a bot in ~8 seconds. The
Feral waiver rests on Q108's guard, which addresses a *non-adjacent swing* —
a different fault entirely. `exists(enemy)` is true for any perceived enemy,
including one across water or behind a demolished bridge, so a Drone or
Stinger that sights an unreachable target self-destructs unattended and nests
near water depopulate themselves. Q108's own principle ("shipped sources must
not crash-loop") points the other way from the waiver built on it.
Needs one ruling: do the attacker archetypes take `try_move_to` (⚠HASH — Feral
program text is hashed into the program library), or is unreachable-target
self-destruction intended Feral behavior that the waiver should state
positively instead of deriving from Q108?

---

## Mechanical — decided text left behind

These need no ruling; the decision exists and the text was not propagated.

*(Empty as of 2026-08-01 — every entry fixed and moved to the Fixed log.)*

---

## Checked and cleared

Raised during the same review and **refuted** on verification — recorded so they
are not re-raised:

- **`closest_minable`/`exists_minable` leak live state through fog.** The
  predicate's scoping is consistent with docs/05's live-only remaining amounts.
- **`try_mine()` has no tie-break among in-range nodes.** Determinism is covered
  by the existing entity-ID rule.
- **Q120's "fails its own range check" implies a fault and an HP chip.** The
  same sentence disclaims it — "no fault, no HP chip."
- **Newly hash-affecting behavior in TASKS.md carries no ⚠HASH marker.**
  Markers are present where the convention requires them.
- **"Dense to grade 5" contradicts levels past 5 being pure score.** Both are
  true of different things.
- **The XP-core task mixes centi and deci units.** The deci figure is a stated
  conversion, not a storage claim.
- **The tool licence's "or its total level" branch is inert all session.** The
  floored mean does reach useful values within a match.

---

## Fixed

*(entries move here with the fixing commit's hash when they close)*

**P1 — the Upgrade Station is priced in a material only the Upgrade Station can
unlock. FIXED (`2c56fdf`).**
[03-resources/structures-and-start.md:32](03-resources/structures-and-start.md) (the Station's price),
[03-resources/decided.md:16](03-resources/decided.md) ("The bootstrap works"),
[03-resources/harvest-tiers.md](03-resources/harvest-tiers.md) (the drill ladder),
[06-progression/upgrade-station.md:34](06-progression/upgrade-station.md)

The Station costs **10 Steel, 5 Chips, 3 Wire**. Chips are 1 Silver + 2 Crystal
+ 1 Wire, and Crystal is resource tier 4. A fresh print carries the free grade-1
drill, which reaches tiers 0–1 (Wood/Stone/Sand/Iron/Coal) only. Grade 2 — the
sole route to Copper/Tin, hence Bronze, hence the Foundry (25 Steel + 10 Bronze)
that makes Chips in the first place — is purchasable **only at an Upgrade
Station**, and no doc grants a pre-built one. The colony cannot build the
structure that sells the upgrade it needs to build the structure. It is hard
capped at Iron/Coal forever and no tool of any grade is ever buyable, while
"The bootstrap works (Q72)" at `:227` asserts the opposite **on the same page**.

The old formulation survived this because tools also had a Fabricator path;
Q105 folded tool-making into the one pad flow and Q118 narrowed the ladder rule
to bind **on the drill alone**, so the rule as written no longer catches the
case where the *seller itself* is priced above the ladder it sells.

A fix must do one of: grant a pre-built Station in the starting kit, reprice the
Station below tier 2, unlock drill grade 2 off-Station, or re-widen the ladder
rule to bind on structures that sell tools. **Whichever is chosen, the ladder
rule at `:227` needs restating so it catches this class, not just this
instance.**

*(Resolution: a **ruined Upgrade Station** in the start base, repairable for
tier-0/1 materials — the Red-Fabricator pattern — plus the seller-side ladder
corollary in [harvest-tiers.md](03-resources/harvest-tiers.md).)*

*(Amended: the ruling text's bootstrap chain misstated Crystal as drill-grade-2
reachable (it is tier 4 — grade 4); corrected in all three carriers, `c87ee66`.)*

**P2 — Mining's `curve_base` is derived from a rate 8× the docs' own mine yield.
FIXED (`d90a428`).**
[02-agents/xp-and-specialization.md:84](02-agents/xp-and-specialization.md) (the pacing table),
[03-resources/the-tree.md](03-resources/the-tree.md) (mine yield),
[history/questions-answered.md](history/questions-answered.md) (Q122/Q123)

The Q123 pacing table reads `| Mining | ~80 /tick | 32,000 |`. But
[03-resources.md](03-resources.md)'s tuning manifest fixes mine yield at **2
units/swing**, [02-agents/xp-and-specialization.md](02-agents/xp-and-specialization.md) fixes one `mine()` swing at **~20
ticks**, and Mining income is 1 XP (100 centi) per unit. A bot swinging nonstop
earns **200 centi / 20 ticks = 10 centi/tick**, not ~80.
([history/questions-worksheets.md:510](history/questions-worksheets.md)
repeats the same unchecked assumption in prose.)

With `curve_base` 32,000, Mining L5 costs 15 × 32,000 = 480,000 centi = **48,000
ticks ≈ 80 minutes**, against the stated 10-minute job-track target and the
50-minute ambient target. So an idle bot that never mines reaches Age or
Processing L5 **before** a dedicated miner reaches Mining L5: seniority beats
specialisation, which is precisely the failure Q123 exists to fix. Knock-on:
drill grade 2 (Mining L2 ≈ 16 minutes of *uninterrupted* swinging, far longer
once hauling is counted) gates Copper/Tin well past
[06-progression/pacing.md:11](06-progression/pacing.md)'s 15–30 minute
midgame beat.

`curve_base = dedicated_rate × target_ticks_to_L5 / 15` is sound; the
substituted rate is not. **Every job-track row in the table is one substitution
of a rate that was never checked against `costs.ron`'s action times** — the
whole table needs recomputing, not just Mining's row.

*(Resolution: Mining recomputed to 10 centi/tick → `curve_base` 4,000; the
other four job rows verified against their inputs (Hauling and Building
derive; Scouting and Combat annotated as duty-cycle placeholders) and a
derivation paragraph added so the table is recomputed, never re-guessed.)*

*(Amended twice: 600 → 560 (`c87ee66`) still baked display rounding; the exact
derivation (10/7 centi/tick × 400) gives **571**, landed in `6686866`.)*

**P3 — Q120 both mandates and forbids the same silent hold. FIXED (`c1b26a7`). ⚠HASH**
[03-resources/decided.md:8](03-resources/decided.md) ("HOLDS — silently") and
[03-resources/decided.md:10](03-resources/decided.md) ("never hold"); also
[history/questions-answered.md](history/questions-answered.md) (Q120)

Within one *Decided* entry: line 219 says that when the displacement BFS
exhausts, the completing build **"HOLDS — silently"** (re-parks and retries next
tick, no progress, no XP, no fault); line 221 says the build must **"never
hold"**, and that holding "was tried during M16 and was wrong."

The two readings produce different sim behavior — an infinite silent stall
versus whatever the never-hold branch does (fault, delete, or force-complete) —
and every alternative is hash-affecting, so **two implementations of the same
spec desync in lockstep multiplayer.**

[history/questions-worksheets.md:417](history/questions-worksheets.md)–`:427` carries only the unconditional "never hold" version and argues
the case cannot arise ("a colony's fleet cap sits far below the map's tile
count, so a legal state always has a free tile somewhere"), while
03-resources/decided.md explicitly rejects that argument ("no tile count argument
covers it"). They also disagree on the **BFS domain** — whole map versus the
build site's passable connected component — which is what decides whether a bot
sealed in a pocket by Mountain/Water/barricades is reachable at all. Both the
exception's existence and the search domain need one answer.

*(Resolution: component-scoped BFS ratified; exhaustion holds, non-minting
and UI-visible — the one legal stall. The "never hold" bullet now forbids
minting/faulting stalls specifically. The history log keeps the superseded
whole-map wording as a closed record.)*

**P4 — `try_*` verbs type-faulting on `Result` re-creates the double-handle the
amendment was written to remove. FIXED (`e913c27`).**
[01-language/builtins.md](01-language/builtins.md) (the `try_*` rows and signal-safe flags),
[01-language/types-and-env.md](01-language/types-and-env.md) (`Result`)

[history/questions-worksheets.md:220](history/questions-worksheets.md)–`:230` deleted the old unwrap rule because it left
`try_move_to(try_receive("orders"))` undefined, and "one reading makes that line
a fault inside a running handler, i.e. a double-handle that wrecks the wounded
bot it was meant to save." The replacement makes that line an **always**-fault
instead of a sometimes-fault. Both operands are signal-safe
([01-language/builtins.md](01-language/builtins.md)), so the idiom is legal inside
`on hurt:` and the fault lands in the handler.

Worse, `closest` and `closest_minable` return `Result` (see [01-language/builtins.md](01-language/builtins.md)), so the
natural spelling of "the fault-free walk" — `try_move_to(closest(depot))`,
**verbatim the code [history/questions-worksheets.md:264](history/questions-worksheets.md) shipped one amendment earlier** — is a
runtime fault. Nothing specifies a deploy-time type check; deploy validates only
program memory and variable slots ([02-agents/decided.md](02-agents/decided.md)). A
hurt-handler retreat written the obvious way turns every hurt signal into an
abort, i.e. the rescue-denial path.

A fix must pick one: `try_*` accepts and propagates `Result`/`Option`, or the
type error is caught at deploy (which needs the deploy validator's scope
widened), or `try_*` loses its signal-safe status (which costs more than it
saves).

*(Resolution — ruled the other way: `try_` covers the action, never the
argument. try_* verbs take concrete arguments; Result/Option arguments are
ordinary type faults, resolved before the verb by guard-then-act or match.
The contract is now stated in builtins.md and types-and-env.md; the
composition idiom is defined by exclusion rather than absorbed.)*

**P5 — the bounded perk truncates to zero on integer stats. FIXED (`9921848`).**
[02-agents/xp-and-specialization.md:33](02-agents/xp-and-specialization.md) (the formula),
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) ("flat-only stats stay whole")

Q121's `bonus = max_bonus × level / (level + K)` is applied to sensor range and
max HP, which [02-agents/stat-sheet.md](02-agents/stat-sheet.md) keeps as whole integers
("Flat-only stats (HP, slots, sensor tiles) stay whole" — sensor range has no
`unit_scale`). With a plausible `max_bonus` of 3 tiles and K of 10, integer
division gives 3×1/11 = 0, 3×2/12 = 0, 3×3/13 = 0, 3×4/14 = 0 — **a bot that has
ground Scouting to level 4 sees exactly as far as a fresh print**, with no UI
signal that the perk exists. This contradicts the perk table's "sensor range
(bounded)" entry and `:167`'s "This is why every level still matters."

Two further gaps in the same formula: the doc promises "half of `max_bonus` at
level K" and 3×10/20 = 1, not 1.5, so odd `max_bonus` values silently lose their
claimed midpoint; and the **evaluation order is unstated** —
`max_bonus * (level / (level + K))` is 0 at every level forever, and nothing in
the spec rules that grouping out. A deterministic sim cannot leave that
ambiguous.

*(Resolution: grouping mandated — `(max_bonus × level) / (level + K)`, floor
division; bounds restated honestly (⌊max_bonus/2⌋ at K, strictly below
max_bonus forever); centi-unit progress display plus an xp.ron load assert
that every perk grants ≥ 1 unit by its track's L5.)*

*(Amended: the Hiding/Flinch stat-sheet rows still wrote the unparenthesized
grouping; swept in `c87ee66`.)*

**P6 — the Flinch perk saturates to zero, deleting the forced prologue
outright. FIXED (`3e21e89`).**
[02-agents/xp-and-specialization.md:66](02-agents/xp-and-specialization.md) (the Flinch row),
[09-quirks/acquired-quirks.md:8](09-quirks/acquired-quirks.md); Q121 in [history/questions-answered.md](history/questions-answered.md)

Q121 ratified Flinch's −10%/level as "self-saturating," but it saturates **at
zero**: "floors at L10" means a bot that has endured enough hostile flinches has
flinch duration 0. [02-agents/damage-faults-death.md](02-agents/damage-faults-death.md)'s "forced prologue on most
signals — time spent locked and vulnerable" then stops existing for veterans,
removing the vulnerability window the entire double-handle and rescue economy is
priced against. This is the one surviving linear perk Q121 declined to convert
to the bounded hyperbolic; converting it, or flooring it at a nonzero fraction,
are the two fixes.

*(Resolution: converted to Q121's bounded hyperbolic with `max_cut` below
100% — the prologue shortens, never vanishes. Ruled together with P20.)*

**P7 — the shipped Tier-0 starter faults to death on unreachable ore, and does
nothing at all when no ore is minable. FIXED (`84e1e68`).**
[01-language/syntax-tiers.md](01-language/syntax-tiers.md) (the shipped starter),
[01-language/builtins.md](01-language/builtins.md) (`move_to`'s no-path fault)

Two defects in one program, both introduced by Q117's rewrite:

  - **No reachability guard.** The starter guards drill grade and ore remaining,
    then unwraps into the **faulting** `move_to`. An Iron seam on the far bank
    of a river (water is impassable; sight is not blocked by it) makes
    `exists_minable(ore)` True, `closest_minable(ore)` return it, `.expect()`
    unwrap Ok, and `move_to` hit "the normal no-path fault" (see [01-language/builtins.md](01-language/builtins.md)). Nothing in
    the loop ever observes the node as unreachable, so the guard stays True and
    the program faults **every iteration** — 2 HP a fault, a 40 HP chassis dead
    in ~20, and every bot on the shipped program does it at the same seam
    simultaneously. That is Q117's own fleet-killer re-entered through
    unreachability instead of tier or depletion. `try_move_to` was added to the
    start kit in the same change as "the fault-free walk" and goes unused. (Note
    the interaction with **P4**: the obvious rewrite,
    `try_move_to(closest_minable(ore))`, is itself a fault until P4 is settled.)

  - **No fallback branch.** When `exists_minable(ore)` is False, both guards
    fail closed and the program does nothing — no fault, no error template, no
    thought cloud. Start-zone nodes are finite by design, so once a colony works
    out the ore its grade-1 drill can reach, every bot walks to the depot, gets
    False from `try_deposit()`, and loops **silently, forever**: a full fleet
    pacing between depot and nothing, paying upkeep against the fleet cap, with
    zero diagnostics. Q117 removed the fault that used to announce this
    condition without specifying a replacement signal. `wander` and `explore`
    are both already in the start kit — docs/04's Feral Harvester uses the
    identical guard followed by `wander()`.

*(Resolution: both walking legs became `try_move_to` (P4-legal composition)
and the starter gained the unconditional `wander()` tail — the Feral
Harvester's idiom. Unreachable ore is a False, not a fault-loop; an
out-of-ore fleet searches visibly instead of stalling silently.)*

*(Amended: the same audit ratified the try_ pass-assignment rule this fix
created the need for — a `try_` verb resolves in its sibling's pass, spec in
[07-architecture/tick-model.md](07-architecture/tick-model.md) (`c87ee66`).
Also: this resolution note was misfiled under P27's entry by the e125abc
over-match; returned here in `c87ee66`.)*

**P8 — `investment()` still sums deleted capability tiers. FIXED (`93d6b25`).**
[07-architecture/vm.md:13](07-architecture/vm.md),
[01-language/program-colors.md:47](01-language/program-colors.md) (the ghost-exemption bullet),
[02-agents/decided.md](02-agents/decided.md), [TASKS.md](TASKS.md)

Q115 cut the Backup Core and Q111 deleted `Capability` and the tier catalog.
[01-language/program-colors.md](01-language/program-colors.md) and [02-agents/xp-and-specialization.md](02-agents/xp-and-specialization.md) were
updated to "lifetime XP plus the value of installed tools" — but the scrap
valve's spec in **07-architecture.md** (the doc an implementer builds phase 8
from), the ghost-exemption bullet in **01-language.md** *three lines below the
corrected one*, and the **Decided entry that owns the ruling** in 02-agents.md
all still read "lifetime XP plus bought capability-tier value … so a Backup-Core
reprint's tier-4 hardware is never mistaken for a rookie."

An implementer following docs/07 has no `capability_tier` field to sum, so the
hardware term evaluates to nothing and `investment()` degenerates to raw
lifetime XP. On the first sustained Steel shortfall with `rust_scraps` on, the
valve ranks a bot carrying grade-5 drill, optics and CPU **below** a rookie
hauler with slightly more Mileage — it spent the match on a pad and in transit,
so its XP is lower — recalls it, and dismantles the colony's single largest
hardware investment for a partial refund. That is exactly the failure Q105-R3
was written to close. docs/01 additionally now gives two different formulas for
the same selection twelve lines apart.

*(Amended: the TASKS.md carrier (Q105-R3 entry) was never touched by the close;
restated [~] in `c87ee66`.)*

**P9 — docs/02's *Decided* section was never swept. FIXED (`d5b561f`).**
[02-agents/decided.md:14](02-agents/decided.md) (the `100×n` curve), `:12` (Q68 upkeep),
plus the module-slot and Optics entries in the same file

The owning doc's authority under this repo's conventions still ratifies the
whole pre-sweep model: the flat **`100×n` XP curve** (`:266`), **module slots
unlocking at total-XP milestones, cap 3** (`:262`), **Optics as a slotted tool
module** (`:259`), and Q68's upkeep as "per station upgrade, module, and track
level" with a Mk2→Mk3 catalog curve (`:265`).

A tuner writing `xp.ron` from this section ships one global `100×n` curve
instead of Q123's per-track `curve_base`. Every track then climbs at one pace,
the job/ambient two-tier gap disappears, a dedicated miner takes the same ~50
minutes to L5 as the Age clock does by merely existing, and specialisation stops
beating seniority for tool licensing — the entire outcome Q123 was decided to
produce. The same section also re-introduces the unbounded `Σ levels` upkeep
term Q122 replaced, so an old fleet browns out its colony purely by being old.

*(Resolution: the XP-curve entry restated per-track (Q123) and Q68's upkeep
clause converted to Q122's bounded hyperbolic with the tool-rebased term;
the module-slot and Optics monolith entries were already gone.)*

*(Amended: the resolution's 'already gone' claim was FALSE — the Optics-module
and slot-milestone clauses survived in decided.md's line tails (truncated-grep
verification, again); actually swept in `c87ee66`.)*

**P10 — the Feral Harvester's verbatim source is still the Q117 crash-loop.
FIXED (`d5b561f`).**
[04-enemies/archetypes.md:42](04-enemies/archetypes.md)–`:48` (also [TASKS.md](TASKS.md),
[06-progression/unlock-tree.md:71](06-progression/unlock-tree.md), `:76`,
[06-progression/pacing.md:10](06-progression/pacing.md))

[04-enemies/archetypes.md:5](04-enemies/archetypes.md) states these code blocks are the archetypes'
***actual* shipped source**, and Q117's answer
([history/questions-worksheets.md:273](history/questions-worksheets.md)–`:275`) explicitly records that
`crates/sim/src/feral.rs` takes the new guarded form and that "docs/04's
verbatim sources need re-syncing." The sweep updated only docs/04's nest-claim
gate (now [04-enemies/nests-and-claims.md:9](04-enemies/nests-and-claims.md)) and left the programs untouched.

The Harvester still guards with tier-blind `exists(ore)` — which per
03-resources.md Design Rule 4 answers from **permanent map knowledge**, so it
stays True on a seam the map emptied an hour ago — binds `closest(ore).expect()`
with no minable filter, and calls the **faulting** `mine()` rather than
`try_mine()`. That is the loop Q117 measured at [history/questions-worksheets.md:168](history/questions-worksheets.md)–`:174`: closest →
`move_to` (0 ticks at chebyshev ≤ 1) → `mine` → fault → restart, ~3–4 ticks per
iteration, 2 HP per fault, a 40 HP chassis dead in about eight seconds.

So every Harvester a nest prints grinds itself into a wreck within seconds of
reaching a worked-out or over-grade vein: the PvE *economic* enemy deletes
itself, docs/04's "starve the nest (kill Harvesters) and it prints less"
counterplay becomes unreachable, and **the first Feral program a player decrypts
teaches exactly the bug** [04-enemies/archetypes.md:23](04-enemies/archetypes.md) and Q108 say a shipped source must
never teach.

*(Resolution: the Harvester carries the ratified form — minable-scoped
queries, try_ verbs, bound target, wander tail; code re-sync tracked in the
Shipped-programs task.)*

*(Amended: archetypes' verbatim-source claim now marks the code re-sync as
pending rather than asserting byte-exactness the lagging feral.rs breaks;
`c87ee66`.)*

**P11 — module slots were deleted but four places still specify them. FIXED (`d5b561f`).**
[02-agents/anatomy.md](02-agents/anatomy.md) (`| Module slots | 1 |` — the
row itself, deleted by the fix, so the line number is dropped),
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) (the modifier pipeline),
[02-agents/damage-faults-death.md](02-agents/damage-faults-death.md) (the salvage receipt),
[02-agents/decided.md](02-agents/decided.md);
[03-resources/the-tree.md:94](03-resources/the-tree.md) (Lens); [07-architecture/world-state.md:6](07-architecture/world-state.md)

docs/06 deleted the entire slotted-module catalog (Optics and Backup Core
entries plus the swap-economics paragraph) and 02-agents/decided.md's entry
dropped "slots 1" from the print floor. Left behind: the universal base statline
still prints `| Module slots | 1 |` (`:24`); the modifier pipeline still runs
through "Upgrade Station purchases **+ slotted modules**" (`:32`); the salvage
receipt still derives from "slotted modules … swapped-out modules drop off — Q72
swap rules" (`:62`), citing a rules paragraph this sweep deleted; and `:262`
still rules slots unlock at total-XP milestones, cap 3.

Worst of the set is `:259` — "On a one-slot rookie, Optics is the whole build —
a dedicated prospector that gave up its ability to work" — which flatly
contradicts the sensor-range row the same sweep wrote at `:51`: "optics is one
of the ten tools since Q111 … so no rookie ever trades its working ability for
eyes." A reader cannot tell whether a bot has a slot, whether Optics consumes
it, or how salvage values a part that no longer exists.

Separately, [03-resources/the-tree.md:94](03-resources/the-tree.md) still routes the whole Lens
supply chain into "The **Optics module** (2 Lens + 1 Bronze)" — a deleted
catalog entry — leaving **Lens with no priced consumer anywhere in the design**.

*(Resolution: statline row, pipeline clause, and the salvage receipt's
slot/swap clause deleted (the receipt carrier was in stat-sheet.md, not
damage-faults-death.md as cited); Lens retargeted to the optics tool's upper
grades — a priced consumer via the ratified sensing chain, no ruling needed.)*

*(Amended: four more carriers survived the close — the pipeline's slot-order
tie-break, anatomy's identity and floor-statline clauses, the 02 doorway row,
and the Q72 receipt clause in 03-resources/decided and reprinting; swept in
`c87ee66`.)*

**P12 — two identical "Cycles per tick" rows with contradictory growth sources.
FIXED (`93d6b25`).**
[02-agents/stat-sheet.md:15](02-agents/stat-sheet.md) vs `:20` (also
`:66`; [03-resources/the-tree.md](03-resources/the-tree.md);
[06-progression/scopes.md:20](06-progression/scopes.md))

Line 40 says cycles per tick is grown by "**Upgrade Station** (walk there, pay
Chips)" — a flat buy — while line 45 says it is grown by the "**CPU tool**
(Upgrade Station), licensed by the **Processing track**." Q111 moved cycles off
flat buys onto the tool/licence model ([02-agents/anatomy.md](02-agents/anatomy.md): "Cycles
per tick is the CPU tool"), so line 40 states the superseded model.

Before this sweep the second row carried the suffix "— see the Processor
capability" in its Stat column, which marked it as the cross-reference rather
than a second canonical row; the edit deleted the marker, leaving **two
indistinguishable canonical rows for the single most contested stat in the
game**. An implementer building `stats.ron` from the sheet gets two conflicting
growth sources for one stat, and line 45 still closes in the deleted model's
language ("joins Q105's capability model — buy the tier, then sharpen it by
working").

**P13 — `repair()` gates the rescue verb on both the new grade and the deleted
Building tier. FIXED (`93d6b25`).**
[01-language/builtins.md:26](01-language/builtins.md) (the `repair()` row)

The builtin row was edited in place without deleting the old clause, so one cell
now reads: "field repair of a wreck needs **a build tool of grade ≥ 2** (Q105-R2,
restated for Q111); on a wreck = field repair (the rescue verb), which needs
**Building tier ≥ 2** (Q105-R2 — the replacement for the deleted build-tool
gate)." Q111 deleted capability tiers entirely
([history/questions-worksheets.md:22](history/questions-worksheets.md): "TIERS
ARE REMOVED"), so the trailing clause gates the rescue verb on a stat no bot has,
and its parenthetical asserts the opposite of the sentence in front of it.

This is the **sole surviving "Building tier" reference in docs/01–09** — the one
cell the mechanical propagation missed.

**P14 — the `XP gain` stat row was deleted, but two quirks still modify it.
FIXED (`d5b561f`).**
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) (the deleted row and the canonicity rule),
[02-agents/xp-and-specialization.md](02-agents/xp-and-specialization.md);
[09-quirks/catalog.md:19](09-quirks/catalog.md), `:39`;
[07-architecture/tick-model.md:29](07-architecture/tick-model.md);
[04-enemies/capturing-wrecks.md:5](04-enemies/capturing-wrecks.md); [TASKS.md](TASKS.md)

[02-agents/stat-sheet.md](02-agents/stat-sheet.md) declares the sheet canonical: "Anything anywhere
in the design that makes one bot better or worse than another — hardware, XP
perks, quirks … modifies a row on this sheet; **if an effect can't name its row,
it isn't a stat effect.**" The sweep deleted `| Survival | XP gain | 100% |
Learning track |` along with the Learning track — but 10x Developer (+15% XP
earned, all tracks), Tech Debt (−15% XP earned), [history/questions-worksheets.md:468](history/questions-worksheets.md) ("quirk
`XpPct` effects … stay"), docs/07's "any per-bot XP-gain multiplier (quirks
only) applies at its start-of-tick value" and 02-agents/xp-and-specialization.md itself all still
specify it.

An implementer building `stats.ron` from the canonical sheet ships no XP-gain
stat and the two quirks have nothing to apply to; the modifier-pipeline position
and the pessimistic-rounding rule for that multiplier are gone with the row.

*(Resolution: the row is restored as quirks-only — start-of-tick value,
pessimistic rounding — per the recorded 'XpPct effects stay' intent.)*

**P15 — the disconnect ruling's footnote points PvP disconnects at "open
questions", but they are decided two bullets down. FIXED (`93d6b25`).**
[08-multiplayer/decided.md:11](08-multiplayer/decided.md)

The colony-keeps-running ruling closes with "(Decided for co-op / non-harm
play; PvP disconnects need more thought — see open questions.)" — stale since
"PvP disconnects: free farm until reconnect" was ratified in the same Decided
section; there is no open question to see. The fix is a one-clause pointer
("see below"). Registered rather than silently reworded so the 04–09 doc
split stays a byte-exact move of decided text.

**P16 — the Drone's and Stinger's verbatim sources still check-then-act
across a blocking `move_to`, the pattern Q110 ruled out. FIXED (`93d6b25`).**
[04-enemies/archetypes.md:18](04-enemies/archetypes.md)–`:19` (Drone),
`:31`–`:32` (Stinger); the ruling inside Q117's answer
([history/questions-answered.md](history/questions-answered.md))

Q110's ruling — "bind once, never check-then-act", recorded inside Q117's
answer and cited by [01-language/syntax-tiers.md:42](01-language/syntax-tiers.md)
("the bug Q110 ruled against") — rules out re-querying a target around a
blocking verb, whose tens-of-ticks window "is what made Q110's Feral race a
systematic bug"
([history/questions-worksheets.md:247](history/questions-worksheets.md)–`:248`).
The ratified Drone and the ratified Stinger both do exactly that — the
byte-identical pair `move_to(closest(enemy).expect())` then
`attack(closest(enemy).expect())`. Same left-behind class as P10's Harvester.
Until the sources are re-synced, the first combat programs a player decrypts
teach the racing form Q108 says shipped source must never teach. (The doorway's Q110
open-question entry was retired with the split — the question is answered;
this register entry replaces it.)

**P17 — the "hardware is Chips-priced" shorthand survives in four places,
contradicting the ratified tool pricing it summarizes. FIXED (`93d6b25`).**
[06-progression/scopes.md:10](06-progression/scopes.md) (the per-match row)
and `:20` (the three-scopes list),
[06-progression/unlock-tree.md:67](06-progression/unlock-tree.md) (the axis
sentence), [02-agents/decided.md:11](02-agents/decided.md) (the compute-stats
ruling's "(Chips — …)" gloss); the pricing in
[06-progression/upgrade-station.md:30](06-progression/upgrade-station.md)–`:59`

All four lines gloss hardware buys as Chips-priced, but the owning part
prices tool grades by resource role — *Bronze arms, Chips think* — across
Steel, Bronze, Sand/Glass/Lens, Wire and Silver, with Chips entering only at
CPU grade 4, and deliberately starts every flat capacity buy on **Wire**
rather than Chips (upgrade-station.md: "These start on **Wire** rather than
Chips deliberately"). A reader taking the shorthand at face value concludes
Chips are the hardware currency and mis-plans the material gating of nine of
ten tools; the same shorthand in the 06 doorway intro was corrected in the
2026-08-01 sweep. The fix is a wording pass on the four lines (e.g.
"hardware (Upgrade Station)" or "hardware (materials by role)"), not a
pricing change — closing this entry requires re-grepping for the shorthand,
not just fixing the lines listed here.

**P18 — the hijack ruling still credits the deleted Boot XP track. FIXED (`d5b561f`).**
[04-enemies/capturing-wrecks.md:5](04-enemies/capturing-wrecks.md) ("counts
as a rescue boot for its Boot track");
[02-agents/decided.md:11](02-agents/decided.md) ("Boot and Learning were
later cut"),
[02-agents/xp-and-specialization.md:68](02-agents/xp-and-specialization.md)
(Boot "never once awarded"),
[07-architecture/tick-model.md:29](07-architecture/tick-model.md) (the
ten-track settle order)

Q111's sweep cut the Boot track from the XP model — the tick's XP settlement
runs exactly ten tracks and 02-agents records the cut — but the hijack
ruling moved into capturing-wrecks.md still awards the stolen bot's hijack
boot as "a rescue boot for its Boot track." An implementer building the
hijack path from docs/04 credits XP to a track that does not exist: either
the code grows an eleventh track (a hash-affecting divergence between
implementations — the desync class) or the clause is silently dropped with
no record. Same left-behind class as P14's Learning-track modifiers; the
clause needs a ruling-side sweep (drop the award, or re-home it on a
surviving track), not a silent reword.

*(Resolution: the rescue-boot award clause is dropped with its track.)*

**P19 — the Q77 Command inventory omits `ClaimNest` and `RazeNest`. FIXED (`93d6b25`).**
[07-architecture/world-state.md:32](07-architecture/world-state.md) ("the
ONLY external inputs to sim (Q77: list completed"),
[08-multiplayer/decided.md:17](08-multiplayer/decided.md) (Q86 names both);
[TASKS.md](TASKS.md)

The inventory declares itself complete, but Q86's authorization ruling
explicitly lists `ClaimNest` and `RazeNest` among the cross-faction commands
the relay binds to the sender's faction, and TASKS.md specifies their
effects ("RazeNest banks the Data bounty, ClaimNest converts it").
`ClaimNest` appears nowhere in docs/07. An implementer building the command
layer from the canonical inventory ships a sim in which nest conversion —
the gate on every printer/color past the second — has no input path; and
because Commands are the lockstep input stream, implementations that
disagree here also disagree on Q86's forgery-protection set.

**P20 — the Hiding perk is a second linear-uncapped perk, contradicting
Q121's own rule. FIXED (`3e21e89`).**
[02-agents/xp-and-specialization.md:65](02-agents/xp-and-specialization.md) and
[02-agents/stat-sheet.md:26](02-agents/stat-sheet.md) ("−1 signature/level,
tuning") vs
[02-agents/xp-and-specialization.md:15](02-agents/xp-and-specialization.md)
("none of them is linear-per-level")

Q121 converted perks to bounded shapes because the ladder is uncapped; P6
records Flinch as "the one surviving linear perk." Hiding is a second
survivor, registered nowhere: signature falls 1 per level, and heard-at
distance (their hearing radius + this signature) floors at 1 — so a Hiding
bot around level 6–7 against base hearing 7 is heard only at adjacency,
everywhere, permanently. That deletes the movement-noise detection layer
(Sentry early warning, creeping's trade, signature quirks) for veteran
infiltrators — the "switch fog of war off at a reachable level" failure Q121
names as the reason the rule exists. Same two fixes as P6: convert to the
bounded hyperbolic, or floor it at a nonzero signature.

*(Resolution: converted to Q121's bounded hyperbolic with `max_quiet` tuned
below base hearing — hearing detection never switches off. Ruled together
with P6, leaving zero linear perks.)*

**P21 — Q117's branching-at-start never propagated to three "`if` is an
unlock" passages. FIXED (`93d6b25`).**
[06-progression/unlock-tree.md:76](06-progression/unlock-tree.md) (Design
Rule 2: "The player wants `if` because they *felt* its absence") vs the same
file's START node (`:7` grants **if / elif / else** at game start);
[01-language.md:6](01-language.md) ("Construct gating — `if`, loops,
variables, `def` are *unlockable features*");
[00-overview.md:66](00-overview.md) (glossary Construct entry)

Q117 granted branching at game start (the guarded starter needs it). The
tree's START node was updated; the prose was not: the 01 doorway invariant
and the overview glossary still name `if` as the flagship unlockable, and
Design Rule 2 still sells the tree with the example the ruling deleted. A
data author pricing constructs from the doorway adds a research cost to
branching — no tree node exists for it — and a fresh account then cannot
load the shipped Tier-0 starter, which opens with `if exists_minable(ore):`.

*(Amended: three more carriers survived — scopes' construct row, unlock-tree's
reading note, archetypes' Stinger header; swept in `c87ee66`.)*

**P22 — the canonical hurt window faults whenever no Repair Bay is in range;
whether a faction's own structures are map knowledge is undecided. FIXED (`09c3e62`).**
[01-language/signals-and-logging.md:18](01-language/signals-and-logging.md)
(`move_to(closest(repair_bay).expect())`)

Resource nodes have a decided knowledge model (a seen tile is fully known;
queries answer from `known_nodes`); structures have none. If
`closest(repair_bay)` answers from perception, the canonical hurt handler
faults the moment a bot is hurt beyond sensor range of a bay — `.expect()`
on Err inside a running handler is the double-handle wreck path (P4's
class), shipped as the recommended idiom. If it answers from permanent
knowledge, no doc says so, and the two readings diverge — hash-affecting.
Needs one ruling: do a faction's own structures (or all discovered
structures) count as map knowledge for query builtins?

*(Resolution: queries answer from faction knowledge — own structures always,
foreign as last observed via a phase-5 known-structures memory. The canonical
hurt window gained its `exists` guard; ruling in 05-terrain/decided.md.)*

*(Amended thrice — final form: the third audit showed the `faction=` selector
design generating contradictions faster than patches closed them; the ruling
simplified to the **knowledge pool** (own colony state + granted allies',
current by construction, foreign structures not query-reachable — Q126 opened
for a future surface; no per-faction memory, no new hashed state) in
`95c73c8`. Earlier: the second audit scoped `faction=` to structure/designation
kinds only, bound the selector constants, brought blueprints into the ruled
class, and pooled the memory under the ally vision grant (`6686866`).
First: the ruling lacked an ownership filter (queries default `faction=own`,
foreign memory is opt-in — `c87ee66`) and had not propagated to fog-of-war.md,
the stat-sheet sensor row, or 01-language/decided.md; both fixed in `c87ee66`.)*

*(Third-audit addendum, 2026-08-02: two stale carriers of the unguarded
retreat idiom survived every sweep — `05-terrain/tiles.md`'s Crystal Field
cell and `05-terrain/map-generation.md`'s chokepoint idiom; both now carry
the `exists` guard.)*

**P23 — the execution model still grows compute through the deleted
"Processor capability (tier × level)". FIXED (`93d6b25`).**
[01-language/execution-model.md:29](01-language/execution-model.md)
("Compute grows instead through the **Processor capability** (tier ×
level — [02-agents.md](01-language/../02-agents.md))")

Q111 removed tiers and the capability model; cycles per tick is the CPU tool
(grades 1–5, licensed by the Processing track —
[02-agents/anatomy.md](02-agents/anatomy.md),
[06-progression/upgrade-station.md](06-progression/upgrade-station.md)). The
Q100 ruling's closing sentence — in the execution-model part an implementer
of the cycle economy reads first — still cites the deleted formula. Not
covered by P8 (the investment formula) or P12 (the stat-sheet rows).

**P24 — the 01-language doorway's parts table says "Tiers 0–6"; the part
defines Tiers 0–7. FIXED (`93d6b25`).**
[01-language.md:17](01-language.md) vs
[01-language/syntax-tiers.md:144](01-language/syntax-tiers.md) ("## Tier 7 —
Channels") and [01-language/builtins.md:41](01-language/builtins.md)
(`send` "Requires Tier 7")

The ownership table's tier count predates the channels tier. A gating or
renumbering change made against the doorway's 0–6 ladder drops or misplaces
the parse-time gate on `send`/`receive` — a deploy-validation divergence
between peers, and the doorway-drift failure the split convention exists to
catch.

**P25 — two quirks modify a "boot ritual" duration that names no stat-sheet
row. FIXED (`d5b561f`).**
[09-quirks/catalog.md:22](09-quirks/catalog.md) (**Hot Reload**: "boot
ritual half as long — [02-agents.md] stat sheet") and `:52` (**Windows
Update**: "boot ritual twice as long");
[02-agents/stat-sheet.md](02-agents/stat-sheet.md) (the canonicity rule)

The sheet's own rule is "if an effect can't name its row, it isn't a stat
effect." No boot-duration row exists (Print time and the hurt/Damaged lines
are different rows), and Hot Reload even cites the stat sheet as its home.
Same left-behind class as P14's XP-gain quirks, different stat: either the
sheet gains a boot-ritual-duration row (with modifier-pipeline position and
rounding rule) or the two quirks need re-speccing.

*(Resolution: a Boot-ritual row joins the sheet, quirks-only, so both
quirks name a row per the canonicity rule.)*

**P26 — the Scouting income row still asserts "no seen-tile set", which Q94
overturned. FIXED (`93d6b25`).**
[02-agents/xp-and-specialization.md:13](02-agents/xp-and-specialization.md)
("Q83 — sim events; no seen-tile set, so eyes-only fog stays stateless") vs
[05-terrain/decided.md:12](05-terrain/decided.md) ("Seen tiles are sim
state", answers Q94) and
[07-architecture/tick-model.md:28](07-architecture/tick-model.md) (the
phase-5 per-faction map writes)

Q94 made the per-faction known-tiles set hashed sim state; the Scouting
row's parenthetical still asserts the pre-Q94 stateless model. An
implementer deriving discovery events from an ad-hoc structure instead of
the phase-5 writes diverges on when "node discovered" fires — divergent
Scouting XP and Data awards are a replay-hash desync.

*(Amended: a second carrier — the Data-income clause "seen-set-free, like
Scouting" in [03-resources/decided.md:18](03-resources/decided.md) — was
missed by the first close and shut in `0060a47`.)*

**P27 — solid structures have no slot in the ratified tile-composition
model. FIXED (`a2a81ff`).**
[05-terrain/tile-composition.md:9](05-terrain/tile-composition.md) ("An
unwalkable building (exclusive)... the Barricade today — owns its tile
outright: it shares with *nothing*"); Q98's Pump in [TASKS.md](TASKS.md)
(both tiles solid, the intake *in* a Water tile)

The physical model is a strict either/or: exclusive unwalkable building, or
walkable ground stack. The Pump intake is a solid structure standing in
Water it must keep (it pumps it) — a share the shares-with-nothing class
forbids — and solid structures generally (Depot, printers, nests: the tiles
Q120's displacement BFS excludes) are assigned to neither class. Needs one
ruling on where structure solidity lives (tile-kind replacement like the
Barricade, or a contents slot the model currently omits); the answer decides
whether paint and overlays survive under a structure and what demolition
leaves behind.

*(Resolution: occupancy layer — solid structures are entities standing on
the ground stack, solidity from the structure registry, stack inert not
erased beneath; Barricade keeps its Q99 tile-kind exclusivity. Ratifies the
code's existing structure_at shape.)*

**P28 — the function-block scope row still gates some functions on a "tool
module". FIXED (`93d6b25`).**
[06-progression/scopes.md:19](06-progression/scopes.md) ("some also need a
tool module on the bot")

Q111 deleted the slotted-module catalog (P11 records the other survivors);
the per-bot gate on function blocks is tool *grade* (e.g. `hijack()` needs a
build tool of grade ≥ 2). The row sends readers hunting a module catalog
that no longer exists anywhere in the design. A one-clause fix, registered
rather than silently reworded because the text is ratified.
