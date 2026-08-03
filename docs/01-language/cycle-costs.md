*Part of [01-language](../01-language.md).*

# Cycle Costs (base table — moddable per map/biome)

The cost table is **data, not code** (`costs.ron`, see [07-architecture.md](../07-architecture.md)). Maps and biomes ship **overlays** that override any entry, so terrain can stress *program designs*, not just stats: a biome where loop overhead triples punishes iteration-heavy code; one where `send` is cheap invites swarm coordination. Corruption's cycle tax ([05-terrain.md](../05-terrain.md)) is just the first shipped overlay.

Base values:

| Operation | Cost (cycles) | Notes |
|---|---|---|
| Simple statement / no-op line | 1 | |
| Built-in function call | full charge | **Function-table entries are full charges** (Q80) — the call statement's overhead is folded into the listed number: one figure per function, and every quoted cost in prose is the real price (`closest` is 4, the dump is 25, Corruption's max is 26) |
| Methods, operators, properties | 1–2 | **Core language, not registry entries** (round 4): `.expect()`, container methods (`.append`, `.get`, `len`, `range`), boolean ops (`not`/`and`/`or`), and entity property reads (`.distance`, …) are priced here (property/bool 1, method 1–2) and exempt from the acquisition rule — they come with their construct tier |
| Payload size units | — | For sized costs (`send` 3 + size, etc.): int / bool / entity / bare enum = 1; string = its length; list or enum-with-data = 1 + elements/fields. All bounded by `payload_cap` (Q82) |
| Variable read | 0 | Reads are free; storage is the cost. (Not an *operation* — the Q75 ≥1 floor governs executable ops; sub-expression reads sit inside them) |
| Variable assignment | 1 | |
| Arithmetic op (`+ - * // %`) | 1 per operator | |
| Comparison (`== < >` etc.) | 1 | |
| `if` / `elif` evaluation | 1 + condition cost | |
| Loop iteration overhead | 1 per iteration | The "loop tax" — rewards flat code where possible |
| User function call (`def`) | 2 + body | Call overhead; inlining is a real optimization |
| List index / append | 1 | |
| `send()` / `try_send()` | 3 + payload size | Communication is expensive on purpose |
| `broadcast()` / `try_broadcast()` | 5 + payload size | Reaching everyone costs more |
| `receive()` issue | 2, then blocks | Blocking wait; timeout expiry is a fault |
| `match` | 1 + 1 per arm checked | Arms checked top-to-bottom; destructuring bind is free |
| Enum construction | 1 | `Order.Mine(target)` |
| **`upload_crash_dump()`** | 25 | Force-called on unhandled errors; also player-callable |
| **Trap cost** | 5 | Paid to enter the `error` handler on a *fault* (signal-raised entries — hurt/bump/bumped — skip it) |
| ~~**Window caps**~~ | — | **Deleted 2026-08-02** with signal-safety: windows have no instruction cap. Handler *time* is priced by double-handle risk and handler *length* by program memory, so no cost entry remains |
| **`bank_cap`** | flat (~100 cycles) | Max banked cycle budget — a constant, with a **load-time** check that no key's worst-case effective cost (overlays applied to base *plus* the largest per-bot delta) exceeds it (Q75/Q82, reshaped by Q101). Clamped after each grant (see Execution Model) |

Design intent: **cycle costs are the balance dial.** Complex behavior should be *possible* early but *slow*, so hardware upgrades and code golf both feel rewarding.

