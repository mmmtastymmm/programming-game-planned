# docs/history — closed records

Everything in this directory is **history, not spec**. It records what was
decided, built, or found *at a point in time*. Entries here are expected to
contradict the current design where the design has since moved on — that is not
a defect, and it is not something to fix.

The authority on current design is always the live corpus: `docs/00`–`09` for
rules, [QUESTIONS.md](../QUESTIONS.md) for what is still open,
[PROBLEMS.md](../PROBLEMS.md) for known-wrong text, [TASKS.md](../TASKS.md) for
what is left to build.

| File | Holds | Split out |
|---|---|---|
| [questions-answered.md](questions-answered.md) | The per-question ruling log, Q1–Q123. Newest first — **append new rulings at the top.** | 2026-07-29 |
| [questions-status-log.md](questions-status-log.md) | Dated board-state entries: what was open on each date and what the day's rulings changed. Newest first. | 2026-07-29 |
| [tasks-completed.md](tasks-completed.md) | Milestones M0–M3 — fully complete, no open items. | 2026-07-29 |
| [reviews.md](reviews.md) | Six review rounds, 2026-07-16 → 2026-07-20. Every finding fixed. Oldest first. | 2026-07-29 |

## Working rules

- **Neither `design-ruling` nor `docs-coherence` reads this directory** during
  its normal pass, by design — that is the point of the split. Both still *grep*
  it, and a hit for a retired term is usually correct history to leave alone.
- **Answering a question writes here.** The ruling goes to the top of
  `questions-answered.md`; the status block it displaces from `QUESTIONS.md`
  goes to the top of `questions-status-log.md`. Letting status blocks stack in
  `QUESTIONS.md` is what grew it to 166 KB.
- **Archive a milestone only when it is inert** — every item checked, and no
  note inside it binding on unbuilt work. M4–M15 stayed in `TASKS.md` for
  exactly that reason. When a note *is* still live but the milestone is
  otherwise done, copy the note to *Carried forward from completed milestones*
  in `TASKS.md` and archive the rest.
- Nothing here was rewritten on the way in. The text is verbatim; only the
  headers at the top of each file are new.
