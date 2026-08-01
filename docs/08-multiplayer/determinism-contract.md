*Part of [08-multiplayer](../08-multiplayer.md).*

# Determinism Contract

The rules every system must obey (enforced by CI replay tests):

1. Fixed tick rate; sim never reads wall clock or frame time.
2. Integer/fixed-point math only in sim. No `f32`/`f64` in any state-affecting path.
3. All randomness from named, seeded RNG streams (`rng.combat`, `rng.wander`, …) advanced only by sim systems.
4. Stable iteration order everywhere (sort by entity ID before mutation).
5. All external influence enters as ordered `Command`s — including in single-player.
6. Pyrite VM: no nondeterministic builtins; `scan_enemies()` returns results in stable sorted order, etc.
