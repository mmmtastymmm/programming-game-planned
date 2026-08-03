# Pyrite — The Unit Language

Pyrite is a **custom Python-like DSL** with an interpreter written in Rust. We control the whole stack, which buys us three things real Python can't cheaply give us:

1. **Line-at-a-time execution** metered in cycles (bots visibly "think").
2. **Construct gating** — variables, loops, `def`, lists are *unlockable features*, enforced at parse time (branching ships at game start — Q117).
3. **Determinism** — required for lockstep multiplayer ([08-multiplayer.md](08-multiplayer.md)). No floats exposed to programs, no wall clock, no hash-order iteration.

## The parts

| File | Owns |
|---|---|
| [execution-model.md](01-language/execution-model.md) | The cycle budget, saving up, blocking actions, cycle debt, `bank_cap`. |
| [faults-and-handlers.md](01-language/faults-and-handlers.md) | What a fault is, the seven reserved handlers, the double-handle rule (abort), Black Boxes & the boot sequence. |
| [signals-and-logging.md](01-language/signals-and-logging.md) | Signal handlers, the logging verbs, and the consequences the design wants. |
| [cycle-costs.md](01-language/cycle-costs.md) | The base cost table and how overlays modify it. |
| [syntax-tiers.md](01-language/syntax-tiers.md) | Tiers 0–7: what each unlock adds, with a worked program per tier. |
| [program-colors.md](01-language/program-colors.md) | Colors, target shares, the recall interrupt, dormant printers. |
| [modules-and-library.md](01-language/modules-and-library.md) | Modules, the program library, sharing and versioning. |
| [types-and-env.md](01-language/types-and-env.md) | The type set, and the per-bot `key → int` environment. |
| [builtins.md](01-language/builtins.md) | The built-in function blocks and their signatures. |
| [editor-ux.md](01-language/editor-ux.md) | The in-game editor and what the player sees. |
| [decided.md](01-language/decided.md) | Settled rulings owned by this doc. |

## What holds across all of them

Invariants a change to any part above has to keep. **None of them is canonical
here.** Each names the file that owns it; if a bullet and its owner disagree, the
owner wins and the bullet is the bug. This list exists so a change to one part
cannot silently break another — not to save anyone reading the parts.

- **The cycle economy rests on actions blocking.** Thinking and acting never
  overlap (Q100) — canonical in
  [execution-model.md](01-language/execution-model.md). Every cost in
  [cycle-costs.md](01-language/cycle-costs.md) and every signature in
  [builtins.md](01-language/builtins.md) is priced on that assumption; making any
  action non-blocking invalidates both.
- **Handler windows are unrestricted; the double-handle rule is the only
  pricing** (2026-08-02, supersedes Q49/Q51) — canonical in
  [faults-and-handlers.md](01-language/faults-and-handlers.md). No instruction
  cap, no `signal_safe` flag, no deploy rejection: a window takes any builtin,
  any `def`, loops included. Window length is bounded by program memory and
  window danger by abort-on-second-signal. The deploy-time analysis survives as
  **information only** — a worst-case instruction count where computable,
  `unbounded` otherwise, plus one warning for an unbounded window (unbounded
  loop, or a channel call with `timeout=None` — action-blocking verbs like
  `move_to` always resolve and never warn). Anything reintroducing a window-only restriction — in
  [builtins.md](01-language/builtins.md)'s table, a
  [modules-and-library.md](01-language/modules-and-library.md) import rule, or
  an [editor-ux.md](01-language/editor-ux.md) greying rule — contradicts this.
- **Engine-initiated charges are debt; window code pays normally** (Q75) —
  canonical in [execution-model.md](01-language/execution-model.md). The trap
  cost, boot's forced `upload_log()`, and abort's forced sequence drive the
  budget negative rather than waiting to be affordable; anything a player writes
  in a window is ordinary costed code.
- **Budgets are stored in centicycles** (×100) — canonical in
  [execution-model.md](01-language/execution-model.md). Prose throughout these
  files reads in whole cycles; only storage is fine-grained.
- **No key's worst-case effective cost may exceed `bank_cap`**, checked at load
  (Q75/Q82/Q101) — canonical in
  [execution-model.md](01-language/execution-model.md). This is what makes
  freeze-forever unrepresentable, and it couples
  [cycle-costs.md](01-language/cycle-costs.md) to the overlay pipeline in
  [05-terrain.md](05-terrain.md) and to per-bot deltas from
  [09-quirks.md](09-quirks.md).
- **Construct gating is enforced at parse time**, so a tier the player lacks is a
  program that will not load — not a runtime error.
  [syntax-tiers.md](01-language/syntax-tiers.md) owns the ladder;
  [06-progression.md](06-progression.md) owns when it unlocks.
- **Every number in these files is a tuning constant** bound for `costs.ron`,
  never a commitment in code — canonical in CLAUDE.md's doc conventions.
- **Determinism** (CLAUDE.md): no floats reach a program, queries return in
  stable sorted order with ties broken by entity ID, and programs are stored as
  byte-exact source hashed for versioning.
