# Programming Game (working title)

A Bevy multiplayer RTS where players program their units in **Pyrite**, a custom Python-like DSL interpreted one operation at a time. Design lives in `docs/00`–`09`; unresolved design questions in `docs/QUESTIONS.md`.

Every numbered doc except `00-overview` is **split into parts**, Rust-module style: `01`–`09` each keep a doorway `NN-name.md` beside a `NN-name/` directory. The doorway holds only the invariants that cross its parts plus a table of what each part owns — **it is not a summary and does not substitute for the parts.** Reading one of those docs means the doorway *and* its directory. `00-overview.md` is a single file.

Crate layout: `crates/pyrite` (language), `crates/sim` (deterministic world — **plain Rust, no Bevy**), `crates/game` (Bevy app). See `docs/07-architecture.md`.

## Determinism rules (CRITICAL — lockstep multiplayer)

The entire `sim` layer (including the Pyrite VM) must be bit-for-bit deterministic across machines. Violations surface as multiplayer desyncs, which are miserable to debug. Non-negotiable rules for any code in `pyrite` or `sim`:

1. **The `sim` crate has no `bevy_ecs` — keep it that way.** World state is plain Rust structs + `BTreeMap`s, so iteration is deterministic by construction. Never introduce ECS queries or ECS-managed state into `sim`; `bevy_ecs` lives only in the `game` crate, which may influence the sim exclusively through ordered `Command`s. ECS-side code feeding sim state any other way is the #1 architecture violation to flag in review — every time.
2. **No float types (`f32`/`f64`) in any state-affecting path.** Integer / fixed-point math only. Floats are fine in rendering/UI (the `game` crate) only.
3. **No `HashMap`/`HashSet` iteration in sim logic** — hash order is nondeterministic. Use `BTreeMap`, sorted `Vec`s, or sort before iterating.
4. **No wall clock, no frame time, no OS randomness.** All randomness comes from named, seeded RNG streams owned by the sim and advanced only by sim systems.
5. **All external input enters as ordered `Command` values** — even in single-player (which is lockstep with one peer).
6. Pyrite builtins must be deterministic: query results (e.g. `scan_enemies()`) return in stable sorted order; ties break by entity ID.
7. Programs are stored as **byte-exact plain text** (no whitespace normalization, UTF-8); program versions are identified by hashing source bytes.

Testing expectation: golden-replay tests (`(seed, command log) → state hash`) guard determinism in CI. A PR that changes a replay hash must explain why.

## Design-doc conventions

- Every numeric value in docs (cycle costs, XP curves, timers) is a tuning constant, expected to live in data files (`costs.ron` etc.), not code.
- When a design decision is made, it moves to the owning doc's **Decided** section — `NN-name/decided.md` for a split doc, the in-file `## Decided` otherwise. Open items live in `docs/QUESTIONS.md` (numbered — don't renumber, append) and **only** there: any other doc may cite a number inline ("open — Q124") but never restates a question's substance or leans, and doorways carry no Open Questions sections.
- A ruling that changes a **cross-part invariant** must update the doorway's *What holds across all of them* list too, not just the part file. That list is the contract between parts; letting it drift is the split's characteristic failure.
- Known defects in *already-decided* text — a ruling that never propagated to its owning doc, a tuning number that fails arithmetic against its inputs, or a ratified decision the implementation never caught up to — live in `docs/PROBLEMS.md` (numbered P1…, same append-only rule). Fixing one moves it to that file's **Fixed** log with the commit hash; because the fix and its Fixed-log entry land together, the hash is written in a small follow-up commit.
- **Dated status blocks are point-in-time records — supersede, never back-edit.** When the board changes, write a new dated block and drop `(latest)` from its predecessor (`QUESTIONS.md` also archives the displaced block, unchanged, to `docs/history/questions-status-log.md`; `PROBLEMS.md` stacks them in place). Never reopen a dated block to add an entry, correct a count, or extend a sentence — an amended block stops describing any real moment, and a stale headline outlives the body that contradicts it.
- `docs/history/` is **closed records, not spec** — the answered-question log, dated status blocks, completed milestones, fixed review rounds. It is expected to contradict current design and is never the authority on it. Don't read it in a normal doc pass; open a file there only to recover *why* a past call was made. Answering a question appends the ruling to `docs/history/questions-answered.md`, moves the question's worksheet body to `docs/history/questions-worksheets.md`, and moves the displaced `QUESTIONS.md` status block to `docs/history/questions-status-log.md`. See [docs/history/README.md](docs/history/README.md).
