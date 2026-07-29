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

**P1 — the Upgrade Station is priced in a material only the Upgrade Station can
unlock. OPEN.**
[03-resources.md:202](03-resources.md) (also `:176`, `:227`,
[06-progression.md:137](06-progression.md))

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

**P2 — Mining's `curve_base` is derived from a rate 8× the docs' own mine yield.
OPEN.**
[02-agents.md:203](02-agents.md) (also `:60`,
[QUESTIONS.md:416](QUESTIONS.md), `:832`, `:834`)

The Q123 pacing table reads `| Mining | ~80 /tick | 32,000 |`. But
[03-resources.md](03-resources.md)'s tuning manifest fixes mine yield at **2
units/swing**, [02-agents.md:54](02-agents.md) fixes one `mine()` swing at **~20
ticks**, and Mining income is 1 XP (100 centi) per unit. A bot swinging nonstop
earns **200 centi / 20 ticks = 10 centi/tick**, not ~80. (QUESTIONS.md:834
repeats the same unchecked assumption in prose.)

With `curve_base` 32,000, Mining L5 costs 15 × 32,000 = 480,000 centi = **48,000
ticks ≈ 80 minutes**, against the stated 10-minute job-track target and the
50-minute ambient target. So an idle bot that never mines reaches Age or
Processing L5 **before** a dedicated miner reaches Mining L5: seniority beats
specialisation, which is precisely the failure Q123 exists to fix. Knock-on:
drill grade 2 (Mining L2 ≈ 16 minutes of *uninterrupted* swinging, far longer
once hauling is counted) gates Copper/Tin well past
[06-progression.md](06-progression.md)'s 15–30 minute midgame beat.

`curve_base = dedicated_rate × target_ticks_to_L5 / 15` is sound; the
substituted rate is not. **Every job-track row in the table is one substitution
of a rate that was never checked against `costs.ron`'s action times** — the
whole table needs recomputing, not just Mining's row.

**P3 — Q120 both mandates and forbids the same silent hold. OPEN. ⚠HASH**
[03-resources.md:219](03-resources.md) and `:221` (also
[QUESTIONS.md:722](QUESTIONS.md))

Within one *Decided* entry: line 219 says that when the displacement BFS
exhausts, the completing build **"HOLDS — silently"** (re-parks and retries next
tick, no progress, no XP, no fault); line 221 says the build must **"never
hold"**, and that holding "was tried during M16 and was wrong."

The two readings produce different sim behavior — an infinite silent stall
versus whatever the never-hold branch does (fault, delete, or force-complete) —
and every alternative is hash-affecting, so **two implementations of the same
spec desync in lockstep multiplayer.**

QUESTIONS.md:722 carries only the unconditional "never hold" version and argues
the case cannot arise ("a colony's fleet cap sits far below the map's tile
count, so a legal state always has a free tile somewhere"), while
03-resources.md:219 explicitly rejects that argument ("no tile count argument
covers it"). They also disagree on the **BFS domain** — whole map versus the
build site's passable connected component — which is what decides whether a bot
sealed in a pocket by Mountain/Water/barricades is reachable at all. Both the
exception's existence and the search domain need one answer.

**P4 — `try_*` verbs type-faulting on `Result` re-creates the double-handle the
amendment was written to remove. OPEN.**
[01-language.md:436](01-language.md) (also `:474`)

QUESTIONS.md:131–141 deleted the old unwrap rule because it left
`try_move_to(try_receive("orders"))` undefined, and "one reading makes that line
a fault inside a running handler, i.e. a double-handle that wrecks the wounded
bot it was meant to save." The replacement makes that line an **always**-fault
instead of a sometimes-fault. Both operands are signal-safe
([01-language.md:474](01-language.md), `:506`), so the idiom is legal inside
`on hurt:` and the fault lands in the handler.

Worse, `closest` and `closest_minable` return `Result` (`:472`, `:494`), so the
natural spelling of "the fault-free walk" — `try_move_to(closest(depot))`,
**verbatim the code QUESTIONS.md:180 shipped one amendment earlier** — is a
runtime fault. Nothing specifies a deploy-time type check; deploy validates only
program memory and variable slots ([02-agents.md:261](02-agents.md)). A
hurt-handler retreat written the obvious way turns every hurt signal into an
abort, i.e. the rescue-denial path.

A fix must pick one: `try_*` accepts and propagates `Result`/`Option`, or the
type error is caught at deploy (which needs the deploy validator's scope
widened), or `try_*` loses its signal-safe status (which costs more than it
saves).

**P5 — the bounded perk truncates to zero on integer stats. OPEN.**
[02-agents.md:161](02-agents.md)

Q121's `bonus = max_bonus × level / (level + K)` is applied to sensor range and
max HP, which [02-agents.md:82](02-agents.md) keeps as whole integers
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

**P6 — the Flinch perk saturates to zero, deleting the forced prologue
outright. OPEN.**
[QUESTIONS.md:756](QUESTIONS.md) (also [02-agents.md:144](02-agents.md),
[09-quirks.md:99](09-quirks.md))

Q121 ratified Flinch's −10%/level as "self-saturating," but it saturates **at
zero**: "floors at L10" means a bot that has endured enough hostile flinches has
flinch duration 0. [02-agents.md:59](02-agents.md)'s "forced prologue on most
signals — time spent locked and vulnerable" then stops existing for veterans,
removing the vulnerability window the entire double-handle and rescue economy is
priced against. This is the one surviving linear perk Q121 declined to convert
to the bounded hyperbolic; converting it, or flooring it at a nonzero fraction,
are the two fixes.

**P7 — the shipped Tier-0 starter faults to death on unreachable ore, and does
nothing at all when no ore is minable. OPEN.**
[01-language.md:203](01-language.md)–`:204`

Two defects in one program, both introduced by Q117's rewrite:

  - **No reachability guard.** The starter guards drill grade and ore remaining,
    then unwraps into the **faulting** `move_to`. An Iron seam on the far bank
    of a river (water is impassable; sight is not blocked by it) makes
    `exists_minable(ore)` True, `closest_minable(ore)` return it, `.expect()`
    unwrap Ok, and `move_to` hit "the normal no-path fault" (`:471`). Nothing in
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

---

## Mechanical — decided text left behind

These need no ruling; the decision exists and the text was not propagated.

**P8 — `investment()` still sums deleted capability tiers. OPEN.**
[07-architecture.md:77](07-architecture.md),
[01-language.md:393](01-language.md),
[02-agents.md:267](02-agents.md), [TASKS.md:1152](TASKS.md)

Q115 cut the Backup Core and Q111 deleted `Capability` and the tier catalog.
[01-language.md:381](01-language.md) and [02-agents.md:130](02-agents.md) were
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

**P9 — docs/02's *Decided* section was never swept. OPEN.**
[02-agents.md:259](02-agents.md), `:262`, `:265`, `:266`

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

**P10 — the Feral Harvester's verbatim source is still the Q117 crash-loop.
OPEN.**
[04-enemies.md:58](04-enemies.md)–`:64` (also [TASKS.md:1083](TASKS.md),
[06-progression.md:97](06-progression.md), `:102`, `:171`)

[04-enemies.md:35](04-enemies.md) states these code blocks are the archetypes'
***actual* shipped source**, and Q117's answer
([QUESTIONS.md:571](QUESTIONS.md)–573) explicitly records that
`crates/sim/src/feral.rs` takes the new guarded form and that "docs/04's
verbatim sources need re-syncing." The sweep updated only docs/04's nest-claim
gate at `:90` and left the programs untouched.

The Harvester still guards with tier-blind `exists(ore)` — which per
03-resources.md Design Rule 4 answers from **permanent map knowledge**, so it
stays True on a seam the map emptied an hour ago — binds `closest(ore).expect()`
with no minable filter, and calls the **faulting** `mine()` rather than
`try_mine()`. That is the loop Q117 measured at QUESTIONS.md:466–471: closest →
`move_to` (0 ticks at chebyshev ≤ 1) → `mine` → fault → restart, ~3–4 ticks per
iteration, 2 HP per fault, a 40 HP chassis dead in about eight seconds.

So every Harvester a nest prints grinds itself into a wreck within seconds of
reaching a worked-out or over-grade vein: the PvE *economic* enemy deletes
itself, docs/04's "starve the nest (kill Harvesters) and it prints less"
counterplay becomes unreachable, and **the first Feral program a player decrypts
teaches exactly the bug** 04-enemies.md:41 and Q108 say a shipped source must
never teach.

**P11 — module slots were deleted but four places still specify them. OPEN.**
[02-agents.md:24](02-agents.md), `:32`, `:62`, `:262`;
[03-resources.md:94](03-resources.md); [07-architecture.md:90](07-architecture.md)

docs/06 deleted the entire slotted-module catalog (Optics and Backup Core
entries plus the swap-economics paragraph) and 02-agents.md:257's Decided entry
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

Separately, [03-resources.md:94](03-resources.md) still routes the whole Lens
supply chain into "The **Optics module** (2 Lens + 1 Bronze)" — a deleted
catalog entry — leaving **Lens with no priced consumer anywhere in the design**.

**P12 — two identical "Cycles per tick" rows with contradictory growth sources.
OPEN.**
[02-agents.md:40](02-agents.md) vs `:45` (also `:36`, `:46`, `:55`, `:64`,
`:66`; [03-resources.md:92](03-resources.md);
[06-progression.md:18](06-progression.md))

Line 40 says cycles per tick is grown by "**Upgrade Station** (walk there, pay
Chips)" — a flat buy — while line 45 says it is grown by the "**CPU tool**
(Upgrade Station), licensed by the **Processing track**." Q111 moved cycles off
flat buys onto the tool/licence model ([02-agents.md:12](02-agents.md): "Cycles
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
Building tier. OPEN.**
[01-language.md:486](01-language.md) (also `:35`, `:430`)

The builtin row was edited in place without deleting the old clause, so one cell
now reads: "field repair of a wreck needs **a build tool of grade ≥ 2** (Q105-R2,
restated for Q111); on a wreck = field repair (the rescue verb), which needs
**Building tier ≥ 2** (Q105-R2 — the replacement for the deleted build-tool
gate)." Q111 deleted capability tiers entirely (QUESTIONS.md:432: "TIERS ARE
REMOVED"), so the trailing clause gates the rescue verb on a stat no bot has,
and its parenthetical asserts the opposite of the sentence in front of it.

This is the **sole surviving "Building tier" reference in docs/01–09** — the one
cell the mechanical propagation missed.

**P14 — the `XP gain` stat row was deleted, but two quirks still modify it.
OPEN.**
[02-agents.md:59](02-agents.md), `:61`, `:187`;
[09-quirks.md:30](09-quirks.md), `:50`;
[07-architecture.md:60](07-architecture.md);
[04-enemies.md:134](04-enemies.md); [TASKS.md:1155](TASKS.md)

[02-agents.md:30](02-agents.md) declares the sheet canonical: "Anything anywhere
in the design that makes one bot better or worse than another — hardware, XP
perks, quirks … modifies a row on this sheet; **if an effect can't name its row,
it isn't a stat effect.**" The sweep deleted `| Survival | XP gain | 100% |
Learning track |` along with the Learning track — but 10x Developer (+15% XP
earned, all tracks), Tech Debt (−15% XP earned), QUESTIONS.md:766 ("quirk
`XpPct` effects … stay"), docs/07's "any per-bot XP-gain multiplier (quirks
only) applies at its start-of-tick value" and 02-agents.md:187 itself all still
specify it.

An implementer building `stats.ron` from the canonical sheet ships no XP-gain
stat and the two quirks have nothing to apply to; the modifier-pipeline position
and the pessimistic-rounding rule for that multiplier are gone with the row.

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

*(none yet — move entries here with the fixing commit's hash when they close)*
