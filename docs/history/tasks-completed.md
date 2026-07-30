# Completed milestones (archive)

M0–M3, moved out of [TASKS.md](../TASKS.md) on 2026-07-29. These milestones are
fully complete — every item checked, nothing awaiting a decision — and are kept
for provenance: what was built, when, and which judgment calls were made along
the way.

The few notes in here that still bind on unbuilt work were copied forward to
**Carried forward from completed milestones** in [TASKS.md](../TASKS.md); this
file remains their full context. Later completed milestones (M4–M7, M10–M15)
stay in TASKS.md because they carry open items or unresolved discussion notes.

---

## M0 — Test & data groundwork ✅ COMPLETE (2026-07-15)

- [x] **Serde on `Command` + serialized `(seed, command log)` replay artifact.** `sim::replay`
      module (`Replay { spec, commands, ticks }` ↔ RON); golden fixture checked in at
      `crates/sim/tests/golden/` (a 300-tick scenario exercising every Command variant,
      printer prints/boots, a mid-run hot-swap, sidestep RNG, and a kill); regenerate with
      `UPDATE_GOLDEN=1 cargo test -p sim --test golden` and explain the hash change in the PR.
      CI added (`.github/workflows/ci.yml`, sim+pyrite tests). *Note: no rustfmt gate — the
      tree has pre-existing fmt drift; add one after a dedicated whole-tree `cargo fmt`
      commit.* [sim] (M)
- [x] **Cross-process replay test** — `cross_process_replay_matches` re-runs the golden
      replay in a spawned process and compares final hashes. [sim] (S)
- [x] **Extract tuning to data files**: `crates/sim/data/tuning.ron` +
      `crates/pyrite/data/costs.ron` (values verbatim, `include_str!` + RON parse,
      `deny_unknown_fields`, load-time validation asserts). *Note: `stats.ron` deferred to
      M5 — no stat sheet exists yet to extract; the printed_* chassis defaults stay in
      tuning.ron until then.* [pyrite][sim] (S)
- [x] **Named RNG streams**: `World.rng: RngStreams` (combat / wander / explore / sidestep /
      quirk_roll / feral_mutation, each seeded from (match seed, stream name)) + per-bot
      `BotData.rng_program` seeded by (match seed, entity ID), feeding the `rng()` builtin.
      *Judgment call to review: death cargo-spill scatter draws from `rng.combat` — that use
      isn't in docs/07's inventory; flagged in a code comment.* [sim] (S) ⚠HASH
- [x] **Program versions = source-byte hashes**: `ColorProgram.hash` (FNV-1a over source
      bytes) replaces `version: u32`; `World.program_library: BTreeMap<hash, source>` retains
      every deployed version; the editor shows short hashes. [sim] (S)

## M1 — Language core: cost model & semantics cluster ✅ COMPLETE (2026-07-15)

Landed as one change set with one golden-fixture regeneration (the hash explanation:
full charges + centicycles + wrap-surviving variables move every replay hash at once).

- [x] **Full-charge cost convention** (Q80): `call_base` deleted; registry figures are total
      prices (`closest` = 4, `mine` = 2); a bare-call statement pays only the call's figure
      (the statement overhead is folded in). [pyrite] (S) ⚠HASH
- [x] **Centicycle storage** (Q56/Q75): budgets/debt stored ×100 (`CENT`), table entries stay
      whole cycles, converted at charge time; `Vm::budget()` returns centicycles (the HUD
      divides for display). [pyrite][sim] (S) ⚠HASH
- [x] **Variables survive the loop-around** (Q80): the wrap keeps globals; fault/handler
      restarts (and redeploys landing at the wrap) clear them. Tests inverted. [pyrite] (S) ⚠HASH
- [x] **Grace-window/overtime tax deleted** (`grace_window_ticks`, `overtime_mult`,
      `adjusted()`, the handler tick clock) — per-signal caps replace it in M3. [pyrite] (S) ⚠HASH
- [x] **Payload-sized costs**: `CostSpec::{Fixed, PlusPayload, LogSized}`;
      `Value::payload_units()` (int/bool/entity/bare-enum 1, string = length, containers
      1 + contents recursively); `send`/`broadcast` price + payload; `upload_log` =
      min(5+buffer, 25) via a new `Host::log_len()` hook; `payload_cap` 8, oversize faults
      `err_payload` before the host sees the call. *Judgment call: the doc's "1 + elements/
      fields" was read as recursive units so nesting can't smuggle bulk — flag if you meant
      flat counts.* *Note: `blackbox_budget` 10→20 so the factory death report (log + full-
      buffer upload at new prices) still fits; the field dies in M3 (abort's upload charges
      as debt).* [pyrite][sim] (M) ⚠HASH
- [x] **Keyword args & optional defaults**: `f(a, key=v)` parses (positionals-first, Python
      rules); `def f(a, b=5)` with literal defaults (trailing-defaults enforced); user defs
      and registry builtins bind by name with defaults filled; the host always receives the
      canonical positional form (`log` always gets `[val, level]`). [pyrite] (M)
- [x] **`None` reserved** = `Option.None` (assignment is a parse error; `case None:` sugar;
      `Option.Some(v)` / `Result.Ok/Err` constructible from source). [pyrite] (S)
- [x] **Fault-id constants**: `pyrite::faults` registry (err_type / err_name /
      err_unknown_function / err_arity / err_stack / err_index / err_key / err_div_zero /
      err_overflow / err_no_match / err_expect / err_range / err_payload / err_control /
      err_action / err_timeout), auto-bound as VM constants; every fault site carries an id;
      `HostCall::Fault(Fault{id, msg})`; `last_error()` returns the id constant (the message
      still rides in `Signal.Error(msg)` and crash dumps). *Judgment call: the language-level
      id list is my drafting — docs only name examples; ratify or trim before it fossilizes.
      Host-domain ids (err_tool_jam, err_unknown_contact) land with their systems (M4/M7).*
      [pyrite][sim] (M) ⚠HASH
- [x] **Match arity fall-through** (Q80): name+variant+arity is the identity; wrong arity is
      a non-match that falls to the next arm, not a fault. [pyrite] (S) ⚠HASH
- [x] **Function registry as data**: `pyrite/data/builtins.ron` — name → (cost, signal_safe,
      params+defaults, signature, summary, cost_note) for the FULL docs/01 table, including
      not-yet-implemented verbs (calling one faults err_unknown_function until its system
      lands). Replaces sim's `BUILTIN_DOCS`; editor hover reads it (`builtin_doc(costs, name)`
      + `cost_display`); `signal_safe` recorded for M3's static checks. [pyrite][sim] (M)

## M2 — Nine-phase tick skeleton ✅ COMPLETE (2026-07-15)

- [x] **Reorder `Sim::step()` into the nine phases** (07): Commands → VM step → collect →
      resolve → **Perception (5, stub)** → damage/countdowns/blasts (6) → **XP settlement (7)**
      → economy (8, regen moved in) → snapshot hash (9, stored as `Sim.last_hash` for the
      lockstep relay). Damage moved out of inline resolution (attack, bump crunch, fault chip
      all queue to `pending_damage`, settled 6a); XP credits queue to `pending_xp`, settled
      phase 7 under an identity Learning multiplier (M6b makes it real — awards for bots that
      died in phase 6 drop with them). Phase-0 perception seed hook at match start.
      *Note: the ⚠HASH toll wasn't owed — end-of-tick states came out identical in the golden
      scenario (the reorder only moves work within a tick), so the fixture stands unchanged.*
      [sim] (M)
- [x] **Severity-order co-arrival**: signals queue to `pending_signals`, dispatched once per
      bot at the phase-6 op boundary; `Signal::severity()` orders abort > error > recall >
      hurt > bumped > bump (Death holds the reserved top tier until M3's abort; error is sync
      and never queued; gaps left for M3's ranks), extras dropped; co-arrival ≠ double-handle
      (Q81) — regression-tested (`co_arriving_signals_resolve_by_severity_not_double_handle`:
      under the old immediate-raise code that scenario exploded the bot). [pyrite][sim] (M)
- [x] **Spatial index** (bots per tile): `World.occupancy: BTreeMap<pos, BTreeSet<id>>`, kept
      in sync by `index_bot`/`unindex_bot`/`move_bot` at every spawn/move/death/scrap/explode;
      `tile_occupied`, the bump blocker lookup, and both replan obstacle sets read the index
      (`occupied_tiles`). [sim] (S)

*Audit follow-ups (2026-07-15 M1–M4 verification) — swept 2026-07-26: the sub-order is FIXED
(Q102 first half, below); inline structure damage is Q102's open second half; the
hash-shallowness was fixed by the 2026-07-16b review (`hash_bot_data` covers all in-flight
state):*
- [x] *Phase-4 sub-order* — **done 2026-07-26 (Q102)**: phase 4 runs docs/07's three passes
  (move → combat → work; engine walks ride the move pass; pass classification snapshotted at
  phase entry so no bot acts twice). Combat now sees a settled world — a measured artifact
  (same fight, 90 hp attacker-first vs 100 hp victim-first) is gone, guarded by
  `combat_outcome_does_not_depend_on_spawn_order`. ⚠HASH, golden regenerated.
- [x] *Structure damage inline in phase 4* — **done 2026-07-26 (Q102, second half)**:
  `PendingDamage` carries a `DamageTarget` (bot / structure / nest / blight / wreck), so one
  phase-6 settle owns every hp change, XP credit, and destruction (deferred to the end of the
  drain). Two blows on one mass in a tick no longer fault the higher-id attacker — measured
  before the fix (1 fault + 5-hp chip), guarded by
  `a_felled_structure_does_not_punish_the_other_attacker`, verified against the old code.
  Golden unchanged (the fixture exercises none of the touched paths).
- *Phase-9 hash is shallow on in-flight state*: `bot.data.requested`, `bot.data.action`
  (path/ticks/goals) and the recall path aren't hashed — a peer divergence there stays
  invisible until a position changes. (Shallow VM hashing is already a known TODO.)

## M3 — Signals v3: the seven-template model ✅ COMPLETE (2026-07-15)

- [x] **Per-signal reserved templates**: `on error/hurt/bump/bumped/boot:` player windows
      (`SignalKind` reshaped; `on signal(s):`/`on death:`/`SignalKind::Death` deleted);
      `abort`/`recall` fully reserved — writing them is a parse error. Every signal ALWAYS
      enters its sandwich: forced `handler_init()` prologue (boot: forced `upload_log()` when
      the buffer is non-empty), then the player window or its FACTORY contents (error:
      `upload_crash_dump()`, bump: the `wait(35)` stun; hurt/bumped/boot ship empty — the
      flinch is the reaction), then restart at line 1. `RaiseOutcome::Ignored` is gone for live
      bots — nothing is unhandled, just uncustomized. Black box = whatever you logged while
      alive (wrecks carry leveled logs + env snapshot for M10's drop). *Note: the tuning field
      `bump_victim_freeze_ticks` died — the victim stagger IS the flinch.* [pyrite][sim] (L) ⚠HASH
- [x] **`abort()` verb** — the only player scuttle: VM-intercepted, runs the fully reserved
      sequence (forced `upload_log()` charged as debt → `become_disabled()`), un-interruptible,
      absorbs signals afterwards. `become_disabled` is off the registry (player calls fault
      err_unknown_function; the host arm stays engine-only). `KillBot` kept, doc'd dev-only
      (the replay fixture exercises it). [pyrite][sim] (S) ⚠HASH
- [x] **Double-handle → abort**: `explode()`, `Outcome::Exploded`, and `State::Exploded` are
      gone — a signal or fault landing on ANY running template (factory contents included,
      Q50 — the humble-defaults carve-out is deleted) or engine context forces abort; the bot
      wrecks where it stands. No instant-destroy path exists. [pyrite][sim] (M) ⚠HASH
- [x] **Recall via the signal system**: `Signal::Recall` (severity 4) — `raise` interrupts
      Running AND Blocked, records the engine context, and double-handles mid-template;
      engine-fired selection (rebalance + scrap) now also skips **mid-template** bots, not just
      booting/recalling ones (Q85 — scrap re-selects the next-lowest). *Judgment call: the walk
      home stays an engine state machine rather than a literal Pyrite `move_to(home_printer)`
      program on the VM — observable semantics match the doc; flagged for discussion.*
      [pyrite][sim] (M) ⚠HASH
- [x] **Per-signal instruction caps + `signal_safe`**: `pyrite::analysis::check_windows` at
      deploy (sim `DeployProgram`/`SpawnBot` + the editor's live parse) — worst-case statement
      counts (longest branch; user-def calls charge their deploy-computed worst case),
      signal-safe-only calls from the registry flag (defs derive; methods exempt), loop +
      recursion ban window-reachable. Caps live in costs.ron (`window_cap_error` 8 / hurt 6 /
      bump 4 / bumped 4 / boot 4). [pyrite] (L)
- [x] **Unlock surgery**: `OnError`/`OnHurt`/`OnBumpBumped` (one unlock for both, per 06's
      tree)/`OnBoot` replace `OnSignal`/`OnDeath`; `Import` its own construct (gates both
      import forms); `Channels` added (syntax lands M11). [pyrite] (S)
- [x] **Run-state enum to 07's shape**: `RunState { Running | Faulted | Blocked |
      Template{signal, flinching} | Boot | Recall | PadSit | Disabled }` as `Vm::run_state()`
      — a projection the clouds/tests/inspector switch on (Blocked's channel variant lands
      M11; PadSit is wired but unreachable until M5). [pyrite] (S)
- [x] **Editor**: one file per signal window assembling to `on <signal>:` blocks (the unified
      `match s:` splicer deleted); sandwich rendered as locked phantom prologue/epilogue lines;
      live cap meter (worst-case/cap, red on overrun) in the window chrome and file-viewer
      outline; signal-safe verdict on hover docs; deploy checks run in the live parse; thought
      clouds switch on `run_state()` with the skull for abort/disabled. [game] (M)
- [x] **Env registry**: `setenv`/`getenv` host arms over `ENV_KEYS` (`hurt_line` 1–99, default
      = tuning `hurt_line_pct`; `log_min_level` 0–4) — unknown key faults err_key, out-of-range
      err_range, unset reads default; `hurt_line` read live by the hurt latch, regen re-arm,
      and `health_low()`; env snapshot rides wrecks (→ M10 black boxes) and the state hash.
      [pyrite][sim] (S) ⚠HASH
- [x] **Log levels**: `log(msg, level=info)` with `trace…error` pre-bound INT constants (ints
      so the same names work as env values); below-`log_min_level` entries discarded at the
      call (cost still paid); ring buffer, wrecks, black boxes, and archive entries all carry
      the level; the inspector prints `[level]` prefixes. [pyrite][sim] (S)
