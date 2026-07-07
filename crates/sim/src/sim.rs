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
    Action, ActionRequest, BlackBox, Bot, BotData, BotId, Wreck, World,
};
use pyrite::{CostTable, Outcome, PyriteError, UnlockSet, Value, Vm, VmConfig};
use std::collections::BTreeSet;
use std::rc::Rc;

/// External inputs: the ONLY way anything outside the sim mutates it
/// (single-player is lockstep with one peer).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SpawnBot { pos: TilePos, source: String, cpu: u64, cargo_cap: u32 },
}

pub struct Sim {
    pub world: World,
    pub costs: CostTable,
    pub vm_config: VmConfig,
}

impl Sim {
    pub fn new(spec: &MapSpec) -> Self {
        Self {
            world: World::from_spec(spec),
            costs: CostTable::default(),
            vm_config: VmConfig::default(),
        }
    }

    /// Phase 1: apply a command. Deterministic given identical call order.
    pub fn apply(&mut self, command: &Command) -> Result<BotId, PyriteError> {
        match command {
            Command::SpawnBot { pos, source, cpu, cargo_cap } => {
                let program = pyrite::parse(source, &UnlockSet::all())?;
                let id = self.world.alloc_bot_id();
                let vm = Vm::new(Rc::new(program), self.vm_config.clone());
                self.world.bots.insert(
                    id,
                    Bot {
                        data: BotData {
                            id,
                            pos: *pos,
                            cargo: 0,
                            cargo_cap: *cargo_cap,
                            cpu: *cpu,
                            requested: None,
                            action: None,
                            dying: false,
                            log_buf: Vec::new(),
                            xp_mining: 0,
                            xp_hauling: 0,
                        },
                        vm: Some(vm),
                    },
                );
                Ok(id)
            }
        }
    }

    /// One fixed simulation tick (phases 2–5).
    pub fn step(&mut self) {
        self.world.tick += 1;

        // --- phase 2: grant + step VMs, stable id order ---
        let ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            if bot.data.dying {
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

        // --- phase 5: deaths ---
        for id in ids {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if !bot.data.dying {
                continue;
            }
            let bot = self.world.bots.remove(&id).expect("checked above");
            let data = bot.data;
            // Disabled: an inert wreck (self-destruct countdown comes later).
            self.world.wrecks.insert(id, Wreck { pos: data.pos, cargo: data.cargo, logs: data.log_buf });
        }
    }

    /// Store the VM back (or destroy the bot, per the outcome).
    fn after_vm(&mut self, id: BotId, vm: Vm, outcome: Outcome) {
        match outcome {
            Outcome::Paused | Outcome::Blocked | Outcome::Dead => {
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.vm = Some(vm);
                }
            }
            Outcome::Exploded => {
                // Double handle: no wreck — but every destruction drops a
                // Black Box (docs/02-agents.md).
                if let Some(bot) = self.world.bots.remove(&id) {
                    self.world.black_boxes.push(BlackBox {
                        tick: self.world.tick,
                        bot: id,
                        pos: bot.data.pos,
                        cause: vm.last_fault().unwrap_or("double handle").to_string(),
                        logs: bot.data.log_buf,
                    });
                }
            }
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
        for (id, bot) in &w.bots {
            h.write_u32(id.0);
            h.write_i32(bot.data.pos.x);
            h.write_i32(bot.data.pos.y);
            h.write_u32(bot.data.cargo);
            h.write_u64(bot.data.cpu);
            h.write_u8(bot.data.dying as u8);
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
