# Signals & Logging

*Part of [01-language](../01-language.md).*

## Signal handlers

Top-level `on <signal>:` blocks — each signal's window is its own unlockable construct ([06-progression.md](../06-progression.md)), independent of `def`. What you write is the *window*; the forced prologue/epilogue never appear in source:

```python
on error:               # window only — handler_init() runs before this, unskippable
    log(last_error())
    drop_cargo()
    upload_log()

on hurt:
    drop_cargo()
    if exists(repair_bay):              # own bays are always known (P22);
        move_to(closest(repair_bay).expect())   # the guard is for a colony with none

# there is no "on abort:" — abort is fully engine-reserved:
# forced upload_log() + become_disabled(). Your black box is
# whatever you logged while alive.
```

Rules (all deterministic):

- At most one block per signal per program. Signals are checked at operation boundaries.
- **Windows take any code** (redesign 2026-08-02, supersedes Q49/Q51): any builtin, any `def`, loops and recursion included. There is no instruction cap, no signal-safe flag, and nothing is rejected at deploy. Only `abort` and `recall` have no window at all, and that is template structure rather than a safety rule.
- **Handler risk is priced by the double-handle rule alone.** A fault — or any second signal — while a template runs forces `abort`, so code you move into a window runs under "one mistake is a wreck" instead of "one mistake costs 2 HP". Long windows are legal and expensive; that is the whole pricing.
- **The editor still tells you what it costs.** Each `def` carries its **worst-case instruction count** where that is computable (longest branch, calls expanded) and reads **unbounded** where a loop or recursion makes it undecidable. This is information, not a gate — nothing is greyed out. One case earns a deploy-time *warning* because it has no natural teaching loop: an unbounded blocking call inside a window parks the bot in a handler indefinitely, and the next signal aborts it.
- **Window size is limited by program memory, not by a cap.** Window contents are program source, so they count against the same hardware bar as the main program ([06-progression.md](../06-progression.md)) — a bigger handler is a real purchase, and handlers stay lean because memory is scarce rather than because the compiler said no.
- **Handler code is just code** — it pays per-op cycle costs and calls ordinary function blocks. Every constant here (trap cost, dump cost, `handler_init` ticks) is a cost-table entry, so biome overlays can tune them.
- **Forced calls are ordinary functions.** The engine's mandatory behaviors are implemented as calls of registry builtins: abort → `upload_log()` + `become_disabled()` (truly forced); an unhandled error runs the error window's **factory contents**, `upload_crash_dump()` — the default, not a force: replace or delete it and your choice runs instead. One code path, one cost model — engine policy "isn't even different" from player code. (`become_disabled` itself is **engine-only** — the player-facing scuttle verb is `abort()`, Q76.)
- Variables are **preserved while a handler runs** (so it can inspect state), then cleared on restart.

## Logging

- `log(value, level=info)` — append to the bot's local ring buffer (cost 1) at a **severity level**: `trace`, `debug`, `info`, `warn`, `error` — five pre-bound constants, same convention as the kind constants (ordinary shadowable names). `log(msg, level=error)` for the loud ones; bare `log(value)` stays `info`. **Buffer size is a hardware stat**: base 8 entries, grown by Memory-bank purchases at the Upgrade Station ([06-progression.md](../06-progression.md)); each entry stores its level.
- `upload_log()` — transmit the buffer to **the cloud**: the colony's printers (cost min(5 + size, 25) — capped at the dump, Q82), levels preserved. **Printers always accept log traffic** — no extra structure required, no capacity limit; if you have a printer (and a colony without one is already dying), you have telemetry. Viewable in any printer's inspector, **color-coded by level** (error red, warn amber, info neutral, debug/trace dimmed) and filterable — a colony's cloud reads like a real log aggregator. Black Boxes show levels the same way.
- `upload_crash_dump()` — the expensive one (~25): uploads a full structured debug report — **bot ID, position, inventory/cargo, error reason, faulting line, tick** — filed at `error` level. This is the error window's factory contents — what runs on unhandled errors *unless you replaced it*; players can also call it anywhere (it's just a function).

Persistent *telemetry* is player-built infrastructure — a colony with good logs is one someone programmed. But *crash* reporting has a floor **by default**: the factory error window dumps on every unhandled error, so "why is that bot blinking?" has an answer in the Archive — unless you deleted the window, in which case you chose blindness on purpose. Logs are as inspectable as everything else (transparency pillar): allies — and in PvP, anyone who `analyze()`s your wreck — can read them.

## Consequences we *want*

- **The forced crash dump is a tax on branchless code.** A Tier-0 program that blindly calls `mine()` faults when the vein is empty and pays ~25 cycles for a dump it didn't ask for — but that dump is also how a new player learns *why* the bot is stuck. The punishment is the tutorial.
- **Handlers are the graduation.** Factory dump (~25) → your own window (~5 trap + lean code of your choice): the error system itself has a skill curve. Nothing stops you moving your main loop into a handler — what stops you is that every fault in there is a wreck rather than a scratch, and that the lines cost the same scarce program memory either way.
- **Hurt's freedom is priced in risk, not cycles.** Length was never the danger — a one-instruction blocking `move_to` retreat can run for a minute, and every tick of it is another tick you can be double-handled. A slow limp to the Repair Bay is legal; it's also a bet.
- **Rescue denial is combat depth.** Every downed bot becomes a wreck, so denial is now *physical*: double-handle a retreating veteran to down it early and deep in your territory, then guard the wreck until the countdown expires — or `salvage()` it first. You always get the story (forced logs + Black Box); you don't always get the bot.
- **Fault loops are legal, visible — and lethal.** Every *unhandled* fault also chips the chassis (tuning); a program broken at line 1 crash-loops itself to death, wreck and all. Handlers are literal armor: handled faults cost no health. Debug it or bury it.
- Reading an **unset variable** is a fault. Variables survive the loop-around (Q80) but not fault/handler restarts — so state must be re-derived after every *crash or signal*, which is exactly when it might be corrupted.

