# Open Questions Worksheet

All design questions across the design docs (00–09). As each is decided, its ruling moves to the owning doc's *Decided* section and its worksheet entry moves to [history/questions-worksheets.md](history/questions-worksheets.md) — answered entries don't linger here. Numbering is stable — append new questions, never renumber. Open questions live **only** in this file: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans.

This file is for things **not yet decided**. Text that is already wrong — a decision contradicted, or a number that fails arithmetic — is tracked in [PROBLEMS.md](PROBLEMS.md), numbered P1… on the same append-only rule.

**Status 2026-08-02 (latest): Q126 OPENED — foreign-structure queryability.**
P22's structure-query ruling simplified to its knowledge-pool form (own
colony state plus granted allies', current by construction) after the
`faction=` selector design was retired for compounding contradictions
(PROBLEMS.md, P22's amendments); whether programs should reach *foreign*
structure intel at all is now the open question. Q124–Q125 remain open;
everything through Q123 remains decided.

*Earlier status entries — the dated record of how the board got here — are in
[history/questions-status-log.md](history/questions-status-log.md). The
per-question ruling log is in
[history/questions-answered.md](history/questions-answered.md).*

---

## Open

**Q124 — can opponents see a color's *version counter* tick? OPEN (opened
2026-08-01, docs/08).** Swept in from an unnumbered note in
[08-multiplayer/code-visibility.md](08-multiplayer/code-visibility.md). A
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

The **playtest-tuning** bucket also remains (numbers that need the prototype, not a choice, so they never block design): upkeep mix balance — does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? ([02-agents.md](02-agents.md)); Corruption spread/re-corruption rates, source radii, and cleanse speed ([05-terrain.md](05-terrain.md)), and — per the 2026-07-26 sweep — the first-pass figures shipped inside completed milestones: body-perk magnitudes (+ Age's deferred max-HP growth), quirk weights and the per-slot dial shape, upgrade-catalog times, upkeep.ron figures, guard/escort leash and cooldown, the Feral footprint metric and nest income, and the 14-ticks/tile pacing floor (with the boot/print-tick spec pass flagged in TASKS.md). Implementation-milestone work (e.g. the deferred PvP mapgen symmetry) is tracked in [TASKS.md](TASKS.md), not here.

---

## Answered

Every numbered question through Q123 is answered. The rulings live in
[history/questions-answered.md](history/questions-answered.md) — newest first,
append new rulings at the top of that file. The full worksheet bodies are
archived in
[history/questions-worksheets.md](history/questions-worksheets.md).
