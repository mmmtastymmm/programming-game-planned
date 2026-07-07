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
use crate::map::{astar, MapSpec, TilePos};
use crate::world::{
    Action, ActionRequest, ArchiveEntry, ArchiveKind, BlackBox, Bot, BotData, BotId, Color,
    ColorProgram, EntityId, PrinterState, Recall, RecallPurpose, Wreck, World,
};
use pyrite::{CostTable, Outcome, PyriteError, RaiseOutcome, Signal, UnlockSet, Value, Vm, VmConfig};
use std::collections::BTreeSet;
use std::rc::Rc;

/// Melee damage per hit (tuning constant; per-weapon hardware later).
pub const ATTACK_DAMAGE: i64 = 10;

/// Sim tuning constants (all numbers are data — CLAUDE.md convention).
#[derive(Debug, Clone)]
pub struct Tuning {
    pub print_ticks: u32,
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
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            print_ticks: 5,
            print_cost_ore: 2,
            repair_cost_ore: 5,
            scrap_refund_ore: 1,
            boot_ticks: 2,
            printed_hp: 30,
            printed_cpu: 2,
            printed_cargo_cap: 2,
            capacity: 10_000,
        }
    }
}

/// External inputs: the ONLY way anything outside the sim mutates it
/// (single-player is lockstep with one peer).
#[derive(Debug, Clone, PartialEq)]
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
    /// Deploy source to a (faction, color) slot. Affects new prints and
    /// re-colors; live bots hot-swap at their next loop boundary (TODO).
    DeployProgram { faction: u8, color: Color, source: String },
    /// The population dial on a printer.
    SetDesiredMax { printer: EntityId, value: u32 },
    /// Fix a ruined printer (Data cost; ore stands in until Data exists).
    RepairPrinter { printer: EntityId },
}

pub struct Sim {
    pub world: World,
    pub costs: CostTable,
    pub vm_config: VmConfig,
    pub tuning: Tuning,
}

impl Sim {
    pub fn new(spec: &MapSpec) -> Self {
        Self {
            world: World::from_spec(spec),
            costs: CostTable::default(),
            vm_config: VmConfig::default(),
            tuning: Tuning::default(),
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
                let version =
                    self.world.color_programs.get(&slot).map(|c| c.version + 1).unwrap_or(1);
                self.world.color_programs.insert(
                    slot,
                    ColorProgram { source: source.clone(), program: Rc::new(program), version },
                );
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
        }
    }

    /// Shared bot construction. Printed bots start in the Boot Sequence
    /// (an engine interrupt context); test spawns skip it.
    #[allow(clippy::too_many_arguments)]
    fn insert_bot(
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
                    dying: false,
                    log_buf: Vec::new(),
                    xp_mining: 0,
                    xp_hauling: 0,
                    xp_combat: 0,
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
                let mut host = BotHost { world: &mut self.world, bot: id };
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

        // --- phase 5: deaths ---
        for id in ids {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if !bot.data.dying {
                continue;
            }
            let bot = self.world.bots.remove(&id).expect("checked above");
            let data = bot.data;
            self.world.bot_entities.remove(&data.entity);
            // Disabled: an inert wreck (self-destruct countdown comes later).
            self.world.wrecks.insert(id, Wreck { pos: data.pos, cargo: data.cargo, logs: data.log_buf });
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

    /// Phase 4 for one bot: start its requested action or advance the
    /// in-flight one; on completion, resume the VM with the result.
    fn resolve_bot(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        if bot.data.dying {
            return;
        }

        // Start a freshly requested action.
        if let Some(request) = bot.data.requested.take() {
            let pos = bot.data.pos;
            match request {
                ActionRequest::MoveTo(target) => {
                    let Some(target_pos) = self.world.entity_pos(target) else {
                        self.finish_action(id, Err("move_to: no such entity".into()));
                        return;
                    };
                    if pos.chebyshev(target_pos) <= 1 {
                        self.finish_action(id, Ok(Value::Unit));
                        return;
                    }
                    // Goal set: passable tiles adjacent to the target.
                    let mut goals = BTreeSet::new();
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let goal = TilePos::new(target_pos.x + dx, target_pos.y + dy);
                            if self.world.grid.get(goal).is_some_and(|t| t.move_ticks().is_some()) {
                                goals.insert(goal);
                            }
                        }
                    }
                    match astar(&self.world.grid, pos, &goals) {
                        Some(path) if path.is_empty() => {
                            self.finish_action(id, Ok(Value::Unit));
                        }
                        Some(path) => {
                            let first_cost = self
                                .world
                                .grid
                                .get(path[0])
                                .and_then(|t| t.move_ticks())
                                .expect("path tiles are passable");
                            let bot = self.world.bots.get_mut(&id).expect("bot exists");
                            bot.data.action = Some(Action::Move { path, ticks_left: first_cost });
                        }
                        None => {
                            self.finish_action(id, Err("move_to: unreachable".into()));
                        }
                    }
                }
                ActionRequest::Mine => {
                    let node = self
                        .world
                        .ore_nodes
                        .iter()
                        .filter(|(_, n)| n.amount > 0 && pos.chebyshev(n.pos) <= 1)
                        .map(|(nid, _)| *nid)
                        .next();
                    match node {
                        Some(node) => {
                            bot.data.action = Some(Action::Mine { node, ticks_left: 2 });
                        }
                        None => self.finish_action(id, Err("mine: no ore in range".into())),
                    }
                }
                ActionRequest::Attack(target) => {
                    if self.world.entity_pos(target).is_none() {
                        self.finish_action(id, Err("attack: no such target".into()));
                        return;
                    }
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.action = Some(Action::Attack { target, ticks_left: 1 });
                }
                ActionRequest::Deposit => {
                    let depot = self
                        .world
                        .depots
                        .iter()
                        .filter(|(_, d)| pos.chebyshev(d.pos) <= 1)
                        .map(|(did, _)| *did)
                        .next();
                    match depot {
                        Some(depot) => {
                            if bot.data.cargo == 0 {
                                self.finish_action(id, Err("deposit: no cargo".into()));
                            } else {
                                bot.data.action = Some(Action::Deposit { depot, ticks_left: 1 });
                            }
                        }
                        None => self.finish_action(id, Err("deposit: no depot in range".into())),
                    }
                }
            }
            return;
        }

        // Advance an in-flight action.
        let Some(action) = bot.data.action.take() else { return };
        match action {
            Action::Move { mut path, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Move { path, ticks_left });
                    return;
                }
                let entered = path.remove(0);
                bot.data.pos = entered;
                if path.is_empty() {
                    self.finish_action(id, Ok(Value::Unit));
                } else {
                    let next_cost = self
                        .world
                        .grid
                        .get(path[0])
                        .and_then(|t| t.move_ticks())
                        .expect("path tiles are passable");
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.action = Some(Action::Move { path, ticks_left: next_cost });
                }
            }
            Action::Mine { node, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Mine { node, ticks_left });
                    return;
                }
                if bot.data.cargo >= bot.data.cargo_cap {
                    self.finish_action(id, Err("mine: cargo full".into()));
                    return;
                }
                let Some(ore) = self.world.ore_nodes.get_mut(&node) else {
                    self.finish_action(id, Err("mine: ore node gone".into()));
                    return;
                };
                if ore.amount == 0 {
                    self.finish_action(id, Err("mine: ore depleted".into()));
                    return;
                }
                ore.amount -= 1;
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                bot.data.cargo += 1;
                bot.data.xp_mining += 1;
                self.finish_action(id, Ok(Value::Unit));
            }
            Action::Attack { target, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Attack { target, ticks_left });
                    return;
                }
                let pos = bot.data.pos;
                let Some(target_bot) = self.world.bot_entities.get(&target).copied() else {
                    self.finish_action(id, Err("attack: target destroyed".into()));
                    return;
                };
                let Some(target_pos) = self.world.entity_pos(target) else {
                    self.finish_action(id, Err("attack: target destroyed".into()));
                    return;
                };
                if pos.chebyshev(target_pos) > 1 {
                    self.finish_action(id, Err("attack: target out of range".into()));
                    return;
                }
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                bot.data.xp_combat += ATTACK_DAMAGE as u64;
                self.finish_action(id, Ok(Value::Unit));
                self.apply_damage(target_bot, ATTACK_DAMAGE);
            }
            Action::Deposit { depot, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Deposit { depot, ticks_left });
                    return;
                }
                let cargo = bot.data.cargo;
                bot.data.cargo = 0;
                bot.data.xp_hauling += cargo as u64;
                self.world.stockpile_ore += cargo as u64;
                self.finish_action(id, Ok(Value::Unit));
            }
        }
    }

    /// Resume a bot's VM with an action result (fault path may run handlers
    /// or force a crash dump — hence the host).
    fn finish_action(&mut self, id: BotId, result: Result<Value, String>) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.action = None;
        let mut vm = bot.vm.take().expect("vm present between phases");
        {
            let mut host = BotHost { world: &mut self.world, bot: id };
            vm.resolve_action(result, &mut host, &self.costs);
        }
        if let Some(bot) = self.world.bots.get_mut(&id) {
            bot.vm = Some(vm);
        }
    }

    /// Phase 4.5: engine-driven state — boot countdowns and recall walks.
    fn advance_engine(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        if bot.data.dying {
            return;
        }

        // Boot Sequence countdown.
        if let Some(ticks) = bot.data.booting {
            if ticks > 1 {
                bot.data.booting = Some(ticks - 1);
            } else {
                self.finish_boot(id);
            }
            return;
        }

        // Recall walk.
        let Some(mut recall) = bot.data.recall.take() else { return };
        if !recall.path.is_empty() {
            recall.ticks_left -= 1;
            if recall.ticks_left > 0 {
                bot.data.recall = Some(recall);
                return;
            }
            let entered = recall.path.remove(0);
            bot.data.pos = entered;
            if let Some(next) = recall.path.first() {
                recall.ticks_left = self
                    .world
                    .grid
                    .get(*next)
                    .and_then(|t| t.move_ticks())
                    .expect("recall path tiles are passable");
                bot.data.recall = Some(recall);
                return;
            }
        }
        // Arrived at the home printer.
        match recall.purpose {
            RecallPurpose::Recolor { dest } => self.recolor_bot(id, dest),
            RecallPurpose::Scrap => self.scrap_bot(id),
        }
    }

    /// Boot step 1: forced `upload_log()` if the buffer is non-empty
    /// (docs/02); step 2: program from line 1, interrupt context ends.
    fn finish_boot(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.booting = None;
        let logs = std::mem::take(&mut bot.data.log_buf);
        let tick = self.world.tick;
        for text in logs {
            self.world.archive.push(ArchiveEntry {
                tick,
                bot: id,
                kind: ArchiveKind::Log,
                line: 0,
                text,
            });
        }
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        if let Some(vm) = bot.vm.as_mut() {
            vm.set_engine_interrupt(false);
        }
    }

    /// Recall arrival: "transported to the new printer for a new color,
    /// keeping XP" (docs/01). Fresh VM on the destination color's program,
    /// then the Boot Sequence.
    fn recolor_bot(&mut self, id: BotId, dest: EntityId) {
        let Some(printer) = self.world.printers.get(&dest) else {
            // Destination vanished: end the recall, resume the old program.
            self.finish_boot(id);
            return;
        };
        let (printer_pos, color, faction) = (printer.pos, printer.color, printer.faction);
        let Some(cp) = self.world.color_programs.get(&(faction, color.0)) else {
            self.finish_boot(id);
            return;
        };
        let program = Rc::clone(&cp.program);
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.pos = printer_pos;
        bot.data.color = color;
        bot.data.recall = None;
        bot.data.booting = Some(self.tuning.boot_ticks);
        let mut vm = Vm::new(program, self.vm_config.clone());
        vm.set_engine_interrupt(true);
        bot.vm = Some(vm);
    }

    /// Over-capacity decommission: logs upload to the cloud (the bot is at
    /// the printer), partial refund, no wreck, no black box — an orderly
    /// recycling, not a destruction.
    fn scrap_bot(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.remove(&id) else { return };
        self.world.bot_entities.remove(&bot.data.entity);
        let tick = self.world.tick;
        for text in bot.data.log_buf {
            self.world.archive.push(ArchiveEntry {
                tick,
                bot: id,
                kind: ArchiveKind::Log,
                line: 0,
                text,
            });
        }
        self.world.stockpile_ore += self.tuning.scrap_refund_ore;
    }

    /// Phase 6: printers — advance/start print jobs, rebalance recalls,
    /// capacity scrap. All iteration in stable id order.
    fn run_printers(&mut self) {
        let printer_ids: Vec<EntityId> = self.world.printers.keys().copied().collect();

        // Advance and complete print jobs.
        for pid in printer_ids.iter() {
            let printer = self.world.printers.get_mut(pid).expect("printer exists");
            if printer.state != PrinterState::Working {
                continue;
            }
            if let Some(ticks) = printer.job {
                if ticks > 1 {
                    printer.job = Some(ticks - 1);
                } else {
                    printer.job = None;
                    let (pos, faction, color) = (printer.pos, printer.faction, printer.color);
                    match self.world.color_programs.get(&(faction, color.0)) {
                        Some(cp) => {
                            let program = Rc::clone(&cp.program);
                            let mut vm = Vm::new(program, self.vm_config.clone());
                            vm.set_engine_interrupt(true);
                            let t = &self.tuning;
                            let (hp, cpu, cap) = (t.printed_hp, t.printed_cpu, t.printed_cargo_cap);
                            self.insert_bot(pos, faction, color, hp, cpu, cap, vm, true);
                        }
                        None => {
                            // Program was undeployed mid-print: refund.
                            self.world.stockpile_ore += self.tuning.print_cost_ore;
                        }
                    }
                }
            }
        }

        // Rebalance BEFORE starting print jobs: moving an existing bot is
        // cheaper than printing and preserves XP, so recalls claim headroom
        // first (incoming re-colors count toward the destination's
        // population, which the job loop below then sees as filled).
        // Recall only fires when a destination exists — docs/01.
        for pid in printer_ids.iter().copied() {
            let printer = &self.world.printers[&pid];
            if printer.state != PrinterState::Working {
                continue;
            }
            let (faction, color, desired) = (printer.faction, printer.color, printer.desired_max);
            let population = self.world.color_population(faction, color);
            if population <= desired {
                continue;
            }
            // Destination: lowest-id working printer of this faction with
            // headroom (population + pending job below its dial).
            let dest = printer_ids.iter().copied().find(|did| {
                let p = &self.world.printers[did];
                if p.faction != faction || p.color == color || p.state != PrinterState::Working {
                    return false;
                }
                if !self.world.color_programs.contains_key(&(p.faction, p.color.0)) {
                    return false;
                }
                let pop = self.world.color_population(p.faction, p.color)
                    + p.job.is_some() as u32;
                pop < p.desired_max
            });
            let Some(dest) = dest else { continue };
            self.start_recall(faction, color, pid, RecallPurpose::Recolor { dest });
        }

        // Start new jobs where population is below the dial.
        for pid in printer_ids.iter() {
            let printer = &self.world.printers[pid];
            if printer.state != PrinterState::Working || printer.job.is_some() {
                continue;
            }
            let (faction, color, desired) = (printer.faction, printer.color, printer.desired_max);
            if !self.world.color_programs.contains_key(&(faction, color.0)) {
                continue;
            }
            let population = self.world.color_population(faction, color);
            if population < desired && self.world.stockpile_ore >= self.tuning.print_cost_ore {
                self.world.stockpile_ore -= self.tuning.print_cost_ore;
                self.world.printers.get_mut(pid).expect("printer exists").job =
                    Some(self.tuning.print_ticks);
            }
        }

        // Capacity: scrap recalls when the colony over-extends.
        let mut factions: BTreeSet<u8> = BTreeSet::new();
        for bot in self.world.bots.values() {
            factions.insert(bot.data.faction);
        }
        for faction in factions {
            let live = self
                .world
                .bots
                .values()
                .filter(|b| b.data.faction == faction && !b.data.dying && b.data.recall.is_none())
                .count() as u32;
            if live > self.tuning.capacity {
                // Lowest-XP bot colony-wide walks home for scrap.
                let victim = self
                    .world
                    .bots
                    .values()
                    .filter(|b| {
                        b.data.faction == faction
                            && !b.data.dying
                            && b.data.recall.is_none()
                            && b.data.booting.is_none()
                    })
                    .map(|b| (b.data.xp_mining + b.data.xp_hauling + b.data.xp_combat, b.data.id))
                    .min();
                if let Some((_, victim)) = victim {
                    let home = self.nearest_faction_printer(victim);
                    if let Some(home) = home {
                        self.begin_recall_walk(victim, home, RecallPurpose::Scrap);
                    }
                }
            }
        }
    }

    /// Pick the lowest-total-XP bot of (faction, color) and start its
    /// recall toward its own printer.
    fn start_recall(&mut self, faction: u8, color: Color, home: EntityId, purpose: RecallPurpose) {
        let victim = self
            .world
            .bots
            .values()
            .filter(|b| {
                b.data.faction == faction
                    && b.data.color == color
                    && !b.data.dying
                    && b.data.recall.is_none()
                    && b.data.booting.is_none()
            })
            .map(|b| (b.data.xp_mining + b.data.xp_hauling + b.data.xp_combat, b.data.id))
            .min();
        if let Some((_, victim)) = victim {
            self.begin_recall_walk(victim, home, purpose);
        }
    }

    fn nearest_faction_printer(&self, bot: BotId) -> Option<EntityId> {
        let data = &self.world.bots.get(&bot)?.data;
        self.world
            .printers
            .iter()
            .filter(|(_, p)| p.faction == data.faction && p.state == PrinterState::Working)
            .map(|(id, p)| (data.pos.manhattan(p.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    /// Suspend the program (engine interrupt), cancel any action, and start
    /// walking to the home printer. Double-handle applies the whole way.
    fn begin_recall_walk(&mut self, id: BotId, home: EntityId, purpose: RecallPurpose) {
        let Some(home_pos) = self.world.printers.get(&home).map(|p| p.pos) else { return };
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        let start = bot.data.pos;
        let mut goals = BTreeSet::new();
        goals.insert(home_pos);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let g = TilePos::new(home_pos.x + dx, home_pos.y + dy);
                if self.world.grid.get(g).is_some_and(|t| t.move_ticks().is_some()) {
                    goals.insert(g);
                }
            }
        }
        let path = astar(&self.world.grid, start, &goals).unwrap_or_default();
        let ticks_left = path
            .first()
            .map(|p| self.world.grid.get(*p).and_then(|t| t.move_ticks()).unwrap_or(1))
            .unwrap_or(0);
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.requested = None;
        bot.data.action = None;
        bot.data.recall = Some(Recall { path, ticks_left, purpose });
        if let Some(vm) = bot.vm.as_mut() {
            vm.set_engine_interrupt(true);
        }
    }

    /// Damage pipeline: hp change, then signals per docs/01-language.md.
    /// Hurt is edge-triggered at the program's threshold (default 50%);
    /// death raises `Signal::Death`; a signal landing while a handler (or
    /// boot/recall) is active is a double handle — the bot explodes.
    fn apply_damage(&mut self, id: BotId, amount: i64) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        if bot.data.dying {
            return; // effectively a wreck already
        }
        bot.data.hp = (bot.data.hp - amount).max(0);
        let hp = bot.data.hp;
        let max_hp = bot.data.max_hp;
        if hp == 0 {
            self.raise_signal(id, Signal::Death);
            return;
        }
        let threshold_pct = self
            .world
            .bots
            .get(&id)
            .and_then(|b| b.vm.as_ref())
            .and_then(|vm| vm.hurt_threshold())
            .unwrap_or(50);
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        if !bot.data.hurt_fired && hp * 100 < max_hp * threshold_pct {
            bot.data.hurt_fired = true;
            self.raise_signal(id, Signal::Hurt);
        }
    }

    fn raise_signal(&mut self, id: BotId, signal: Signal) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        let mut vm = bot.vm.take().expect("vm present between phases");
        let outcome = {
            let mut host = BotHost { world: &mut self.world, bot: id };
            vm.raise(signal, &mut host, &self.costs)
        };
        match outcome {
            RaiseOutcome::Handled => {
                // Entering a handler abandons any in-flight action (the
                // pending world action is cancelled).
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.data.action = None;
                    bot.data.requested = None;
                    bot.vm = Some(vm);
                }
            }
            RaiseOutcome::Ignored | RaiseOutcome::Died => {
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.vm = Some(vm);
                }
            }
            RaiseOutcome::Exploded => self.explode(id, &vm),
        }
    }

    /// Double handle: instant destruction — no wreck, but every destruction
    /// drops a Black Box (docs/02-agents.md).
    fn explode(&mut self, id: BotId, vm: &Vm) {
        if let Some(bot) = self.world.bots.remove(&id) {
            self.world.bot_entities.remove(&bot.data.entity);
            self.world.black_boxes.push(BlackBox {
                tick: self.world.tick,
                bot: id,
                pos: bot.data.pos,
                cause: vm.last_fault().unwrap_or("double handle").to_string(),
                logs: bot.data.log_buf,
            });
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
        for ((faction, color), cp) in &w.color_programs {
            h.write_u8(*faction);
            h.write_u8(*color);
            h.write_u32(cp.version);
            h.write_str(&cp.source);
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
            h.write_u8(bot.data.dying as u8);
            h.write_u32(bot.data.booting.unwrap_or(0));
            h.write_u8(bot.data.recall.is_some() as u8);
            h.write_u64(bot.data.xp_mining);
            h.write_u64(bot.data.xp_hauling);
            for entry in &bot.data.log_buf {
                h.write_str(entry);
            }
            if let Some(vm) = &bot.vm {
                h.write_i64(vm.budget());
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
