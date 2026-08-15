# Inbox — raw observations, not yet triaged

Somewhere to write a problem down the moment it is noticed, in whatever words
come out, without stopping to find the right register or the right doc. **This
file is not spec and carries no rulings.** Nothing here binds anything; an entry
is a note to future-you until it is triaged.

Tracked, so an entry is visible to a review the moment it is written — which is
the point: the cost of a scratch note is that nobody else can see it, and the
cost of a register is that you have to know where a thing goes before you can
write it down. This file is the intake that has neither cost.

**Triage** means turning an entry into something that can actually hold it — a
numbered defect in [PROBLEMS.md](PROBLEMS.md), a question in
[QUESTIONS.md](QUESTIONS.md), or an implementation item in [TASKS.md](TASKS.md) —
and then moving it to the log at the bottom with a pointer to where it went.
Entries are never fixed *here*, and they do not expire: an untriaged entry is a
review finding, and a doc pass that walks past this file has missed something.

Write freely — half-formed is fine. P42 came out of "Alligence is really what
team the structure or bot is on", which named a real collision precisely enough
to act on.

## Open

*(nothing untriaged)*

## Triaged

- **"The fabricator should only ever be called the printer."** → **P41**
  (triaged 2026-08-15, fixed in `925b5f5`). The glossary had *ratified* the dual
  name — the row read "Fabricator / Printer" — so neither spelling was wrong and
  both kept spreading, to 21 live-doc mentions and 11 in `crates/`, while Pyrite
  only ever knew `printer`. Swept to **Printer** everywhere except closed
  records.
- **"Alligence is really what team the structure or bot is on."** → **P42**
  (triaged 2026-08-15, fixed in `925b5f5`). Q127 had taken *allegiance* — the
  glossary term for a Nest's tarot rank 0–21 — to mean which faction owns a
  building. The Arcana meaning kept the word; Q127's vocabulary became
  **faction**. *Team* could not be the replacement: docs/08 already uses it for
  alliance groups, so the glossary now separates faction / colony / team / color
  explicitly.
