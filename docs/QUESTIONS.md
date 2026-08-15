# Open Questions Worksheet

All design questions across the design docs (00–09). As each is decided, its ruling moves to the owning doc's *Decided* section and its worksheet entry moves to [history/questions-worksheets.md](history/questions-worksheets.md) — answered entries don't linger here. Numbering is stable — append new questions, never renumber. Open questions live **only** in this file: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans.

This file is for things **not yet decided**. Text that is already wrong — a decision contradicted, or a number that fails arithmetic — is tracked in [PROBLEMS.md](PROBLEMS.md), numbered P1… on the same append-only rule.

**Dated status blocks are point-in-time records — supersede, never back-edit.**
When the board changes, write a new block and move the displaced one, unchanged,
to the top of
[history/questions-status-log.md](history/questions-status-log.md). Never reopen
a block to add a ruling or correct a count: the 2026-08-12 block was amended four
times in one day, and its "five open entries" headline outlived the body that put
the register at six. The rule used to live *inside* the status blocks, where
archiving them carried it away — hence its being restated here, in text that
stays. `CLAUDE.md` and [PROBLEMS.md](PROBLEMS.md) carry the same rule.

**Status 2026-08-15 (latest): three questions are open — Q127, Q128 and Q129;
the problem register carries seven open entries.** Everything through Q126
remains decided. **P29–P33**, **P37** and **P38** are open in
[PROBLEMS.md](PROBLEMS.md); P29 closes as a consequence of Q127 and P38 as a
consequence of Q128.

**Q129 opened 2026-08-15**, promoted out of a rider on P37. `find_kind`'s `enemy`
arm filters on faction alone, so `closest(enemy)` returns a **declared ally**, and
`World::allied` is consulted nowhere in `host.rs` — no Pyrite query respects an
alliance at all. Q91 ruled the harm side (auto-fire spares allies; explicit
`attack()` stays legal because betrayal is legal play) and said nothing about
what a query hands you, which is where the accidental friendly fire Q91 meant to
prevent still happens — through the combat program nobody revised after allying.
It was filed as a task rider first and promoted because it is undecided rather
than merely unbuilt, and because it is **coupled to P37**: if `enemy` stops
returning allies while `ally` stays unbuilt, betrayal becomes unwritable and Q91
is repealed by omission.

The three open questions divide cleanly and were written not to pre-empt each
other: **Q127** owns the query *domain* (what a program may find), **Q128** the
access *domain* (what a bot beside a building may do), **Q129** the *relationship
predicates* both of them spell as own / ally / enemy.

*Earlier status entries — the dated record of how the board got here — are in
[history/questions-status-log.md](history/questions-status-log.md). The
per-question ruling log is in
[history/questions-answered.md](history/questions-answered.md).*

---

## Open

**Q127 — does every building carry an owning faction, and may programs query
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
carries an owning faction; neutral field objects (Template Caches, Blight
Cores) are a separate class that has none. Buildings are then **perceived and
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
hands via `ClaimNest`, so ownership itself can go stale, not just
existence. **(2) The hash story** — barricades are cheap because a Barricade
is a `TileKind` riding the per-faction known-tiles set Q94 already hashed,
while every other building is an entity at a position, so remembering those
re-introduces the per-faction structure memory P22's final form deleted. A
narrower scope (barricades now, other buildings later) is available on that
asymmetry alone. **(3) Where a built barricade's faction comes from** —
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
([03-resources/structures-and-start.md:21](03-resources/structures-and-start.md)).
The undocumented-rule half is **P38**.

*The proposed shape (the starting position, not a ruling).* Every building
interaction requires a **relationship** to proceed, drawn from the closed set
Q127 already fixes at design time (own / ally / enemy), with neutral field
objects the separate unowned class Q127 names. `World::allied` already
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

**Q129 — what do `enemy` and `ally` select, and may a program still target a
declared ally? OPEN (opened 2026-08-15, docs/01 / docs/08 / docs/04).** Sits
between Q91, which ruled the *harm* side of alliance, and Q127/Q128, which own
the relationship vocabulary. Nothing has ruled the *query* side, and the shipped
behaviour picked an answer.

*The state today.* `find_kind`'s `enemy` arm skips a bot only when
`b.data.faction == faction || b.data.dying` (`crates/sim/src/host.rs:223`), so
**`closest(enemy)` returns a declared ally**. `World::allied` exists
(`world.rs:1865`, symmetric, and a faction is its own ally) and is called
**nowhere in `host.rs`** — no Pyrite-visible query respects an alliance at all.
The canonical combat loop is therefore `attack(closest(enemy).expect())` against
whoever is nearest, ally included: declaring an alliance protects you from a
partner's `guard()`/`escort()` auto-fire and from nothing else they have running.
`ally` has no kind constant to begin with (**P37**), so there is no handle for the
other direction either.

*Why Q91 does not already answer it.* Q91 ruled that auto-fire spares declared
allies "to prevent *accidental* friendly fire", while explicit `attack()` and the
wreck-race verbs gate only on the server harm setting, because "betrayal is legal
PvP play". That is a rule about what a verb may *do to* a target. It says nothing
about what a query *hands you*, and the gap between the two is exactly where the
accident Q91 wanted to prevent still happens — not through auto-fire, but through
the ordinary combat program the player never revised after allying.

*The proposed shape (the starting position, not a ruling).* `enemy` means **not
own and not allied**; `ally` means **allied but not own**; `enemy` picks a former
ally back up the tick a pact lapses. The two then partition the non-own bots and
neither overlaps own, which keeps them usable as the closed own/ally/enemy set
Q127 and Q128 both build on.

**This cannot land without `ally`.** If `enemy` stops returning allies while P37
leaves `ally` unbuilt, no query can produce an allied bot at all and explicit
betrayal becomes *unwritable* — repealing Q91 by omission rather than by ruling.
The two ship together or neither ships.

*What it must still settle.* **(1) Does `ally` include your own colony?**
`World::allied` says yes — a faction is its own ally — so the predicate as
written makes `closest(ally)` mean "nearest friendly bot", convenient for escort
and repair idioms but no longer the complement of `enemy`, leaving the closed set
overlapping. A separate `own` spelling is the alternative, and it is a constant
nobody has asked for. **(2) Ferals.** `FERAL_FACTION` allies with nobody, so a
Feral's `enemy` is unchanged and its `ally` is empty or own-only depending on
(1) — worth stating rather than leaving to fall out. **(3) How far the ruling
reaches**: only the generic `closest`/`exists` pair, or every query that names
enemies (`scan_enemies()` and friends)? Anything else naming enemies has the same
gap, and a split answer is how one of them drifts. **(4) The betrayal ergonomics**
the ruling implies — afterwards, attacking an ally means asking for
`closest(ally)` and handing it to `attack()`: still legal under Q91, but now
deliberate rather than the default. That is the behaviour change, and it is
⚠HASH.

The **playtest-tuning** bucket also remains (numbers that need the prototype, not a choice, so they never block design): upkeep mix balance — does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? ([02-agents.md](02-agents.md)); Corruption spread/re-corruption rates, source radii, and cleanse speed ([05-terrain.md](05-terrain.md)), and — per the 2026-07-26 sweep — the first-pass figures shipped inside completed milestones: body-perk magnitudes (+ Age's deferred max-HP growth), quirk weights and the per-slot dial shape, upgrade-catalog times, upkeep.ron figures, guard/escort leash and cooldown, the Feral footprint metric and nest income, and the 14-ticks/tile pacing floor (with the boot/print-tick spec pass flagged in TASKS.md). Implementation-milestone work (e.g. the deferred PvP mapgen symmetry) is tracked in [TASKS.md](TASKS.md), not here.

---

## Answered

Every numbered question through Q126 is answered. The rulings live in
[history/questions-answered.md](history/questions-answered.md) — newest first,
append new rulings at the top of that file. The full worksheet bodies are
archived in
[history/questions-worksheets.md](history/questions-worksheets.md).
