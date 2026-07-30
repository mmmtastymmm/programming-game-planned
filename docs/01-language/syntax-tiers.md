*Part of [01-language](../01-language.md).*

# Syntax by Tier

Constructs are unlocked in tiers ([06-progression.md](../06-progression.md) owns the tree; this section defines what each construct *is*).

## Tier 0 — Straight-line programs (game start)

Sequential calls to unlocked function blocks, plus **branching** — Tier 2's
`if` / `elif` / `else` is **granted at game start** rather than researched
(Q117). Branching is not a luxury here: a starter that cannot guard a
fallible query is a starter that faults every loop once the ore it can work
runs out, and Q109's fault damage turns that into a dead fleet in about
eight seconds. No state, no loops.

```python
if exists_minable(ore):
    move_to(closest_minable(ore).expect())
    try_mine()
if exists(depot):
    move_to(closest(depot).expect())
    try_deposit()
# program loops back to line 1
```

**The guard-then-query race is accepted, deliberately.** `exists_minable`
and `closest_minable` are two queries, so a node can be taken or emptied
between them and `.expect()` faults. Two things make that a fair price
rather than the bug Q110 ruled against: the two calls are *adjacent ops*
with no blocking verb between them, so the window is a tick or two rather
than the tens of ticks a blocking `move_to` opens; and it faults
*occasionally* rather than every iteration, so it costs 2 HP that passive
repair heals instead of grinding the fleet down. Binding once would need
Variables, and the starter is deliberately a *Tier-0* program.

## Tier 1 — Variables & arithmetic

```python
target = closest(ore).expect()
move_to(target)
mine()
```

## Tier 2 — Branching (`if` / `elif` / `else`) — **granted at game start**

Numbered here for concept depth, but **not researched**: branching ships with
the chassis (Q117), because the Tier-0 starter needs it to guard a fallible
query. Everything downstream of it in the unlock tree keeps Variables as its
prerequisite.

```python
if cargo_full():
    move_to(closest(depot).expect())
    deposit()
else:
    mine()
```

## Tier 3 — Loops (`while`, `break`, `continue`)

Condition loops and loop control. (`for x in container` arrives with containers in Tier 5.)

```python
while not cargo_full():
    mine()
```

`while True:` is legal but redundant — programs already loop forever implicitly. The implicit loop stays because Tier 0–2 programs need it; `while True:` exists so Python intuition doesn't fault.

## Tier 4 — User functions (`def`, `return`)

The big one: reusable subroutines, shareable across your colony as a **program library**.

**Recursion is allowed** — bounded by the bot's **call stack cap** (base **4 frames**, +4 per Stack extension bought at the Upgrade Station, see [06-progression.md](../06-progression.md)). Exceeding the cap is a stack-overflow fault: penalty + restart, like every other error. Deep recursion on stock hardware is a self-inflicted fault loop; buying stack is what makes recursive style viable.

`def` parameters follow the builtin convention: **optional parameters last, with Python-style defaults** — `def haul_to(target, drop=1):` — passed positionally or by keyword at the call site.

Every `def` also gets a **derived signal-safety** at deploy (see Signal handlers): safe iff it only calls safe things and contains no loops or recursion — safe defs are callable from handler windows at their computed worst-case instruction cost. Writing your colony's library so the recovery verbs stay signal-safe is real API design.

**Docstrings, Python-style (DECIDED)** — a leading `"""triple-quoted"""` string in a `def` body is the function's documentation: captured at parse, **stripped from the runtime body** (free — like import lines, it doesn't exist at runtime), and surfaced by the editor (hovering the function — in the file viewer or in any code window — shows it, exactly like builtin hover docs). Triple-quoted strings may span lines and take their content raw (no escapes, literal newlines); elsewhere they're ordinary string values. A docstring alone is a legal (documented, do-nothing) body. The starter `hauling` module ships with one, so the idiom is taught by example.

```python
def haul_home():
    """Take the cargo home: nearest depot, then deposit."""
    move_to(closest(depot).expect())
    deposit()
```

## Tier 5 — Collections & iteration (lists, dicts, `for x in xs`)

Python-style containers and iteration (no C-style index loops; `range(n)` / `range(a, b)` is a container builtin here, capped by the `range_cap` cost entry). `break`/`continue` work in `for` exactly as in `while`.

- **Lists**: `[a, b]` literals, `xs[i]` (negative indices count from the end; out of range faults), `xs[i] = v`, `xs.append(v)`, `x in xs`, `len(xs)`.
- **Dicts**: `{k: v}` literals; keys are int, string, or entity — **entity keys are the headline**: per-target state like `seen[enemy] = tick`. `d[k]` faults on a missing key (Python's KeyError); `d.get(k)` is the fault-free form, giving `Option.Some(v)` / `Option.None`. `d[k] = v` inserts or overwrites, `d.remove(k)` deletes (returning the Option), `k in d` tests membership, `d.keys()` / `d.values()` give lists.
- **Dict iteration order is sorted key order, always** — never insertion order (deterministic by construction, CLAUDE.md rule 3). `for k in d:` walks keys sorted; so do `.keys()` / `.values()`.
- `in` also works on strings (substring test). Containers are **values**, not references — see Types.

```python
threats = scan_enemies()
seen = {}
for t in threats:
    seen[t] = t.distance          # entity-keyed dict
    if t.distance < 10:
        alert(t)
if len(seen) > 3:
    retreat()
```

## Tier 6 — Enums & `match`

Rust-style sum types in Python clothing: variants may carry associated data, and `match` destructures them. Arms are checked top-to-bottom, first match wins.

```python
enum Order:
    Idle
    Mine(target)
    Guard(post, radius)

match current_order:
    case Order.Mine(target):
        move_to(target)
        mine()
    case Order.Guard(post, radius):
        move_to(post)
    case Order.Idle:
        wander()
```

Enum values are first-class: storable in variables and lists — and **sendable on channels** (Tier 7), which is the real payoff: colonies develop *typed command protocols*.

## Tier 7 — Channels (inter-bot messaging)

**Blocking channels.** Any value — int, entity, list, enum — can travel a named channel. The API is a 2×2: **delivery** (one receiver vs. everyone) × **send mode** (block until heard vs. fire-and-forget):

| | Blocks until delivered | Fire-and-forget |
|---|---|---|
| **One receiver** | `send(ch, val, timeout=None)` — rendezvous handoff to exactly one receiver | `try_send(ch, val)` → bool — delivers to one blocked receiver, else the message is **lost** |
| **All receivers** | `broadcast(ch, val, timeout=None)` — blocks until ≥1 receiver, then all blocked receivers get a copy | `try_broadcast(ch, val)` → bool — copies to all currently blocked, else **lost** |

Receive side: `receive(channel)` **blocks** until a message arrives (`timeout=None`, the default, means forever); `receive(channel, timeout=ticks)` blocks up to the timeout, then **faults** (timeouts are ordinary faults — write an `on error:` window); `try_receive(channel)` returns an `Option` — `Option.Some(v)` or `None` — for non-blocking polls. Blocking sends time out the same way: fault, handle it or don't.

```python
on error:
    upload_log()        # a timeout landed here

order = receive("orders", timeout=100)   # block up to 100 ticks
match order:
    case Order.Mine(target):
        move_to(target)
        mine()
```

Semantics (deterministic):

- **No queues, no mailboxes**: messages exist only in the instant of delivery. Fire-and-forget with nobody blocked = message gone. Persistent listening posts are something you build out of bots.
- **One-receiver selection**: the longest-blocked receiver on the channel wins; ties break by lowest entity ID.
- **Blocking consumes cycles.** A blocked bot (send *or* receive) executes nothing else, and its per-tick cycle budget burns while it waits — waiting *is* what its CPU is doing. No banking cycles, no free listening posts: a bot blocked for 100 ticks spent 100 ticks of compute on patience. Handlers still fire while blocked — with the usual double-handle stakes.
- Channels are names (strings); the namespace is per-faction but **allies can be granted channels** (shared-library-style), enabling cross-colony coordination.
- **Foreign channels require a comm key.** Knowing a channel's *name* (from decrypted code — player or Feral) is not enough — every faction's traffic is keyed. Extract a key by **`analyze()`-ing any faction's wreck** (the intel verb, Q76 — one rule for keys); with key + name you can `receive` (eavesdrop / steal) and `send` (spoof) on their channels — the channel verbs take an optional **faction argument** (`receive("intruder", faction)`, using the per-match faction constants) to address a foreign namespace you hold the key for (round 4). Reading is reconnaissance; interacting takes fieldwork.
- Corruption jams channel traffic in/out ([05-terrain.md](../05-terrain.md)) — a blocking `send` from inside Corruption faults on timeout like anything else. **Cloud telemetry is exempt** (Q76): the jam blocks the channel verbs, never `upload_log()`/crash dumps — the logs always go home.

