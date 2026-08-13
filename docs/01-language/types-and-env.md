# Types & the Environment

*Part of [01-language](../01-language.md).*

## Types

Deliberately small: `int` (i64), `bool`, `string` (labels/channels only, no manipulation initially), `entity` (opaque handle to a world object), `list`, `dict` (keys: int / string / entity), and `enum` values (user-declared sum types with associated data, Tier 6). **No floats** — all world math is fixed-point internally and exposed to Pyrite as scaled integers (e.g. positions in millitiles).

**Containers are values, not references (DECIDED).** Assignment and argument passing copy; there is no aliasing. Mutation is always rooted at a variable — `xs.append(v)`, `d[k] = v` — and inside a `def` those writes hit the variable where it lives, Python-consistent (`xs[0] = 1` in a function mutates the outer list; only `xs = ...` makes a local). Mutating a temporary (`[1,2].append(3)`) is a fault, not a silent no-op. Simpler to reason about, deterministic by construction, and it keeps snapshots/state comparison trivial.

Three builtin conventions ride on these types:

- **Kind constants** — pre-bound global constants naming entity kinds; the generic queries take them as arguments (`closest(ore)`, `exists(blueprint)`). Assignments may shadow them (ordinary names, unlike the reserved `None`/`True`/`False`) and they survive post-fault restarts. **Every resource and every registry kind gets one** (Q79, completed round 4):
  - **Resources**: all eleven raws (`water` included) and all seven refined goods (`steel`, `bronze`, `wire`, `chips`, `glass`, `lens`, `gold_chip` — note `chips` is plural, matching the material and the Foundry recipe, while `gold_chip` is singular) — plus **`ore`, the family constant**: any *discovered* mineral vein or seam (Iron, Coal, Copper, Tin, Silver, Gold, Crystal; tier-0 surface kinds answer only their own names). `closest(ore)` in the starter program still means "nearest thing to mine"; `closest(silver)` is what a specialist writes. Queries return nodes regardless of tool tier — sensing isn't harvesting; an under-tiered `mine()` faults as usual.
  - **Structures**: `depot`, `smelter`, `foundry`, `generator`, `geothermal`, `pump`, `archive`, `repair_bay`, `upgrade_station`, `sentry`, `lantern`, `request_box`, `printer`.
  - **Battlefield objects**: `wreck`, `black_box`, `blueprint`, `cache`, `nest`, `blight`, `barricade`; **bots**: `enemy` / `ally`. Two of these carry rules the others don't:
    - **`blight`** (the creep's heart) is **perception-ungated** — the creep front is visible terrain, so `closest(blight)` answers without eyes on it. Every other findable placement is gated.
    - **`barricade`** is decided by Q99 on this same rule ("attackable, so it must be findable") and is **perception-gated like a structure** — a wall's position is exactly the intel fog protects. It is **not yet in the shipped registry**: the entry is blocked on **Q127**, which rules its query *domain* (P29 — whether a rival's wall is reachable at all), never its existence.
  - **Factions**: per-match faction constants, one per colony/nest — the handle for foreign-channel work.
  - **Feral bindings**: Feral programs additionally run with nest-bound values — `home` (*their own* nest; the global `nest` kind still means any nest) and `patrol_route` — supplied at print, the same mechanism faction-scoped. Player programs never get user bindings (Q59).
- **`Option` and `None`** — Pyrite has **no null**; absence is an enum, exactly as in Rust: the builtin `Option.Some(v)` / `Option.None`, with **`None`** as sugar for `Option.None`. `None`, `True`, and `False` are **reserved words** (Python-style): assigning to them is a parse error — unlike the kind and level constants, which stay ordinary shadowable names. Optional-typed parameters accept the value or `None` — `send(ch, val, timeout=None)` means "no timeout." `.expect()` works on it (`Some` unwraps, `None` faults), `match` destructures it like any enum, and a bare `case None:` is accepted as sugar for `case Option.None:`.
- **`Result`** — a builtin enum for fallible queries: `Result.Ok(entity)` / `Result.Err(msg)`. A `Result` never passes straight into another verb — `try_*` included ([builtins.md](builtins.md), the P4 contract): unwrap or `match` first. Unwrap with `.expect()` (returns the entity, or faults with the carried message) or handle the miss fault-free with `match` (Tier 6). **`try_*` verbs do not unwrap** — they take a concrete target, so handing one a `Result` or an `Option` is an ordinary type fault, not a silent no-op. The Tier-0 route is to guard with `exists_minable` / `exists` before unwrapping (see Tier 0):

```python
match closest(ore):
    case Result.Ok(t):
        move_to(t)
    case Result.Err(msg):
        wait(10)
```

## The Environment (env variables)

Every bot carries a small **environment**: a `key → int` store of *policy* parameters the engine consults. It's the settable half of a bot's identity — the stat sheet ([02-agents.md](../02-agents.md)) is what a bot *is*; the environment is what a bot has been *told*.

- **Keys are engine-defined and pre-bound** (same convention as kind and level constants): a fixed, enumerable set with defaults and bounded ranges. `getenv(key)` never faults — unset means default.
- **Env lives on the bot, exactly like XP**: it survives restarts, faults, redeploys, and recall re-colorings; it dies with the bot. This is deliberately *not* general persistent storage — ordinary variables still clear on every restart ("re-derive your state"); env is a settings panel with engine-defined slots. **User-defined keys are out for v1** (Q59, decided): they'd be genuine persistent bot memory, undermining the fault-restart guarantee (no corrupted state survives) and the re-derive discipline. Maybe later — only with playtest evidence that the discipline reads as tedium rather than craft.
- **`setenv` / `getenv` are ordinary costed builtins**, callable in a window like anything else — a hurt handler may lower its own `hurt_line` mid-retreat so the signal doesn't re-arm and re-fire on the limp home.
- **The `on boot:` window is your bot's dotfile.** Configuration at wake-up is the idiomatic pattern: print → boot window sets env → main program runs. Different colors can ship different profiles on identical chassis.
- **Env is private while alive — it leaks three ways** (answers Q58). No builtin or UI reads a foreign bot's live env. Instead: **behavior** — you infer a `hurt_line` by watching when the bot retreats, earned counterplay rather than a stat screen; **source** — the dotfile is code, so a color's configured values leak with its decryption % like every other line; **death** — the Black Box includes an **env snapshot**, so exact runtime values (mid-run `setenv`s, quirk clamps included) are read on murder, the game's oldest intel rule. Free live reading would make configuring bots self-defeating — `hurt_line` is precisely the number an attacker wants.

Well-known keys (v1 set — grows like the function catalog):

| Key | Default | Range | Engine behavior it parameterizes |
|---|---|---|---|
| `hurt_line` | 50 | 1–99 | The HP percentage where the `hurt` signal fires (edge-triggered; re-arms at the same line). Read live at each evaluation — moving it mid-flight is legal. Decoupled from the **Damaged** state penalty, which stays fixed at 50% ([02-agents.md](../02-agents.md)) |
| `log_min_level` | `trace` | `trace`–`error` | Minimum severity actually recorded to the ring buffer — lower entries are discarded before they consume a slot (the call still costs 1). A veteran runs quiet at `warn`; a bot under diagnosis runs at `trace` |

Design rule: **env keys are policy, never stats.** A key may change *when* engine behaviors fire (thresholds, filters), never how strong, fast, or far-sensing the bot is — capability lives on the stat sheet and is paid for in hardware, XP, or quirks.

