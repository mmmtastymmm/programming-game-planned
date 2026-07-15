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

## M2 — Nine-phase tick skeleton (sim restructure the rest slots into)

- [ ] **Reorder `Sim::step()` into the nine phases** (07): Commands → VM step → collect →
      resolve → **Perception (5, stub)** → damage/countdowns/blasts (6) → **XP settlement (7)**
      → economy (8) → snapshot hash (9). Damage moves out of inline resolution; XP credits
      become queued events settled with a start-of-tick Learning multiplier (multiplier is
      identity until M6b lands). Phase-0 perception seed hook at match start. [sim] (M) ⚠HASH
- [ ] **Severity-order co-arrival**: pending-signal set resolved at op boundaries,
      abort > error > recall > hurt > bumped > bump, extras dropped; co-arrival ≠ double-handle
      (Q81). [pyrite][sim] (M) ⚠HASH
- [ ] **Spatial index** (bots per tile): occupancy and proximity queries are O(bots) linear
      scans today; perception (M6) multiplies query volume. BTreeMap<pos, ids>. [sim] (S)

## M3 — Signals v3: the seven-template model (largest single divergence)

Code still implements the pre-2026-07-13 design (one `on signal(s): match s:` + `on death:`).

- [ ] **Per-signal reserved templates**: `on error/hurt/bump/bumped/boot:` player windows;
      `abort`/`recall` fully reserved (zero window). Forced prologue/epilogue AST splicing;
      delete `on death:` and `SignalKind::Death`; black box = "whatever you logged while
      alive". [pyrite][sim] (L) ⚠HASH
- [ ] **`abort()` verb** — the only player scuttle (forced `upload_log()` → `become_disabled()`
      → Disabled). Remove `become_disabled` from the player-callable surface (engine-only).
      Remove the `KillBot` dev command or mark it dev-only. [pyrite][sim] (S) ⚠HASH
- [ ] **Double-handle → abort** (not instant destruction): kill the no-wreck `explode()` path;
      spec says no instant-destroy path exists. Also remove the "humble engine defaults"
      carve-out (factory contents double-handle like player code, Q50). [pyrite][sim] (M) ⚠HASH
- [ ] **Recall as an engine-owned Pyrite program** on the same VM (ordinary signal: interrupts
      Running AND Blocked; double-handles mid-template). Engine-fired recalls (deploy drops,
      scrap) stay polite — and politeness must also skip **mid-handler** bots, not just
      booting/recalling ones. [pyrite][sim] (M) ⚠HASH
- [ ] **Per-signal instruction caps + `signal_safe`**: static analysis pass at deploy — window
      worst-case instruction counts, safe-function set from the M1 registry, loop/recursion ban
      in windows. Deploy-time rejection only. [pyrite] (L)
- [ ] **Unlock surgery**: per-signal window constructs replacing `OnSignal`/`OnDeath`; `Import`
      as its own construct; add `Channels`. Align with 06's tree. [pyrite] (S)
- [ ] **Run-state enum to 07's shape**: `Running | Faulted | Blocked(action|channel) |
      Template(signal, phase) | Boot | PadSit`. [pyrite] (S)
- [ ] **Editor**: per-signal handler windows (sandwich per template), cap meter, signal-safe
      greying, distinct abort cloud (black skull) vs death. [game] (M)
- [ ] **Env registry**: `setenv`/`getenv`, key/default/range registry, `hurt_line` (replaces
      the hardcoded 50% in `health_low()` and the hurt latch), `log_min_level`; env snapshot
      into Black Boxes. [pyrite][sim] (S) ⚠HASH
- [ ] **Log levels**: `log(msg, level=…)` with `trace…error` constants (needs M1 kwargs);
      leveled ring buffers and archive entries. [pyrite][sim] (S)

## M4 — Typed resources & economy (the Ore→Metal migration the docs earmark)

- [ ] **11 raws → 7 refined as first-class kinds**; typed colony stock, typed cargo manifest
      (deci-units), typed costs. Wire nodes onto the nine already-rendered resource-ground
      tiles (+ water, stone) with amounts/regen (Grove regenerates); `mine()` yields the node's
      kind. Retire `stockpile_ore`/`OreNode`. [sim] (L) ⚠HASH
- [ ] **Generic `Structure { kind, hp, pos, buffers, recipe, pad }`** replacing per-type maps;
      structures gain HP and can be attacked. [sim] (M) ⚠HASH
- [ ] **Smelter + Foundry** with recipes (Steel 2Fe+1Coal, Bronze 1Cu+1Sn, Glass 2Sand; Wire,
      Chips 1Ag+2Crystal+1Wire, Lens 2Glass, Gold Chip 1Chip+1Au), `SetRecipe` command,
      physical input feeds / abstract payments split (Q84), lowest-ID tie-breaks. [sim] (L)
- [ ] **Re-price everything typed**: tool ladder rule, Bridge in Stone, printer repair in Data,
      print cost, build-tool Steel exception. Values already inventoried in 03. [sim] (M) ⚠HASH
- [ ] **Data currency + Research Archive** (structure-free research rule), `Research` command,
      UnlockSets consumed at parse (replace the `UnlockSet::all()` sandbox default with
      per-faction sets; keep a dev flag). [pyrite][sim][game] (M)
- [ ] **Verbs**: `withdraw(kind)`/`try_withdraw`, generalized `deposit`/`try_deposit` (any
      acceptor, faults on full/empty), `cargo_count(kind)`, `study()`, `scan_resources`,
      `drop_cargo` host impl. Most are host-arm + cost-entry cheap once cargo is typed.
      [pyrite][sim] (M)
- [ ] **Kind-constant catalog**: bind the full ~40-name set (all raws incl. water, all refined,
      `ore` family, structures, wreck/black_box/blueprint/cache/nest, enemy/ally, per-match
      factions) — today `KINDS` has 4 entries. [sim] (S)
- [ ] **Game**: render Smelter/Foundry/Archive/etc., typed stock in the world bar, structure
      HP bars. [game] (M)

## M5 — Universal chassis: stats, energy, upgrades

- [ ] **Floor statline + stat pipeline**: HP 40, move 14 ticks/tile (a real move-rate stat —
      today all bots move at tile-cost speed), cargo 4, sensors 5, slots 1; modifier pipeline
      base → hardware → XP → quirks → state; deci/centi stored units; `stats.ron`. [sim] (L) ⚠HASH
- [ ] **Energy & upkeep**: Generator + fuel, Geothermal Tap, per-bot draw, brownout
      cycle-halving, Steel-shortfall rust (configurable), Fabricator backup trickle (one bot
      always powered, lowest ID). [sim] (L) ⚠HASH
- [ ] **Upgrade Station**: placeable structure, pad-sit run state, `QueueUpgrade`, hardware
      catalog (Coprocessor, Backup Core, Optics, …), module slots + swap economics, pad pull
      skips mid-template bots, skip-not-repull. [sim][game] (L)
- [ ] **`bank_cap`** derived per-bot-per-tile (needs overlays from M8; until then derive from
      base costs) + "no banking while blocked" moves from sim special-case into the rule.
      [pyrite][sim] (S) ⚠HASH
- [ ] **Game**: inspector budget meter scaled to bank_cap, per-line cycle-cost gutter in the
      editor (docs ask for gutter, not hover-only), Upgrade Station catalog UI. [game] (M)

## M6 — XP v2 & quirks

- [ ] **Five task tracks** (add Scouting) with spec incomes: hauling 1 per unit-per-10-tiles,
      combat 1 per 10 damage + 25/kill, building 1 per 10 progress; deci-XP storage; levels,
      perk milestones, slot milestones. [sim] (M) ⚠HASH
- [ ] **Six body tracks**: Age (1/10 ticks → max HP + self-repair), Mileage (1/tile → move
      rate), Hiding (25/detection-episode — needs M7), Flinch (10/hostile flinch), Boot
      (100/hostile-caused rescue boot; source filters against farming), Learning (10% of other
      post-multiplier XP, +5%/level, settled at phase 7). [sim] (M) ⚠HASH
- [ ] **Quirks** (09): match-setting probability (default 0.5/bot), latent roll at print
      (`rng.quirk_roll`), manifestation at 300/900 total XP, no removal, enemy-visible free;
      `my_quirks()`/`has_quirk()` + quirk-name constants; per-quirk effects via the stat/cost
      pipeline; quirk scratch state. [pyrite][sim][game] (L) ⚠HASH

## M7 — Perception: the seeing/hearing model (biggest behavioral shift for programs)

- [ ] **Two-circle model** (Q74): sensor stat → seeing radius (total info, fog lifted) and
      hearing radius (× `sense_factor` ~150%, movers only); LoS blocks both; signature offsets
      heard-at distance; snow mutes movement, fords quiet, creeping verb. [sim] (L) ⚠HASH
- [ ] **Queries become perception-scoped**: `closest`/`exists`/`scan_enemies` filter to
      seen ∪ heard ∪ map knowledge; heard-only contacts are position-only handles whose
      property reads fault `err_unknown_contact`; **stale handles fault**; `is_seen()`;
      deterministic order preserved (distance, id). [pyrite][sim] (L) ⚠HASH
- [ ] **Detection episodes** per (bot, enemy faction) → Hiding XP; per-faction map knowledge
      (discovered nodes/geology, exhausted states); prospecting. [sim] (M) ⚠HASH
- [ ] **`search()` scouting stance** (rooted, seeing expands to hearing radius, resolves at
      full reach → Scouting XP), `wander()`, `explore()` (~15 tiles fogged-tile pick,
      `rng.explore` / `rng.wander`), `path_blocked()`. [pyrite][sim] (M)
- [ ] **Game: fog of war** — per-faction visibility, greyed last-known snapshot with frozen
      animations, heard-contact pulsing blips, search-stance survey ring, `Hiding`/signature
      tells. First rendering feature with real sim coupling. [game] (L)

## M8 — Terrain v2 & terraforming

- [ ] **×2 move-cost scale** + full tile table: Road ½× (=1), Mountain as its own kind with
      **edge costs** (A* signature change), Ice slides, Dune idle-sink counters, Ford
      (signature bonus), Scree collapse counters, Snow effects, HighGround ramps (+2 sensors
      at edge), loaded-cargo multiplier. Split Mountain from Rubble (game renders Rubble as a
      mountain block today — becomes 2× debris). [sim][game] (L) ⚠HASH
- [ ] **Cost overlays**: base + per-biome/per-tile overlay layering resolved at step time,
      floored at 1, load-validated; Corruption cycle tax; feeds real `bank_cap`. [pyrite][sim]
      (M) ⚠HASH
- [ ] **Corruption dynamics**: spread counters, Blight Cores, Cleanse, channel jamming (cloud
      telemetry exempt). [sim] (M) ⚠HASH
- [ ] **Terraform set**: Clear, Barricade, Demolish, Cleanse, Road blueprints (+ structure
      placement via blueprint); build bar categories. [sim][game] (M)

## M9 — Printers v2: target shares (replaces the superseded `desired_max` dial)

- [ ] **Allocation table**: fleet cap +15/printer, per-printer target (count or floored % of
      cap), selection key = any stat with best/worst direction, player priority order,
      first-printer un-editable remainder, check interval (default 1000 ticks — today it
      rebalances every tick), `EditPrinterRules` command replacing `SetDesiredMax`. [sim] (L) ⚠HASH
- [ ] **Dispatch rules**: lame-duck deploys (assignment changes at once, recall entry waits),
      polite engine-fired recalls (incl. mid-handler), ghost machines (unowned printer → bots
      orphan, re-upload on recapture), scrap picks lowest **total** XP (today omits
      xp_building), hardware bars (Q52: deployed artifact sets memory/variable needs; printer
      claims only fitting bots; remainder capped at 32 lines/8 names), `QueuePrint(loadout)`.
      [sim] (L) ⚠HASH
- [ ] **9 named colors** with procedural tints (today hard-capped at 3 bake-time atlases);
      printers born with color slot + empty file. [sim][game] (M)
- [ ] **Game**: printer rules UI (targets/keys/priority/interval), deploy warnings ("exceeds N
      members' memory" — proceed anyway), reprint queue, fleet-cap display, dormant-printer
      state, per-printer telemetry viewer with level filtering (replaces the flat "Cloud"
      panel). [game] (L)

## M10 — Death, wrecks & intel

- [ ] **Wreck v2**: HP (~25% max), countdown 20s + 1s/100 total XP, blast in damage phase
      (max-HP scaled, friend-and-foe, never chains), re-wreck countdown resumes, rescue boot
      at the Damaged line with hurt re-armed. [sim] (M) ⚠HASH
- [ ] **The wreck race verbs**: field `repair()` (80 progress), `salvage` (build receipt + 5%
      decryption), `analyze` (other factions only: Data + logs/env + comm key; destroys wreck;
      banned in Non-PvP), `hijack` (→ claimer's remainder color, full fleet member),
      `recover_black_box`, `guard`/`escort` (entity-anchored). [pyrite][sim] (L)
- [ ] **Decryption & comm keys**: per-(color, faction) levels, masked-source rendering,
      version hashing (M0), alliances never share decryption. [sim] (M)
- [ ] **Game**: clickable Black Boxes, wreck countdown display, Codex/decryption viewer with
      per-color enemy-decryption % in the file viewer. [game] (M)

## M11 — Channels ∥ (after M1 kwargs + M3 run states)

- [ ] **Blocking `send`/`receive`** with rendezvous, longest-blocked-receiver selection,
      timeout faults (`err_timeout`), `try_*` message-lost variants, per-faction namespaces
      (`faction=` param), comm-key gating, mutex-as-lease idiom support; `Blocked(channel)`
      state; `Channels` construct unlock. [pyrite][sim] (L)

## M12 — Ferals ∥ (after M7 perception, M9 colors)

- [ ] **Feral faction**: nests, nest-bound `home`/`patrol_route` bindings, arcana
      (max-arcanum match setting), escalation, `rng.feral_mutation`, nest-gated printer
      counts, Feral programs in current builtins, see-first acquisition. [sim] (M) ⚠HASH

## M13 — Match plumbing & multiplayer (last; single-player is lockstep-with-one already)

- [ ] **Match settings struct** (08's inventory: harm mode, print cost, max arcanum, quirk
      probability, decryption %, vote cooldown, Ferals toggle) wired through world init +
      lobby UI. [sim][game] (M)
- [ ] **Remaining commands**: `ExchangeData`, `PostRequest`, `Grant`, `SetAlliance`, `Vote`
      (+ sim-speed voting replacing the viewer-local speed control); Request Box structure;
      ally grant pools ears (M7 hook). [sim][game] (M)
- [ ] **Lockstep relay**: actual networking, per-tick command exchange, hash comparison,
      desync surfacing. The sim API (ordered commands + phase-9 hash) is already shaped for
      this. [game/new crate] (L)
- [ ] **Q71 map generation** — still an open design question; unblock before this ships.

---

## Cross-cutting quick wins (small, independent, grab anytime)

- [ ] Delete the spurious `become_disabled` cost entry once M3 lands. [pyrite]
- [ ] `health_low()` reads env `hurt_line` (after M3 env). [sim]
- [ ] Fold `PlacePaint` into `PlaceOverlay(arrow|paint)` per 07. [sim][game]
- [ ] `RepairPrinter` re-priced in Data (~60) once Data exists (M4). [sim]
- [ ] Tuning values to spec first-pass numbers: fault_damage 5→2, boot_ticks 2→~20,
      print_ticks 5→~100 (in the M0 data files). ⚠HASH
- [ ] Snow tile comment cites superseded Q67 — re-point at Q78 when M7 lands. [game]
- [ ] Thought-cloud states to the doc's list (normal/boot/handler/searching/low-health/abort)
      switched on VM run state rather than view-derived flags. [game]

## Verb-layer index (every spec'd builtin → its milestone)

| Verb | Milestone | | Verb | Milestone |
|---|---|---|---|---|
| `abort` | M3 | | `is_seen` | M7 |
| `setenv`/`getenv` | M3 | | `search`/`wander`/`explore` | M7 |
| `log(level=)` | M3 | | `path_blocked` | M7 |
| `withdraw`/`try_withdraw` | M4 | | `creep` (stealth move) | M7 |
| `deposit`/`try_deposit` | M4 | | `repair`/`salvage`/`analyze` | M10 |
| `cargo_count` | M4 | | `hijack`/`recover_black_box` | M10 |
| `study` | M4 | | `guard`/`escort` | M10 |
| `scan_resources` | M4 | | `send`/`receive`/`broadcast` + `try_*` | M11 |
| `my_quirks`/`has_quirk` | M6 | | `scan_enemies` | M7 |

Existing and staying: `closest`, `exists`, `move_to`, `mine`, `build`, `attack`, `wait`,
`rng`, `log`, `upload_log`, `upload_crash_dump`, `cargo_full`, `health_low`, `last_error`,
`handler_init`, `drop_cargo` (cost entry exists; host impl in M4).
