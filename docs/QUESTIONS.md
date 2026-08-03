# Open Questions Worksheet

All design questions across the design docs (00–09). As each is decided, its ruling moves to the owning doc's *Decided* section and its worksheet entry moves to [history/questions-worksheets.md](history/questions-worksheets.md) — answered entries don't linger here. Numbering is stable — append new questions, never renumber. Open questions live **only** in this file: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans.

This file is for things **not yet decided**. Text that is already wrong — a decision contradicted, or a number that fails arithmetic — is tracked in [PROBLEMS.md](PROBLEMS.md), numbered P1… on the same append-only rule.

**Status 2026-08-02 (latest): Q127 OPENED — building allegiance and the
remembered-building query surface.** Q126 closed the foreign-structure
surface for v1 and recorded what any future one must solve; a use case
arrived immediately — P29's barricade contradiction — and with it a simpler
shape than the design Q126 retired, so the question is reopened on its own
terms as **Q127**. Everything through Q126 remains decided; **P29–P33**
are open in [PROBLEMS.md](PROBLEMS.md), and P29 closes as a consequence of
Q127.

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
