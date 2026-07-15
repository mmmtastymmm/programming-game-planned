//! The deterministic tick loop (docs/07-architecture.md):
//!
//! 1. apply agreed Commands (caller does this via [`Sim::apply`])
//! 2. grant cycles, step every VM (stable BotId order)
//! 3. collect issued actions (recorded as `ActionRequest`s during step)
//! 4. resolve actions: start requests, advance in-flight actions
//! 5. deaths: wreck dying bots, drop black boxes
//! 6. economy (nothing yet)
//! 7. state hash for desync detection ([`Sim::state_hash`])

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

/// Melee damage per hit (tuning constant; per-weapon hardware later).
pub const ATTACK_DAMAGE: i64 = 10;

/// Sim tuning constants (all numbers are data — CLAUDE.md convention; the
/// values live in `data/tuning.ron`, baked in at compile time and parsed
/// once at load).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub print_ticks: u32,
    /// Ore per print. DEFAULT FREE: a colony must never be soft-locked out
    /// of bots (no ore + no bots = dead end). Maps/servers can set a cost;
    /// population is bounded by dials and capacity either way.
    pub print_cost_ore: u64,
    pub repair_cost_ore: u64,
    pub scrap_refund_ore: u64,
    /// Boot Sequence duration — an engine interrupt context.
    pub boot_ticks: u32,
    /// Printed-bot chassis defaults (hardware modules come later).
    pub printed_hp: i64,
    pub printed_cpu: u64,
    pub printed_cargo_cap: u32,
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
    pub bridge_cost_ore: u64,
    /// Builder-ticks of labor a bridge takes.
    pub bridge_build_ticks: u32,
    /// Placing a traffic overlay (arrow) — instant signage.
    pub overlay_cost_ore: u64,
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
        assert!(self.printed_hp > 0, "tuning: printed_hp must be > 0");
        assert!(self.printed_cpu > 0, "tuning: printed_cpu must be > 0");
        assert!(
            (1..=100).contains(&self.hurt_line_pct),
            "tuning: hurt_line_pct must be a percentage in 1..=100"
        );
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
    /// labor via closest(blueprint).expect()/build().
    PlaceBlueprint { pos: TilePos, kind: BlueprintKind },
    /// Set or clear a traffic overlay on any tile — instant signage, not
    /// construction (small ore cost to place; clearing is free).
    PlaceOverlay { pos: TilePos, overlay: Option<OverlayKind> },
    /// Set or clear cosmetic tile paint (free).
    PlacePaint { pos: TilePos, color: Option<u8> },
    /// Emergency stop for a stuck bot: straight to wreck (no death
    /// handler — the owner pulled the plug). Logs ride in the wreck;
    /// carried cargo spills onto the ground.
    KillBot { bot: BotId },
}

pub struct Sim {
    pub world: World,
    pub costs: CostTable,
    pub vm_config: VmConfig,
    pub tuning: Tuning,
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
        let mut sim = Self {
            world: World::from_spec(spec),
            costs: CostTable::default(),
            vm_config,
            tuning,
            last_hash: 0,
        };
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
                let program = pyrite::parse(source, &UnlockSet::all())?;
                // Deploy-time window analysis (M3): caps, signal safety,
                // loop/recursion ban — rejected here, never at runtime.
                pyrite::check_windows(&program, &self.costs)?;
                let vm = Vm::new(Rc::new(program), self.vm_config.clone());
                let id = self.insert_bot(*pos, *faction, *color, *hp, *cpu, *cargo_cap, vm, false);
                Ok(Some(id))
            }
            Command::DeployProgram { faction, color, source } => {
                let program = pyrite::parse(source, &UnlockSet::all())?;
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
                let cost = self.tuning.repair_cost_ore;
                if let Some(p) = self.world.printers.get_mut(printer)
                    && p.state == PrinterState::Ruined
                    && self.world.stockpile_ore >= cost
                {
                    p.state = PrinterState::Working;
                    self.world.stockpile_ore -= cost;
                }
                Ok(None)
            }
            Command::PlaceBlueprint { pos, kind } => {
                let valid_site = match kind {
                    BlueprintKind::Bridge => self.world.grid.get(*pos) == Some(TileKind::Water),
                };
                let occupied_by_blueprint =
                    self.world.blueprints.values().any(|b| b.pos == *pos);
                let cost = match kind {
                    BlueprintKind::Bridge => self.tuning.bridge_cost_ore,
                };
                if valid_site && !occupied_by_blueprint && self.world.stockpile_ore >= cost {
                    self.world.stockpile_ore -= cost;
                    let needed = match kind {
                        BlueprintKind::Bridge => self.tuning.bridge_build_ticks,
                    };
                    let id = self.world.alloc_entity();
                    self.world
                        .blueprints
                        .insert(id, Blueprint { pos: *pos, kind: *kind, progress: 0, needed });
                }
                Ok(None)
            }
            Command::PlaceOverlay { pos, overlay } => {
                if self.world.grid.in_bounds(*pos) {
                    match overlay {
                        Some(kind) => {
                            let cost = self.tuning.overlay_cost_ore;
                            if self.world.stockpile_ore >= cost {
                                self.world.stockpile_ore -= cost;
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
    /// (an engine interrupt context); test spawns skip it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_bot(
        &mut self,
        pos: TilePos,
        faction: u8,
        color: Color,
        hp: i64,
        cpu: u64,
        cargo_cap: u32,
        mut vm: Vm,
        boot: bool,
    ) -> BotId {
        let id = self.world.alloc_bot_id();
        let entity = self.world.alloc_entity();
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
                    cargo: 0,
                    cargo_cap,
                    cpu,
                    color,
                    requested: None,
                    action: None,
                    booting,
                    recall: None,
                    bump_frozen: 0,
                    dying: false,
                    log_buf: Vec::new(),
                    xp_mining: 0,
                    xp_hauling: 0,
                    xp_combat: 0,
                    xp_building: 0,
                    crash_seen: 0,
                    env: std::collections::BTreeMap::new(),
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
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            if bot.data.dying || bot.data.booting.is_some() || bot.data.recall.is_some() {
                // Boot/recall are engine interrupt contexts: the program is
                // suspended and the engine drives the bot.
                continue;
            }
            if bot.data.bump_frozen > 0 {
                continue; // stunned by a bump — no thinking either
            }
            let mut vm = bot.vm.take().expect("vm present between phases");
            if vm.is_dead() {
                bot.vm = Some(vm);
                continue;
            }
            if vm.is_blocked() {
                // Waiting burns the tick's cycles (docs/01: blocking is not
                // free compute) — no grant accrues while blocked.
                bot.vm = Some(vm);
                continue;
            }
            let cpu = bot.data.cpu;
            vm.grant(cpu);
            let outcome = {
                let mut host = BotHost { world: &mut self.world, bot: id, tuning: &self.tuning };
                vm.run(&mut host, &self.costs)
            };
            self.after_vm(id, vm, outcome);
        }

        // --- phases 3+4: collect issued actions, resolve them (move →
        // combat → mine/build), then the engine-driven walks (boot
        // countdowns, recall walks). Damage and signals these produce are
        // QUEUED for phase 6, not applied inline. ---
        for id in ids.iter().copied() {
            self.resolve_bot(id);
        }
        for id in ids.iter().copied() {
            self.advance_engine(id);
        }

        // --- phase 5: perception (stub until M7) ---
        self.run_perception();

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
                self.queue_damage(id, delta as i64 * self.tuning.fault_damage);
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

        // --- phase 8: economy — self-repair first (docs/07: regen lives
        // here; the hurt latch re-arms when health climbs back over the
        // line, docs/02: edge-triggered), then printers (jobs,
        // rebalancing, capacity). ---
        if self.world.tick.is_multiple_of(self.tuning.regen_interval_ticks) {
            let amount = self.tuning.regen_amount;
            for id in ids.iter() {
                let Some(bot) = self.world.bots.get_mut(id) else { continue };
                if bot.data.dying || bot.data.hp >= bot.data.max_hp {
                    continue;
                }
                bot.data.hp = (bot.data.hp + amount).min(bot.data.max_hp);
                // The latch re-arms against the SAME line it fires on — the
                // bot's own hurt_line env — so a moved line can't make the
                // edge trigger re-fire mid-template or stick forever.
                let line = crate::world::env_read(&bot.data.env, "hurt_line", &self.tuning);
                if bot.data.hurt_fired && bot.data.hp * 100 >= bot.data.max_hp * line {
                    bot.data.hurt_fired = false; // back over the Damaged line
                }
            }
        }
        self.run_printers();

        // --- phase 9: snapshot hash for desync detection ---
        self.last_hash = self.state_hash();
    }

    /// Phase 5: perception — seeing/hearing recomputed from post-move
    /// positions, detection episodes, per-faction map knowledge, survey
    /// steps (docs/07). STUB until M7 lands the two-circle model: queries
    /// stay omniscient, but the phase slot — and the phase-0 seed call in
    /// [`Sim::new`] — exist so M7 drops in without reordering the tick.
    /// Phase-2 queries read the *previous* tick's perception by design.
    pub(crate) fn run_perception(&mut self) {}

    /// Phase 7: XP settlement — every award earned anywhere in the tick
    /// queued, then settled here in arrival order (phases queue in stable
    /// id order). The Learning multiplier applies at its start-of-tick
    /// level; it is IDENTITY until M6 lands the body tracks, so today this
    /// is a plain sum. Awards for bots that died in phase 6 are dropped
    /// with them.
    pub(crate) fn settle_xp(&mut self) {
        let events = std::mem::take(&mut self.world.pending_xp);
        for (id, track, amount) in events {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            match track {
                XpTrack::Mining => bot.data.xp_mining += amount,
                XpTrack::Hauling => bot.data.xp_hauling += amount,
                XpTrack::Combat => bot.data.xp_combat += amount,
                XpTrack::Building => bot.data.xp_building += amount,
            }
        }
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
        for (id, node) in &w.ore_nodes {
            h.write_u64(id.0);
            h.write_i32(node.pos.x);
            h.write_i32(node.pos.y);
            h.write_u32(node.amount);
        }
        for (id, depot) in &w.depots {
            h.write_u64(id.0);
            h.write_i32(depot.pos.x);
            h.write_i32(depot.pos.y);
        }
        h.write_u64(w.stockpile_ore);
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
            h.write_u32(bot.data.cargo);
            h.write_u64(bot.data.cpu);
            h.write_u8(bot.data.faction);
            h.write_u8(bot.data.color.0);
            h.write_i64(bot.data.hp);
            h.write_u8(bot.data.hurt_fired as u8);
            h.write_u64(bot.data.xp_combat);
            h.write_u64(bot.data.xp_building);
            h.write_u8(bot.data.dying as u8);
            h.write_u32(bot.data.booting.unwrap_or(0));
            h.write_u32(bot.data.bump_frozen);
            h.write_u8(bot.data.recall.is_some() as u8);
            h.write_u64(bot.data.rng_program);
            h.write_u64(bot.data.xp_mining);
            h.write_u64(bot.data.xp_hauling);
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
