*Part of [08-multiplayer](../08-multiplayer.md).*

# Determinism Contract

The rules every system must obey (enforced by CI replay tests). These are the
same rules CLAUDE.md states as law for contributors — one list, two audiences:

1. **No `bevy_ecs` in `sim`.** World state is plain Rust structs + `BTreeMap`s; `bevy_ecs` lives only in the `game` crate, which influences the sim exclusively through ordered `Command`s. ECS-side code feeding sim state any other way is the #1 architecture violation ([07-architecture.md](../07-architecture.md)).
2. Fixed tick rate; sim never reads wall clock or frame time.
3. Integer/fixed-point math only in sim. No `f32`/`f64` in any state-affecting path.
4. All randomness from named, seeded RNG streams (`rng.combat`, `rng.wander`, …) advanced only by sim systems.
5. **Stable iteration order everywhere** — no `HashMap`/`HashSet` iteration in sim logic. Ordering comes free from the `BTreeMap` world, so no sort-before-mutate discipline is needed inside sim ([07-architecture/decided.md](../07-architecture/decided.md)); sort explicitly only where a collection isn't inherently ordered.
6. All external influence enters as ordered `Command`s — including in single-player.
7. Pyrite VM: no nondeterministic builtins; `scan_enemies()` returns results in stable sorted order, ties by entity ID.
8. **Programs are byte-exact plain text** (no whitespace normalization, UTF-8); program versions are identified by hashing source bytes.
