# Open Questions Worksheet

All design questions across the design docs (00–09). As each is decided, its ruling moves to the owning doc's *Decided* section and its worksheet entry moves to [history/questions-worksheets.md](history/questions-worksheets.md) — answered entries don't linger here. Numbering is stable — append new questions, never renumber. Open questions live **only** in this file: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans.

This file is for things **not yet decided**. Text that is already wrong — a decision contradicted, or a number that fails arithmetic — is tracked in [PROBLEMS.md](PROBLEMS.md), numbered P1… on the same append-only rule.

**Status 2026-08-01 (latest): Q124–Q125 OPENED — the open-questions
consolidation sweep.** Unnumbered open questions found living outside this
file were swept in: the two code-visibility texture calls (version-counter
visibility, always-visible whitespace) are now **Q124–Q125**, and doorway
"Open Questions" sections were removed — per the ratified convention
(CLAUDE.md), open questions live **only** here; other docs cite numbers
without restating substance. The doorway tuning notes (upkeep mix, Corruption
rates) were folded into the playtest-tuning bucket at the bottom of this
file, which now carries their full substance. In the same sweep the answered worksheet bodies (Q111–Q123) moved
to [history/questions-worksheets.md](history/questions-worksheets.md), so
this file now holds only what is open. Everything through Q123 remains
decided.

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

The **playtest-tuning** bucket also remains (numbers that need the prototype, not a choice, so they never block design): upkeep mix balance — does Steel maintenance earn its complexity alongside Energy, or should the v1 config lean harder on Energy? ([02-agents.md](02-agents.md)); Corruption spread/re-corruption rates, source radii, and cleanse speed ([05-terrain.md](05-terrain.md)), and — per the 2026-07-26 sweep — the first-pass figures shipped inside completed milestones: body-perk magnitudes (+ Age's deferred max-HP growth), quirk weights and the per-slot dial shape, upgrade-catalog times, upkeep.ron figures, guard/escort leash and cooldown, the Feral footprint metric and nest income, and the 14-ticks/tile pacing floor (with the boot/print-tick spec pass flagged in TASKS.md). Implementation-milestone work (e.g. the deferred PvP mapgen symmetry) is tracked in [TASKS.md](TASKS.md), not here.

---

## Answered

Every numbered question through Q123 is answered. The rulings live in
[history/questions-answered.md](history/questions-answered.md) — newest first,
append new rulings at the top of that file. The full worksheet bodies are
archived in
[history/questions-worksheets.md](history/questions-worksheets.md).
