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
    World,
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
    /// Rammer's freeze after bumping into an occupied tile (50 = 5s @10Hz).
    /// The at-fault party sits longest — by the time it re-plans, the
    /// victim has cleared the scene.
    pub bump_freeze_ticks: u32,
    /// Victim's (shorter) freeze — a stagger, then it moves on.
    pub bump_victim_freeze_ticks: u32,
    /// The forced handler-entry ritual: EVERY unified-handler entry waits
    /// this long first (the visible flinch). Death is exempt.
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
}

impl Sim {
    pub fn new(spec: &MapSpec) -> Self {
        let tuning = Tuning::default();
        let mut vm_config = VmConfig::default();
        // Engine default handlers are REAL Pyrite (docs/01): watchable,
        // line-highlighted, costed. There is ONE unified default — a match
        // over everything that can go wrong (every entry already paid the
        // forced handler_init() stagger) — plus the tiny death handler.
        let unified = format!(
            "match s:\n    case Signal.Error(msg):\n        upload_crash_dump()\n    case Signal.Bump:\n        wait({})\n    case _:\n        wait(0)\n",
            tuning.bump_freeze_ticks.saturating_sub(tuning.handler_init_ticks),
        );
        for (kind, source) in [
            (pyrite::ast::SignalKind::Signal, unified),
            (pyrite::ast::SignalKind::Death, "become_disabled()\n".to_string()),
        ] {
            let program = pyrite::parse(&source, &UnlockSet::all())
                .expect("engine default handlers parse");
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
        Self {
            world: World::from_spec(spec),
            costs: CostTable::default(),
            vm_config,
            tuning,
        }
    }

    /// Phase 1: apply a command. Deterministic given identical call order.
    /// Returns the new bot's id for spawn commands.
    pub fn apply(&mut self, command: &Command) -> Result<Option<BotId>, PyriteError> {
        match command {
            Command::SpawnBot { pos, source, cpu, cargo_cap, faction, hp, color } => {
                let program = pyrite::parse(source, &UnlockSet::all())?;
                let vm = Vm::new(Rc::new(program), self.vm_config.clone());
                let id = self.insert_bot(*pos, *faction, *color, *hp, *cpu, *cargo_cap, vm, false);
                Ok(Some(id))
            }
            Command::DeployProgram { faction, color, source } => {
                let program = pyrite::parse(source, &UnlockSet::all())?;
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
            vm.set_engine_interrupt(true);
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
                    rng_program: crate::world::stream_seed(
                        self.world.seed ^ entity.0,
                        "program",
                    ),
                },
                vm: Some(vm),
            },
        );
        id
    }

    /// One fixed simulation tick (phases 2–5).
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
                let mut host = BotHost { world: &mut self.world, bot: id, tuning_handler_init_ticks: self.tuning.handler_init_ticks };
                vm.run(&mut host, &self.costs)
            };
            self.after_vm(id, vm, outcome);
        }

        // --- phase 3+4: start requested actions, advance in-flight ones ---
        for id in ids.iter().copied() {
            self.resolve_bot(id);
        }

        // --- phase 4.5: engine-driven movement (boot countdowns, recalls) ---
        for id in ids.iter().copied() {
            self.advance_engine(id);
        }

        // --- phase 4.7: fault damage — every unhandled crash this tick
        // (from stepping or action resolution) chips the chassis. Routed
        // through apply_damage so hurt/death signals fire normally.
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
                self.apply_damage(id, delta as i64 * self.tuning.fault_damage);
            }
        }

        // --- phase 4.8: passive regen — and the hurt signal re-arms when
        // health climbs back above its threshold (docs/02: edge-triggered).
        if self.world.tick.is_multiple_of(self.tuning.regen_interval_ticks) {
            let amount = self.tuning.regen_amount;
            for id in ids.iter() {
                let Some(bot) = self.world.bots.get_mut(id) else { continue };
                if bot.data.dying || bot.data.hp >= bot.data.max_hp {
                    continue;
                }
                bot.data.hp = (bot.data.hp + amount).min(bot.data.max_hp);
                if bot.data.hurt_fired && bot.data.hp * 100 >= bot.data.max_hp * 50 {
                    bot.data.hurt_fired = false; // fixed Damaged threshold
                }
            }
        }

        // --- phase 5: deaths ---
        for id in ids {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if !bot.data.dying {
                continue;
            }
            let bot = self.world.bots.remove(&id).expect("checked above");
            let data = bot.data;
            self.world.bot_entities.remove(&data.entity);
            // Carried cargo spills to the ground rather than entombing.
            self.drop_cargo_to_ground(data.pos, data.cargo);
            // Disabled: an inert wreck (self-destruct countdown comes later).
            self.world.wrecks.insert(id, Wreck { pos: data.pos, cargo: 0, logs: data.log_buf });
        }

        // --- phase 6: economy (printers: jobs, rebalancing, capacity) ---
        self.run_printers();
    }

    /// Store the VM back (or destroy the bot, per the outcome).
    fn after_vm(&mut self, id: BotId, vm: Vm, outcome: Outcome) {
        match outcome {
            Outcome::Paused | Outcome::Blocked | Outcome::Dead => {
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.vm = Some(vm);
                }
            }
            Outcome::Exploded => self.explode(id, &vm),
        }
    }

    /// Phase 7: deterministic world hash for desync detection and golden
    /// replays. (VM internals are hashed shallowly for now — budget, line,
    /// blocked/dead — deep state hashing is a TODO.)
    pub fn state_hash(&self) -> u64 {
        let w = &self.world;
        let mut h = Fnv1a::new();
        h.write_u64(w.tick);
        h.write_i32(w.grid.width);
        h.write_i32(w.grid.height);
        for tile in w.grid.tiles() {
            h.write_u8(tile.as_u8());
        }
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
        for ((faction, color), cp) in &w.color_programs {
            h.write_u8(*faction);
            h.write_u8(*color);
            h.write_u64(cp.hash);
            h.write_str(&cp.source);
        }
        for (hash, source) in &w.program_library {
            h.write_u64(*hash);
            h.write_str(source);
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
            for entry in &bot.data.log_buf {
                h.write_str(entry);
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
            for log in &wreck.logs {
                h.write_str(log);
            }
        }
        for bb in &w.black_boxes {
            h.write_u64(bb.tick);
            h.write_u32(bb.bot.0);
            h.write_str(&bb.cause);
            for log in &bb.logs {
                h.write_str(log);
            }
        }
        h.write_u64(w.archive.len() as u64);
        for entry in &w.archive {
            h.write_u64(entry.tick);
            h.write_u32(entry.bot.0);
            h.write_u32(entry.line);
            h.write_str(&entry.text);
        }
        h.finish()
    }
}
