---
name: docs-coherence
description: Audit the whole design corpus (docs/00-09, QUESTIONS.md, PROBLEMS.md, TASKS.md) for incoherence — contradictions between docs, orphaned terms, numbers that don't reconcile, unreachable economy nodes, shipped programs invalid under the current spec. Use when asked to check the docs are coherent/consistent, to audit the design, to find design problems, or after a large sweep. For propagating one specific ruling, use design-ruling instead.
---

# Auditing the design corpus for coherence

This is the **pull-based** counterpart to `design-ruling`. Nothing changed;
the corpus has drifted, and the job is to find where. Both skills check the same
properties, defined once in [.claude/design-invariants.md](../../design-invariants.md)
— **read that file first.** It is the substance; this file is the procedure.

## What this is not

Not a reading-and-comparing exercise. The two most serious defects ever found in
this corpus were invisible to reading:

- **P1** — every document reads correctly on its own. The Upgrade Station is
  simply unreachable once you traverse the economy graph from the starting kit.
- **P2** — every document agreed with every other. The number was still 8× wrong,
  and only arithmetic showed it.

A pass that only reads finds the contradictions between adjacent sentences (real,
but the cheap half) and silently certifies the rest. **Build the derived models
first, while you are still fresh.**

## Step 1 — build the derived models

Rebuild each of these *from the docs*, writing it out rather than holding it in
your head. Each is one invariant made concrete.

**The economy graph (I5).** Tier ladder → raws → recipes → refined goods →
structures and tools → what each one gates. Traverse from the starting kit.
List anything unreached. This is a graph problem; do it as one.

**The derived-constants table (I4).** Every number presented as derived, its
formula, its inputs, where each input is fixed, the recomputed value, and the
doc's claimed target. Recompute whole tables, never single rows — a shared
formula means a shared error.

**The stat sheet closure (I6).** Two set-differences: every effect in the design
(hardware, perks, quirks, terrain, state) against docs/02's rows; every row
against the sources that grow it. Both directions.

**The term index (I2).** Terms the docs define, and terms the docs use. The
symmetric difference is the finding: used-but-undefined is residue from a
deletion, defined-but-unconsumed is an orphan.

**The shipped programs (I8).** Every verbatim program in docs/01 and docs/04,
executed against the current language reference — resource-absent, unreachable,
and depleted cases, faulting vs non-faulting verbs. Then the hardcoded programs
in `crates/game`.

## Step 2 — the read pass

Now read all of it, in order: `00-overview` through `09-quirks`, then
`QUESTIONS.md`, `PROBLEMS.md`, `TASKS.md`.

**Four docs are split into parts** (2026-07-29): `01-language`, `02-agents`,
`03-resources` and `05-terrain` each have a doorway `NN-name.md` beside a
`NN-name/` directory. Reading one means the doorway **plus every file in its
directory** — the doorway carries only cross-part invariants and a table of what
each part owns, so auditing it alone audits almost nothing.

The split creates one new failure mode worth hunting deliberately: **a doorway
invariant that no longer matches the part that owns it.** The doorway's *What
holds across all of them* list is a contract; check each bullet against the file
it points at. A drifted bullet is worse than a drifted paragraph, because it
reads as the authoritative summary.

**`docs/history/` is out of scope for the audit.** Archived rulings, status
blocks, completed milestones and closed review rounds are records of what was
true when written — they are *expected* to contradict current design, so
auditing them manufactures false findings. The corpus under audit is the live
files above. Open a history file only to answer "why was this decided this
way?", and never file a finding against its contents.

Two passes get separated out because the general read skims them:

- **Every doc's Decided section, on its own** (I3) — the four `*/decided.md`
  files plus the in-file `## Decided` of the unsplit docs. These read as settled
  history and the eye slides past. This is where the worst drift accumulates.
- **Behavioral rules, branch by branch** (I7) — including the exhausted, empty
  and absent cases, which is where single-valuedness usually fails.

While reading, resolve citations (I9), and watch for the same fact stated twice
without one being marked a cross-reference (I1).

## Step 3 — a finding must quote both sides

The last full review generated 84 candidates and **8 were refuted** on
verification — roughly one in ten was noise. Before recording anything:

- Quote **both** conflicting texts verbatim, with file and line.
- For an arithmetic finding, show the computation.
- Ask what a reader or implementer actually does wrong, concretely. If you can't
  name the wrong outcome, it is a wording preference, not a defect.
- Check it is not already a `P` entry, or already in PROBLEMS.md's
  **Checked and cleared** list — those were refuted once and should not return.

## Step 4 — triage

- **Mechanical** (the decision exists, the text didn't follow): fix it, and run
  the deleted-term grep (I2) so the fix doesn't leave residue of its own.
- **Needs a ruling** (the docs don't contain the answer): **do not guess.**
  Append a `P` to PROBLEMS.md, or a `Q` to QUESTIONS.md if it is genuinely
  undecided rather than wrong. Never renumber either file.

  This is the M16 lesson and it is not optional: three review passes over one
  milestone found 45 defects, and each pass found that most of the *previous*
  pass's fixes were themselves broken — "decisions implemented without ever being
  made, then patched three times by guesswork." Both fix commits were reverted.

- **Cleared:** a candidate you investigated and refuted goes in PROBLEMS.md's
  **Checked and cleared** section with the one-line reason, so the next audit
  doesn't spend budget on it again.

## Step 5 — report

Lead with what an implementer would actually build wrong, ordered by that — not
by how many documents each defect touches. Group by root cause: the last audit's
76 verified findings collapsed to 15 defects, and reporting 76 would have buried
the three that mattered.

Say plainly what you did **not** check. A partial audit reported as complete is
how this corpus got two consecutive sweeps whose commit messages claimed
completeness and did not have it.

## Scope and cost

A full corpus audit is expensive and the invariants are independent, so it
parallelizes cleanly — one agent per derived model, or per invariant, if the user
has opted into that. Otherwise scope it: a named subsystem, the docs touched by
one milestone, or a single invariant across all docs (I5 alone is a good cheap
run after any pricing change).

For adversarial depth on a diff rather than the corpus, `/code-review max` is the
stronger tool — that is what produced P1–P14. This skill covers what a
diff-scoped review structurally cannot see: drift that was already there before
the diff.
