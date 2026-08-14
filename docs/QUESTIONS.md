# Open Questions Worksheet

All design questions across the design docs (00–09). As each is decided, its ruling moves to the owning doc's *Decided* section and its worksheet entry moves to [history/questions-worksheets.md](history/questions-worksheets.md) — answered entries don't linger here. Numbering is stable — append new questions, never renumber. Open questions live **only** in this file: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans.

This file is for things **not yet decided**. Text that is already wrong — a decision contradicted, or a number that fails arithmetic — is tracked in [PROBLEMS.md](PROBLEMS.md), numbered P1… on the same append-only rule.

**Status 2026-08-14 (latest): two questions are open — Q127 and Q128; the
problem register carries seven open entries.** Everything through Q126 remains
decided. **P29–P33**, **P37** and **P38** are open in
[PROBLEMS.md](PROBLEMS.md); P29 closes as a consequence of Q127, and P38 as a
consequence of Q128.

**Q128 opened 2026-08-14**, out of a review of the depot access path. Q89 ruled
that a Depot's `faction` field governs perception and stopped there, while the
sim enforces an access rule — any bot may deposit at or withdraw from any
depot, whoever owns it — that no design doc states; that half is **P38**. The
question generalises past the depot to what relationship *any* building
interaction requires, and takes the position that the relationship is fixed per
verb rather than passed by the caller. It is scoped so it does not pre-empt
Q127: Q127 owns the query domain, Q128 the access domain, and what the two must
share is their treatment of allies.

One correction to the record. The 2026-08-12 block headlined "five open
entries" while its own closing sentence put the register at 37 opened / 31
fixed — six — because **P37 was added to that block by back-edit after the
headline was written**, the fourth in-place amendment it took that day. This is
its replacement, not a fifth: the block is archived unchanged in the status
log, per the rule that block itself restated — a dated block is a point-in-time
record, so the fix is a new block, never a back-edit.

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

**Q128 — what relationship does a building interaction require, and who names
it: the caller or the verb? OPEN (opened 2026-08-14, docs/03 / docs/08 /
docs/01).** Q89's neighbour on the same field — it ruled what a Depot's
`faction` governs and stopped at perception. Scoped so it does not pre-empt
Q127: that question owns the **query** domain (what a program may find), this
one owns the **access** domain (what a bot already standing next to a building
may do).

*The state today.* Q89 gave Depots a real `faction` field and ruled it governs
perception — the Depot **sees/hears for its owner**, in "One rule across the
sim's perception, reachability checks, and the fog view"
([03-resources/decided.md](03-resources/decided.md), the Q89 depot bullet).
Access was never ruled, and the sim answers it anyway: both halves of the haul
loop accept **any** adjacent depot whatever its faction, then settle against
the *caller's* colony — `deposit()` credits `stock_add(faction, …)` and
`withdraw()` draws `stock_get(faction, …)`. So a Depot is a public terminal
into your own abstract stock. Nothing is stolen from its owner, but a rival's
depot is a free forward base. The structure arms of the same two verbs *do*
filter `st.faction == faction`, so production is private while drop-off is
public, and no doc states either half — the Depot's catalog row reads "Cargo
drop-off, storage."
([03-resources/structures-and-start.md:24](03-resources/structures-and-start.md)).
The undocumented-rule half is **P38**.

*The proposed shape (the starting position, not a ruling).* Every building
interaction requires a **relationship** to proceed, drawn from the closed set
Q127 already fixes at design time (own / ally / enemy), with neutral field
objects the separate no-allegiance class Q127 names. `World::allied` already
supplies the predicate and already counts a faction as its own ally, so "own"
needs no separate spelling.

The relationship is **fixed per verb, not a caller argument.** An access verb
acts on the one building the bot is standing next to, so there are no
candidates for a selector to narrow: the question is permission, not selection.
That lands on the same side as the parameter rulings already on the board —
Q126 retired the `faction=` selector on four failures Q127 records as "all
properties of the parameter", and Q127 prefers a relationship-named query on
the Q117 precedent — but by a different route, and it carries a consequence
worth stating: the access half needs no language surface at all, so Q126's
"value domain that must not collide with kind constants" requirement never
arises here.

Defaults split on **direction**, because giving and taking are not symmetric.
`deposit()` to a depot or to a structure feed is **allied** — a gift, and
`accepted_feed()` already bounds it to recipe inputs. A depot `withdraw()` is
**allied**, since it draws the caller's own stock either way and the
relationship governs only where a bot may stand. A structure-output
`withdraw()` stays **own-only**: that one takes produce.

*What it must still settle.* **(1) Whether allies may take, and by what
mechanism.** M13 already has the opt-in shape — `granted(from, to, GrantKind)`
— but `GrantKind` is `{ Vision, Channels }`, so letting an ally pull refinery
output needs a third variant rather than riding the alliance alone. Own-only
is the other live answer; the Request Box already owns the aid story.
**(2) Whether access and query must agree.** If Q127 rules queries
own-by-default while access is allied, an ally's depot is usable but
unfindable, and a program reaches one only by remembering a position. Either
Q127's answer binds this one or the two defaults diverge deliberately, and
that has to be a choice rather than an accident of ruling order.
**(3) Cost.** The change is ⚠HASH, and it needs a check that no shipped map
puts a foreign depot inside the opening's reach — the starter haul leg is the
guarantee P22 was opened to protect. Ferals ride along free: `FERAL_FACTION`
allies with nobody, and the nest arm of `deposit()` is already feral-only.

The **playtest-tuning** bucket also remains (numbers that need the prototype, not a choice, so they never block design): upkeep mix balance — does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? ([02-agents.md](02-agents.md)); Corruption spread/re-corruption rates, source radii, and cleanse speed ([05-terrain.md](05-terrain.md)), and — per the 2026-07-26 sweep — the first-pass figures shipped inside completed milestones: body-perk magnitudes (+ Age's deferred max-HP growth), quirk weights and the per-slot dial shape, upgrade-catalog times, upkeep.ron figures, guard/escort leash and cooldown, the Feral footprint metric and nest income, and the 14-ticks/tile pacing floor (with the boot/print-tick spec pass flagged in TASKS.md). Implementation-milestone work (e.g. the deferred PvP mapgen symmetry) is tracked in [TASKS.md](TASKS.md), not here.

---

## Answered

Every numbered question through Q126 is answered. The rulings live in
[history/questions-answered.md](history/questions-answered.md) — newest first,
append new rulings at the top of that file. The full worksheet bodies are
archived in
[history/questions-worksheets.md](history/questions-worksheets.md).
