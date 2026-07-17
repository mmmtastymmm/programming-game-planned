# Implementation tasks: making the design real

Generated 2026-07-15 from a three-crate audit against docs/00–09 (post round-5 verification).
Status shorthand: **[pyrite] [sim] [game]** = crate(s) touched; **⚠HASH** = changes golden-replay
hashes (per CLAUDE.md, the PR must say why); **(S/M/L)** = small/medium/large.

## Where the code stands

The crates are a clean, well-tested implementation of the *round-1/2* design. The determinism
discipline is intact everywhere (BTreeMap world, command-only mutation, no floats, seeded RNG,
stable tie-breaks), and the game crate has zero architecture violations — all mutation already
flows through `Command`s. What's stale is the *design generation*: generic ore instead of 11
raws → 7 refined, `desired_max` dials instead of target shares, one unified `on signal(s):`
handler instead of seven per-signal templates, instant-explode double-handle instead of abort,
omniscient sensing instead of the seeing/hearing model, 4 inline XP tracks instead of 5+6
settled tracks. Each of those lands as a replay-hash change — and since M0 landed, stored golden fixtures
make every one of them pay the explain-your-hash-change toll the docs prescribe
(`UPDATE_GOLDEN=1` regenerates; the PR explains why).

Milestones are dependency-ordered. Within a milestone, tasks are roughly sequenced. Milestones
marked ∥ can proceed in parallel once their prerequisites land.

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

*Audit follow-ups (2026-07-15 M1–M4 verification), NEEDS DISCUSSION:*
- *Phase-4 sub-order*: docs/07 says "resolve actions (move → combat → mine/build)"; the code
  resolves PER BOT in id order (deterministic, but a lower-id attacker range-checks a
  higher-id mover pre-move while the reverse pairing sees post-move). Reconcile doc or code.
- *Structure damage is still inline in phase 4* (`actions.rs` attack arm): only bot damage
  rides `pending_damage` to phase 6. Deterministic, but contradicts "damage is a phase".
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

## M4 — Typed resources & economy ✅ CORE COMPLETE (2026-07-15) — discussion items below

- [x] **11 raws → 7 refined as first-class kinds** (`sim::resources`): typed per-faction colony
      stock + typed cargo manifests, all deci-units; nodes ride the nine resource-ground tiles
      (+ legacy OreVein→Iron, CrystalField→Crystal); Grove regenerates (per-node-type flag,
      `node_regen_deci` per regen interval); `mine()` yields the node's kind
      (`mine_yield_deci` 20 = the 2/swing manifest); `stockpile_ore`/`OreNode` retired
      (`starting_ore` seeds Iron for old specs; `starting_stock` is the typed kit). [sim] (L) ⚠HASH
- [x] **Generic `Structure { kind, faction, pos, hp, input, output, recipe, batch }`** for
      Smelter/Foundry/Archive (placed by `PlaceStructure`, typed docs/03 prices from stock);
      solid, attackable, fall at 0 HP. *NEEDS DISCUSSION: printers/depots staying separate —
      Printer carries color/job/dial state M9 reworks anyway, and Depot is load-bearing in the
      deposit path; migrating them into Structure now churns M9's ground. Also: structures
      place instantly (blueprint-labor for structures wasn't specced — Bridge keeps its
      blueprint flow).* [sim] (M) ⚠HASH
- [x] **Smelter + Foundry** running the full docs/03 recipe book (`resources::RECIPES`:
      steel/bronze/glass at the Smelter, wire/chips/lens/gold_chip at the Foundry),
      `SetRecipe` command (validates station, scraps the in-flight batch), physical
      input/output buffers bots feed and empty, phase-8 batch timer (`recipe_batch_ticks`
      ~30), lowest-ID acceptor/source tie-breaks. Energy gating lands with M5. [sim] (L)
- [x] **Re-priced typed**: Bridge + overlays in Stone (faction-paid placement commands),
      printer repair 60 Data, print cost in Steel (default free), scrap refund Steel. *NEEDS
      DISCUSSION: tool-ladder + build-tool-Steel pricing belongs to M5's tool modules (no
      tools exist to price yet) — the tier data (`Resource::tool_tier`) is in place,
      unenforced.* [sim] (M) ⚠HASH
- [x] **Data currency** per faction: first hostile kill (10), delivery milestones (20 per 500
      units — depot deposits only, minus each bot's stock-withdrawn cargo via PER-BOT
      PROVENANCE (`withdrawn_aboard`), paid against a high-water mark: cycling and refinery
      feeds mint nothing, and spending seeded stock never suppresses real income; review
      rulings 2026-07-15/16), printer-repair sink; `Research { faction, construct }` command spends Data on
      docs/06's price tree; per-faction UnlockSets consumed at parse (`MapSpec.
      dev_all_unlocks`, default true, keeps sandboxes/tests/replays on the old behavior).
      *NEEDS DISCUSSION: the Research Archive structure exists but the Data EXCHANGE
      (Data→resources, Chips-favored) has no tuned rates in docs — left unimplemented.*
      [pyrite][sim][game] (M)
- [x] **Verbs**: `withdraw(kind)`/`try_withdraw` (adjacent refinery output first, colony stock
      at a depot second), `deposit`/`try_deposit` generalized (depot → stock; refinery → only
      its recipe's inputs; try_ returns False instead of faulting), `cargo_count(kind)`,
      `scan_resources` (all live nodes, distance/id order — omniscient until M7),
      `drop_cargo` (deliberate spill: typed nodes on the bot's tile, no scatter). *NEEDS
      DISCUSSION: `study()` deferred — it needs Template Caches (map placement is Q71
      territory) and the per-match FUNCTION-block unlock model (docs/06's F_* sets), a whole
      subsystem the other M4 tasks don't touch. withdraw/deposit run instant/1-tick rather
      than "+ action" costed ticks — flag if the action-time matters before M5.* [pyrite][sim] (M)
- [x] **Kind constants**: all 11 raws + 7 refined + `ore` family + smelter/foundry/archive/
      printer/depot/blueprint/enemy/wreck bound; `closest()`/`exists()` resolve resource kinds
      to nodes and structure kinds to structures. *(cache/nest/ally/faction constants land
      with their systems — M12/M13.)* [sim] (S)
- [ ] **Game**: render Smelter/Foundry/Archive/etc., typed stock in the world bar, structure
      HP bars. [game] (M)

## M5 — Universal chassis: stats, energy, upgrades ✅ COMPLETE (2026-07-16) — notes below ✅ CORE COMPLETE (2026-07-15) — discussion items below

- [x] **Floor statline + stat pipeline**: `stats.ron` (HP 40, move 140 deci-ticks/tile — a real
      move-rate stat, terrain multiplies it; cargo 40 deci, sensors 5, slots 1, cpu 100 centi,
      32 lines / 8 vars / stack 4 / log 8); pipeline base → hardware → XP (identity until M6)
      → quirks (identity until M6) → state (Damaged −25% speed+cycles at the FIXED 50% line,
      brownout −50% cycles) → clamp ≥1 stored unit, pessimistic rounding; `printed_*` left
      tuning.ron; per-bot BASES on BotData so dev spawns override and M6 growth mutates.
      *NEEDS DISCUSSION: the 14-ticks/tile floor is a big pacing change pre-M8 (tile costs
      still act as multipliers 1–3×; the ×2 scale + Road ½× land M8) — sandbox/demo tests pin
      `sim.stats.move_rate_deci = 10`. Damaged "speed −25%" was read as +25% ticks/tile.*
      [sim] (L) ⚠HASH *(golden regenerated: statline + longer scenario, 300→1500 ticks)*
- [x] **Energy & upkeep**: `upkeep.ron` (all FIRST-PASS numbers — docs give shape, not
      figures); Generator (8 Steel) burns deposited Wood/Coal from its physical intake (Coal
      preferred — the strong fuel; map-authored generators start stoked); Geothermal Tap (12
      Steel, Vent tiles only); per-bot draw = base + per-upgrade + per-module (per-track-level
      joins M6); refineries draw too and STAND IDLE browned out ("needs energy"); brownout
      halves grants via the pipeline; Fabricator trickle keeps one bot (lowest id) powered
      while a working printer exists; Steel shortfall rusts (self-repair halts + decay through
      the damage phase; `rust_scraps` off by default). *NEEDS DISCUSSION: `MapSpec.
      dev_free_power` default TRUE (the dev_all_unlocks pattern) keeps sandboxes powered;
      Steel maintenance is all-or-nothing; fuel burns whole units per settlement regardless of
      surplus.* [sim] (L) ⚠HASH
- [x] **Upgrade Station**: StructureKind::UpgradeStation (10 Steel + 5 Chips + 3 Wire);
      catalog as data in stats.ron (CPU Mk2/Mk3 SET 2/4 cyc, Memory bank +32/+4/+8, Stack ext
      +4 live-VM depth, Coprocessor; modules Backup Core, Optics +2 sensors); `QueueUpgrade
      { bot, order, replace }` (names resolve against the catalog; invalid = ignored); pad
      pulls the lowest-entity-id adjacent queued bot, skipping mid-template/boot/recall (and
      engine-fired recalls now skip pad-sitters); payment at mount (stock + 1 Water coolant
      from the station's PHYSICAL buffer; modules draw no coolant); unaffordable = skip &
      re-arm, invalid (duplicate CPU tier, no legal slot) = drop; sit = EngineCtx::PadSit
      (double-handle applies; wreck-in-place clears the pad; a destroyed station frees its
      sitter); step-off restarts at line 1. *NEEDS DISCUSSION: (1) Coprocessor and Backup Core
      are PURCHASABLE BUT INERT — think-while-blocked needs a VM concurrency design, XP
      preservation needs M6/M10 death rework; (2) no Water SOURCE exists — the Pump structure
      (docs/03) is in no milestone, so coolant only flows from starting_stock/dev feeds; (3)
      catalog time_ticks are invented first-pass numbers.* [sim][game] (L)
- [x] **`bank_cap`** derived at load from the base cost table (max effective op cost = 25:
      crash dump / upload_log cap; payload ops at payload_cap) as `CostTable.bank_cap`;
      budget clamps after every grant to max(bank_cap, THIS grant) — the cap bounds SAVING,
      never a fast CPU's per-tick throughput (review ruling 2026-07-16); debt untouched;
      "no banking while blocked" now lives in `Vm::grant_centi` (the sim's skip is just a
      shortcut). Per-tile re-derivation waits on M8 overlays. [pyrite][sim] (S) ⚠HASH
- [x] **Game**: inspector budget meter is a bar scaled to bank_cap; per-line cycle-cost
      gutter in the editor (painted in the TextEdit margin off `pyrite::analysis::
      line_costs` — deliberately approximate: sized ops render base+`+`, branch lines charge
      dispatch only); hardware & catalog section in the bot inspector queues `QueueUpgrade`
      (module swap defaults to slot 1 when full). *Note: UI exercised by build only — verify
      in-game alongside M4's still-open structure rendering (Smelter/Foundry/Archive/
      Generator/Tap/Station have no sprites yet).* [game] (M)

## M6 — XP v2 & quirks ✅ COMPLETE (2026-07-16) — notes below

*Landed together with M5 (and the M7 perception core the tests pulled in) against the
authored data files (`stats.ron`, `xp.ron`, `quirks.ron`, `upkeep.ron`) and acceptance
suites (`chassis.rs`, `station.rs`, `energy.rs`, `growth.rs`). NEEDS DISCUSSION, carried
from the data files: xp.ron body-perk magnitudes are first-pass inventions; upkeep.ron
figures likewise (and `rust_scraps` ships off); Coprocessor is purchasable but its
think-while-acting VM support is pending; program_lines/variable_slots enforcement is
M9's deploy bar; the Station coolant source (Pump) is still open from M4. Integration
notes: the phase-0 perception seed now also runs after `SpawnBot` (tick-1 blindness ate
one crash per spawned starter program); legacy pacing/vision test maps carry explicit
`sim.stats` overrides where fog/pacing wasn't what they test; the golden scenario gained
a within-sight node and a 1500-tick window (fixture regenerated — M5/M6 change every
hash: statline, XP map, quirk rolls, upkeep settlements).* ✅ CORE COMPLETE (2026-07-16) — discussion items below

- [x] **Five task tracks + deci-XP** (`data/xp.ron`, `sim::xp`): `BotData.xp` is a
      `BTreeMap<XpTrack, u64>` in deci-XP (all 11 tracks exist — storage never migrates
      again); quadratic curve (100×n, cap L5); incomes per Q83 — mining 1/unit, hauling 1 per
      unit-per-10-tiles ACCRUED per loaded tile and PAID AT DELIVERY (`haul_accum`; drops/
      spills forfeit it), combat 1 per 10 damage + 25/kill (`pending_damage` now carries the
      attacker BOT so the kill credits in settle), building 1 per 10 progress (blueprint
      progress converted to deci-units), Scouting exists with zero income until M7. Task
      perks live: mine yield +10%/L (L3 swing −25%), cargo +10%/L (L3 loaded speed), damage
      +5%/L (`attack_damage` moved to tuning.ron), build rate +10%/L, sensors +1/L. Slot
      milestones +1 at 1000/3000 total XP (cap 3). [sim] (M) ⚠HASH *(golden regenerated)*
- [x] **Six body tracks**: Age (1 deci/tick, added at settle → self-repair +1/L; max-HP
      growth NOT yet wired — see discussion), Mileage (10 deci per tile actually walked,
      engine walks included → move rate −4%/L), Flinch (100 deci per HOSTILE-source flinch —
      `pending_signals` carries a source faction: hurt=attacker, bumped=rammer, bump/error
      =self), Hiding/Boot exist with zero income until M7/M10, Learning (10% of other
      post-multiplier XP via a per-bot fractional carry so slow drips don't floor away;
      +5% gain/level; capped tracks still feed it; never re-multiplied; multiplier memoized
      at start-of-settle). Upkeep gains `draw_per_track_level`. *NEEDS DISCUSSION: every
      body-perk MAGNITUDE (age_hp/repair, mileage −4%, flinch/boot −10%/L) is a first-pass
      invention — docs name the growth, not the numbers. Age's max-HP growth is deferred
      until its magnitude is ratified (mutating max_hp interacts with the Damaged line).*
      [sim] (M) ⚠HASH
- [x] **Quirks** (`data/quirks.ron`, `sim::quirks`): MapSpec `quirk_permille` match dial
      (500 = 0.5/bot default, 0 = off, slot n's chance = dial − n×1000); latent rolls at
      print from `rng.quirk_roll` (rarity-weighted); manifestation at 300/900 total XP in
      phase 7 (one-time effects: MaxHpPct, LogCapPct, live-VM StackDepth); pipeline effects
      (cpu/sensors/cargo/move/flinch/boot/fault-chip/damage/XP%/brownout-softening); POLICY
      quirks ride the env registry (docs/09 Q60: temperament shifts the default, compulsion
      clamps on READ so `getenv` reports the landing and stored values clip quietly);
      `my_quirks()`/`has_quirk()` host arms + quirk names as pre-bound constants; latent
      rolls invisible to everything including introspection; inspector lists manifested
      quirks (enemy-visible free). *NEEDS DISCUSSION: (1) the v1 catalog is the ~26-entry
      subset whose hooks exist — COST-OVERLAY quirks (Tail-Call Optimized, Kernel Bypass,
      Dial-Up, Telemetry Enabled, Eventual Consistency…) wait for M8's per-bot cost
      overlays, and Lazy Evaluation / Graceful Shutdown / Kernel Panic / countdown quirks
      wait for their systems; (2) weights are invented first-pass rarities; (3) "expected
      quirks per bot" is implemented as independent per-slot per-mille draws — ratify the
      dial's shape; (4) `quirk_permille` lives on MapSpec until M13's match-settings
      struct.* [pyrite][sim][game] (L) ⚠HASH

## M7 — Perception: the seeing/hearing model ✅ COMPLETE (2026-07-16) — notes below

- [x] **Two-circle model** (Q74): chebyshev seeing (sensors stat, Optics/Scouting/quirks
      through the pipeline) + hearing (× `sense_factor_pct` tuning), movers-only hearing,
      supercover LoS (High Ground blocks unless the perceiver is elevated), signature offsets
      heard-at distance, Snow mutes movement. *NEEDS DISCUSSION: `creep` — docs/05 calls
      creeping EMERGENT (move, freeze, move), but the verb index lists `creep (stealth
      move)`; no registry verb was invented. Ford quieting waits for M8's Ford tile.*
      [sim] (L) ⚠HASH
- [x] **Queries perception-scoped**: seen ∪ heard ∪ map knowledge; heard-only contacts are
      position-only handles (property reads fault `err_unknown_contact`); stale handles
      fault; `is_seen()`; (distance, id) order everywhere. [pyrite][sim] (L) ⚠HASH
- [x] **Detection episodes** per (bot, enemy faction) with `episode_rearm_ticks` re-arm →
      Hiding XP; per-faction permanent `known_nodes` (existence forever, exhaustion only
      when observed); node discovery + completed surveys mint Data (docs/03 round-4
      manifest). *Integration note: the phase-0 perception seed also runs after `SpawnBot`
      — a spawned starter program's first tick must not be blind.* [sim] (M) ⚠HASH
- [x] **`search()`** (rooted ring-by-ring expansion to the hearing radius, Scouting XP per
      new node + per completed survey, signals end it), **`wander()`/`explore()`**
      (`rng.wander`/`rng.explore` streams), **`path_blocked()`**. [pyrite][sim] (M)
- [x] **Game: fog of war** (`fog.rs`) — pure view layer mirroring faction 0's two circles:
      dark unknown / greyed known / clear seen tile overlay, undiscovered nodes and unseen
      enemy bots hidden, heard-only contacts as pulsing blips, search-stance survey rings
      scaled to the live reach. *Partial: fogged ambient animations are covered by the
      overlay rather than frozen per-tile (shared frame-swap materials — per-tile freezing
      needs per-tile material instances); signature tells ride the inspector, not the
      world view. Both flagged for the rendering pass.* [game] (L)

## M8 — Terrain v2 & terraforming

- [x] **×2 move-cost scale** + full tile table: `tuning.tile_costs` (×2 scale — Plains 2 so
      Road ½× = 1); eight new TileKinds (Mountain, Ramp, Dunes, Ice, Ford, Road, Scree,
      Barricade; as_u8 20–27 appended, existing hashes stable). Costs are per EDGE
      (`TileCostTable::edge_cost_x2`; A* signature gained the table): Mountain climb 6 /
      descend 4 / ridge 2; Mud 8 while loaded (per-bot state rides `stats::step_ticks` —
      from-tile = `data.pos`, signature unchanged). Ice slides (momentum chains across ice
      until solid ground; arrows redirect; slide-into-occupant = collision with the SLIDER at
      fault; engine walks slide but raise no bump; recall arrival guard replans an overshot
      doorstep). Dunes idle-sink (`BotData.dune_idle`, hashed: +1/tick standing on sand, each
      full `dune_sink_ticks` interval adds `dune_sink_step_x2` to the next step, capped at
      `dune_sink_cap_x2` — buried, never trapped; every move resets). Ford quiets the wader
      (`ford_quiet` off heard-at) and costs 4×. Scree wear (`world.scree_wear`, hashed;
      collapses to Rubble at `scree_crossings` entries in the end-of-tick terrain settle;
      `set_tile` drops the counter). HighGround entry Ramp-gated (or via Mountain); Mountain
      summits join `on_high_ground` (sensor bonus + LoS exemption) and block ground-level LoS.
      Game: Mountain takes the full block + cliff art from Rubble (now flat debris);
      placeholder art reuse for the other kinds; the slab layer rebuilds INCREMENTALLY on
      terrain change (`resync_terrain` diffs a grid snapshot, redraws changed tiles + 3×3
      neighborhoods); demolished bridge planks despawn. Review 2026-07-16 hardening:
      `move_ticks` is GONE — `passable()` is the one passability source and the tuning table
      the one cost source, validated as a biconditional at load; `spawnable()` gates every
      materialization site (prints, dev spawns, structure placement, cargo spills — nothing
      pops into existence on High Ground); ground hardening under an in-flight plan (new
      barricade, demolished bridge) re-plans instead of panicking, for program walks and
      recall walks both.
      *NEEDS DISCUSSION: (1) Snow stays 1× and mute-only (Q67 open — no cost/tracks effects
      invented); (2) HighGround's +2 bonus and the Chebyshev spread metric are still
      hardcoded first-pass; (3) slide steps cost normal step ticks (no momentum speed-up);
      (4) a Barricade completing under a standing bot leaves it free to step off (entry-only
      blocking).* [sim][game] (L) ⚠HASH *(golden regenerated: hash format only — dune_idle,
      scree_wear, blight_cores joined the snapshot; legacy behavior bit-identical, the ×2
      scale doubles both cost and divisor)*
- [x] **Cost overlays**: FLAT per-op overlay only — `Vm.cost_overlay_centi`, re-set by the
      sim before every grant from the tile under the chassis (derived state, never hashed);
      charged ops pay base + overlay floored at one full cycle (zero-cost bookkeeping stays
      free); `grant_centi`'s bank cap grows by the overlay margin (the cap stays "the
      priciest effective op", Q75). Corruption tax = `tuning.corruption_op_tax` (100 centi =
      +1cy/op). *NEEDS DISCUSSION: (1) per-op-KEY / per-biome overlay LAYERING was not built
      — the flat surcharge covers Corruption; a real layering design should say how overlays
      compose and which op classes they touch; (2) forced charges (trap cost, crash dump,
      abort upload) stay untaxed — punishments keep fixed figures.* [pyrite][sim] (M) ⚠HASH
- [x] **Corruption dynamics**: `BlightCore { pos, radius, hp }` in `world.blight_cores`
      (hashed; `MapSpec.blight_cores`, serde-defaulted; allocated after printers so fixture
      entity ids stay put; its tile painted Corruption at build). Spread every
      `corruption_spread_ticks`: each living core corrupts the nearest non-Corruption
      passable tile in radius, (chebyshev, y, x) order — cleansed ground re-corrupts for free
      while the source lives. Cores are solid, perceivable (seen-only, like structures),
      queryable (`closest(blight)`), and attackable like structures; killing one stops the
      spread, the creep stays. *NEEDS DISCUSSION: (1) channel jamming waits for M11 channels;
      (2) Bridges, Ramps, and Roads are spared from spread (creep would delete the river
      crossing; a corrupted Ramp would permanently trap a plateau — review 2026-07-16); (3)
      `closest(blight)` is perception-UNGATED (the creep front is visible terrain — but the
      heart's exact position leaking is a choice); (4) cores render nowhere in the viewer —
      neither do Smelters/Foundries (the M4 structure-rendering gap).* [sim] (M) ⚠HASH
- [x] **Terraform set**: BlueprintKinds Clear (Rubble→Plains, labor-only, completion YIELDS
      `clear_yield_stone` to the builder's faction), Barricade (Plains→Barricade, Stone;
      solid + blocks LoS for everyone), Demolish (Bridge→Water / Barricade→Plains, labor-
      only, re-checks the tile at completion), Cleanse (Corruption→Plains, slow), Road
      (Plains|Rubble→Road, Stone). ONE rule set (`BlueprintKind::site_ok/cost_stone/
      build_ticks` + `World::blueprint_site_ok`) drives the placement command, the
      completion re-check (EVERY kind re-validates its ground at completion — void work
      stamps nothing, so a 10-tick Road can't erase creep 4× faster than Cleanse), and the
      build bar's ghost (review 2026-07-16). Blueprint `kind` joined the phase-9 hash (a
      kind divergence desyncs immediately, not at completion); the terrain hash refreshes
      once per tick off a dirty flag instead of once per set_tile. Terraform tab + icons.
      Tests:
      `tests/terrain.rs` (16 tests: cost table, mountain edges, ramp gate, A* road detours,
      ice slide overshoot, dune sink/reset, scree collapse, ford quieting, corruption tax,
      blight lifecycle, all five blueprints, site validation). *NEEDS DISCUSSION: (1)
      structure placement via blueprint was NOT migrated — `PlaceStructure` still lands
      structures instantly, and no build-bar tab places them; (2) Cleanse yields Plains — the
      pre-creep tile kind is not preserved anywhere; (3) Barricades have no HP and are not
      attackable — Demolish labor is the only removal; (4) terraform blueprints carry no
      faction, so any faction's builder can finish them (Clear pays the finisher).*
      [sim][game] (M)

## M9 — Printers v2: target shares (replaces the superseded `desired_max` dial)

*Review round (2026-07-16, 10 confirmed findings fixed):* signal-mode allocation now DEFERS
booting/pad-sitting bots to the polite queue (engine states aren't the player's clock — only
mid-TEMPLATE landings keep the double-handle gamble); a ruined REMAINDER receives nobody
(no marched-to-the-ruin ghost manufacturing; unclaimed bots keep their colors until repair);
`recolor_bot` enforces the Q52 bar and printer state AT ARRIVAL too; polite queue entries are
consumed only when the walk actually starts (politeness retries); walking + queued re-colors
count toward the destination's print target (no replacement-print churn); fleet-cap math
saturates against hostile replay config; the legacy `SetDesiredMax` command variant is kept
as a deserialize-only alias so pre-M9 replay FILES still load; remainder-aimed
`EditPrinterRules` still retunes the faction clock; the rules UI stages DragValue edits and
commits ONE command when the interaction settles. *RULING (docs/01): over-capacity scrap is
an ECONOMY event only — the cap-shrink trigger is GONE (prints stop, attrition shrinks;
sustained-rust `rust_scraps` is the surviving valve).*

- [x] **Allocation table**: `data/printers.ron` (fleet cap +15/working printer — the Q84
      manifest figure; check interval default 1000 ticks, player-set per faction).
      `PrinterRules { target: Count | CapPct (floored % OF THE CAP, Q64), key, best_first,
      priority }` on every printer AFTER the faction's first-born — the FIRST printer is the
      remainder bucket (no dials, edits ignored, implicitly last). SelectKey = stat-sheet
      rows + XP ledgers (TotalXp/Xp(track)/Hp/MaxHp/CpuCenti/Sensors/CargoCap/MoveRate/
      ModuleSlots) with best/worst by the key's improvement direction (MoveRate improves
      downward); key + entity-id tiebreak is the whole sort (no composites, Q64). The pass:
      down the priority list, hardware-bar filter FIRST (Q52), sort, claim up to target;
      remainder takes the rest. Triggers: rule edit (signal-like, immediate), the per-faction
      check interval (signal-like, `tick % interval`), a deploy (polite, scoped to its color).
      Prints: a dialed printer short of its target prints its own color (priority order),
      else the remainder prints, while fleet < cap; `EditPrinterRules` replaces
      `SetDesiredMax`; rules/interval/pending-recalls/reprint-queue all hashed.
      *NEEDS DISCUSSION: (1) `MapSpec.fleet_cap_override` dev knob added (tests/demos need
      small populations and the replay format carries only spec+commands — the
      dev_all_unlocks pattern); (2) the remainder is the FIRST-BORN printer even while
      ruined (its color's bots are ghosts until repair); (3) nest-gating of colors 3+ waits
      for M12 nests — printers only come from map specs today.* [sim] (L) ⚠HASH
- [x] **Dispatch rules**: deploys change assignments at once but their drop/claim recalls
      land POLITELY via `world.pending_recalls` (retried each tick, never mid-template — the
      lame-duck rule, Q85 round 4); a lame duck visibly runs the FINAL OLD VERSION (the
      hot-swap skips over-bar members). Player-fired triggers (rule edits, the interval)
      dispatch like signals — mid-template landings double-handle to a wreck, as decided.
      Re-targets are engine-side: an already-walking re-color gets its destination updated
      (no re-signal); a same-color re-target cancels in place (restart line 1, no boot).
      Ghost machines are DERIVED (Q65): a bot whose color has no working faction printer is
      outside the allocation, recalls, and scrap, still drawing upkeep — repair re-uploads
      survivors by construction. Scrap picks lowest TOTAL XP of the fleet (every track,
      Building included; ghosts and scrap-walkers excluded from the fleet count so the valve
      fires once per surplus body). Hardware bars (Q52): deploy computes the artifact's
      (lines, distinct names) via `pyrite::analysis::artifact_requirements`, stored on
      `ColorProgram`; printers claim only fitting bots; the REMAINDER deploy is refused over
      stock (32 lines / 8 names — `RemainderOverBar`); above-stock-bar printers don't print.
      `QueuePrint { faction }` = a per-faction convenience counter consumed as jobs start.
      *NEEDS DISCUSSION: (1) the docs' `QueuePrint(loadout)` parameter is UNDEFINED — all
      prose says a reprint is a fresh stock print with allocation-chosen color, so the
      counter is the whole feature until "loadout" means something; (2) docs/02 says "a
      deploy IS a rule edit" while docs/01 says deploys are NOT rule edits in the dispatch
      taxonomy — same end behavior, opposite wording, needs reconciling; (3) variable-name
      requirements count assignment targets, loop vars, params, and match binds — reads are
      free.* [sim] (L) ⚠HASH
- [x] **9 named colors** (Green, Red, Blue, Yellow, Cyan, Magenta, Orange, Purple, White —
      docs/01 order): nine bake-time palette-swap atlases (build.rs TEAMS), scene/view/editor
      plumbed for all nine, `Color::NAMES`/`Color::name()` in the sim. Printers are born with
      their color slot AND an empty program file (Q85: `Sim::new` deploys `""` per unfilled
      slot; re-colored bots idle visibly on it). *NEEDS DISCUSSION: tints beyond the ninth
      reuse the white atlas — "procedurally patterned tints" wants real art direction.*
      [sim][game] (M)
- [x] **Game**: printer rules UI (target count/%cap toggle, key combo, best/worst toggle,
      priority — every change fires EditPrinterRules), fleet-cap display ("Fleet N / cap M"),
      reprint-queue button with queued count, dormant label on ruined printers ("its bots
      are ghosts"), Q52 deploy warning ("exceeds N members' memory — deploying drops them to
      the remainder", proceed allowed), and a per-printer Telemetry viewer with min-level
      filtering (trace…error) replacing the flat "Cloud" panel. *NEEDS DISCUSSION: the check
      interval has no UI dial yet (command support exists); telemetry attributes archive
      entries via LIVE bots only — dead bots' lines don't group under their old color.*
      [game] (L)

## M10 — Death, wrecks & intel ✅ CORE COMPLETE (2026-07-16) — discussion items below

- [x] **Wreck v2** (`sim::wrecks`): the whole BotData rides the wreck (rescue/hijack rebuild
      from it; salvage reads its receipt); hull = 25% max HP, attackable (0 = destroyed,
      black box, NO blast); countdown 200 ticks + 10/100-XP, ticked at phase-6 start so
      expiry blasts settle in the SAME damage phase; blast = 50% of the wreck's max HP,
      radius 1, friend-and-foe, entity-id-ordered expiries, NEVER chains (blast-hit wrecks
      are destroyed, not detonated); re-wreck RESUMES the carried countdown
      (`BotData.countdown_carry`); rescue boots at the Damaged line, hurt latch re-armed.
      The entity handle now outlives the bot into its wreck (targetable). [sim] (M) ⚠HASH
      *(golden regenerated: the fixture's KillBot wreck now expires and blasts)*
- [x] **The wreck race verbs**: `repair(target)` (wreck = field repair at the build rate +
      Building L3's +25%, holds at full progress while the tile is blocked; structures and
      bots mend too), `salvage` (25% receipt cut — chassis line + bought hardware — plus +5%
      permanent decryption; destroys the wreck, box drops), `analyze` (other factions only,
      faults on your own; Data + logs into the cloud + the victim's comm key), `hijack`
      (boots under the claimer's WORKING remainder color, XP intact, holds while no
      remainder/blocked tile), `recover_black_box` (banks contents to the archive),
      `guard`/`escort` (entity-anchored stance: leash 2/1, engages adjacent enemies on a
      swing cooldown, follows via per-step A*). *NEEDS DISCUSSION: (1) TOOL GATING —
      repair/hijack should require a build tool; tool modules still don't exist, so both are
      ungated; (2) analyze's Non-PvP ban waits on M13's harm mode; (3) a rescued dev bot
      re-boots on its COLOR's program (its custom source died with its VM — wrecks don't
      carry programs); (4) guard/escort semantics are a first-pass reading (swing cooldown
      10, leash 2/1, per-tick A* while out of leash); (5) the archive is faction-less, so
      analyzed logs land in the shared cloud.* [pyrite][sim] (L)
- [x] **Decryption & comm keys**: `world.decryption[(viewer, owner, color)]` percent —
      grows +5%/salvage, capped 100, never down, never shared; `world.comm_keys[viewer]` =
      addressable factions (M11's `faction=` channels consume it; analyze steals one). Both
      hashed. Masked-source RENDERING deferred with the Codex UI below. [sim] (M)
- [ ] **Game**: clickable Black Boxes, wreck countdown display, Codex/decryption viewer with
      per-color enemy-decryption % in the file viewer. *(Deferred — the sim exposes
      everything: wrecks carry countdown/hp, boxes carry entity + cause, decryption is a
      readable map.)* [game] (M)

## M11 — Channels ∥ ✅ CORE COMPLETE (2026-07-16) — discussion items below

- [x] **Blocking `send`/`receive`/`broadcast`** (`sim::channels`): rendezvous only — no
      queues, no mailboxes; a phase-4b settle pairs blocked participants each tick
      (longest-blocked receiver first, ties by lowest entity id; one broadcast then consumes
      every remaining receiver). Timeouts fault a TYPED `err_timeout` (new
      `Vm::resolve_action_fault`); `try_send`/`try_broadcast` park instant deliveries on the
      receiver's action (message LOST when nobody's blocked), `try_receive` takes from the
      longest-blocked sender. Per-faction namespaces via the `faction=` param, gated on the
      target's COMM KEY (`analyze()` steals one; ally grants land M13); the `Channels`
      construct gates the verbs per faction (Research; dev maps exempt). Corruption jams
      both ends (blocked participants inside never wake; timeouts still run — the lease
      recovery). Blocking burns the budget (M5's rule) and signals still interrupt (raise
      cancels the parked op — the owed result never arrives, which is exactly the
      mutex-as-lease recovery story). *NEEDS DISCUSSION: (1) `try_receive` matches blocked
      SENDERS only — polling a blocked broadcaster doesn't count as its audience; (2) the
      docs' `Blocked(channel)` run-state variant is served by the sim-side action (the HUD
      shows the channel) rather than a pyrite RunState change; (3) sender-side selection
      mirrors the receiver rule (longest-blocked, lowest entity) — docs only specify the
      receiver side; (4) faction ids in `faction=` are raw ints until M13's faction
      constants.* [pyrite][sim] (L)

## M12 — Ferals ∥ ✅ CORE COMPLETE (2026-07-16) — discussion items below

- [x] **Feral faction** (`sim::feral`, faction id 255): nests (MapSpec `nests: (pos,
      arcanum)`, filtered by the `max_arcanum` match option) print the v1 archetypes —
      Drone/Stinger/Harvester/Warden, real Pyrite on the shared VM, each on a FIXED Feral
      color slot (200–203) so decryption accrues per archetype. `home`/`patrol_route`
      pre-bind as per-print VM constants (Q79's kind-constant mechanism); `deposit()`
      treats the nest as the Harvester's depot (its stock funds prints). Escalation 0–3 is
      FOOTPRINT-driven (structures + printers + claims + kills×weight, never wall-clock)
      and widens the deterministic round-robin print mix. Magician/Moon arcana mutate one
      integer literal per print via `rng.feral_mutation` (parse-valid by construction;
      every variant enters the program library for the Codex diff). Beating a nest to 0
      leaves a DEFEATED site: `RazeNest` banks the Data bounty, `ClaimNest` converts it —
      and claimed nests gate `PlacePrinter` on docs/01's triangular curve (2 free slots,
      then 1/3/6/10 nests). Undefended claims are RETAKEN by adjacent Feral activity
      (guard radius, tuning). Nests see for their side (perception eyes) and see-first
      acquisition falls out of M7's scoped queries. Sighting-only Feral perception, 9
      tests in `tests/ferals.rs`. *NEEDS DISCUSSION: (1) archetype sources deviate from
      docs/04's listings — a `move_to` before each `attack`, an `exists(ore)` guard on the
      Harvester, and `wait(n)` beats added as mutation targets; ratify or restore the
      crash-loop-y originals; (2) claim/raze are instant Commands — docs want a build-tool
      bot converting the site; (3) of the v1 arcana subset only the MUTATION flag (1, 18)
      is mechanically distinct — Hierophant hijack, Death salvage-denial, Tower siege, and
      Moon counter-intel personalities are still just difficulty scaling; (4) losing a
      claimed nest gates only NEW printer placements — docs/01 wants the over-curve
      printer sent DORMANT with its bots as ghosts (Q65 machinery exists from M9); (5)
      `patrol_route` = nest + 3 nearest nodes is my drafting; (6) the footprint metric
      (docs say "territory claimed, energy output, Ferals killed") and the
      `nest_income_deci` trickle (keeps barren-map nests printing) are first-pass
      stand-ins; (7) Feral bots have no `color_programs` entry, so a rescued Feral wreck
      boots the `wait(1)` fallback — the Codex/decrypted-view UI is [game] work that
      hasn't landed.* [sim] (M) ⚠HASH — golden regenerated: phase 9 now hashes the
      escalation dial + kill counter unconditionally (and nests when present), so every
      replay hash moves; the fixture scenario's behavior is unchanged.

## M13 — Match plumbing & multiplayer ✅ SIM CORE COMPLETE (2026-07-16) — game-side work + discussion below

- [x] **Match settings** (docs/08 Q77): `MapSpec.settings: MatchSettings` — harm mode
      (Open / NonPvp / Duel), Ferals toggle, print-cost and salvage-decryption-% overrides
      (shadow tuning.ron at `Sim::new`), vote cooldown + window. `quirk_permille` and
      `max_arcanum` remain direct MapSpec fields (same inventory, older plumbing). Non-PvP
      enforcement: `World::harm_allowed` gates `attack()` resolution (structures, wrecks,
      bots — Ferals and your own things always fair game; blasts stay indiscriminate per
      Q55) and the salvage/analyze/hijack verbs fault `err_action` on other players'
      wrecks (Q76). *NEEDS DISCUSSION: no lobby UI — [game] work; the PvP entry gate
      (full construct knowledge to join harm servers) is a matchmaking-layer rule with no
      sim hook yet.* [sim] (M)
- [x] **Remaining commands**: `ExchangeData` (clamped Data gifts), `PostRequest` (a
      64-entry, 200-char-clamped world message board), `Grant`/revoke (Vision pools the
      granter's eyes into the grantee's perception each tick — the M7 hook; Channels opens
      the granter's namespace without a stolen comm key), `SetAlliance` (allies advance
      salvage decryption TOGETHER from then on — docs/08's team level; dissolving takes
      its grants with it), `Vote` (unanimous `SetSpeed` proposals across live factions;
      one refusal or window expiry fails it; every attempt starts the cooldown; the agreed
      `sim_speed_permille` is world state the game layer paces by). *NEEDS DISCUSSION:
      (1) the Request Box is a world board, not the docs' physical structure; (2)
      `ExchangeData`/`Grant` trust the relay to forward only a faction's own commands —
      command AUTHORSHIP isn't modeled in the sim; (3) prior decryption isn't merged when
      an alliance forms (only future salvages pool) — ratify; (4) the viewer-local speed
      control still exists in the game crate and should defer to the voted speed
      [game].* [sim] (M)
- [x] **Lockstep relay scaffold** (`sim::lockstep`): `Transport` trait (send/recv of
      `Commands { peer, tick, … }` / `Hash` messages) + `LockstepPeer` — schedules local
      input at `T + delay` (warm-up ticks pre-seeded empty), barriers on the full roster's
      command sets, applies them in peer-id order, steps, broadcasts the phase-9 hash, and
      LATCHES the first `Desync { tick, hashes }` once every roster hash disagrees.
      `LocalHub` is the in-process reference transport; tests drive two real Sims through
      command traffic and a deliberate divergence. *NEEDS DISCUSSION: real networking
      (sockets, host relay ordering, late join / host migration serialization) is [game]/
      new-crate work — the docs' pause/dump/resync policy on desync also lives above this
      layer.* [sim] (L→M as scoped)
- [ ] **Q71 map generation** — still an open design question; unblock before v1 ships.
      (Unchanged: everything above runs on authored/dev MapSpecs.)

---

## Review round 2026-07-16 (xhigh, M10–M13 working tree) — all 15 findings fixed

Crashes/losses: `PostRequest` clamps on a char boundary (a mid-codepoint `String::truncate`
was a remote-triggerable lockstep panic); `LockstepPeer::submit` claims a fresh tick per
frame (`next_submit`), so a stalled barrier no longer overwrites queued commands. Harm &
perception gates: guard/escort swings now pass `harm_allowed` + never hit declared allies +
require the victim in the perception cloud; `attack()`'s victim lookup covers nests (a
CLAIMED nest is the claimant's property on Non-PvP); `move_to`'s stale-handle exemption is
owner-scoped (foreign nests need eyes, killing the entity-id fog sweep); channel verbs
accept faction 0 (per-site range checks replaced the shared `> 0` guard; out-of-u8 factions
fault instead of truncating). Hash coverage (⚠HASH, golden regenerated — every replay hash
moves; fixture behavior unchanged): `hash_bot_data` now hashes EVERY BotData field and is
shared by live bots and wrecks (upgrade/module identity, per-track XP, quirks, carries,
rng_program, crash_seen — plus in-flight `Action`/`ActionRequest` state incl. channel
`waited`/parked payloads via exhaustive `hash_action`/`hash_request`); `harm_enabled` +
vote plumbing and `BlackBox.pos` joined phase 9. Game rules: hijack AND rescue hold at the
fleet cap (ghosts exempt — the countdown keeps burning, so a stuffed roster can lose the
race); `try_send`/`try_receive` are jammed from the CALLER's tile too (Corruption blocks
both ways, matching the blocking verbs); vision grants copy from a pre-grant snapshot
(never transitive, faction-number independent); nests are solid (structure_at,
A* blocked set, spawn tiles, PlacePrinter's free check); repair pays Building XP only for
work actually done. Regression tests in multiplayer/lockstep/channels/ferals/wreckrace/
building suites.

## Cross-cutting quick wins (small, independent, grab anytime)

- [x] Delete the spurious `become_disabled` cost entry once M3 lands. [pyrite] *(with M3)*
- [x] `health_low()` reads env `hurt_line` (after M3 env). [sim] *(with M3)*
- [ ] Fold `PlacePaint` into `PlaceOverlay(arrow|paint)` per 07. [sim][game]
- [x] `RepairPrinter` re-priced in Data (~60) once Data exists (M4). [sim] *(with M4)*
- [ ] Tuning values to spec first-pass numbers: fault_damage 5→2, boot_ticks 2→~20,
      print_ticks 5→~100 (in the M0 data files). ⚠HASH
- [ ] Snow tile comment cites superseded Q67 — re-point at Q78 when M7 lands. [game]
- [x] Thought-cloud states to the doc's list (normal/boot/handler/searching/low-health/abort)
      switched on VM run state rather than view-derived flags. [game] *(with M3; searching
      lands with M7's stance)*

## Verb-layer index (every spec'd builtin → its milestone)

| Verb | Milestone | | Verb | Milestone |
|---|---|---|---|---|
| `abort` ✅ | M3 | | `is_seen` | M7 |
| `setenv`/`getenv` ✅ | M3 | | `search`/`wander`/`explore` | M7 |
| `log(level=)` ✅ | M3 | | `path_blocked` | M7 |
| `withdraw`/`try_withdraw` | M4 | | `creep` (stealth move) | M7 |
| `deposit`/`try_deposit` | M4 | | `repair`/`salvage`/`analyze` | M10 |
| `cargo_count` | M4 | | `hijack`/`recover_black_box` | M10 |
| `study` | M4 | | `guard`/`escort` | M10 |
| `scan_resources` | M4 | | `send`/`receive`/`broadcast` + `try_*` | M11 |
| `my_quirks`/`has_quirk` ✅ | M6 | | `scan_enemies` | M7 |

Existing and staying: `closest`, `exists`, `move_to`, `mine`, `build`, `attack`, `wait`,
`rng`, `log`, `upload_log`, `upload_crash_dump`, `cargo_full`, `health_low`, `last_error`,
`handler_init`, `drop_cargo` ✅ (host impl landed with M4).
