*Part of [01-language](../01-language.md).*

# Built-in Function Blocks (starter set)

**`try_` covers the action, never the argument** (P4 ruling, 2026-08-01). A `try_*` verb takes **concrete arguments** — the same values its faulting sibling takes — and what it makes fault-free is the *attempt*: unreachable, empty, full, or lost is no fault, no action, and a `False`/`None` return the program can branch on. Handing a `try_*` verb a `Result` or an `Option` is ordinary mis-typed code and faults like it would anywhere else — at runtime, since deploy validates memory and variable slots only, never types. Resolve absence *before* the verb: guard-then-act (`if exists_minable(ore): try_move_to(closest_minable(ore).expect())` — the adjacent-ops race this leaves is the accepted one, [syntax-tiers.md](syntax-tiers.md)) or `match` on the query. This closes the composition hole the deleted unwrap rule left open: `try_move_to(try_receive("orders"))` is not a fault-free order-follower, it is a type fault — receive first, test what arrived, then move.

The full catalog and unlock order live in [06-progression.md](../06-progression.md). Signature convention: **optional parameters come last and are Python-style keyword defaults** — `log(msg, level=warn)`, `receive(ch, timeout=100)`; omitted means the default (`timeout=None` = block forever; `None` is the builtin `Option.None` — see Types). Handler windows may call **any** function block, and any user `def`, loops included — the `signal_safe` property and the window caps were deleted 2026-08-02 ([07-architecture.md](../07-architecture.md)), leaving the double-handle rule as the only pricing of handler risk (see Signal handlers). Costs below are first-pass tuning and live in data:

| Function | Cost | Effect |
|---|---|---|
| `move_to(entity, only=None, avoid=None)` | 2 + travel | Pathfind and move; blocks until arrival or failure. **Tracks moving targets** (re-paths — there is no `chase()`; `move_to` *is* the chase). The canonical hurt retreat is one of these. **Paint-routed (Q95/Q96)**: `only=`/`avoid=` take a paint-color constant or a list of them (`unpainted` is a color too) and make forbidden colors impassable to this route search, like water — unreachable = the normal no-path fault; per call, no persistent binding; omitted = paint-blind ([05-terrain.md](../05-terrain.md) Tile Composition) |
| `closest_minable(kind)` → `Result` | 4 | Nearest node of `kind` this bot can work **right now** — within the grade of its installed drill **and** with ore remaining (Q117). The plain `closest` stays tier-blind by ruling (sensing isn't harvesting); this is the verb that asks the other question. Start kit |
| `exists_minable(kind)` → bool | 2 | Is there anything of `kind` this bot can work right now? Same predicate as `closest_minable`. Start kit |
| `try_move_to(target, only=None, avoid=None)` → bool | 2 + travel | The fault-free walk: takes a concrete entity or position, and an **unreachable goal** is no action and `False` rather than a fault. It does *not* accept a `Result` or an `Option` — unwrap first (guard with `exists`). Start kit |
| `mine()` | 2 + action | Extract from resource node in range |
| `try_mine()` → bool | 2 + action | The fault-free swing: extracts if an in-range node is workable by this bot's drill and not empty, else `False`. Start kit — the starter's verb |
| `deposit()` | 1 + action | Unload cargo into the adjacent **accepting structure** (Q79): Depot storage, refinery input, Generator intake, Station coolant tank, Request Box — and a Feral's nest is *their* depot. If several adjacent structures accept, lowest entity ID wins; **no acceptor / full buffer = a fault**, like any failed action (round 4) |
| `try_deposit()` → bool | 1 + action | The fault-free form (mirrors `try_send`): unloads if an acceptor has room, else returns `False` — the branching hauler's verb |
| `withdraw(kind)` | 2 + action | **The take verb** (Q79): load `kind` from any adjacent holder — Depot stock, refinery output buffer, Pump tank, dropped cargo — up to cargo capacity. **Empty holder / nothing of `kind` = a fault** (round 4). Start kit |
| `try_withdraw(kind)` → bool | 2 + action | The fault-free take: loads what's there, `False` if nothing — pairs with `try_deposit` for crash-free logistics |
| `study()` | 2 + action | **The learning verb** (Q79): adjacent to a Template Cache, root for ~10 s (tuning); completing it teaches the colony that Cache's function block. Start kit — the unlock verb can't be locked |
| `cargo_count(kind)` | 1 | How many units of `kind` in cargo (0 if none) — typed-manifest routing |
| `wander(only=None, avoid=None)` | 2 + action | A seeded random walk leg (stream `rng.wander`) — the dumb explorer; takes the Q95 paint args (`wander(only=green)` = drift inside the green zone). Start kit |
| `explore(only=None, avoid=None)` | 2 + action | The smart explorer (Q79): picks a random **currently-fogged** tile within ~15 tiles (tuning; stream `rng.explore`), walks there under the Q95 paint args, and drops into the scouting stance; resolves when the survey completes |
| `health_low()` | 1 | True iff own HP is below the bot's own `hurt_line` — the pre-handler polling idiom |
| `repair(target)` | 2 + action | Repairs structures and bots with any build tool; **field repair of a wreck needs a build tool of grade ≥ 2** (Q105-R2, restated for Q111) — **on a wreck = field repair**, the rescue verb |
| `guard(entity)` | 2 + action | Blocking stance: hold near the target **entity** (never a tile), engage perceived enemies; any signal ends it |
| `escort(entity)` | 2 + action | Follow + guard the target entity |
| `hijack(wreck)` | 2 + action | Needs a **build tool of grade ≥ 2** (Q105-R2, restated for Q111's tool model); the slowest race verb (Q84) — boots the wreck under your remainder color ([04-enemies.md](../04-enemies.md)) |
| `scan_resources()` | 8 | List of perceived + known resource nodes nearby (map knowledge included) |
| `my_quirks()` | 2 | List of this bot's **manifested** quirks (latent ones invisible); free of any unlock whenever quirks are on |
| `has_quirk(q)` | 1 | Quirk names are **pre-bound constants like kind constants** (no third builtin enum) |
| `path_blocked()` | 2 | Is the current move path obstructed by a bot? The Tier-2 corridor sensor |
| `closest(kind)` → `Result` | 4 | Generic nearest-of-kind query over what the bot perceives **and its faction knows** — everything within **seeing**, movers within **hearing**, discovered nodes from map knowledge; **structure and designation kinds answer from the faction's knowledge pool** (P22: your own — colony state, always current — plus a granting ally's own while their vision grant stands). **Foreign structures are not in the query domain** (the fog display shows that intel to the player; no program-side surface in v1 — Q126), so the canonical retreat, deposit, and build idioms can never resolve to an enemy structure; `Result.Ok(entity)` / `Result.Err(msg)` |
| `is_seen(contact)` → bool | 1 | Is this contact *seen* (full dossier) or heard-only (position, nothing else)? The chase-investigate predicate (Q80) |
| `exists(kind)` → bool | 1 | Any entity of `kind` perceived or known (seen / heard-moving / known node / pool structure — same domain as `closest`)? |
| `.expect()` (method on `Result` / `Option`) | 1 | Unwrap: `Ok`/`Some` → the value; `Err`/`None` → faults (with the carried message, for `Err`) |
| `cargo_full()` → bool | 1 | True iff the manifest is at cargo capacity |
| `attack(entity)` | 2 + action | Swing at an adjacent target — bot, structure, wreck, nest or Blight Core. A **non-adjacent swing at a seen target faults**, which is why every shipped source `move_to`s first (Q108); the exception is a **heard-only contact**, which `attack` closes to engage, resolving on sight (Q74 — see Decided). The swing issues in phase 4's combat sub-pass; **hp settles in the phase-6 damage pass** with every other attackable mass (Q102), so two blows on one target in a tick resolve by rule. Non-PvP gates it (Q76) |
| `scan_enemies()` → list | 4 | Requires Tier 5 |
| `send(ch, val, timeout=None, faction=own)` | 3 + size, blocks | Requires Tier 7; one receiver, rendezvous; `timeout=None` blocks forever, expiry faults. **Payload size caps at `payload_cap`** (~8, data — oversized faults `err_payload`, Q82), so sized costs are bounded. `timeout=None` blocks forever; inside a handler window that parks the bot in the template indefinitely, so the editor warns (see Signal handlers) |
| `try_send(ch, val)` → bool | 3 + size | One receiver or lost — the fire-and-forget distress call |
| `broadcast(ch, val, timeout=None)` | 5 + size, blocks | All blocked receivers; waits for ≥1; timeout faults |
| `try_broadcast(ch, val)` → bool | 5 + size | All blocked receivers or lost |
| `receive(ch, timeout=None, faction=own)` | 2 + blocks | Timeout expiry faults |
| `try_receive(ch)` → `Option` | 2 | `Option.Some(v)` / `None` |
| `log(val, level=info)` | 1 | Append to the local ring buffer at a level — `trace / debug / info / warn / error` (pre-bound constants, like the kind constants) |
| `upload_log()` | min(5 + size, 25) | Transmit buffer to the cloud (printers always accept), levels preserved. Cost caps at the dump's 25 (Q82 — big Memory banks never price the upload past the bank) |
| `upload_crash_dump()` | 25 | Full debug report (id, position, cargo, error, line), filed at `error` level; the error window's **factory contents** — runs on unhandled errors unless replaced |
| `abort()` | 1 | Deliberate scuttle (Q76): raises the abort sequence — forced `upload_log()` + `become_disabled()` — wreck + countdown. **Nothing player-side can call `become_disabled()` directly** (it's engine-internal); abort is the only deliberate door into Disabled, so the logs always go home, no exceptions. Giving up is always safe |
| `analyze(wreck)` | 2 + action | The **intel verb** (Q76): dissect a wreck — from **other factions' wrecks** it yields **Data**, reads logs + env snapshot, and extracts the faction's **comm key**; Feral wrecks also grant per-arcanum decryption ([04-enemies.md](../04-enemies.md)). **Your own wrecks yield nothing** (no staged Data — you already own their logs and key). Destroys the wreck. Materials or intel, pick your verb — `salvage()` is the economy one |
| `salvage(entity)` | 2 + action | Recover a cut of the wreck's **build receipt** — a fraction of every material invested in it ([02-agents.md](../02-agents.md)) — from anyone's wreck; destroys it → drops its Black Box. Salvager also gains **+N% permanent decryption of the bot's program color** (default 5%, [08-multiplayer.md](../08-multiplayer.md)) |
| `recover_black_box(entity)` | 2 + action | Pick up a Black Box and bank its contents to the cloud |
| `last_error()` → string | 1 | Most recent fault; mainly for handlers |
| `drop_cargo()` | 1 + action | Dump cargo on current tile (grabbable by others) — lightening the load is a recovery move |
| `wait(n)` | 1 + n idle ticks | Deliberate idling; the Tier-0 traffic tool (and the factory bump response) |
| `setenv(key, val)` | 1 | Set an env variable (engine-defined keys, bounded; see The Environment). Out-of-range faults |
| `getenv(key)` → int | 1 | Read an env variable; unset = the key's default, never a fault |
| `rng(n)` → int | 1 | Uniform in [0, n) from the sim's seeded stream — `wait(rng(20))` desyncs identical programs |
| `build()` | 2 + action | Work the nearest in-range blueprint (a `blueprint`-kind entity, [05-terrain.md](../05-terrain.md)), 1 progress/tick; earns Building XP |
| `search()` | 2 + action | The **scouting stance** ([05-terrain.md](../05-terrain.md)): the bot roots in place and its *seeing* circle expands one ring per N ticks (tuning) out to its survey reach (the hearing radius + stance bonuses like Ore-acle) — full sight at range, geology included; each new node discovered earns Scouting XP. **Resolves when the survey reaches full reach** (the program continues at the next line); moving or any signal ends it early |

