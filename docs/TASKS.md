# Implementation tasks: making the design real

Generated 2026-07-15 from a three-crate audit against docs/00–09 (post round-5 verification).
Status shorthand: **[pyrite] [sim] [game]** = crate(s) touched; **⚠HASH** = changes golden-replay
hashes (per CLAUDE.md, the PR must say why); **(S/M/L)** = small/medium/large.

## Where the code stands

The crates are a clean, well-tested implementation of the design **through M15**
(2026-07-20; the round-1/2 staleness this paragraph used to list — generic ore, `desired_max`
dials, omniscient sensing, inline XP tracks — landed across M4–M9). The determinism
discipline is intact everywhere (BTreeMap world, command-only mutation, no floats, seeded RNG,
stable tie-breaks), and the game crate has zero architecture violations — all mutation already
flows through `Command`s. What's stale now is the **Q111 generation**: the tree carries the
reverted M16 capability-tier code awaiting the M16b rebuild (per-track `curve_base` XP, ten
tools), the M17 overlay pipeline is unbuilt, and the [game]-side gaps (structure rendering,
the Codex/decryption UI) are open below. Each lands as a replay-hash change — stored golden
fixtures make every one pay the explain-your-hash-change toll the docs prescribe
(`UPDATE_GOLDEN=1` regenerates; the PR explains why).

Milestones are dependency-ordered. Within a milestone, tasks are roughly sequenced. Milestones
marked ∥ can proceed in parallel once their prerequisites land. **M0–M3 are fully closed and
archived** in [history/tasks-completed.md](history/tasks-completed.md); this file starts at M4.

**2026-07-26 sweep:** the *NEEDS DISCUSSION* markers below were audited and promoted into
[QUESTIONS.md](QUESTIONS.md) — genuine judgment calls are now **Q98–Q108** (Pump/water source,
Barricade HP, inert Coprocessor/Backup Core, overlay layering, phase-4 inner order, creep,
QueuePrint loadout, labor/tool-gating consistency, the Data Exchange, alliance decryption
merge, Feral archetype sources) — **all of which were answered the same day**; first-pass
**numbers** joined the playtest-tuning bucket
there; markers answered by later rulings (Q88/Q89/Q90, M13's Non-PvP gate, the 2026-07-16b
hash coverage, Blueprint.faction) are historical. The markers stay in place below as landing
records — QUESTIONS.md is the live registry.

---

## Carried forward from completed milestones

Notes left inside finished milestones that still bind on unbuilt work. Full
context for each is in [history/tasks-completed.md](history/tasks-completed.md).

- [x] **`rng.combat` isn't in docs/07's inventory** (M0) — the death cargo-spill
      scatter draws from it; flagged in a code comment, never ratified. Either
      add the stream to docs/07 or move the draw. [sim] (S) *(Resolved
      2026-08-02: the first arm was already done — the ratified stream
      inventory in [07-architecture/tick-model.md](07-architecture/tick-model.md)
      enumerates `rng.combat`. No draw moves.)*
- [ ] **Shallow VM hashing** (M2) — per-bot movement intent (path/ticks/goals)
      and the recall path aren't hashed, so a peer divergence there stays
      invisible until a position changes. Known TODO. [sim] (M) ⚠HASH
- [ ] **Recall home is an engine state machine** (M3), not a literal Pyrite
      `move_to(home_printer)` program on the VM. Observable semantics match the
      doc; flagged for discussion. [pyrite][sim] (M)

*M7 still carries one live deferral — the fog-of-war rendering pass (per-tile
ambient freezing, signature tells in the world view), recorded in place under
M7 below. (Ford quieting, once deferred here, shipped with M8's Ford tile.)*

---

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
      `drop_cargo` (deliberate spill: typed nodes on the bot's tile, no scatter). *`study()`
      + Template Caches + the per-match FUNCTION-block unlock model (docs/06's F_* sets) landed
      in **M15** (2026-07-20). withdraw/deposit run instant/1-tick rather than "+ action" costed
      ticks — flag if the action-time matters before M5.* [pyrite][sim] (M)
- [x] **Kind constants**: all 11 raws + 7 refined + `ore` family + smelter/foundry/archive/
      printer/depot/blueprint/enemy/wreck bound; `closest()`/`exists()` resolve resource kinds
      to nodes and structure kinds to structures. *(cache/nest/ally/faction constants land
      with their systems — M12/M13.)* [sim] (S)
- [ ] **Game**: render Smelter/Foundry/Archive/etc., typed stock in the world bar, structure
      HP bars. [game] (M)

## M5 — Universal chassis: stats, energy, upgrades ✅ COMPLETE (2026-07-16) — notes below

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
      (docs/03) was in no milestone, so coolant only flows from starting_stock/dev feeds —
      since ruled by Q98 (the two-tile waterworks) and tracked under *Decided-but-unbuilt*; (3)
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
M9's deploy bar; the Station coolant source (Pump), open from M4, has since been
answered by Q98 (the two-tile waterworks — [03-resources.md](03-resources.md)). Integration
notes: the phase-0 perception seed now also runs after `SpawnBot` (tick-1 blindness ate
one crash per spawned starter program); legacy pacing/vision test maps carry explicit
`sim.stats` overrides where fog/pacing wasn't what they test; the golden scenario gained
a within-sight node and a 1500-tick window (fixture regenerated — M5/M6 change every
hash: statline, XP map, quirk rolls, upkeep settlements).*

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
      heard-at distance, Snow mutes movement. *RESOLVED 2026-07-26 (Q103): creeping is a
      `creep=True` ARGUMENT on the pathing builtins (slower steps + a signature cut), not a
      verb and not emergent — the emergent claim was inexpressible (blocking `move_to`, no
      position literals) and wouldn't have beaten a static listener anyway. Ford quieting has since shipped with M8's Ford tile.*
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

## M8 — Terrain v2 & terraforming ✅ COMPLETE (2026-07-16) — notes below

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
      *NEEDS DISCUSSION: (1) Snow stays 1× and mute-only (Q67 — since answered, 2026-07-14
      terrain backlog, snow's hook re-ruled by Q78; no cost/tracks effects invented); (2) HighGround's +2 bonus and the Chebyshev spread metric are still
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

## M9 — Printers v2: target shares (replaces the superseded `desired_max` dial) ✅ COMPLETE (2026-07-16) — notes below

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
      per-color enemy-decryption % in the file viewer. The masked view carries two ruled
      requirements: **structural whitespace is exempt from the reveal mask at every level**
      (Q125 — line breaks and indentation always render, so a level-0 silhouette is real
      intel) and **a color's version counter is opponent-visible** (Q124 — shown wherever its
      bots are, no decryption state). *(Deferred — the sim exposes
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
      tests in `tests/ferals.rs`. *NEEDS DISCUSSION: (1) RESOLVED — Q108 ratified the `move_to`-before-`attack`
      guard and the `wait(n)` beats; Q117/P10 superseded the `exists(ore)` guard with
      minable scoping (code re-sync tracked in *Shipped programs*); (2) claim/raze are instant Commands — docs want a build-tool
      bot converting the site; (3) of the v1 arcana subset only the MUTATION flag (1, 18)
      is mechanically distinct — Hierophant hijack, Death salvage-denial, Tower siege, and
      Moon counter-intel personalities are still just difficulty scaling; (4) ~~losing a
      claimed nest gates only NEW printer placements~~ **RESOLVED 2026-07-17 (Q87)**: an
      over-base printer now binds to the nest it was built against and is sent
      `PrinterState::Dormant` (bots become ghosts) when that nest reverts; re-claiming
      reactivates it. See `Sim::reconcile_dormancy`; (5)
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
- [x] **Q71 map generation** — **DESIGN DECIDED 2026-07-17** ([05-terrain.md](05-terrain.md)
      *Map Generation*, [history/questions-answered.md](history/questions-answered.md)); **SIM CORE IMPLEMENTED 2026-07-18** (M14 below — the
      `sim::mapgen` producer, validator, config, and tests all land). The game wiring landed
      with it (`MAPGEN_SEED` opt-in; the hand-authored showcase stays the default) — the one
      open M14 item is PvP rotational symmetry.

---

## M14 — Procedural map generation (Q71) ✅ CO-OP v1 COMPLETE (2026-07-18) — PvP symmetry deferred

Design is settled (docs/05 *Map Generation*): a deterministic, seeded, **integer-only,
setup-time** producer — never in the tick, never in the phase-9 hash. Consumes a config +
seed, emits a `MapSpec` the existing `Sim::new`/`World::from_spec` path takes unchanged, so
this milestone adds a producer and a validator without touching the sim's hot path. **v1 is
co-op-first**; PvP rotational symmetry is deferred to a later pass.

- [x] **`sim::mapgen` module** — `fn generate(&MapgenConfig, seed, players) -> MapSpec`. Draws
      from a dedicated `mapgen` stream (`stream_seed(seed, "mapgen")` + `next_rand` SplitMix64
      via `Fnv1a`-hashed value-noise — integer only, BTree/sorted, no floats). Pipeline
      **skeleton → fill → validate → regenerate**; each attempt folds the retry counter into
      the seed (`mix(base, k)`) so `seed S` deterministically resolves to the first passing
      candidate. Runs before the tick loop, once — untouched by `RngStreams`/the state hash.
      `spec.seed` (the sim's runtime RNG) is `stream_seed(seed, "sim")`. [sim] (L)
- [x] **Skeleton stage** — center-out concentric bands (Chebyshev), rim start zones on a
      square-ring perimeter walk (any player count, no trig). Place-by-construction: the
      start-zone kit (Iron+Coal+Wood+Stone within `start_vein_sight` of the printer), a Vent,
      a reachable Water+Sand shore strip, Green(remainder)+ruined-Red printers, a depot, a
      stoked Generator; per-wedge Copper/Tin (mid) and Silver/Gold (deep) along the radial; a
      shared central Blight Core ringed by Corruption + Crystal; per-wedge and apex Feral
      nests (arcanum scaled, capped at `nest_max_arcanum`). A reserved set (start disc + kit +
      a 4-connected radial corridor to center) keeps fill off every guarantee. *Template
      Caches now placed too (M15): shallow blocks ring each start, deep blocks at the core.*
      [sim] (M)
- [x] **Fill stage** — coherent integer value-noise (coarse-cell corners + integer bilinear)
      paints decorative biomes against per-band budgets, skipping reserved tiles. *v1 palette
      is limited to the kinds `MapSpec` can carry (Rubble/Mud/Snow, + Water/High Ground in the
      deep band only); Dunes/Ice/Scree have no `MapSpec` list yet and degrade to Rubble.* [sim] (M)
- [x] **Validator + regenerate loop** — `mapgen::playability_floor` runs a flat-bitmap BFS
      flood-fill per start and checks the floor (kit walkable, an ore-family vein in sight,
      reachable shoreline, Copper+Tin reachable, start not sealed from the frontier); on
      failure `generate` tries the next sub-seed, capped at `retry_cap`, panicking loudly if
      the cap is hit. Also the general `MapSpec::validate` (bounds, spawnability, no bare-ground
      nodes, no duplicate printers) + the shared `MapSpec::paint_grid` the codebase lacked;
      `World::from_spec` now uses the shared painter. [sim] (M)
- [x] **`MapgenConfig` in data** — `crates/sim/data/mapgen.ron`: band percents, per-band fill
      budgets + weighted biome palettes, `start_vein_sight`, `node_amount`, `retry_cap`,
      `noise_cell`, Blight radius/hp, `nest_max_arcanum`, per-faction starting stock, size
      scaling (`base_size` + `size_per_player`, `max_size`), dev flags. Load + validate mirror
      `Tuning` (`include_str!` + `ron::from_str`). [sim] (S)
- [x] **Headless tests** (`crates/sim/tests/mapgen.rs`, 14 cases) — same-seed byte-identical
      reproduction (across seeds × player counts), the floor holds across 1–8 players × many
      seeds, distinct seeds differ, size scales with players, 0→1 clamp, a generated spec
      builds + steps a `Sim`, every start gets an ore vein in sight, the floor *rejects* a
      sealed start, and the `MapSpec::validate` positive/negative cases. [sim] (M)
- [x] **Wire into game match setup** — `scene::build_generated_colony(seed, players)` calls
      `mapgen::generate`, builds the `Sim`, and deploys the starter Green/Red programs to every
      faction's printers. `main::setup_sim` picks it when `MAPGEN_SEED` is set (`MAPGEN_PLAYERS`
      optional, default 1); with no seed the hand-authored showcase demo runs as before, so the
      demo set pieces + smoke test are untouched. The existing terrain renderer already draws
      every `TileKind` the generator emits. Guarded by a game-side smoke test (build + deploy +
      step). *Making generated the default (retiring the showcase) is a later call.* [game] (M)
- [ ] **PvP rotational symmetry** — generate one wedge, rotate-copy N times, resource-exact.
      **Deferred past co-op v1** — layers on the co-op generator. [sim] (M)

---

## M15 — Template Caches & function-block progression (docs/06) ✅ CORE COMPLETE (2026-07-20)

The last decided-but-unbuilt subsystem: `study()` shipped as a dead builtin (nothing to learn
from), so docs/06's per-match function-block axis was unreachable and M14 couldn't place its
by-construction Caches. Now built. Constructs (syntax, permanent, Data-researched) were already
gated by `pyrite::UnlockSet`; this adds the OTHER axis — **function blocks** (which builtins a
colony may CALL this match, LEARNED at Caches).

- [x] **`sim::progression` module** — `FunctionBlock` enum (Sense/Log/Search/Attack/Salvage/
      Build/Env/Analyze/Scan/Guard/Hijack) with each block's builtins, cache **depth** (docs/06
      tree numbers), and `block_of(builtin)`. A builtin in no block is ungated (the start kit).
      A test cross-checks every listed name against `builtins.ron`. [sim]
- [x] **Template Cache entity** — `world::Cache { pos, block }` + `World.caches`; `MapSpec.caches:
      Vec<(TilePos, FunctionBlock)>` (serde-defaulted); `from_spec` builds them; perceived like
      other field objects (eyes-only); `entity_pos`/`closest("cache")`/`.distance` resolve them;
      hashed in phase 9. Non-consumable (a school, not a pickup). [sim]
- [x] **`study()` verb** — start-kit, rooted `study_ticks` (~10 s, tuning) at an adjacent Cache
      (`ActionRequest::Study` → `Action::Study`), then the block unlocks colony-wide + a little
      Learning XP. Faults with no Cache in range. [sim]
- [x] **Per-match function-block state** — `World.studied: BTreeMap<faction, BTreeSet<block>>`;
      `studied_blocks()`/`locked_builtins()` helpers; dev sandboxes act as if all studied. [sim]
- [x] **Deploy-time gating** — `pyrite::called_names` + a new `PyriteErrorKind::LockedFunction`;
      `Sim::check_functions` rejects a deploy calling an un-studied block builtin (both DeployProgram
      and SpawnBot), like locked syntax. Inert under `dev_all_unlocks`, so every existing map/test
      is untouched; Ferals bypass (they parse elsewhere; docs/06 rule 3 previews unlocks). [sim][pyrite]
- [x] **Mapgen places Caches** — shallow blocks (Sense/Log/Attack/Search) ring each start on the
      reserved disc (reachable from tick one); the deep blocks (Build/Env/Salvage/Analyze/Scan/
      Guard/Hijack) cluster as shared, contested Caches at the core (docs/06: "shared map features
      worth controlling access to"). [sim]
- [x] **Tests** (`crates/sim/tests/progression.rs`, 8 cases) — block-table integrity, `block_of`,
      deploy gating rejects/allows, start-kit deploys free, dev bypass, study() unlocks
      colony-wide + non-consumable, study faults with no Cache, mapgen rings starts + places deep
      Caches. ⚠HASH-neutral (caches/studied hash bytes only emit on non-empty; golden unchanged).
- [ ] **Follow-on** — the per-match FUNCTION-block editor tree/greying (docs/06 rule 2, [game]);
      F_TERRA gating the terraform blueprint COMMANDS (blueprints aren't builtins, so out of the
      builtin-gating scope here); construct *research* economy already exists (M4's `Research`).

---
## Review rounds

Six review rounds closed between 2026-07-16 and 2026-07-20; every finding was
fixed. Archived in full at [history/reviews.md](history/reviews.md).

---

## Cross-cutting quick wins (small, independent, grab anytime)

- [x] Delete the spurious `become_disabled` cost entry once M3 lands. [pyrite] *(with M3)*
- [x] `health_low()` reads env `hurt_line` (after M3 env). [sim] *(with M3)*
- [x] ~~Fold `PlacePaint` into `PlaceOverlay(arrow|paint)` per 07~~ — superseded
      2026-07-26 (Q97): painting became blueprint-flow LABOR while overlays stay
      instant signage, so the two commands are correctly distinct; 07's list updated.
- [x] `RepairPrinter` re-priced in Data (~60) once Data exists (M4). [sim] *(with M4)*
- [x] `fault_damage` 5→2 — the spec figure (Q84's manifest), landed with Q109: at 5 any
      crash-loop killed a stock bot in ~25 s. ⚠HASH (golden regenerated).
- [ ] Remaining tuning-to-spec numbers: boot_ticks 2→~20, print_ticks 5→~100 (M0 data
      files) — a pacing pass that deserves its own session. ⚠HASH
- [ ] **The `try_*` completion pass** (Q109 + Q110, backlogged) — `try_attack()` returning
      `False` on a dead or out-of-range target lands with `try_move_to()`; one coherent pass
      over the fault-free family rather than two one-offs. `try_attack` closes Q110's residual
      (a bound target that dies mid-walk faults on a stale handle). [pyrite][sim]
- [ ] **`try_move_to()`** (Q109, backlogged) — the fault-free mover, joining
      `try_deposit`/`try_withdraw`/`try_send`: returns `False` instead of faulting when no
      route exists (paint-blocked, walled off, or demolished bridge). The right answer for
      programs that want robustness; deliberately NOT the only mitigation, since it is
      opt-in. [pyrite][sim]
- [x] Snow tile comment cites superseded Q67 — re-pointed at Q78 (map.rs + scene.rs,
      2026-07-26). [game]
- [x] Thought-cloud states to the doc's list (normal/boot/handler/searching/low-health/abort)
      switched on VM run state rather than view-derived flags. [game] *(with M3; searching
      lands with M7's stance)*
- [x] Fog view, Q70 gem gate: a gem on a memory tile must hold its last-observed scale and
      existence — today its scale tracks the live amount and it despawns at 0 under fog,
      leaking amounts the docs say are live-only-when-seen. [game] *(with Q92's strict
      snapshot, 2026-07-25)*
- [x] Sim: `Blueprint` gains a `faction` field so the view can snapshot-gate enemy
      blueprints (Q92 exempts them until then). *(2026-07-26: field set at placement,
      hashed; the view spawns enemy designations only while watched, despawns ghosts
      on the next look — own blueprints always live.)* [sim] ⚠HASH
- [x] Fog view: replace `gate_fogged_views`' hand-enumerated object registries with a
      `FogGated { pos, dims }` component attached at spawn in `sync_view`, so new spawn
      paths can't silently skip fog gating (2026-07-25 review). *(2026-07-26: all eight
      classes carry it; ghosts keep theirs, so Q92 memory keeps dimming.)* [game]
- [x] Sim: completing a Barricade clears the tile's overlay and paint entries (the
      2026-07-26 tile-composition rule: an unwalkable building shares with nothing).
      *(Done 2026-07-26, `barricade_swallows_signage` guards it.)* [sim] ⚠HASH
- [x] Sim: painting becomes serviced labor (Q97) — `PlacePaint` places a
      faction-attributed `BlueprintKind::Paint` designation (Q86 auth applies) serviced
      like any blueprint (`paint_ticks` 3, material-free); erase = designating
      `unpainted`. Pathfinding honors the Q95/Q96 args: `move_to`/`wander`/`explore`
      take `only=`/`avoid=` (paint constants `unpainted`/`red`/`green`/`blue`/`yellow`,
      bare or Tier-5 list); forbidden colors are impassable to that route search
      (A* + sidesteps + wander/explore picks), engine walks stay paint-blind.
      *(Done 2026-07-26; tests in terrain.rs + building.rs.)* [sim][pyrite][game] ⚠HASH
- [x] Sim: per-faction **known-tiles** joins the world (Q94) — `run_perception`
      records every faction's seeing union into hashed `World.known_tiles` (+ the
      derived, unhashed `visible_tiles` live union), known-node ground folded in;
      `recompute_fog` now just reads both — the view-side eye mirror is gone.
      *(Done 2026-07-26; `seen_tiles_are_durable_sim_state` guards it.)* [sim+game] ⚠HASH

## Decided-but-unbuilt (design ruled, implementation pending)

- [ ] **Ruined Upgrade Station in the start base** (P1 ruling, 2026-08-01) — the
      Red-Fabricator pattern: start-state generation places a ruined Upgrade Station
      in every player base; repairable for tier-0/1 materials (`tuning.ron`) through
      the existing repair flow; sells nothing until repaired. Closes the bootstrap
      deadlock (Station prices in Chips → Crystal → drill grade **4** → Station;
      even the Foundry's Bronze needs grade 2). The
      seller-side ladder corollary ([03-resources/harvest-tiers.md](03-resources/harvest-tiers.md))
      is a load-time assert candidate alongside Q118's three. [sim][game]
      ⚠HASH (start state changes)
- [ ] **Perk-formula spec** (P5 ruling, 2026-08-01) — the bounded hyperbolic
      evaluates as `(max_bonus × level) / (level + K)`, floor division, one
      integer expression; `xp.ron` load asserts every perk grants ≥ 1 whole
      unit by its track's L5; perk progress is additionally computed in
      centi-units for UI display (granted stat stays whole). [sim][game]
      ⚠HASH once perks land (grouping decides granted values)
- [ ] **Structure-pool query domain** (P22 ruling, 2026-08-02 form) — `closest`/`exists`
      on structure and designation kinds resolve from the **faction's knowledge
      pool**: own colony state (always current) plus, while an ally vision grant
      stands, the granting ally's own structures/designations as the ally knows
      them; revoke removes them. Foreign structures are not query-reachable
      (Q126 ruled: no v1 surface). **No new sim state** — the pool
      reads colony state and grants, both already hashed. Canonical hurt window
      gains the `exists` guard
      ([01-language/signals-and-logging.md](01-language/signals-and-logging.md)).
      [sim] ⚠HASH (query-domain change only)
- [ ] **Delete signal-safety** (redesign 2026-08-02, supersedes Q49/Q51) — mostly deletion,
      and **not hash-affecting**: these are deploy-time checks, so execution is unchanged and
      every stored golden replay contains programs that already passed. `crates/pyrite`:
      drop `analysis::check_windows`, `window_cap`, `window_usage` and `signal_safe`; drop the
      `signal_safe` column from `data/builtins.ron` (55 entries) and the per-signal window-cap
      entries from `data/costs.ron`; delete the handler-context bit from the parser and the
      matching `PyriteError` variants. **Keep `def_worst_case`** — it stops being a gate and
      becomes a readout, so it must now *return* `unbounded` for a cycle or loop node instead
      of erroring. `crates/game`: the editor stops greying anything in a window and shows the
      worst-case (or `unbounded`) badge; add the one deploy-time **warning** for an unbounded
      window (unbounded loop, or a **channel** call with `timeout=None` — action-blocking
      verbs like `move_to` always resolve and must not warn). Tests in
      `pyrite/tests/language.rs` that assert rejection invert to assert acceptance.
      **`crates/sim` is in scope**: `sim.rs` calls `check_windows` on the deploy path twice
      (`Command::SpawnBot`, `Command::DeployProgram`), and `game/src/editor/window.rs` twice
      more — all four call sites go with the function, which is what makes this a change to
      *which deploy Commands succeed*. [pyrite][sim][game] (M)
- [ ] **Function-granularity tree-shaking** (Q61) — deploy assembles the artifact from only
      the module functions transitively called by the program and its handlers
      ([01-language/modules-and-library.md](01-language/modules-and-library.md)); the sandbox
      currently ships whole imported modules. Memory charge, version hash, and decryption
      leakage all key off the tree-shaken artifact, so the whole-module stopgap overcharges
      program memory and over-leaks library code. [sim] ⚠HASH (version hashes change)
- [ ] **The Pump** (Q98, 2026-07-26) — the water source the Upgrade Station's coolant needs.
      Two tiles: intake in any Water tile + pump house on orthogonally adjacent walkable
      ground. `Structure` gains an optional second tile (`intake: Option<TilePos>`), NOT a
      general footprint system; `structure_at`/A*-blocked sets/spawn guards cover both tiles;
      `PlaceStructure` validates the (walkable house, water intake, orthogonal) triple and
      takes an intake side; extraction ticks Water into the house's output buffer (rate + cap
      in `upkeep.ron`/`tuning.ron`); adjacency to either tile counts for damage; the house
      carries the one seeing circle and the entity position. Game: two meshes.
      [sim][game] ⚠HASH
- [ ] **Barricade HP** (Q99, 2026-07-26) — walls become targets. Blight-Core-shaped:
      `world.barricades: BTreeMap<EntityId, Barricade { pos, hp }>` (hashed), tile stays
      `TileKind::Barricade` for passability/LoS, 0 HP reverts the tile to Plains (the
      Demolish path, ground stack stays cleared); built by the existing Barricade blueprint;
      a `barricade` kind constant joins `KINDS` + `find_kind`. **Blocked on Q127** for the
      registry's allegiance field and the query domain — the "perception-gated like
      structures, unlike `blight`" line written here at Q99 time is the text P29 registers,
      so build neither until the ruling lands; `attack()`'s victim lookup and the damage settle learn
      the new registry — add a `DamageTarget::Barricade` variant (Q102's second half landed
      the enum, so the path already exists). First-pass HP scaled to the 20-Stone price.
      [sim][game] ⚠HASH

## M16 — Capability slots (Q105) ⚠️ REVERTED; REDESIGNED AS M16b (2026-07-27)

> **Status.** M16 shipped 2026-07-26 and was reviewed three times (xhigh, max,
> max), confirming **45 defects** — and each fix commit was found by the next
> pass to have introduced more than it closed. Both fix commits are reverted;
> the tree is M16-as-originally-built and the attempts are preserved at tag
> `m16-fix-attempts`.
>
> The 45 sorted into five clusters, none a coding slip: tier-scaled XP storage
> (11 findings, 3 failed fixes), the structure-by-labor completion path (11, 3
> failed fixes), Q100's Processing track (5, 2 failed fixes), tier gates vs.
> tier-blind queries (6), and residue from the retired module system (4). Each
> was a decision M16 implemented without ever making. **Q111–Q123 now decide
> all of them**, and the answer is a materially simpler and different design —
> so this is a **fresh build against a spec, not a repair**. Most of the
> reverted code implements concepts that no longer exist.

### M16b — the rebuild scope

**Do not start by un-reverting.** Only four pieces of `m16-fix-attempts` are
worth recovering: the `ops_executed` ordering in `vm.rs` (count *after* the
budget check), the `ops_seen` rebase on VM swap, closing the `PlaceBlueprint`
free-structure laundering, and a handful of genuinely repaired tests.

- [ ] **XP core (Q111, Q121, Q123)** — ten tracks (Boot and Learning deleted),
      `i64` **centi-points**, one quadratic curve with a **per-track
      `curve_base`**, **uncapped**, strictly monotonic. Total level = the mean
      across the ten. Delete `Capability`, `tiers[]`, `tier_value()`,
      `TIER_INVESTMENT_WEIGHT`, `TierSpec` and the tier catalog, every `tier_*`
      stat, `tier_xp_scale_pct`, `track_scale`, `capability_level`,
      `track_cap_deci(_scaled)`, the settle-time clamp, `UpgradeOrder::Tier`,
      the Q105-R1/R3 validations, `learning_carry`, and `settle_xp`'s second
      pass. Age income → **0.2 deci/tick**; per-track bases per Q123's table (also carried in [02-agents.md](../docs/02-agents.md), the owning doc).
      ⚠HASH + units migration. [sim]
- [ ] **Tools (Q111, Q118)** — ten tools, one per track (drill, build tool,
      weapon, optics, CPU, hull plating, drivetrain, signature dampener, gyros,
      cargo rack). Grade 1 free with the chassis, **grades 2–5 purchasable** —
      ~40 catalog entries against today's 12. **Bought** with materials and
      **licensed by level**: the specific skill's *or* the total. No separate
      use-gate (XP never decreases, so a bot cannot hold an unlicensed tool);
      quirks may grant tools outright. [sim]
- [ ] **Three load-time assertions (Q118)** — anti-circularity (no tool priced
      in a material its own ladder unlocks at or above the grade being bought,
      resolving refined goods through their recipes); no orphan materials; no
      gaps in a tool's grade sequence, so no level is dead. [sim]
- [ ] **Perks (Q121)** — tools carry the step changes; qualitative growth is
      **sparse milestones** at named levels; genuinely continuous perks use the
      bounded integer hyperbolic `max × level / (level + K)`. Rewrite the whole
      perk table off linear-per-level. [sim]
- [ ] **Upkeep (Q122)** — same hyperbolic on `Σ levels` (it lost its 60-level
      ceiling); the `draw_per_module` term re-bases on installed tools. [sim]
- [ ] **Compute pacing (Q118)** — the compute ladder starts on **Wire** and
      escalates (Wire → Silver+Wire → Chips → Gold Chips), and program-capacity
      buys (memory bank, stack ext, log buffer) start on Wire too. Program size
      must not sit behind maxed mining. [sim]
- [ ] **Coolant (Q119)** — declared **per catalog entry in data**, not by code
      branch. Compute family only. Surface what a blocked Station order is
      waiting on. [sim][game]
- [ ] **Backup Core: DELETE it (Q115)** — the item preserved capability tiers
      and Q111 deleted tiers, so it has nothing left to preserve. Remove the
      catalog entry, `UpgradeEffect::BackupCore`, and every doc reference;
      total loss on destruction becomes unconditional. [sim][game][docs]
- [ ] **Structures by labor (Q120, amended)** — a completing build
      **displaces** the occupant: BFS outward from the site over passable
      tiles, first free tile, ties on lowest `(x, y)`. **Nothing dies** — the
      entombment death was cut. If the BFS exhausts the component (a bot
      sealed in a pocket) the build **holds, non-minting and UI-visible**
      (P3 ruling): re-park, no progress, no XP, no fault; the held state
      shows as a "build held: no room" badge derived from sim state. The displaced bot's action is **re-planned, not
      failed**, so being pushed never costs HP. Never delete the designation.
      Blueprints stay passable. [sim]
- [ ] **Queries (Q117)** — add `closest_minable(kind)` and
      `exists_minable(kind)` beside the untouched tier-blind `closest`/`exists`
      (docs/01 needs no amendment). Add `try_mine()` with the backlogged
      `try_*` family. Both new queries scope to `known_nodes` and sort
      `(distance, id)`. [sim][pyrite]
- [ ] **Shipped programs (Q117, Q108, Q110)** — re-sync code to the now-ratified
      doc sources: the Tier-0 starter (docs/01 syntax-tiers, P7 form) into the
      GREEN/RED sandbox programs, and the Harvester's P10 form into
      `crates/sim/src/feral.rs` (minable-scoped queries, try_ verbs, bound
      target, wander tail). docs/04 carries the sources verbatim again. [game][sim]
- [x] **Doc sync** — docs/03's ladder paragraph still says buying a tier
      "resets that capability's earned level" (Q111 deleted that); docs/02 and
      docs/06 carry the tier/level model throughout. [docs] *(Resolved
      2026-08-02: already done by the P9/P11 fixes and the Q111 propagation
      sweeps — grep finds neither the quoted docs/03 paragraph nor a live
      tier/level carrier in docs/02 or docs/06.)*

**Method, learned the hard way.** Build in verified slices, and **write the
failing test before deciding the fix** — the one discipline that worked when it
was applied, and that was abandoned as soon as the items got small. Five tests
across this milestone passed against broken code (two asserted on a helper
rather than an observable, one had its action overwritten by the bot's own
program, one was satisfied by a wreck expiring, one by a stale counter healing
inside the test window), and the structure-by-labor completion arm had **no
end-to-end test at all**, which is why its worst defect shipped green.

## M17 — The overlay pipeline (Q101) — NOT STARTED

One flat `cost_overlay_centi` is the whole system today, so docs/05's overlay table and five
written-up quirks are unbuildable. ⚠HASH.

- [ ] **Rules, not a flat surcharge** — a `CostOverlay` = one rule per key (delta and/or
      multiplier) over `CostTable`'s named op classes + builtin names; `Vm::charged` resolves
      `floor₁(region(tile(base + Σ per-bot deltas)))`. Per-bot deltas FIRST — terrain amplifies
      quirks by design. Specific row beats general within a layer. [pyrite][sim]
- [ ] **Per-bot overlays** — unblocks the five queued quirks (Tail-Call Optimized, Kernel
      Bypass, Dial-Up, Telemetry Enabled, Eventual Consistency) and Q75's perk slot (Scouting
      L3's Corruption exemption becomes an ordinary per-bot rule). [sim]
- [ ] **Regions** — `MapSpec` gains region definitions + a per-tile region index (parallel to
      the terrain grid: deterministic, O(1), hashed with the map, never per tick). Authored
      biomes (Static Wastes, Loop Desert, Overclock Field) and **boss biomes** live here.
      Corruption's tax stays TILE-based — a region-scoped tax would vanish with its Blight
      Core and make Cleanse pointless. [sim]
- [ ] **Forced charges become taxable** — drop M8's exemption; debt (Q75) makes it safe, and
      Overclock Field's ×2 crash dump needs it. [pyrite]
- [ ] **`bank_cap` → flat ceiling (~100 cycles) + load-time validation** that no overlay pushes
      a non-forced op above it; delete the per-tile derivation and the overlay-margin term in
      `grant_centi`. [pyrite][sim]
- [ ] **Editor shows EFFECTIVE per-line costs** for the selected bot's tile (docs/05 promises
      it; `analysis::line_costs` paints base costs today). [game]

## Small decided-but-unbuilt items (from the 2026-07-26 sweep)

- [ ] **Data Exchange** (Q106) — `ExchangeResources { faction, data, kind }`-style Command at a
      built Research Archive: flat rate table in data (Chips-favored, Gold densest per unit),
      no scarcity scaling. Data's only other sinks are finite research + printer repair, so
      this is what keeps Data worth earning late. [sim][game]
- [~] **Feral sources: doc/code agreed at Q108** — SUPERSEDED. The Q108-era parity is gone:
      docs/04's archetype sources were since restated to the P7/P10/P16 forms, so the code
      re-sync is genuinely pending and is tracked in **M16b → *Shipped programs*** above.
      `feral.rs`'s "NEEDS DISCUSSION / flagged in TASKS.md" comment **stays** until that
      lands — it is the only in-code marker of the divergence. [sim]
- [x] **`QueuePrint(loadout)`** (Q104) — parameter deleted from docs/07; the shipped
      per-faction counter was always the whole feature. No code change.
- [x] **Alliance decryption** (Q107) — shipped forward-only pooling ratified; docs/07's
      "never decryption" line corrected. No code change.

## Post-sweep review refinements (2026-07-26) — fold into M16/M17

- [ ] **Q101-R1** — the load-time `bank_cap` check evaluates the WORST CASE per key:
      `region(tile(base + largest cost-raising per-bot delta))`, not overlays alone. Validating
      overlays only leaves quirked bots outside the certified invariant and freeze-forever
      reachable for them. [pyrite][sim]
- [~] **Q105-R1** — SUPERSEDED by Q111 (tiers deleted); replaced by Q118's three
      catalog asserts. Original: load-time assert that each capability tier's grant ≥ the L5 bonus of the
      tier below, so a bought upgrade is never a net downgrade of the stat it buys (Optics
      tier 2 must not leave a Scouting-L5 scout seeing less). [sim]
- [~] **Q105-R2** — RESTATED for Q111: a **build tool of grade ≥ 2** gates field repair (wreck rescue), `hijack`, and nest
      claim/raze; base tier 1 covers `build()` and structure `repair()`. Base weapon damage
      comes from Combat tier 1. Replaces the deleted build-tool gate. [sim]
- [~] **Q105-R3** — RESTATED for Q111/Q115 (the P8 follow-up): the scrap valve and
      `SelectKey::TotalXp` rank by INVESTMENT (lifetime XP + **the value of installed
      tools** — tiers and the Backup Core are deleted), not raw XP: a tooled-up veteran
      with low XP must never be selected as the fleet's cheapest machine. [sim]
- [~] **Processing track** — PARTLY SUPERSEDED by Q111/Q121: Processing is one of the **ten**
      tracks (not a twelfth), and the Learning clause is void — Learning was retired entirely,
      so phase 7 has no second pass. What stands: Processing's income (first pass: 1 per 10 ops
      executed), its perk magnitudes, and its slot in the ten-track settlement order. [sim]

## Verb-layer index (every spec'd builtin → its milestone)

✅ = host implementation landed. As of M15 every verb below has landed; the
milestone column records which milestone shipped it. Still-unbuilt verbs
(`try_move_to`, `try_attack`, `closest_minable`/`exists_minable`/`try_mine`)
live in the quick-wins backlog and M16b above, not here.

| Verb | Milestone | | Verb | Milestone |
|---|---|---|---|---|
| `abort` ✅ | M3 | | `is_seen` ✅ | M7 |
| `setenv`/`getenv` ✅ | M3 | | `search`/`wander`/`explore` ✅ | M7 |
| `log(level=)` ✅ | M3 | | `path_blocked` ✅ | M7 |
| `withdraw`/`try_withdraw` ✅ | M4 | | `creep=` arg ✅ | Q103 |
| `deposit`/`try_deposit` ✅ | M4 | | `repair`/`salvage`/`analyze` ✅ | M10 |
| `cargo_count` ✅ | M4 | | `hijack`/`recover_black_box` ✅ | M10 |
| `study` ✅ | M15 | | `guard`/`escort` ✅ | M10 |
| `scan_resources` ✅ | M4 | | `send`/`receive`/`broadcast` + `try_*` ✅ | M11 |
| `my_quirks`/`has_quirk` ✅ | M6 | | `scan_enemies` ✅ | M7 |

Existing and staying: `closest`, `exists`, `move_to`, `mine`, `build`, `attack`, `wait`,
`rng`, `log`, `upload_log`, `upload_crash_dump`, `cargo_full`, `health_low`, `last_error`,
`handler_init`, `drop_cargo` ✅ (host impl landed with M4).
