*Part of [07-architecture](../07-architecture.md).*

# Testing Strategy (day one, not later)

- **Golden replays**: `(seed, command stream) → final state hash` tests; any PR changing a hash must explain why.
- **Cross-run determinism test in CI**: run the same replay twice in one process + once in another, compare hashes every 100 ticks.
- VM unit tests: each construct/function, cycle-cost accounting, gating errors.
- Headless balance harness: run scripted colonies for N ticks, assert economy curves — the `sim` crate split makes this a plain `cargo test`.
