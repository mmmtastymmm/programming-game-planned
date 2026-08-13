# Open Questions Worksheet

All design questions across the design docs (00–09). As each is decided, its ruling moves to the owning doc's *Decided* section and its worksheet entry moves to [history/questions-worksheets.md](history/questions-worksheets.md) — answered entries don't linger here. Numbering is stable — append new questions, never renumber. Open questions live **only** in this file: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans.

This file is for things **not yet decided**. Text that is already wrong — a decision contradicted, or a number that fails arithmetic — is tracked in [PROBLEMS.md](PROBLEMS.md), numbered P1… on the same append-only rule.

**Status 2026-08-12 (latest): Q127 is still the only open question; the problem
register carries five open entries.** Everything through Q126 remains decided.
**P29–P33** are open in [PROBLEMS.md](PROBLEMS.md), and P29 closes as a
consequence of Q127.

Two corrections to the record, both about *when* things entered the board. **P32
and P33 were opened on 2026-08-03**, by that day's full-corpus consistency audit;
the 2026-08-02 block (now archived in the status log) had been edited in place to
say "P29–P33" rather than being superseded, which left the 08-03 board state
recorded nowhere and put the wrong date on it. Its original wording is restored
in the archive — a dated block is a point-in-time record, so the fix is a new
block, never a back-edit. Second, a **2026-08-12** corpus audit re-anchored
PROBLEMS.md and widened P33 from one catalog row to the class of six that share
its defect (see that file's re-anchor note). That audit's substantive findings are
being ruled one at a time. The first opened and closed **P34** (XP stored in
deci-units in the Q56 entry, a month after Q111 moved it to centi-points); the
second **cut the Lazy Evaluation quirk** for banking cycles while blocked
([09-quirks/decided.md](09-quirks/decided.md)), which closes one of P33's rows and
leaves five; and a comprehension question about the execution model turned up
**P35** (the blocking-burn rule never said whether a bank held from before the
block survives it — it does; only the grant burns). A fourth closed **P36** (the
kind-constant inventory was missing `blight` and `barricade` and misspelled
`chips`) and opened **P37** for the gap running the other way — three structures
that ship with no constant to query them. The register now reads
**37 opened, 31 fixed**. None of this touches the question board: Q127 is still
the only open question, and P36 was written so it does not pre-empt it — the
`barricade` constant is listed with its *domain* flagged open.

*Earlier status entries — the dated record of how the board got here — are in
[history/questions-status-log.md](history/questions-status-log.md). The
per-question ruling log is in
[history/questions-answered.md](history/questions-answered.md).*

---

## Open

**Q127 — does every building carry an allegiance, and may programs query
remembered foreign buildings? OPEN (opened 2026-08-02, docs/05 / docs/01 /
docs/02).** Reopens what Q126 closed, on a different shape, and subsumes the
ruling P29 needs.

*The state today.* `Structure` has carried a `faction` field since M4 and
`Blueprint` gained one on 2026-07-26 (set at placement, hashed). The
**Barricade** is the exception — its decided-but-unbuilt spec is
`Barricade { pos, hp }`, no faction — which is exactly why P29 could arise:
the one attackable placement whose ownership decides the ruling cannot
currently express ownership. Adding the field is free while it is unbuilt.

*The proposed shape (the starting position, not a ruling).* Every building
carries an allegiance; neutral field objects (Template Caches, Blight Cores)
are a separate class that has none. Buildings are then **perceived and
remembered like anything else** — the asymmetry to remove is that the fog
display already shows the player a scouted enemy depot while no program can
ask about it, against Q94's "knowledge is sim, appearance is view". Queries
stay **own-by-default** (`closest(depot)` means yours, so the starter's haul
leg and the canonical hurt retreat keep the guarantee P22 was opened to
win), and foreign lookup arrives as a **relationship-named query** rather
than a parameter — the Q117 precedent, where naming the query beat
parameterizing it. Relationship is a closed set fixed at design time (own /
ally / enemy) and the language already spells two of them as kind constants
for bots, so Q126's "value domain that must not collide with kind
constants" requirement does not arise rather than being worked around; the
four failures that retired the `faction=` selector were all properties of
the parameter and are likewise inapplicable.

*What it must still settle.* **(1) Staleness** — what a remembered foreign
building returns once it is destroyed or converted under fog: a handle that
faults on property reads, a position-only handle like a heard-only contact,
or observation-corrected knowledge on the `known_nodes` model. Nests change
hands via `ClaimNest`, so allegiance itself can go stale, not just
existence. **(2) The hash story** — barricades are cheap because a Barricade
is a `TileKind` riding the per-faction known-tiles set Q94 already hashed,
while every other building is an entity at a position, so remembering those
re-introduces the per-faction structure memory P22's final form deleted. A
narrower scope (barricades now, other buildings later) is available on that
asymmetry alone. **(3) Where a built barricade's allegiance comes from** —
presumably the Barricade blueprint's `faction`, which requires settling
whether the completion policy recorded in M8 ("any faction's builder can
finish them") survived the 2026-07-26 change that gave every blueprint a
faction field.

The **playtest-tuning** bucket also remains (numbers that need the prototype, not a choice, so they never block design): upkeep mix balance — does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? ([02-agents.md](02-agents.md)); Corruption spread/re-corruption rates, source radii, and cleanse speed ([05-terrain.md](05-terrain.md)), and — per the 2026-07-26 sweep — the first-pass figures shipped inside completed milestones: body-perk magnitudes (+ Age's deferred max-HP growth), quirk weights and the per-slot dial shape, upgrade-catalog times, upkeep.ron figures, guard/escort leash and cooldown, the Feral footprint metric and nest income, and the 14-ticks/tile pacing floor (with the boot/print-tick spec pass flagged in TASKS.md). Implementation-milestone work (e.g. the deferred PvP mapgen symmetry) is tracked in [TASKS.md](TASKS.md), not here.

---

## Answered

Every numbered question through Q126 is answered. The rulings live in
[history/questions-answered.md](history/questions-answered.md) — newest first,
append new rulings at the top of that file. The full worksheet bodies are
archived in
[history/questions-worksheets.md](history/questions-worksheets.md).
