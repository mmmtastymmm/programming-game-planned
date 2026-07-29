# Design-doc invariants

Properties the design corpus must have. Shared by the `design-ruling` skill
(which checks them against a change) and the `docs-coherence` skill (which
checks them against the whole corpus). Edit here, not in either skill.

Each invariant records what it caught, because every one of them was written
after something got through. `P<n>` refers to [docs/PROBLEMS.md](../docs/PROBLEMS.md).

---

## I1 — One canonical statement per fact

Exactly one place defines each fact; everywhere else cites it. Two canonical
statements diverge — not *may* diverge, do.

**Check:** for any fact stated twice, one of the two must be marked a
cross-reference. Watch for the marker being dropped by a later edit.

**Caught:** P12 — two rows both named exactly "Cycles per tick" with
contradictory growth sources, after an edit removed the "— see the Processor
capability" suffix that made one of them a pointer.

## I2 — No orphan terms, in either direction

Every term used is defined somewhere; every defined thing has at least one
consumer. Deletions break both directions at once.

**Check:** grep each retired term across `docs/` — hits are needed edits or
deliberate history in an Answered log, decided per hit. Then the reverse: for
each thing the docs define (a resource, a recipe output, a stat, a builtin), find
who consumes it.

**Caught:** P13 — `repair()` gates on "Building tier", the sole surviving
reference to a stat Q111 deleted. P11 — deleting the Optics module left **Lens
with no priced consumer anywhere in the design**, which no amount of reading the
Lens entry would reveal.

## I3 — Decided sections are normative and get a separate pass

Under this repo's conventions the owning doc's *Decided* section is what an
implementer builds from. It must agree with its own doc's body, and with every
other doc's Decided section. General reading passes skim it — it reads as
settled history, so the eye slides past.

**Check:** read every doc's Decided section as its own pass, not as part of
reading the doc.

**Caught:** P9 — after a sweep whose commit message claimed it hit "every owning
doc", docs/02's Decided section still ratified the flat `100×n` XP curve, module
slots and the superseded upkeep model. The stalest text in the repo sat in the
most authoritative place.

## I4 — Every derived number reconciles with its inputs

A constant presented as derived must be recomputable from constants fixed
elsewhere. Consistency across documents is not evidence — a wrong number
propagates consistently.

**Check:** name the inputs, cite where each is fixed, do the arithmetic, compare
against the target the doc claims. When a table shares a formula, **recompute
every row** — they all inherit the error.

**Caught:** P2 — Mining's `curve_base` came from a "~80 centi/tick" rate. Yield
is 2 units/swing at 100 centi/unit, a swing is ~20 ticks, so the real rate is 10.
Every doc agreed with every other; the number was still 8× off, and it inverted
the two-tier pacing conclusion the ruling existed to establish.

## I5 — The economy graph is traversable from the starting kit

Resources, recipes, tools, structures and their gates form a directed graph.
Every buildable thing must be reachable from what a colony starts with.
Unreachability is invisible locally — each node looks correctly priced.

**Check:** build the graph (tier ladder → raws → recipes → structures/tools →
what each gates) and traverse from the starting kit. Anything unreached is a
bootstrap break. Re-traverse whenever a price, a gate or a tier changes.

**Caught:** P1 — the Upgrade Station costs Chips, which bottom out at Crystal
(tier 4), reachable only with a drill grade sold exclusively at an Upgrade
Station. The colony cannot build the structure that sells the upgrade it needs to
build the structure, while the same page asserts "the bootstrap works."

## I6 — The stat sheet is closed in both directions

docs/02's sheet declares itself canonical: "if an effect can't name its row, it
isn't a stat effect." So every effect anywhere maps to a row, and every row has
at least one source that grows or modifies it.

**Check:** set-difference both ways — effects (hardware, perks, quirks, terrain,
state) against rows, and rows against their sources.

**Caught:** P14 — the `XP gain` row was deleted with the Learning track while two
quirks that modify it survived, leaving a live effect with no row on a sheet
whose stated rule forbids exactly that.

## I7 — Behavioral rules are single-valued

Any rule governing sim behavior must admit one reading. Two competent
implementers reading it must produce identical tick-by-tick behavior — this is a
lockstep-multiplayer game, so ambiguity is a desync, not a wording nit.

**Check:** for each behavioral rule, ask what an implementer does at every branch,
including the exhausted/empty/absent case. Hash-affecting rules carry `⚠HASH` in
TASKS.md. Verify CLAUDE.md's determinism rules still hold: no floats in
state-affecting paths, no hash-order iteration, no wall clock, sorted queries
with entity-ID tiebreaks.

**Caught:** P3 — one Decided bullet both mandates a silent hold and forbids
holding outright, four lines apart, with QUESTIONS.md arguing the case cannot
arise at all and the two texts disagreeing on the BFS search domain.

## I8 — Shipped programs are source code

docs/04's archetype programs are stated to be the *actual shipped source*, and
docs/01 carries the shipped starter. They are subject to the language reference,
not to prose review.

**Check:** execute them mentally against the current spec — the resource-absent
case, the unreachable case, the depleted case — and confirm the faulting vs
non-faulting verb is right in each. Re-run whenever syntax, builtins, fault
behavior or the start kit changes. Then check `crates/game`, which `.expect()`-
parses hardcoded programs at startup: an invalid one panics the game on launch
while every `sim` and `pyrite` test stays green.

**Caught:** P10 — the Feral Harvester is still the exact crash-loop Q117 was
written to delete, so the PvE economic enemy grinds itself to a wreck in eight
seconds. P7 — the shipped starter faults to death on unreachable ore and stalls
silently when nothing is minable.

## I9 — Citations resolve

A doc citing `Q118`, `docs/06-progression.md` or "Q105-R2" must match what is
actually there. Rulings get amended, and citations to the pre-amendment meaning
survive the amendment.

**Check:** resolve citations in changed regions; sample them elsewhere. Pay
attention to a citation whose *summary* of the cited ruling has drifted from what
the ruling now says.

**Caught:** P1's root cause — Q118 narrowed the ladder rule to bind on the drill
alone without recording that the narrowing stopped covering the case the wider
version caught.
