# Programming Game (working title)

A Bevy multiplayer RTS where players program their units in **Pyrite**, a custom Python-like DSL interpreted one operation at a time. Design lives in `docs/00`–`08`; unresolved design questions in `docs/QUESTIONS.md`.

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
- When a design decision is made, it moves to the owning doc's **Decided** section; open items live in `docs/QUESTIONS.md` (numbered — don't renumber, append).
