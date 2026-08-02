# Open Questions Worksheet

All design questions across the design docs (00–09). As each is decided, its ruling moves to the owning doc's *Decided* section and its worksheet entry moves to [history/questions-worksheets.md](history/questions-worksheets.md) — answered entries don't linger here. Numbering is stable — append new questions, never renumber. Open questions live **only** in this file: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans.

This file is for things **not yet decided**. Text that is already wrong — a decision contradicted, or a number that fails arithmetic — is tracked in [PROBLEMS.md](PROBLEMS.md), numbered P1… on the same append-only rule.

**Status 2026-08-02 (latest): Q124–Q126 ANSWERED — THE BOARD IS CLEAR.**
Version counters are opponent-visible (Q124), structural whitespace is
always visible in masked views (Q125), and no program-side foreign-structure
query surface ships in v1 (Q126). Every numbered question through Q126 is
decided; only the playtest-tuning bucket remains open, and it never blocks
design. Implementation proceeds from TASKS.md.

*Earlier status entries — the dated record of how the board got here — are in
[history/questions-status-log.md](history/questions-status-log.md). The
per-question ruling log is in
[history/questions-answered.md](history/questions-answered.md).*

---

## Open

The **playtest-tuning** bucket also remains (numbers that need the prototype, not a choice, so they never block design): upkeep mix balance — does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? ([02-agents.md](02-agents.md)); Corruption spread/re-corruption rates, source radii, and cleanse speed ([05-terrain.md](05-terrain.md)), and — per the 2026-07-26 sweep — the first-pass figures shipped inside completed milestones: body-perk magnitudes (+ Age's deferred max-HP growth), quirk weights and the per-slot dial shape, upgrade-catalog times, upkeep.ron figures, guard/escort leash and cooldown, the Feral footprint metric and nest income, and the 14-ticks/tile pacing floor (with the boot/print-tick spec pass flagged in TASKS.md). Implementation-milestone work (e.g. the deferred PvP mapgen symmetry) is tracked in [TASKS.md](TASKS.md), not here.

---

## Answered

Every numbered question through Q126 is answered. The rulings live in
[history/questions-answered.md](history/questions-answered.md) — newest first,
append new rulings at the top of that file. The full worksheet bodies are
archived in
[history/questions-worksheets.md](history/questions-worksheets.md).
