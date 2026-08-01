*Part of [09-quirks](../09-quirks.md).*

# The Quirk Catalog (tunable — `quirks.ron`, not commitments)

## Positive quirks

| Quirk | Effect |
|---|---|
| **Overclocked** | +1 cycle per tick |
| **Tail-Call Optimized** | loop-iteration overhead costs 1 cycle less (min 1) — a no-op at base costs; earns its keep under cost-raising overlays (Corruption, Loop Desert) |
| **Branch Predictor** | an `if` that takes the same branch it took last time costs 1 cycle less |
| **Memoized** | calling the same builtin as the immediately previous action costs 1 cycle less |
| **Lazy Evaluation** | its budget **accumulates while blocked** instead of burning (cap `bank_cap`) — the listening-post quirk: everyone else's waiting is wasted compute; this bot wakes from a long `receive` with a full tank |
| **Borrow Checker Approved** | stack depth +1 — memory-safe by construction |
| **Retina Display** | +1 sensor range |
| **Huffman Coded** | +10% cargo capacity (better packing) |
| **Production-Hardened** | +10% max HP |
| **Auto-Patcher** | passive self-repair trickle ×2 — installs its own hotfixes |
| **10x Developer** | +15% XP earned, all tracks |
| **Graceful Shutdown** | self-destruct countdown +50% — a much wider rescue window |
| **Vim User** | tool-function action time −10% — never leaves home row |
| **Hot Reload** | boot ritual half as long ([02-agents.md](../02-agents.md) stat sheet) — halves the double-handle vulnerability window on prints, rescues, and re-colorings |
| **Rubber Ducky** | `handler_init()` flinch 5 ticks shorter — talking the problem through speeds up the ritual |
| **Energy Star** | brownout reduces this bot's cycle budget by 25% instead of 50% |
| **Verbose Logging** | log ring buffer ×2 — richer black box, richer `upload_log()` |
| **Statically Typed** | unhandled faults chip half the usual HP — caught most of them at compile time |
| **Simulated Annealing** | when blocked, may sidestep to neighbors that lose up to 1 tile of ground toward the goal — escapes local optima, almost never truly boxed in |
| **Kernel Bypass** | channel `send()`/`broadcast()` cost 1 cycle less |

## Negative quirks

| Quirk | Effect |
|---|---|
| **Crypto Miner** | every Nth tick, one cycle is spent mining something for nobody |
| **Memory Leak** | stack depth −1 |
| **Deprecated Drivers** | −1 sensor range |
| **Bloatware** | −10% cargo capacity — the preinstalled junk takes up space |
| **Shipped on a Friday** | −10% max HP |
| **Tech Debt** | −15% XP earned, all tracks — the interest compounds |
| **Kernel Panic** | self-destruct countdown −50% — no graceful shutdown; rescue this one *fast* |
| **GC Pause** | every Kth action takes +1 tick — stop-the-world, deterministic counter |
| **Heisenbug** | every Mth tool action faults `tool_jam` — the bot forces you to write error handling |
| **Works on My Machine** | tool actions fault every Mth use, but *only* farther than N tiles from its home Fabricator — runs flawlessly in the demo |
| **Loud Fans** | heard at +1 range *when moving* (signature is movement noise — a stationary bot is silent, Loud Fans or not) — probably the Crypto Miner's fault |
| **Fragile Base Class** | bump collision damage taken ×2 |
| **Dial-Up** | channel `send()`/`broadcast()` cost +1 cycle |
| **Logs to /dev/null** | log ring buffer half size (cause-of-death always survives — the black box invariant holds) |
| **Abandonware** | no passive self-repair — no more patches, ever |
| **Cold Start** | first move after idling more than N ticks costs double (pairs dangerously with Dunes — the sinking clock) |
| **Off-by-One** | every Kth `move_to()` stops one tile short of the target — defensive programs re-check arrival |
| **Race Condition** | `handler_init()` flinch 5 ticks longer — always loses the race |
| **Windows Update** | boot ritual twice as long — installing updates, do not power off |
| **O(n²)** | tool-function action time +10% — it works, it just doesn't scale |
| **Merge Conflict** | the bump factory window's built-in `wait` runs +50% longer (irrelevant once you write your own `on bump:`) |
| **Stripped Binary** | `log_min_level` clamped to `warn`+ — compiled without debug symbols; this bot cannot be trace-diagnosed |

## Double-edged quirks

The most interesting shelf — whether these are good depends on the *program* the bot runs, which is exactly the point.

| Quirk | Effect |
|---|---|
| **`unsafe` Block** | +2 cycles per tick; fault chip damage ×2 — blazing fast until undefined behavior finds you |
| **Written in C** | +1 cycle per tick; stack depth −1 — fast and leaky |
| **Move Fast and Break Things** | +10% damage dealt; `hurt_line` defaults to 40 and clamps to 1–45 (later warning — the Damaged line and its penalties stay at 50%) |
| **Defensive Programming** | `hurt_line` defaults to 60 and clamps to 55–99 (an env compulsion — see [policy-quirks.md](policy-quirks.md)) — earlier retreats or wasted uptime, your handler decides which |
| **Minified** | +10% move rate; −20% max HP — stripped every byte that wasn't load-bearing |
| **Monorepo** | +25% cargo; −10% speed while loaded — everything in one place, murder to move |
| **Open Source** | salvaging this bot's wreck grants the enemy double decryption %; it prints at a discount (free as in beer, when prints cost anything) |
| **Telemetry Enabled** | +2 sensor range; every scan builtin costs +1 cycle — it's phoning home |
| **Eventual Consistency** | scan builtins cost 1 cycle less but return data that is **one additional tick stale** (everyone's queries already read last tick's perception — this bot reads the tick before that) |
| **Microservices** | channel `send()`/`broadcast()` cost 1 cycle less; every tool action costs +1 cycle — everything is a network call now |
| **Recursion Enthusiast** | stack depth +2; function calls cost +1 cycle |
| **Thermal Runaway** | +20% move speed; its wreck's blast damage is doubled (Q55 landed: every wreck explodes for real — this one just explodes *more*, one more reason to win its rescue race) |
