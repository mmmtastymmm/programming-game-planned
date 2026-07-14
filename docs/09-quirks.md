# Bot Quirks (brainstorm)

**Status: proposal — nothing here is Decided.** Open questions live in [QUESTIONS.md](QUESTIONS.md) (Q44–Q48).

A **quirk** is a small per-bot deviation from the universal chassis spec ([02-agents.md](02-agents.md)) — a positive or negative "personality" of the individual machine. With chassis classes gone, quirks and XP are the *only* things making two prints differ. Two bots printed from the same Fabricator, running the same color, are no longer perfectly identical. Quirk names are programming jokes on purpose: the audience writes code, and a joke name that *explains its own effect* (Memory Leak, Cold Start) is free documentation.

## Design rules (what keeps quirks on-pillar)

1. **Quirks bend the hardware, never the program** (pillar 1). A quirk may change stats, costs, timings, ranges, or *where a signal line sits* — it never makes a decision, overrides an instruction, or moves the bot on its own. The player's code stays the only brain.
2. **Deterministic by construction** (pillar 4). Quirks are rolled from a named, seeded RNG stream (`quirk_roll`) advanced only by print events. "Every Nth" in any quirk below is a deterministic counter, never a random check.
3. **Data-driven.** Quirk ids, rarity weights, and effect magnitudes live in `quirks.ron`. Every number below is a tuning constant.
4. **Programs can read them.** Builtins `my_quirks()` / `has_quirk(Quirk.X)` let one color program adapt per-bot (`if has_quirk(Quirk.DeprecatedDrivers): stay closer to escort`). This is the payoff: quirks turn "one program, many bots" into a real programming problem instead of cosmetic noise.
5. **Losses hurt more** (pillar 3). A veteran isn't just XP — it's *this body*, with *these quirks*. Reprinting rerolls quirks: destruction can cost you a lucky roll on top of the levels.
6. **Quirks touch runtime stats only.** Every quirk modifies a row of the stat sheet ([02-agents.md](02-agents.md)) or lays a per-bot overlay on the cost table — never a *deploy-time* validation stat (program memory, variable slots, handler window caps). A color program that deploys must deploy to **every** bot of its color; a quirk that could make one bot reject the colony's code would break that (Q52).

## Positive quirks

| Quirk | Effect |
|---|---|
| **Overclocked** | +1 cycle per tick |
| **Tail-Call Optimized** | loop-iteration overhead costs 1 cycle less (min 1) |
| **Branch Predictor** | an `if` that takes the same branch it took last time costs 1 cycle less |
| **Memoized** | calling the same builtin as the immediately previous action costs 1 cycle less |
| **Lazy Evaluation** | unused cycles carry over to the next tick (banked, cap N) |
| **Borrow Checker Approved** | stack depth +1 — memory-safe by construction |
| **Retina Display** | +1 sensor range |
| **Huffman Coded** | +10% cargo capacity (better packing) |
| **Production-Hardened** | +10% max HP |
| **Auto-Patcher** | passive self-repair trickle ×2 — installs its own hotfixes |
| **10x Developer** | +15% XP earned, all tracks |
| **Graceful Shutdown** | self-destruct countdown +50% — a much wider rescue window |
| **Vim User** | tool-function action time −10% — never leaves home row |
| **Hot Reload** | boot ritual half as long ([02-agents.md](02-agents.md) stat sheet) — halves the double-handle vulnerability window on prints, rescues, and re-colorings |
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
| **Loud Fans** | enemies sense this bot at +1 range — probably the Crypto Miner's fault |
| **Fragile Base Class** | bump collision damage taken ×2 |
| **Dial-Up** | channel `send()`/`broadcast()` cost +1 cycle |
| **Logs to /dev/null** | log ring buffer half size (cause-of-death always survives — the black box invariant holds) |
| **Abandonware** | no passive self-repair — no more patches, ever |
| **Cold Start** | first move after idling more than N ticks costs double (pairs dangerously with Sand, Q35) |
| **Off-by-One** | every Kth `move_to()` stops one tile short of the target — defensive programs re-check arrival |
| **Race Condition** | `handler_init()` flinch 5 ticks longer — always loses the race |
| **Windows Update** | boot ritual twice as long — installing updates, do not power off |
| **O(n²)** | tool-function action time +10% — it works, it just doesn't scale |
| **Merge Conflict** | unhandled bump stuns last +50% longer |
| **Stripped Binary** | `log_min_level` clamped to `warn`+ — compiled without debug symbols; this bot cannot be trace-diagnosed |

## Double-edged quirks

The most interesting shelf — whether these are good depends on the *program* the bot runs, which is exactly the point.

| Quirk | Effect |
|---|---|
| **`unsafe` Block** | +2 cycles per tick; fault chip damage ×2 — blazing fast until undefined behavior finds you |
| **Written in C** | +1 cycle per tick; stack depth −1 — fast and leaky |
| **Move Fast and Break Things** | +10% damage dealt; `hurt_line` defaults to 40 and clamps to 1–45 (later warning — the Damaged line and its penalties stay at 50%) |
| **Defensive Programming** | `hurt_line` defaults to 60 and clamps to 55–99 (an env compulsion — see *Policy quirks ride the environment*) — earlier retreats or wasted uptime, your handler decides which |
| **Minified** | +1 speed tier; −20% max HP — stripped every byte that wasn't load-bearing |
| **Monorepo** | +25% cargo; −10% speed while loaded — everything in one place, murder to move |
| **Open Source** | salvaging this bot's wreck grants the enemy double decryption %; it prints at a discount (free as in beer, when prints cost anything) |
| **Telemetry Enabled** | +2 sensor range; every scan builtin costs +1 cycle — it's phoning home |
| **Eventual Consistency** | scan builtins cost 1 cycle less but return data that is 1 tick stale |
| **Microservices** | channel `send()`/`broadcast()` cost 1 cycle less; every tool action costs +1 cycle — everything is a network call now |
| **Recursion Enthusiast** | stack depth +2; function calls cost +1 cycle |
| **Thermal Runaway** | +20% move speed; when its wreck's countdown expires, the explosion damages adjacent tiles — friend or foe (one more reason to win this rescue race). *Assumes Q55 lands with real blast damage; if wreck explosions stay cosmetic, this needs a new drawback* |

## Policy quirks ride the environment

Some quirks need no stat plumbing at all. Any quirk whose effect is "*when* does an engine behavior fire" is really a modified **env registry** entry ([01-language.md](01-language.md), The Environment): Defensive Programming is just a bot that ships with a higher `hurt_line`. Two strengths, chosen per quirk (Q60):

- **Temperament — a shifted default.** The key's *default* changes (unset `hurt_line` reads 60, not 50). Programs that never touch the key inherit the personality; one `setenv` in the boot window overrides it entirely. Temperaments tax only unwritten code — the quirk is real on day one and evaporates under a good dotfile, which is about as "code is the game" as a quirk can get.
- **Compulsion — a clamped range.** The key's legal *range* narrows (`hurt_line` 55–99). Proposed semantics: `setenv` past an *engine* bound still faults (that's a program bug, identical on every bot), but `setenv` past a *quirk* clamp **clips** quietly — the hardware refuses, deterministically, and `getenv` reports where the value actually landed. One color program stays valid on every bot; the compelled bot just can't be talked out of its fear.

Every future env key is free quirk surface — the registry is the natural home for personality, and `getenv` doubles as quirk introspection for these (relevant to Q48).

## Acquired quirks (beyond the print roll)

- **Scars.** Each field-repair rescue may add a scar (a negative quirk) — rescued veterans come back with a limp. Sharpens the existing triangle: rescue keeps XP but accumulates scars; reprint is clean but forfeits XP *and* rerolls the good quirks.
- **Merits.** Hitting L5 on a track grants that track's mastery quirk — deterministic, earned, and one more reason a veteran is irreplaceable:

  | Track (L5) | Merit | Effect |
  |---|---|---|
  | Mining | **Ore-acle** | resource nodes visible at +2 range |
  | Hauling | **CDN** | +10% move speed while loaded — content, delivered |
  | Combat | **Aimbot** | +5% damage dealt |
  | Building | **Infrastructure as Code** | repairs restore +25% more |
  | Scouting | **Wardriver** | +1 sensor range |

- **Corruption exposure.** Long dwell time on corrupted tiles can grant *corrupted quirks* — always double-edged (e.g. **Promiscuous Mode**: overhears fragments of Feral channel traffic, but enemies sense this bot at +1 range — the interface reads every packet, and every packet reads it back). Ties quirks into terrain (pillar 5).
- **Hijacked bots keep their quirks.** A stolen quirky veteran is even more of a unique prize (Q30/Q33).

## Visibility & intel

XP levels are visible to everyone as wear/decals (pillar 2); the transparent-by-default answer is that quirks show the same way — a Loud Fans bot audibly *is* loud. Whether enemies get the exact list for free or via decryption % is open (Q47). `my_quirks()` on your own bots is always free.

## Determinism & data notes

- Roll at print time, `quirk_roll` RNG stream, weighted by `quirks.ron` rarity.
- Reprint = new body = new roll. Recall/re-coloring and rescue keep the body, so quirks persist (like XP, they live on the bot, not the color).
- Golden-replay note: introducing quirks changes state hashes; land behind a match setting first (Q44).
