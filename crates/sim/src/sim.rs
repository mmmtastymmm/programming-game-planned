//! The deterministic nine-phase tick loop (docs/07-architecture.md):
//!
//! 1. apply agreed Commands (caller does this via [`Sim::apply`])
//! 2. grant cycles, step every VM (stable BotId order)
//! 3. collect issued actions (recorded as `ActionRequest`s during step)
//! 4. resolve actions per bot in id order, then engine-driven walks
//! 5. perception (stub until M7)
//! 6. damage settlement, signal dispatch by severity, deaths → wrecks
//! 7. XP settlement (awards for bots that died in 6 drop with them)
//! 8. economy: regen, refineries, printers
//! 9. state hash for desync detection ([`Sim::state_hash`])

use crate::hash::Fnv1a;
use crate::host::BotHost;
use crate::map::{MapSpec, OverlayKind, TileKind, TilePos};
use crate::world::{
    Blueprint, BlueprintKind, Bot,
    BotData, BotId, Color, ColorProgram, EntityId, PrinterState, Wreck,
    World, XpTrack,
};
use pyrite::{CostTable, Outcome, PyriteError, UnlockSet, Value, Vm, VmConfig};
use std::rc::Rc;

// (Melee damage moved to tuning.ron `attack_damage` with M6 — every
// number is data; per-weapon hardware still lands later.)

/// Sim tuning constants (all numbers are data — CLAUDE.md convention; the
/// values live in `data/tuning.ron`, baked in at compile time and parsed
/// once at load).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub print_ticks: u32,
    /// Steel (deci-units) per print. DEFAULT FREE: a colony must never be
    /// soft-locked out of bots (no steel + no bots = dead end). Maps/
    /// servers can set a cost; population is bounded by dials and capacity
    /// either way.
    pub print_cost_steel: u64,
    /// Repairing the ruined Red printer prices in DATA (docs/03: ~60 —
    /// the flagship early Data sink).
    pub repair_cost_data: u64,
    pub scrap_refund_steel: u64,
    /// mine() yield per swing, deci-units (docs/03 first-pass: 2 units).
    pub mine_yield_deci: u32,
    /// Ticks one mine() swing takes (every number is data — CLAUDE.md).
    pub mine_swing_ticks: u32,
    /// Regenerating nodes (Groves) gain this many deci-units per
    /// regen_interval_ticks, up to the global node_regen_cap_deci ceiling
    /// (per-node caps at the seeded amount are map-gen territory, Q71).
    pub node_regen_deci: u32,
    pub node_regen_cap_deci: u32,
    /// Data income: 20 per this many deci-units delivered (docs/03).
    pub delivery_milestone_deci: u32,
    pub milestone_data: u64,
    pub first_kill_data: u64,
    /// Generic structure durability (per-kind sheets land with M5 data).
    pub structure_hp: i64,
    /// Refinery batch duration (docs/03 first-pass: ~30 ticks).
    pub recipe_batch_ticks: u32,
    /// Boot Sequence duration — an engine interrupt context.
    pub boot_ticks: u32,
    /// Typed build prices per structure kind (docs/03), in UNITS —
    /// every number is data, not code (CLAUDE.md).
    pub structure_costs: Vec<(crate::world::StructureKind, Vec<(crate::resources::Resource, u32)>)>,
    /// Melee damage per hit (per-weapon hardware lands later).
    pub attack_damage: i64,
    // (printed_* chassis defaults moved to data/stats.ron with M5 — the
    // universal floor statline is the print.)
    /// Colony population the economy sustains before scrap recalls fire
    /// (Energy upkeep stands in later; docs/02).
    pub capacity: u32,
    /// Rammer's total at-fault stun (50 = 5s @10Hz): expressed as the bump
    /// FACTORY WINDOW's `wait(bump_freeze_ticks - handler_init_ticks)` on
    /// top of the forced flinch, and applied directly on engine walks. The
    /// at-fault party sits longest — by the time it re-plans, the victim
    /// has cleared the scene. (The victim's shorter stagger IS the flinch:
    /// the old bump_victim_freeze_ticks died with the template model.)
    pub bump_freeze_ticks: u32,
    /// The forced template prologue: every entry waits this long first
    /// (the visible flinch). Boot's prologue is the upload instead.
    pub handler_init_ticks: u32,
    /// Collisions are accidents: BOTH bots take this chassis damage.
    pub bump_damage: i64,
    /// Bridges price in STONE (docs/03: Stone owns civil works). Deci.
    pub bridge_cost_stone: u64,
    /// Builder-ticks of labor a bridge takes.
    pub bridge_build_ticks: u32,
    /// Placing a traffic overlay (arrow) — instant signage.
    /// Overlays (arrows) price in Stone too — signage is civil kit. Deci.
    pub overlay_cost_stone: u64,
    /// Chassis damage per UNHANDLED fault: crash loops are lethal, and
    /// `on error:` handlers are literal armor (handled faults are free).
    pub fault_damage: i64,
    /// Passive self-repair: +regen_amount hp every regen_interval_ticks.
    pub regen_interval_ticks: u64,
    pub regen_amount: i64,
    /// The Damaged line, in percent of max hp: the hurt signal fires when
    /// hp drops below it and the latch re-arms when regen climbs back over
    /// it — ONE value so the edge trigger can't drift apart. M3's env
    /// registry makes it per-bot (`hurt_line`); this is the match-wide
    /// default until then.
    pub hurt_line_pct: i64,
    // --- perception (M7, docs/05) ---
    pub sense_factor_pct: u32,
    pub structure_sensors: u32,
    pub episode_rearm_ticks: u32,
    pub search_ring_ticks: u32,
    pub explore_radius: u32,
    pub wander_leg: u32,
    pub ford_quiet: i64,
    // --- terrain v2 (M8, docs/05 Q35–Q40) ---
    /// The ×2-scale move-cost table + Mountain/Mud edge parameters.
    pub tile_costs: crate::map::TileCostTable,
    /// Dunes idle-sink interval / per-interval surcharge / total ceiling.
    pub dune_sink_ticks: u32,
    pub dune_sink_step_x2: u32,
    pub dune_sink_cap_x2: u32,
    /// Scree collapses to Rubble after this many bot entries (Q40).
    pub scree_crossings: u32,
    /// Corruption's per-op cycle tax, in CENTICYCLES (docs/05; M8-B).
    pub corruption_op_tax: u64,
    /// Blight Cores corrupt one nearby tile per this many ticks (M8-C).
    pub corruption_spread_ticks: u64,
}

impl Default for Tuning {
    fn default() -> Self {
        let tuning: Tuning = ron::from_str(include_str!("../data/tuning.ron"))
            .expect("data/tuning.ron parses (unknown fields are errors)");
        tuning.validate();
        tuning
    }
}

impl Tuning {
    /// Load-time sanity: durations that gate progress must be non-zero
    /// (a zero here means division-by-zero ticks or instant loops, not a
    /// legitimate tuning choice).
    fn validate(&self) {
        assert!(self.print_ticks > 0, "tuning: print_ticks must be > 0");
        assert!(self.bridge_build_ticks > 0, "tuning: bridge_build_ticks must be > 0");
        assert!(self.regen_interval_ticks > 0, "tuning: regen_interval_ticks must be > 0");
        // The old hardcoded 2 was the implicit >=1 guard; the Mine advance
        // decrements unchecked, so a zero here would underflow-freeze
        // every miner.
        assert!(self.mine_swing_ticks > 0, "tuning: mine_swing_ticks must be > 0");
        for kind in crate::world::StructureKind::ALL {
            assert!(
                self.structure_costs.iter().any(|(k, _)| *k == kind),
                "tuning: structure_costs must price every kind ({} missing)",
                kind.name()
            );
        }
        assert!(
            (1..=100).contains(&self.hurt_line_pct),
            "tuning: hurt_line_pct must be a percentage in 1..=100"
        );
        // Terrain v2: A*'s heuristic is manhattan × 1, admissible only
        // while every edge costs at least 1; and Water/Barricade are the
        // impassable kinds, so the table must not price them.
        for &(kind, cost) in &self.tile_costs.x2 {
            assert!(cost >= 1, "tuning: tile_costs.x2 must be >= 1 ({kind:?})");
            assert!(kind.passable(), "tuning: tile_costs.x2 prices impassable {kind:?}");
        }
        for cost in [
            self.tile_costs.mountain_climb_x2,
            self.tile_costs.mountain_descend_x2,
            self.tile_costs.mud_loaded_x2,
        ] {
            assert!(cost >= 1, "tuning: mountain/mud edge costs must be >= 1");
        }
        assert!(self.dune_sink_ticks > 0, "tuning: dune_sink_ticks must be > 0");
        assert!(self.scree_crossings > 0, "tuning: scree_crossings must be > 0");
        assert!(
            self.corruption_spread_ticks > 0,
            "tuning: corruption_spread_ticks must be > 0"
        );
    }
}

/// Hash a Station order (queued or mounted) into the phase-9 snapshot.
fn hash_order(h: &mut Fnv1a, order: &crate::world::UpgradeOrder) {
    match order {
        crate::world::UpgradeOrder::Compute(idx) => {
            h.write_u8(1);
            h.write_u8(*idx);
        }
        crate::world::UpgradeOrder::Module { idx, replace } => {
            h.write_u8(2);
            h.write_u8(*idx);
            // Presence + raw value, NO arithmetic: `slot + 1` overflowed
            // on a hostile replace=Some(255) command (hashing must never
            // panic on queueable state).
            h.write_u8(replace.is_some() as u8);
            h.write_u8(replace.unwrap_or(0));
        }
    }
}

/// Upkeep config (M5, docs/02-03 Q84): the data-driven resource mix —
/// v1 = Energy (primary drain) + Steel (chassis maintenance). Values in
/// `data/upkeep.ron`; maps with `dev_free_power` (the default) skip the
/// whole settlement.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upkeep {
    pub interval_ticks: u64,
    pub base_draw: u64,
    pub draw_per_upgrade: u64,
    pub draw_per_module: u64,
    /// Per XP track LEVEL (M6, docs/02: veterans cost more to run).
    pub draw_per_track_level: u64,
    pub draw_per_refinery: u64,
    pub generator_output_wood: u64,
    pub generator_output_coal: u64,
    pub generator_fuel_deci: u32,
    pub generator_stoke_deci: u32,
    pub geothermal_output: u64,
    pub steel_per_bot_deci: u64,
    pub rust_decay_hp: i64,
    pub rust_scraps: bool,
}

impl Default for Upkeep {
    fn default() -> Self {
        let upkeep: Upkeep = ron::from_str(include_str!("../data/upkeep.ron"))
            .expect("data/upkeep.ron parses (unknown fields are errors)");
        assert!(upkeep.interval_ticks > 0, "upkeep: interval_ticks must be > 0");
        assert!(upkeep.generator_fuel_deci > 0, "upkeep: generator_fuel_deci must be > 0");
        upkeep
    }
}

/// External inputs: the ONLY way anything outside the sim mutates it
/// (single-player is lockstep with one peer).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    /// Test/debug spawn that bypasses printers (bots normally print).
    SpawnBot {
        pos: TilePos,
        source: String,
        cpu: u64,
        cargo_cap: u32,
        faction: u8,
        hp: i64,
        color: Color,
    },
    /// Deploy source to a (faction, color) slot: new prints and re-colors
    /// use it immediately; live bots of that color hot-swap at their next
    /// loop boundary (docs/01).
    DeployProgram { faction: u8, color: Color, source: String },
    /// The population dial on a printer.
    SetDesiredMax { printer: EntityId, value: u32 },
    /// Fix a ruined printer (Data cost; ore stands in until Data exists).
    RepairPrinter { printer: EntityId },
    /// Designate a terraform site (the build UI's output). Bots do the
    /// labor via closest(blueprint).expect()/build(). The placing faction
    /// pays from its stock (serde-defaulted for stored replays).
    PlaceBlueprint {
        pos: TilePos,
        kind: BlueprintKind,
        #[serde(default)]
        faction: u8,
    },
    /// Set or clear a traffic overlay on any tile — instant signage, not
    /// construction (small ore cost to place; clearing is free).
    PlaceOverlay {
        pos: TilePos,
        overlay: Option<OverlayKind>,
        #[serde(default)]
        faction: u8,
    },
    /// Set or clear cosmetic tile paint (free).
    PlacePaint { pos: TilePos, color: Option<u8> },
    /// DEV-ONLY emergency stop (M3: abort() is the only PLAYER scuttle):
    /// straight to wreck, no template — the owner pulled the plug. Logs
    /// ride in the wreck; carried cargo spills onto the ground. Kept for
    /// tests, the editor Kill tool, and the golden replay fixture.
    KillBot { bot: BotId },
    /// Place a structure, paying its typed cost from colony stock
    /// (docs/03: payments are abstract). Instant placement for now; build
    /// labor for structures is flagged for discussion in TASKS.md.
    PlaceStructure { pos: TilePos, kind: crate::world::StructureKind, faction: u8 },
    /// Set (or clear) a refinery's recipe by RECIPES index (docs/03,
    /// round 4: recipe set per structure).
    SetRecipe { structure: EntityId, recipe: Option<u8> },
    /// Queue a Station order for a bot by catalog name (docs/03 M5:
    /// designation is the player's; the PROGRAM must bring the bot to a
    /// pad). `replace` names the module slot a swap destroys (modules
    /// only; required when every slot is full). Unknown names and invalid
    /// slots are ignored — lockstep commands never error.
    QueueUpgrade {
        bot: BotId,
        order: String,
        #[serde(default)]
        replace: Option<u8>,
    },
    /// Spend colony Data on a permanent construct unlock (docs/03/06:
    /// research is structure-free — the Archive is the bank, not the
    /// school).
    Research { faction: u8, construct: pyrite::Construct },
}

pub struct Sim {
    pub world: World,
    pub costs: CostTable,
    pub vm_config: VmConfig,
    pub tuning: Tuning,
    /// The universal chassis: floor statline, modifier-pipeline penalties,
    /// and the Upgrade Station catalog (`data/stats.ron`, M5).
    pub stats: crate::stats::Stats,
    /// Energy + Steel upkeep config (`data/upkeep.ron`, M5).
    pub upkeep: Upkeep,
    /// XP curve, incomes, and perk magnitudes (`data/xp.ron`, M6).
    pub xp: crate::xp::XpConfig,
    /// The quirk catalog + manifestation thresholds (`data/quirks.ron`, M6).
    pub quirks: crate::quirks::QuirkCatalog,
    /// Phase-9 snapshot: the state hash of the last completed tick. The
    /// lockstep relay compares this across peers for desync detection.
    pub last_hash: u64,
}

impl Sim {
    pub fn new(spec: &MapSpec) -> Self {
        let tuning = Tuning::default();
        let mut vm_config = VmConfig::default();
        // FACTORY WINDOW contents (docs/01's template table), as REAL
        // Pyrite: watchable, line-highlighted, costed, replaceable by the
        // player's own `on <signal>:` block. Hurt, bumped, and boot ship
        // empty — the forced prologue flinch IS their default reaction —
        // so they simply have no entry here.
        let factory_windows = [
            (pyrite::ast::SignalKind::Error, "upload_crash_dump()\n".to_string()),
            // + the 15-tick init flinch = the rammer's 50-tick at-fault stun.
            (
                pyrite::ast::SignalKind::Bump,
                format!(
                    "wait({})\n",
                    tuning.bump_freeze_ticks.saturating_sub(tuning.handler_init_ticks)
                ),
            ),
        ];
        for (kind, source) in factory_windows {
            let program =
                pyrite::parse(&source, &UnlockSet::all()).expect("factory windows parse");
            vm_config.default_handlers.insert(
                kind,
                pyrite::vm::DefaultHandler { source, program: Rc::new(program) },
            );
        }
        // Entity-kind constants for the generic queries: `closest(ore)`,
        // `exists(blueprint)`, ... They live in the config (not globals) so
        // they survive the post-fault VM reset; assignments can shadow them.
        for kind in crate::host::KINDS {
            vm_config.constants.insert(kind.to_string(), Value::Str(kind.to_string()));
        }
        // Log-level constants (docs/01): ordinary shadowable names, ints so
        // the same constants work as env values (`setenv(log_min_level,
        // warn)`) and as `log(x, level=warn)` arguments. One source —
        // `world::LEVEL_NAMES` — shared with the HUD's display prefixes.
        for (rank, name) in crate::world::LEVEL_NAMES.iter().enumerate() {
            vm_config.constants.insert(name.to_string(), Value::Int(rank as i64));
        }
        // Env keys as constants: `setenv(hurt_line, 30)` reads naturally.
        for key in crate::world::ENV_KEYS {
            vm_config.constants.insert(key.name.to_string(), Value::Str(key.name.to_string()));
        }
        // Host-domain fault ids (M7): comparable via last_error().
        vm_config.constants.insert(
            crate::host::UNKNOWN_CONTACT.to_string(),
            Value::Str(crate::host::UNKNOWN_CONTACT.to_string()),
        );
        // Quirk names as constants (docs/09: pre-bound like kind
        // constants, no enum): `has_quirk(overclocked)` reads naturally.
        let quirk_catalog = crate::quirks::QuirkCatalog::default();
        for quirk in &quirk_catalog.quirks {
            vm_config.constants.insert(quirk.name.clone(), Value::Str(quirk.name.clone()));
        }
        let mut sim = Self {
            world: World::from_spec(spec),
            costs: CostTable::default(),
            vm_config,
            tuning,
            stats: crate::stats::Stats::default(),
            upkeep: Upkeep::default(),
            xp: crate::xp::XpConfig::default(),
            quirks: quirk_catalog,
            last_hash: 0,
        };
        // Map-authored structures place free; Generators start STOKED
        // (docs/03: the opening never brownouts before the player acts).
        for (pos, kind) in &spec.structures {
            let id = sim.world.alloc_entity();
            let mut input = std::collections::BTreeMap::new();
            if *kind == crate::world::StructureKind::Generator {
                input.insert(crate::resources::Resource::Coal, sim.upkeep.generator_stoke_deci);
            }
            sim.world.structures.insert(
                id,
                crate::world::Structure {
                    kind: *kind,
                    faction: 0,
                    pos: *pos,
                    hp: sim.tuning.structure_hp,
                    max_hp: sim.tuning.structure_hp,
                    input,
                    output: std::collections::BTreeMap::new(),
                    recipe: None,
                    batch: None,
                    pad: None,
                },
            );
        }
        // The VM's base stack depth is a chassis stat (stats.ron floor;
        // Stack extensions raise it per bot).
        sim.vm_config.stack_depth = sim.stats.stack_depth as usize;
        // Phase-0 upkeep seed (like the perception seed): tick 1 starts
        // with correct brownout/rust flags rather than a settlement-sized
        // grace window.
        if !sim.world.dev_free_power {
            sim.settle_upkeep();
        }
        // Phase-0 perception seed (docs/07, round 4): tick 1's queries have
        // a "previous tick" to read, so the pre-deployed starter program
        // works from its first operation. A stub until M7, like phase 5.
        sim.run_perception();
        sim.last_hash = sim.state_hash();
        sim
    }

    /// Phase 1: apply a command. Deterministic given identical call order.
    /// Returns the new bot's id for spawn commands.
    pub fn apply(&mut self, command: &Command) -> Result<Option<BotId>, PyriteError> {
        match command {
            Command::SpawnBot { pos, source, cpu, cargo_cap, faction, hp, color } => {
                // Bots are solid: a spawn onto an impassable, structure, or
                // occupied tile is rejected (dev command or not — nothing
                // may stack two live bots on one tile).
                let free = self.world.grid.get(*pos).is_some_and(|t| t.move_ticks().is_some())
                    && !self.world.structure_at(*pos)
                    && !self.world.tile_occupied(*pos, BotId(u32::MAX));
                if !free {
                    return Ok(None);
                }
                let unlocks = crate::world::faction_unlocks(&self.world, *faction);
                let program = pyrite::parse(source, &unlocks)?;
                // Deploy-time window analysis (M3): caps, signal safety,
                // loop/recursion ban — rejected here, never at runtime.
                pyrite::check_windows(&program, &self.costs)?;
                let vm = Vm::new(Rc::new(program), self.vm_config.clone());
                // Dev-spawn overrides arrive in human units (cycles/tick,
                // cargo units); stored units are centi/deci (Q56). Inputs
                // are clamped — a lockstep command must never panic the
                // sim (hostile peers, buggy replay files), and hp feeds
                // `hp * 2` / `hp * 100` comparisons downstream.
                let id = self.insert_bot(
                    *pos,
                    *faction,
                    *color,
                    (*hp).clamp(1, 1_000_000_000),
                    cpu.saturating_mul(100),
                    cargo_cap.saturating_mul(crate::resources::DECI),
                    vm,
                    false,
                );
                // The phase-0 perception seed extends to spawns (docs/07:
                // tick 1's queries must have a "previous tick" to read —
                // otherwise a spawned starter program eats one blind-crash
                // before its first perception pass). Deterministic:
                // commands apply in relay order.
                self.run_perception();
                Ok(Some(id))
            }
            Command::DeployProgram { faction, color, source } => {
                let unlocks = crate::world::faction_unlocks(&self.world, *faction);
                let program = pyrite::parse(source, &unlocks)?;
                pyrite::check_windows(&program, &self.costs)?;
                let slot = (*faction, color.0);
                let hash = crate::world::program_hash(source);
                self.world.program_library.entry(hash).or_insert_with(|| source.clone());
                let program = Rc::new(program);
                self.world.color_programs.insert(
                    slot,
                    ColorProgram { source: source.clone(), program: Rc::clone(&program), hash },
                );
                // Hot-swap every live bot of this color at its next loop
                // boundary (docs/01: redeploy semantics).
                for bot in self.world.bots.values_mut() {
                    if bot.data.faction == *faction
                        && bot.data.color == *color
                        && let Some(vm) = bot.vm.as_mut()
                    {
                        vm.queue_program(Rc::clone(&program));
                    }
                }
                Ok(None)
            }
            Command::SetDesiredMax { printer, value } => {
                if let Some(p) = self.world.printers.get_mut(printer) {
                    p.desired_max = *value;
                }
                Ok(None)
            }
            Command::RepairPrinter { printer } => {
                // Re-priced in Data (docs/03: the first colony milestone).
                let cost = self.tuning.repair_cost_data;
                if let Some(p) = self.world.printers.get_mut(printer)
                    && p.state == PrinterState::Ruined
                {
                    let faction = p.faction;
                    let have = self.world.data.get(&faction).copied().unwrap_or(0);
                    if have >= cost {
                        p.state = PrinterState::Working;
                        self.world.data.insert(faction, have - cost);
                    }
                }
                Ok(None)
            }
            Command::PlaceBlueprint { pos, kind, faction } => {
                let valid_site = match kind {
                    BlueprintKind::Bridge => self.world.grid.get(*pos) == Some(TileKind::Water),
                };
                let occupied_by_blueprint =
                    self.world.blueprints.values().any(|b| b.pos == *pos);
                let cost = match kind {
                    BlueprintKind::Bridge => self.tuning.bridge_cost_stone,
                };
                if valid_site
                    && !occupied_by_blueprint
                    && self.world.stock_take(*faction, crate::resources::Resource::Stone, cost)
                {
                    // Progress is deci-units (Q56): base build rate is 10
                    // deci/tick, so ticks-to-complete stays the tuning
                    // figure for an unleveled builder.
                    let needed = match kind {
                        BlueprintKind::Bridge => self.tuning.bridge_build_ticks,
                    } * crate::resources::DECI;
                    let id = self.world.alloc_entity();
                    self.world
                        .blueprints
                        .insert(id, Blueprint { pos: *pos, kind: *kind, progress: 0, needed });
                }
                Ok(None)
            }
            Command::PlaceOverlay { pos, overlay, faction } => {
                if self.world.grid.in_bounds(*pos) {
                    match overlay {
                        Some(kind) => {
                            let cost = self.tuning.overlay_cost_stone;
                            if self.world.stock_take(
                                *faction,
                                crate::resources::Resource::Stone,
                                cost,
                            ) {
                                self.world.overlays.insert(*pos, *kind);
                            }
                        }
                        None => {
                            self.world.overlays.remove(pos);
                        }
                    }
                }
                Ok(None)
            }
            Command::PlaceStructure { pos, kind, faction } => {
                use crate::resources::{Resource, DECI};
                use crate::world::StructureKind;
                let free = self.world.grid.get(*pos).is_some_and(|t| t.move_ticks().is_some())
                    && !self.world.structure_at(*pos)
                    && !self.world.tile_occupied(*pos, BotId(u32::MAX))
                    // The Tap harnesses a vent — vent tiles only (docs/03).
                    && (*kind != StructureKind::GeothermalTap
                        || self.world.grid.get(*pos) == Some(TileKind::Vent));
                // Typed prices live in tuning.ron (docs/03 figures;
                // validated complete at load — every kind has an entry).
                let cost: &[(Resource, u32)] = self
                    .tuning
                    .structure_costs
                    .iter()
                    .find(|(k, _)| k == kind)
                    .map(|(_, c)| c.as_slice())
                    .expect("validated at load");
                let affordable = cost.iter().all(|(k, units)| {
                    self.world.stock_get(*faction, *k) >= (*units * DECI) as u64
                });
                if free && affordable {
                    for (k, units) in cost {
                        let taken =
                            self.world.stock_take(*faction, *k, (*units * DECI) as u64);
                        debug_assert!(taken, "checked affordable above");
                    }
                    let id = self.world.alloc_entity();
                    self.world.structures.insert(
                        id,
                        crate::world::Structure {
                            kind: *kind,
                            faction: *faction,
                            pos: *pos,
                            hp: self.tuning.structure_hp,
                            max_hp: self.tuning.structure_hp,
                            input: std::collections::BTreeMap::new(),
                            output: std::collections::BTreeMap::new(),
                            recipe: None,
                            batch: None,
                            pad: None,
                        },
                    );
                }
                Ok(None)
            }
            Command::SetRecipe { structure, recipe } => {
                if let Some(st) = self.world.structures.get_mut(structure) {
                    let valid = recipe.is_none_or(|idx| {
                        crate::resources::RECIPES
                            .get(idx as usize)
                            .is_some_and(|r| r.station == st.kind.name())
                    });
                    if valid && st.recipe != *recipe {
                        st.recipe = *recipe;
                        // Recipe change scraps the in-flight batch; its
                        // already-consumed inputs are LOST with it, and old
                        // leftovers stranded in `input` stay there (only
                        // `output` is withdrawable) — scrapping mid-batch
                        // deliberately wastes the feed.
                        st.batch = None;
                    }
                }
                Ok(None)
            }
            Command::QueueUpgrade { bot, order, replace } => {
                let resolved = if let Some((idx, _)) = self.stats.upgrade(order) {
                    // Compute orders never name a slot.
                    (replace.is_none()).then_some(crate::world::UpgradeOrder::Compute(idx))
                } else if let Some((idx, _)) = self.stats.module(order) {
                    Some(crate::world::UpgradeOrder::Module { idx, replace: *replace })
                } else {
                    None
                };
                if let (Some(order), Some(b)) = (resolved, self.world.bots.get_mut(bot)) {
                    if !b.data.dying {
                        b.data.upgrade_queue.push(order);
                    }
                }
                Ok(None)
            }
            Command::Research { faction, construct } => {
                let cost = crate::world::research_cost(*construct);
                let have = self.world.data.get(faction).copied().unwrap_or(0);
                let set = self.world.unlocks.entry(*faction).or_default();
                if !set.has(*construct) && have >= cost {
                    set.unlock(*construct);
                    self.world.data.insert(*faction, have - cost);
                }
                Ok(None)
            }
            Command::KillBot { bot } => {
                if let Some(b) = self.world.bots.get_mut(bot) {
                    b.data.dying = true;
                    let pos = b.data.pos;
                    self.world.unindex_bot(*bot, pos);
                }
                Ok(None)
            }
            Command::PlacePaint { pos, color } => {
                if self.world.grid.in_bounds(*pos) {
                    match color {
                        Some(c) => {
                            self.world.paint.insert(*pos, *c);
                        }
                        None => {
                            self.world.paint.remove(pos);
                        }
                    }
                }
                Ok(None)
            }
        }
    }

    /// Shared bot construction. Printed bots start in the Boot Sequence
    /// (an engine interrupt context); test spawns skip it. `cpu_centi` and
    /// `cargo_cap_deci` are stored-unit BASES (the stats floor, or a
    /// dev-spawn override); everything else on the statline comes from
    /// `stats.ron` — every print is the same machine (docs/02).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_bot(
        &mut self,
        pos: TilePos,
        faction: u8,
        color: Color,
        hp: i64,
        cpu_centi: u64,
        cargo_cap_deci: u32,
        mut vm: Vm,
        boot: bool,
    ) -> BotId {
        let id = self.world.alloc_bot_id();
        let entity = self.world.alloc_entity();
        // Latent quirk rolls (docs/09): at print, from the seeded
        // `quirk_roll` stream, gated by the match's expected-quirks-per-
        // bot dial (per-mille; slot n's chance is the dial minus n×1000,
        // clamped — 500 = 50% of one quirk, 2000 = both certain). Rolls
        // stay LATENT until total XP crosses the manifestation thresholds.
        let mut latent_quirks = Vec::new();
        if self.world.quirk_permille > 0 {
            for slot in 0..self.quirks.manifest_at.len() {
                let prob =
                    self.world.quirk_permille.saturating_sub(slot as u32 * 1000).min(1000);
                if prob == 0 {
                    continue;
                }
                if crate::world::next_rand(&mut self.world.rng.quirk_roll) % 1000
                    < prob as u64
                {
                    let r = crate::world::next_rand(&mut self.world.rng.quirk_roll);
                    latent_quirks.push(self.quirks.pick(r));
                }
            }
        }
        let booting = if boot {
            vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot));
            Some(self.tuning.boot_ticks)
        } else {
            None
        };
        self.world.bot_entities.insert(entity, id);
        self.world.bots.insert(
            id,
            Bot {
                data: BotData {
                    id,
                    entity,
                    faction,
                    pos,
                    hp,
                    max_hp: hp,
                    hurt_fired: false,
                    cargo: std::collections::BTreeMap::new(),
                    cargo_cap: cargo_cap_deci,
                    cpu_centi,
                    move_rate_deci: self.stats.move_rate_deci,
                    sensors: self.stats.sensors,
                    module_slots: self.stats.module_slots,
                    log_cap: self.stats.log_buffer,
                    upgrades: Vec::new(),
                    modules: Vec::new(),
                    upgrade_queue: Vec::new(),
                    pad_sit: false,
                    survey_after_move: false,
                    withdrawn_aboard: 0,
                    color,
                    requested: None,
                    action: None,
                    booting,
                    recall: None,
                    bump_frozen: 0,
                    dying: false,
                    log_buf: Vec::new(),
                    xp: std::collections::BTreeMap::new(),
                    haul_accum: 0,
                    learning_carry: 0,
                    gain_carry: std::collections::BTreeMap::new(),
                    age_hp_levels: 0,
                    moved_tick: 0,
                    episodes: std::collections::BTreeMap::new(),
                    latent_quirks,
                    quirks: Vec::new(),
                    crash_seen: 0,
                    env: std::collections::BTreeMap::new(),
                    dune_idle: 0,
                    rng_program: crate::world::stream_seed(
                        self.world.seed ^ entity.0,
                        "program",
                    ),
                },
                vm: Some(vm),
            },
        );
        self.world.index_bot(id, pos);
        id
    }

    /// One fixed simulation tick — the nine-phase order of docs/07.
    /// Phase 1 (agreed Commands) happens outside, via [`Sim::apply`], in
    /// the relay's total order; everything from the VM grant to the
    /// snapshot hash lives here, in stable id order within each phase.
    pub fn step(&mut self) {
        self.world.tick += 1;

        // --- phase 2: grant + step VMs, stable id order ---
        let ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if bot.data.dying
                || bot.data.booting.is_some()
                || bot.data.recall.is_some()
                || bot.data.pad_sit
            {
                // Boot/recall/pad-sit are engine interrupt contexts: the
                // program is suspended and the engine drives the bot.
                continue;
            }
            if bot.data.bump_frozen > 0 {
                continue; // stunned by a bump — no thinking either
            }
            // The modifier pipeline (docs/02): base → hardware → XP →
            // quirks → state (Damaged, brownout — the Fabricator trickle
            // exempts one bot per faction) → clamp.
            let faction = bot.data.faction;
            let centi = crate::stats::cpu_centi(
                self.ctx(),
                &bot.data,
                self.world.brownout.contains(&faction),
                self.world.powered_bot.get(&faction) == Some(&id),
            );
            let bot = self.world.bots.get_mut(&id).expect("checked above");
            let mut vm = bot.vm.take().expect("vm present between phases");
            if vm.is_dead() {
                bot.vm = Some(vm);
                continue;
            }
            // Corruption's compute tax (M8, docs/05): every charged op
            // costs extra while the chassis stands on corrupted ground.
            // Set fresh before EVERY grant — the overlay is derived from
            // where the bot is now, never persisted. (Flat only; the
            // per-op-key generalization is flagged in TASKS.md.)
            let on_corruption =
                self.world.grid.get(bot.data.pos) == Some(crate::map::TileKind::Corruption);
            vm.set_cost_overlay_centi(if on_corruption {
                self.tuning.corruption_op_tax as i64
            } else {
                0
            });
            // The grant itself enforces the VM rules: no banking while
            // Blocked (waiting burns the tick) and the bank_cap clamp.
            vm.grant_centi(centi, &self.costs);
            if vm.is_blocked() {
                bot.vm = Some(vm);
                continue;
            }
            let outcome = {
                let mut host = BotHost { world: &mut self.world, bot: id, tuning: &self.tuning, ctx: crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning } };
                vm.run(&mut host, &self.costs)
            };
            self.after_vm(id, vm, outcome);
        }

        // --- phases 3+4: collect issued actions, resolve them PER BOT in
        // stable id order (each bot advances whatever action it has — the
        // move → combat → mine/build sub-order of docs/07 is not split out;
        // flagged in TASKS.md for reconciliation), then the engine-driven
        // walks (boot countdowns, recall walks). Damage and signals these
        // produce are QUEUED for phase 6, not applied inline. ---
        for id in ids.iter().copied() {
            self.resolve_bot(id);
        }
        for id in ids.iter().copied() {
            self.advance_engine(id);
        }

        // --- phase 5: perception recompute, then episode settlement
        // (split so seed passes never advance re-arm counters) ---
        self.run_perception();
        self.settle_episodes();

        // --- phase 6: damage, faults, deaths (countdowns and blasts join
        // in M10). Fault chip first: every unhandled crash this tick (from
        // stepping or action resolution) queues chassis damage, so its
        // hurt/death signals ride the same dispatch. ---
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            if bot.data.dying {
                continue;
            }
            let Some(vm) = bot.vm.as_ref() else { continue };
            let crashes = vm.crash_count();
            let delta = crashes.saturating_sub(bot.data.crash_seen);
            if delta > 0 {
                bot.data.crash_seen = crashes;
                // Per-bot chip (M6): Statically Typed halves it, `unsafe`
                // Block doubles it.
                let chip = crate::stats::StatCtx {
                    stats: &self.stats,
                    xp: &self.xp,
                    quirks: &self.quirks,
                    tuning: &self.tuning,
                }
                .fault_damage_for(&bot.data, self.tuning.fault_damage);
                self.queue_damage(id, delta as i64 * chip, None);
            }
        }
        self.settle_damage();
        self.dispatch_signals();

        // Deaths: dying bots become wrecks (the op boundary above may have
        // added to the pile — a Died outcome lands the same tick).
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if !bot.data.dying {
                continue;
            }
            // Already out of the occupancy index — dying bots leave it the
            // moment the flag is set.
            let bot = self.world.bots.remove(&id).expect("checked above");
            let data = bot.data;
            self.world.bot_entities.remove(&data.entity);
            // Carried cargo spills to the ground rather than entombing.
            self.drop_cargo_to_ground(data.pos, data.cargo);

            // Disabled: an inert wreck (self-destruct countdown comes later).
            self.world.wrecks.insert(
                id,
                Wreck { pos: data.pos, cargo: 0, logs: data.log_buf, env: data.env },
            );
        }

        // --- phase 7: XP settlement ---
        self.settle_xp();

        // --- phase 8: economy — upkeep settles FIRST (docs/07: energy,
        // upkeep; its brownout/rust flags feed this tick's self-repair and
        // the NEXT tick's cycle grants), then regen, refineries, printers.
        if !self.world.dev_free_power && self.world.tick.is_multiple_of(self.upkeep.interval_ticks)
        {
            self.settle_upkeep();
        }
        if self.world.tick.is_multiple_of(self.tuning.regen_interval_ticks) {
            // Regenerating nodes (Groves — docs/03: renewable but thin).
            let regen = self.tuning.node_regen_deci;
            let cap = self.tuning.node_regen_cap_deci;
            for node in self.world.nodes.values_mut() {
                if node.regen && node.amount < cap {
                    node.amount = (node.amount + regen).min(cap);
                }
            }
            for id in ids.iter() {
                let Some(bot) = self.world.bots.get_mut(id) else { continue };
                if bot.data.dying || bot.data.hp >= bot.data.max_hp {
                    continue;
                }
                if self.world.rusting.contains(&bot.data.faction) {
                    continue; // unpaid Steel maintenance: self-repair halts (Q84)
                }
                // Seniority mends (docs/02): Age raises the trickle.
                let amount = crate::stats::StatCtx {
                    stats: &self.stats,
                    xp: &self.xp,
                    quirks: &self.quirks,
                    tuning: &self.tuning,
                }
                .regen_for(&bot.data, self.tuning.regen_amount);
                bot.data.hp = (bot.data.hp + amount).min(bot.data.max_hp);
                // The latch re-arms against the SAME line it fires on — the
                // bot's own hurt_line env — so a moved line can't make the
                // edge trigger re-fire mid-template or stick forever.
                let line = crate::world::env_read(&bot.data, "hurt_line", &self.tuning, &self.quirks);
                if bot.data.hurt_fired && bot.data.hp * 100 >= bot.data.max_hp * line {
                    bot.data.hurt_fired = false; // back over the Damaged line
                }
            }
        }
        self.run_refineries();
        self.run_printers();
        self.run_pads();
        self.settle_terrain();
        if self.world.tick.is_multiple_of(self.tuning.corruption_spread_ticks) {
            self.spread_corruption();
        }

        // --- phase 9: snapshot hash for desync detection ---
        self.last_hash = self.state_hash();
    }

    /// Phase 8 (M5): the upkeep settlement — energy is a RATE, not a pile
    /// (docs/03): per-faction generation (Taps free, Generators burn fuel)
    /// vs. draw (bots + working refineries). Draw over generation =
    /// brownout (cycle budgets −50% next grant; the Fabricator trickle
    /// keeps ONE bot — lowest id — fully powered). Steel maintenance
    /// unpaid = rust: self-repair halts and hulls decay (Q84,
    /// `upkeep.ron`-configurable).
    pub(crate) fn settle_upkeep(&mut self) {
        use crate::resources::Resource;
        use crate::world::StructureKind;
        let mut factions: std::collections::BTreeSet<u8> = std::collections::BTreeSet::new();
        for bot in self.world.bots.values() {
            factions.insert(bot.data.faction);
        }
        for st in self.world.structures.values() {
            factions.insert(st.faction);
        }
        for faction in factions {
            // Draw first: every live bot (base + per-acquisition
            // increments — per-track-level joins with M6) + every
            // recipe-set refinery. A faction drawing NOTHING burns no
            // fuel and can't brown out — generators idle.
            let mut draw = 0u64;
            let mut bot_count = 0u64;
            for bot in
                self.world.bots.values().filter(|b| b.data.faction == faction && !b.data.dying)
            {
                bot_count += 1;
                let levels: u64 = bot
                    .data
                    .xp
                    .values()
                    .map(|&deci| self.xp.level(deci) as u64)
                    .sum();
                draw += self.upkeep.base_draw
                    + self.upkeep.draw_per_upgrade * bot.data.upgrades.len() as u64
                    + self.upkeep.draw_per_module * bot.data.modules.len() as u64
                    + self.upkeep.draw_per_track_level * levels;
            }
            draw += self.upkeep.draw_per_refinery
                * self
                    .world
                    .structures
                    .values()
                    .filter(|s| s.faction == faction && s.recipe.is_some())
                    .count() as u64;
            if draw == 0 {
                self.world.brownout.remove(&faction);
                self.world.powered_bot.remove(&faction);
                self.world.rusting.remove(&faction);
                continue;
            }
            // Generation. Generators burn the STRONG fuel first (Coal,
            // then Wood) — a deterministic preference; whether players
            // should choose is flagged in TASKS.md.
            let mut generation = 0u64;
            let st_ids: Vec<EntityId> = self
                .world
                .structures
                .iter()
                .filter(|(_, s)| s.faction == faction)
                .map(|(id, _)| *id)
                .collect();
            for id in st_ids {
                let fuel = self.upkeep.generator_fuel_deci;
                let st = self.world.structures.get_mut(&id).expect("collected above");
                match st.kind {
                    StructureKind::GeothermalTap => generation += self.upkeep.geothermal_output,
                    StructureKind::Generator => {
                        let mut burn = |kind: Resource| -> bool {
                            match st.input.get_mut(&kind) {
                                Some(have) if *have >= fuel => {
                                    *have -= fuel;
                                    if *have == 0 {
                                        st.input.remove(&kind);
                                    }
                                    true
                                }
                                _ => false,
                            }
                        };
                        if burn(Resource::Coal) {
                            generation += self.upkeep.generator_output_coal;
                        } else if burn(Resource::Wood) {
                            generation += self.upkeep.generator_output_wood;
                        }
                    }
                    _ => {}
                }
            }
            if draw > generation {
                self.world.brownout.insert(faction);
            } else {
                self.world.brownout.remove(&faction);
            }
            // The Fabricator backup trickle (Q84): one bot always powered
            // — deterministic pick, lowest id — while a working printer
            // exists to trickle from. Blackout can never deadlock the
            // colony: someone can always walk out for fuel.
            let has_printer = self
                .world
                .printers
                .values()
                .any(|p| p.faction == faction && p.state == PrinterState::Working);
            let pick = if has_printer {
                self.world
                    .bots
                    .iter()
                    .filter(|(_, b)| b.data.faction == faction && !b.data.dying)
                    .map(|(id, _)| *id)
                    .next()
            } else {
                None
            };
            match pick {
                Some(id) => {
                    self.world.powered_bot.insert(faction, id);
                }
                None => {
                    self.world.powered_bot.remove(&faction);
                }
            }
            // Steel maintenance (all-or-nothing from stock; partial
            // payment doesn't partially protect — flagged in TASKS.md).
            let need = bot_count * self.upkeep.steel_per_bot_deci;
            if need == 0 || self.world.stock_take(faction, Resource::Steel, need) {
                self.world.rusting.remove(&faction);
            } else {
                let sustained = !self.world.rusting.insert(faction);
                let bot_ids: Vec<BotId> = self
                    .world
                    .bots
                    .iter()
                    .filter(|(_, b)| b.data.faction == faction && !b.data.dying)
                    .map(|(id, _)| *id)
                    .collect();
                for id in bot_ids {
                    // Rust decay rides the ordinary damage phase (next
                    // tick's phase 6), like every other hurt.
                    self.queue_damage(id, self.upkeep.rust_decay_hp, None);
                }
                if sustained && self.upkeep.rust_scraps {
                    self.scrap_recall_lowest(faction);
                }
            }
        }
    }

    /// Phase 8: refineries (docs/03 — refinement is a logistics step).
    /// Each Smelter/Foundry with a recipe consumes its inputs from its
    /// physically-fed buffer, runs a batch timer, and emits into its
    /// output buffer for bots to withdraw(). Stable id order. Energy
    /// gating (M5): a browned-out faction's refineries stand idle —
    /// batches neither start nor advance until generation recovers.
    pub(crate) fn run_refineries(&mut self) {
        use crate::resources::{DECI, RECIPES};
        let ids: Vec<EntityId> = self.world.structures.keys().copied().collect();
        for id in ids {
            let st = self.world.structures.get_mut(&id).expect("structure exists");
            if self.world.brownout.contains(&st.faction) {
                continue; // needs energy (docs/03)
            }
            let Some(recipe_idx) = st.recipe else { continue };
            let Some(recipe) = RECIPES.get(recipe_idx as usize) else { continue };
            if let Some(ticks) = st.batch {
                if ticks > 1 {
                    st.batch = Some(ticks - 1);
                } else {
                    st.batch = None;
                    let (out_kind, out_units) = recipe.output;
                    *st.output.entry(out_kind).or_insert(0) += out_units * DECI;
                }
                continue;
            }
            // Start a batch when every input is buffered.
            let ready = recipe
                .inputs
                .iter()
                .all(|(k, units)| st.input.get(k).copied().unwrap_or(0) >= units * DECI);
            if ready {
                for (k, units) in recipe.inputs {
                    let have = st.input.get_mut(k).expect("checked ready");
                    *have -= units * DECI;
                    if *have == 0 {
                        st.input.remove(k);
                    }
                }
                st.batch = Some(self.tuning.recipe_batch_ticks);
            }
        }
    }

    /// Phase 5: perception — seeing/hearing recomputed from post-move
    /// positions, detection episodes, per-faction map knowledge, survey

    /// Phase 7: XP settlement — every award earned anywhere in the tick
    /// queued, then settled here in arrival order (phases queue in stable
    /// id order). The Learning multiplier applies at its start-of-tick
    /// level; it is IDENTITY until M6 lands the body tracks, so today this
    /// is a plain sum. Awards for bots that died in phase 6 are dropped
    /// with them.
    /// Phase 8 terrain settle (M8): Dune idle counters advance for every
    /// bot that stood still on sand this tick (Q35 — the counter feeds
    /// step_ticks' exit surcharge), and Scree worn past the crossing
    /// threshold collapses to Rubble (Q40). End-of-tick, so the Nth
    /// crosser finishes its own step on solid ground.
    pub(crate) fn settle_terrain(&mut self) {
        let tick = self.world.tick;
        for bot in self.world.bots.values_mut() {
            if bot.data.dying {
                continue;
            }
            if self.world.grid.get(bot.data.pos) == Some(crate::map::TileKind::Dunes)
                && bot.data.moved_tick != tick
            {
                bot.data.dune_idle = bot.data.dune_idle.saturating_add(1);
            }
        }
        let worn: Vec<crate::map::TilePos> = self
            .world
            .scree_wear
            .iter()
            .filter(|(_, n)| **n >= self.tuning.scree_crossings)
            .map(|(p, _)| *p)
            .collect();
        for p in worn {
            self.world.set_tile(p, crate::map::TileKind::Rubble);
        }
    }

    /// Corruption dynamics (M8-C, docs/05): each living Blight Core (id
    /// order) corrupts the nearest non-Corruption passable tile within
    /// its radius — nearest by (chebyshev, y, x), so the creep front is
    /// deterministic. Cleansed ground inside the radius is simply the
    /// nearest clean tile again: re-corruption falls out for free while
    /// the source lives. Bridges are spared — creep over a river would
    /// delete the crossing outright (cleanse yields Plains; flagged).
    pub(crate) fn spread_corruption(&mut self) {
        let cores: Vec<(crate::map::TilePos, u32)> =
            self.world.blight_cores.values().map(|c| (c.pos, c.radius)).collect();
        for (pos, radius) in cores {
            let r = radius as i32;
            let mut best: Option<(u32, i32, i32)> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    let t = crate::map::TilePos::new(pos.x + dx, pos.y + dy);
                    let Some(kind) = self.world.grid.get(t) else { continue };
                    if kind == crate::map::TileKind::Corruption
                        || kind == crate::map::TileKind::Bridge
                        || !kind.passable()
                    {
                        continue;
                    }
                    let cand = (pos.chebyshev(t), t.y, t.x);
                    if best.is_none_or(|b| cand < b) {
                        best = Some(cand);
                    }
                }
            }
            if let Some((_, y, x)) = best {
                self.world.set_tile(crate::map::TilePos::new(x, y), crate::map::TileKind::Corruption);
            }
        }
    }

    pub(crate) fn settle_xp(&mut self) {
        use std::collections::BTreeMap;
        let mut awards = std::mem::take(&mut self.world.pending_xp);
        // Age drips for every live bot (docs/02: its XP is literally time
        // — 1 deci per tick), through the same multiplier path as
        // everything else.
        for (id, bot) in &self.world.bots {
            if !bot.data.dying {
                awards.push((*id, XpTrack::Age, self.xp.age_deci_per_tick));
            }
        }
        let cap = self.xp.track_cap_deci();
        // The multiplier is the START-OF-SETTLE percent (Learning level +
        // quirk XP%), memoized per bot so this tick's own awards can't
        // compound into themselves.
        let mut pct_memo: BTreeMap<BotId, u64> = BTreeMap::new();
        // Learning feeds on every OTHER track's post-multiplier XP —
        // capped tracks included — and is never re-multiplied (docs/02).
        let mut feeds: BTreeMap<BotId, u64> = BTreeMap::new();
        for (id, track, deci) in awards {
            let Some(bot) = self.world.bots.get(&id) else { continue }; // died in phase 6
            if bot.data.dying {
                continue;
            }
            let pct = *pct_memo.entry(id).or_insert_with(|| self.ctx().xp_gain_pct(&bot.data));
            // Fractional carry per (bot, track), hundredths of a deci: a
            // sub-100% multiplier must REDUCE a 1-deci drip, not floor it
            // to zero forever (tech_debt froze the Age track outright).
            let bot = self.world.bots.get_mut(&id).expect("checked above");
            let carried =
                bot.data.gain_carry.remove(&track).unwrap_or(0) + deci * pct;
            let post = carried / 100;
            let rem = carried % 100;
            if rem > 0 {
                bot.data.gain_carry.insert(track, rem);
            }
            if post == 0 {
                continue;
            }
            if track != XpTrack::Learning {
                *feeds.entry(id).or_insert(0) += post;
            }
            let entry = bot.data.xp.entry(track).or_insert(0);
            *entry = (*entry + post).min(cap);
        }
        for (id, feed) in feeds {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            // Fractional carry (hundredths of a deci): 10% of a slow drip
            // accrues instead of flooring to zero every settlement.
            let carry = bot.data.learning_carry + feed * self.xp.learning_feed_pct;
            let gain = carry / 100;
            bot.data.learning_carry = carry % 100;
            if gain > 0 {
                let entry = bot.data.xp.entry(XpTrack::Learning).or_insert(0);
                *entry = (*entry + gain).min(cap);
            }
        }
        self.settle_milestones();
    }

    /// Phase 7b: total-XP milestones — module slots (+1 at each xp.ron
    /// threshold, capped) and quirk manifestation (docs/09: the nth latent
    /// roll comes alive when total XP crosses the nth threshold —
    /// deterministic check, no RNG).
    fn settle_milestones(&mut self) {
        let ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in ids {
            let (total, slots, latent, manifested) = {
                let d = &self.world.bots[&id].data;
                (d.xp_total(), d.module_slots, d.latent_quirks.len(), d.quirks.len())
            };
            let owed_slots = (1 + self
                .xp
                .slot_milestones
                .iter()
                .filter(|&&m| total >= m * 10)
                .count() as u32)
                .min(self.xp.slot_cap);
            if owed_slots > slots {
                self.world.bots.get_mut(&id).expect("collected").data.module_slots = owed_slots;
            }
            // Age body perk (xp.ron `age_hp_per_level`): each Age level
            // grows the hull — max HP and current HP both rise by the
            // delta (growing tougher never makes a bot instantly Damaged).
            let age_level = {
                let d = &self.world.bots[&id].data;
                self.xp.level(d.xp(XpTrack::Age))
            };
            {
                let d = &mut self.world.bots.get_mut(&id).expect("collected").data;
                if age_level > d.age_hp_levels {
                    let delta =
                        ((age_level - d.age_hp_levels) as i64) * self.xp.age_hp_per_level;
                    d.max_hp += delta;
                    d.hp += delta;
                    d.age_hp_levels = age_level;
                }
            }
            let owed_quirks = self
                .quirks
                .manifest_at
                .iter()
                .filter(|&&t| total >= t * 10)
                .count()
                .min(latent + manifested);
            for _ in manifested..owed_quirks {
                self.manifest_next_quirk(id);
            }
        }
    }

    /// Bring a bot's next latent quirk alive: move it to the manifested
    /// list and apply the one-time effects (max HP, log cap, live-VM stack
    /// depth). Pipeline effects need no action — they read the list.
    fn manifest_next_quirk(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        if bot.data.latent_quirks.is_empty() {
            return;
        }
        let quirk = bot.data.latent_quirks.remove(0);
        bot.data.quirks.push(quirk);
        let Some(spec) = self.quirks.quirks.get(quirk as usize) else { return };
        for effect in &spec.effects {
            match effect {
                crate::quirks::QuirkEffect::MaxHpPct(p) => {
                    let bot = self.world.bots.get_mut(&id).expect("checked");
                    let delta = bot.data.max_hp * *p as i64 / 100;
                    bot.data.max_hp = (bot.data.max_hp + delta).max(1);
                    bot.data.hp = bot.data.hp.min(bot.data.max_hp);
                }
                crate::quirks::QuirkEffect::LogCapPct(p) => {
                    let bot = self.world.bots.get_mut(&id).expect("checked");
                    let delta = bot.data.log_cap as i64 * *p as i64 / 100;
                    bot.data.log_cap = (bot.data.log_cap as i64 + delta).max(1) as u32;
                }
                crate::quirks::QuirkEffect::StackDepth(_) => {
                    let depth = self.ctx().stack_depth_for(&self.world.bots[&id].data);
                    let bot = self.world.bots.get_mut(&id).expect("checked");
                    if let Some(vm) = bot.vm.as_mut() {
                        vm.set_stack_depth(depth);
                    }
                }
                _ => {} // pipeline- or read-side effects
            }
        }
    }

    /// The stat pipeline's read context (floor statline + XP magnitudes +
    /// quirk catalog). All shared borrows of disjoint Sim fields, so it
    /// composes with `world` reads.
    pub fn ctx(&self) -> crate::stats::StatCtx<'_> {
        crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }
    }

    /// Per-bot VM config: the shared template with the hardware and quirk
    /// layers applied (Stack extensions / Memory Leak move the call-depth
    /// cap).
    pub(crate) fn vm_config_for(&self, data: &BotData) -> VmConfig {
        let mut config = self.vm_config.clone();
        config.stack_depth = self.ctx().stack_depth_for(data);
        config
    }

    /// Store the VM back. Every outcome keeps the VM (aborted bots are
    /// dying wrecks-to-be, not vaporized — no instant-destroy path).
    fn after_vm(&mut self, id: BotId, vm: Vm, outcome: Outcome) {
        let _ = outcome;
        if let Some(bot) = self.world.bots.get_mut(&id) {
            bot.vm = Some(vm);
        }
    }

    /// Phase 9: deterministic world hash for desync detection and golden
    /// replays. (VM internals are hashed shallowly for now — budget, line,
    /// blocked/dead — deep state hashing is a TODO.)
    pub fn state_hash(&self) -> u64 {
        let w = &self.world;
        let mut h = Fnv1a::new();
        h.write_u64(w.tick);
        h.write_i32(w.grid.width);
        h.write_i32(w.grid.height);
        // Cached: re-walking the map every tick made phase 9 O(map). Kept
        // fresh by World::set_tile on the rare terrain mutation.
        h.write_u64(w.terrain_hash);
        // Scree wear is real divergent state (M8, Q40): two peers with a
        // half-worn tile must agree before the collapse, not just after.
        h.write_u32(w.blight_cores.len() as u32);
        for (id, core) in &w.blight_cores {
            h.write_u64(id.0);
            h.write_i32(core.pos.x);
            h.write_i32(core.pos.y);
            h.write_u32(core.radius);
            h.write_i64(core.hp);
        }
        h.write_u32(w.scree_wear.len() as u32);
        for (pos, n) in &w.scree_wear {
            h.write_i32(pos.x);
            h.write_i32(pos.y);
            h.write_u32(*n);
        }
        for (id, node) in &w.nodes {
            h.write_u64(id.0);
            h.write_u8(node.kind.as_u8());
            h.write_i32(node.pos.x);
            h.write_i32(node.pos.y);
            h.write_u32(node.amount);
            h.write_u8(node.regen as u8);
        }
        for (id, depot) in &w.depots {
            h.write_u64(id.0);
            h.write_i32(depot.pos.x);
            h.write_i32(depot.pos.y);
        }
        for (id, st) in &w.structures {
            h.write_u64(id.0);
            h.write_u8(st.kind.as_u8());
            h.write_u8(st.faction);
            h.write_i32(st.pos.x);
            h.write_i32(st.pos.y);
            h.write_i64(st.hp);
            // Length-prefix the buffers so {input: X} and {output: X}
            // can't hash identically (collision quality, not a desync
            // vector — every peer runs the same computation).
            h.write_u32(st.input.len() as u32);
            for (k, deci) in &st.input {
                h.write_u8(k.as_u8());
                h.write_u32(*deci);
            }
            h.write_u32(st.output.len() as u32);
            for (k, deci) in &st.output {
                h.write_u8(k.as_u8());
                h.write_u32(*deci);
            }
            h.write_u8(st.recipe.map(|r| r + 1).unwrap_or(0));
            h.write_u32(st.batch.unwrap_or(0));
            match &st.pad {
                Some(job) => {
                    h.write_u8(1);
                    h.write_u32(job.bot.0);
                    hash_order(&mut h, &job.order);
                    h.write_u32(job.ticks_left);
                }
                None => h.write_u8(0),
            }
        }
        for (faction, set) in &w.unlocks {
            h.write_u8(*faction);
            for c in pyrite::Construct::ALL {
                h.write_u8(set.has(c) as u8);
            }
        }
        for ((faction, kind), deci) in &w.stock {
            h.write_u8(*faction);
            h.write_u8(kind.as_u8());
            h.write_u64(*deci);
        }
        for (faction, data) in &w.data {
            h.write_u8(*faction);
            h.write_u64(*data);
        }
        for (faction, delivered) in &w.delivered {
            h.write_u8(*faction);
            h.write_u64(*delivered);
        }
        // Permanent map knowledge is real state (docs/05 Q70); the live
        // perception union is derived every tick and deliberately unhashed.
        for (faction, known) in &w.known_nodes {
            h.write_u8(*faction);
            h.write_u32(known.len() as u32);
            for (id, node) in known {
                h.write_u64(id.0);
                h.write_u8(node.kind.as_u8());
                h.write_u8(node.exhausted as u8);
            }
        }
        for (faction, paid) in &w.milestones_paid {
            h.write_u8(*faction);
            h.write_u64(*paid);
        }
        for faction in &w.first_kill_done {
            h.write_u8(*faction);
        }
        for faction in &w.brownout {
            h.write_u8(*faction);
        }
        for faction in &w.rusting {
            h.write_u8(*faction);
        }
        // The trickle pick controls per-bot cycle grants for a whole
        // settlement interval — a divergence here must trip the desync
        // alarm immediately, not once the extra compute moves a position.
        for (faction, bot) in &w.powered_bot {
            h.write_u8(*faction);
            h.write_u32(bot.0);
        }
        h.write_u64(w.rng.combat);
        h.write_u64(w.rng.wander);
        h.write_u64(w.rng.explore);
        h.write_u64(w.rng.sidestep);
        h.write_u64(w.rng.quirk_roll);
        h.write_u64(w.rng.feral_mutation);
        for (id, printer) in &w.printers {
            h.write_u64(id.0);
            h.write_i32(printer.pos.x);
            h.write_i32(printer.pos.y);
            h.write_u8(printer.faction);
            h.write_u8(printer.color.0);
            h.write_u8(matches!(printer.state, PrinterState::Working) as u8);
            h.write_u32(printer.desired_max);
            h.write_u32(printer.job.unwrap_or(0));
        }
        for (pos, overlay) in &w.overlays {
            h.write_i32(pos.x);
            h.write_i32(pos.y);
            h.write_u8(overlay.as_u8());
        }
        for (pos, color) in &w.paint {
            h.write_i32(pos.x);
            h.write_i32(pos.y);
            h.write_u8(*color);
        }
        for (id, bp) in &w.blueprints {
            h.write_u64(id.0);
            h.write_i32(bp.pos.x);
            h.write_i32(bp.pos.y);
            h.write_u32(bp.progress);
            h.write_u32(bp.needed);
        }
        // Program versions ARE source-byte hashes (CLAUDE.md rule 7), so
        // hashing the stored u64s covers the sources without re-walking
        // every deployed program's bytes each tick.
        for ((faction, color), cp) in &w.color_programs {
            h.write_u8(*faction);
            h.write_u8(*color);
            h.write_u64(cp.hash);
        }
        for hash in w.program_library.keys() {
            h.write_u64(*hash);
        }
        for (id, bot) in &w.bots {
            h.write_u32(id.0);
            h.write_i32(bot.data.pos.x);
            h.write_i32(bot.data.pos.y);
            for (kind, deci) in &bot.data.cargo {
                h.write_u8(kind.as_u8());
                h.write_u32(*deci);
            }
            h.write_u32(bot.data.cargo_cap);
            h.write_u64(bot.data.cpu_centi);
            h.write_u32(bot.data.move_rate_deci);
            h.write_u32(bot.data.sensors);
            h.write_u32(bot.data.module_slots);
            h.write_u32(bot.data.log_cap);
            h.write_u32(bot.data.upgrades.len() as u32);
            for u in &bot.data.upgrades {
                h.write_u8(*u);
            }
            h.write_u32(bot.data.modules.len() as u32);
            for m in &bot.data.modules {
                h.write_u8(*m);
            }
            h.write_u32(bot.data.upgrade_queue.len() as u32);
            for order in &bot.data.upgrade_queue {
                hash_order(&mut h, order);
            }
            h.write_u8(bot.data.pad_sit as u8);
            h.write_u8(bot.data.survey_after_move as u8);
            h.write_u32(bot.data.withdrawn_aboard);
            h.write_u8(bot.data.faction);
            h.write_u8(bot.data.color.0);
            h.write_i64(bot.data.hp);
            h.write_u8(bot.data.hurt_fired as u8);
            h.write_u32(bot.data.xp.len() as u32);
            for (track, deci) in &bot.data.xp {
                h.write_u8(track.as_u8());
                h.write_u64(*deci);
            }
            h.write_u64(bot.data.haul_accum);
            h.write_u64(bot.data.learning_carry);
            h.write_u32(bot.data.age_hp_levels);
            for (track, rem) in &bot.data.gain_carry {
                h.write_u8(track.as_u8());
                h.write_u64(*rem);
            }
            h.write_u64(bot.data.moved_tick);
            h.write_u32(bot.data.dune_idle);
            h.write_u32(bot.data.episodes.len() as u32);
            for (faction, counter) in &bot.data.episodes {
                h.write_u8(*faction);
                h.write_u32(*counter);
            }
            h.write_u32(bot.data.latent_quirks.len() as u32);
            for q in &bot.data.latent_quirks {
                h.write_u8(*q);
            }
            h.write_u32(bot.data.quirks.len() as u32);
            for q in &bot.data.quirks {
                h.write_u8(*q);
            }
            h.write_u8(bot.data.dying as u8);
            h.write_u32(bot.data.booting.unwrap_or(0));
            h.write_u32(bot.data.bump_frozen);
            h.write_u8(bot.data.recall.is_some() as u8);
            h.write_u64(bot.data.rng_program);
            for (level, entry) in &bot.data.log_buf {
                h.write_u8(*level);
                h.write_str(entry);
            }
            for (key, value) in &bot.data.env {
                h.write_str(key);
                h.write_i64(*value);
            }
            if let Some(vm) = &bot.vm {
                h.write_i64(vm.budget());
                h.write_u64(vm.fault_count());
                h.write_u64(vm.crash_count());
                h.write_u32(vm.current_line());
                h.write_u8(vm.is_blocked() as u8);
                h.write_u8(vm.is_dead() as u8);
            }
        }
        for (id, wreck) in &w.wrecks {
            h.write_u32(id.0);
            h.write_i32(wreck.pos.x);
            h.write_i32(wreck.pos.y);
            h.write_u32(wreck.cargo);
            for (level, log) in &wreck.logs {
                h.write_u8(*level);
                h.write_str(log);
            }
            for (key, value) in &wreck.env {
                h.write_str(key);
                h.write_i64(*value);
            }
        }
        for bb in &w.black_boxes {
            h.write_u64(bb.tick);
            h.write_u32(bb.bot.0);
            h.write_str(&bb.cause);
            for (level, log) in &bb.logs {
                h.write_u8(*level);
                h.write_str(log);
            }
            for (key, value) in &bb.env {
                h.write_str(key);
                h.write_i64(*value);
            }
        }
        h.write_u64(w.archive.len() as u64);
        for entry in &w.archive {
            h.write_u64(entry.tick);
            h.write_u32(entry.bot.0);
            h.write_u8(entry.level);
            h.write_u32(entry.line);
            h.write_str(&entry.text);
        }
        h.finish()
    }
}
