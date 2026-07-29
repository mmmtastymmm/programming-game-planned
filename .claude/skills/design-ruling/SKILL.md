---
name: design-ruling
description: Answer an open design question (docs/QUESTIONS.md Q-number) or fix a known problem (docs/PROBLEMS.md P-number), then propagate the ruling through every design doc and verify coherence. Use whenever a design decision is made, changed, or reversed — including "answer Q124", "fix P3", "we decided X, update the docs", or any edit to a Decided section.
---

# Making a design ruling stick

The **push-based** half of doc coherence: a decision was made, propagate it. The
pull-based half — nothing changed, the corpus drifted, go find it — is
`docs-coherence`. Both check the same properties, defined once in
[.claude/design-invariants.md](../../design-invariants.md).

Two entry points, one procedure:

- **A question** — a `Q` in [docs/QUESTIONS.md](../../../docs/QUESTIONS.md) under `## Open`.
- **A problem** — a `P` in [docs/PROBLEMS.md](../../../docs/PROBLEMS.md). Entries under
  *Mechanical* skip step 2; entries under *Needs a ruling* are questions wearing a
  different number and run the whole procedure.

The tail — propagate, then verify — is identical either way, and is where this
project has failed before. **The M16 lesson is the reason this skill exists:**
three review passes over one milestone found 45 defects, and each pass found that
most of the *previous* pass's fixes were themselves broken. The root cause was
recorded in QUESTIONS.md: they were "decisions M16 implemented without ever
making, then patched three times by guesswork." Both fix commits were reverted.

## Step 0 — scope it

Identify the **owning doc** (the one whose *Decided* section will hold the
ruling) and read the entry in full, including any AMENDED clauses. Then read the
neighbouring rulings it cites — a Q rarely stands alone, and the citation chain
is how you find the docs that will need touching.

State the blast radius before editing: which docs, which stat-sheet rows, which
tuning constants, whether it is hash-affecting, and whether any shipped program
text changes.

## Step 1 — if propagation reveals an undecided choice, STOP

The moment you find yourself picking between two readings that the docs do not
settle, **do not pick**. Append a new numbered Q to `docs/QUESTIONS.md`
(never renumber) and either resolve it explicitly with the user or leave it open
and scope the current ruling around it.

Guessing is what produced 45 defects and two reverted commits. A ruling that
lands with a known open edge is fine; a ruling that silently invents an answer is
the failure mode.

## Step 2 — make the ruling (questions and P-needs-ruling only)

Write it into the **owning doc's Decided section**, in this repo's voice: what
was decided, what it replaces, and *why the alternative was rejected*. The "why"
is not decoration — P1 exists because Q118 narrowed a rule without recording that
the narrowing stopped covering a case the wider version caught.

Then mark it answered in `docs/QUESTIONS.md` with the same date, and update that
file's status block at the top.

**Every number is a tuning constant** — it belongs in a data file (`costs.ron`,
`xp.ron`, …), never in code. State the formula and its inputs in the doc so the
next reader can re-derive it. See step 4b.

## Step 3 — read every design doc

All of them, every time, in order: `00-overview`, `01-language`, `02-agents`,
`03-resources`, `04-enemies`, `05-terrain`, `06-progression`, `07-architecture`,
`08-multiplayer`, `09-quirks`, then `QUESTIONS.md`, `PROBLEMS.md`, `TASKS.md`.

For each, ask **both** directions:

- Does it state anything the ruling changed?
- Does it *rely* on something the ruling changed, without naming it? (P11 left
  Lens with no priced consumer anywhere in the design, because deleting the
  Optics module deleted the only recipe that consumed it.)

Two places get a **separate, explicit pass**, because the general read
demonstrably skims them: **every doc's own Decided section** (I3) and **docs/02's
stat sheet** (I6). Also resolve every citation the ruling touches (I9) — P1's
root cause was a citation whose summary drifted from the ruling it cited.

## Step 4 — the mechanical gates

Reading catches contradictions between adjacent sentences. These catch the ones a
reader looks straight past because the text *looks* updated. Run them against the
change; do not eyeball them. Each is an invariant from
[.claude/design-invariants.md](../../design-invariants.md), which carries the
full statement and the case that motivates it.

**4a. Deleted terms grep to zero (I2).** List every term the ruling retires and
grep all of `docs/`. Each hit is a needed edit or deliberate history in an
Answered log — decide per hit.

```
grep -rn "Building tier\|capability tier\|Backup Core\|module slot" docs/
```

Then the reverse direction: anything the ruling *deletes* may have been the only
consumer of something else (P11 left Lens with no consumer anywhere).

**4b. Recompute every new number from its named inputs (I4).** Not checked for
plausibility — recomputed, arithmetic written out, result compared against the
target the doc claims. When a table shares one formula, **recompute the table**,
not the row you changed.

**4c. Behavioral rules stay single-valued (I7).** If the ruling changes sim
behavior it changes replay hashes: mark the TASKS.md entry `⚠HASH`, walk every
branch including the exhausted/empty/absent case, and confirm CLAUDE.md's
determinism rules still hold under the new text.

**4d. Re-execute the shipped programs (I8).** If the ruling touches Pyrite
syntax, builtins, fault behavior or the start kit, mentally run docs/04's and
docs/01's verbatim programs against the new rules — resource-absent, unreachable
and depleted cases, faulting vs non-faulting verb in each. Do not re-read them.

**4e. Re-traverse the economy graph (I5)** if the ruling changed any price, gate
or tier. Local pricing always looks right; only traversal from the starting kit
shows P1-class breaks.

## Step 5 — the game hardcodes Pyrite source

`crates/game` parses hardcoded Pyrite programs at startup with `.expect()`. A
language change that invalidates them **panics the game on launch while every
`sim` and `pyrite` test stays green** — no test suite will tell you.

Known sites (re-grep, they move): `crates/game/src/scene.rs` (miner, red starter,
showcase bot, starter), `crates/game/src/editor/mod.rs` (assembled body + stub
windows, starter module + default program), `crates/game/src/editor/window.rs`
(stub file assembly).

```
grep -rn 'expect("' crates/game/src/ | grep -iE 'pars|program'
```

The scene smoke test plus an actual `cargo run -p game` launch is the only real
guard. Docs and shipped source must change in the same commit.

## Step 6 — close out

- A fixed `P` moves to PROBLEMS.md's **Fixed** log with the commit hash.
- Anything found along the way that is wrong but out of scope gets **appended**
  to PROBLEMS.md as a new P (never renumber), with the same anatomy as the
  existing entries: locations, the quote or arithmetic that proves it, the
  consequence, and what a fix must settle.
- A ruling that opened a new edge gets **appended** to QUESTIONS.md as a new Q.
- Commit by pathspec, never `git add -A` — a second session may have work staged.

## Step 7 — verify

The propagation is not self-verifying; this repo has twice shipped a sweep whose
commit message claimed completeness and did not have it. Close with
`/code-review max` over the resulting diff, scoped to doc coherence.

That command is user-triggered and billed — ask for it, don't attempt to launch
it. Findings come back as a list; the ones that survive verification become P
entries or immediate fixes.
