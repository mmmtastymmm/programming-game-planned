# Pyrite — The Unit Language

Pyrite is a **custom Python-like DSL** with an interpreter written in Rust. We control the whole stack, which buys us three things real Python can't cheaply give us:

1. **Line-at-a-time execution** metered in cycles (bots visibly "think").
2. **Construct gating** — `if`, loops, variables, `def` are *unlockable features*, enforced at parse time.
3. **Determinism** — required for lockstep multiplayer ([08-multiplayer.md](08-multiplayer.md)). No floats exposed to programs, no wall clock, no hash-order iteration.

## The parts

| File | Owns |
|---|---|
| [execution-model.md](01-language/execution-model.md) | The cycle budget, saving up, blocking actions, cycle debt, `bank_cap`. |
| [faults-and-handlers.md](01-language/faults-and-handlers.md) | What a fault is, the seven reserved handlers, the double-handle rule (abort), Black Boxes & the boot sequence. |
| [signals-and-logging.md](01-language/signals-and-logging.md) | Signal handlers, the logging verbs, and the consequences the design wants. |
| [cycle-costs.md](01-language/cycle-costs.md) | The base cost table and how overlays modify it. |
| [syntax-tiers.md](01-language/syntax-tiers.md) | Tiers 0–6: what each unlock adds, with a worked program per tier. |
| [program-colors.md](01-language/program-colors.md) | Colors, target shares, the recall interrupt, dormant printers. |
| [modules-and-library.md](01-language/modules-and-library.md) | Modules, the program library, sharing and versioning. |
| [types-and-env.md](01-language/types-and-env.md) | The type set, and the per-bot `key → int` environment. |
| [builtins.md](01-language/builtins.md) | The built-in function blocks and their signatures. |
| [editor-ux.md](01-language/editor-ux.md) | The in-game editor and what the player sees. |
| [decided.md](01-language/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

These are the invariants a change to any part above has to keep. They live here
because no single part owns them.

- **The cycle economy rests on actions blocking.** Thinking and acting never
  overlap (Q100). Every cost in [cycle-costs.md](01-language/cycle-costs.md) and
  every signature in [builtins.md](01-language/builtins.md) is priced on that
  assumption; making any action non-blocking invalidates both.
- **Engine-initiated charges are debt; window code pays normally** (Q75). The
  trap cost, boot's forced `upload_log()`, and abort's forced sequence drive the
  budget negative rather than waiting to be affordable — logs always go home.
  Anything a player writes in a window is ordinary costed code.
- **Budgets are stored in centicycles** (×100); `costs.ron` entries are whole
  cycles, converted at charge time. Prose throughout these files reads in whole
  cycles — only storage is fine-grained.
- **No key's worst-case effective cost may exceed `bank_cap`**, checked at load
  (Q75/Q82/Q101). This is what makes freeze-forever unrepresentable, and it
  couples [cycle-costs.md](01-language/cycle-costs.md) to the overlay pipeline in
  [05-terrain.md](05-terrain.md) and to per-bot deltas from
  [09-quirks.md](09-quirks.md).
- **Construct gating is enforced at parse time**, so a tier the player lacks is a
  program that will not load — not a runtime error.
  [syntax-tiers.md](01-language/syntax-tiers.md) owns the ladder;
  [06-progression.md](06-progression.md) owns when it unlocks.
- **Every number in these files is a tuning constant** bound for `costs.ron`,
  never a commitment in code.
- **Determinism** (CLAUDE.md): no floats reach a program, queries return in
  stable sorted order with ties broken by entity ID, and programs are stored as
  byte-exact source hashed for versioning.
