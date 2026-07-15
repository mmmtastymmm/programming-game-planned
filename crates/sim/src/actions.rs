//! Action resolution: starting, advancing, and finishing the blocking
//! world actions bots issue (move, mine, build, ...).

use crate::host::BotHost;
use crate::map::{astar_avoiding, TileKind, TilePos};
use crate::sim::{Sim, ATTACK_DAMAGE};
use crate::world::{
    Action, ActionRequest, BlueprintKind, BotId, RecallPurpose,
};
use pyrite::Value;
use std::collections::BTreeSet;

impl Sim {
    /// Phase 4 for one bot: start its requested action or advance the
    /// in-flight one; on completion, resume the VM with the result.
    pub(crate) fn resolve_bot(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        if bot.data.dying {
            return;
        }
        if bot.data.bump_frozen > 0 {
            bot.data.bump_frozen -= 1;
            if bot.data.bump_frozen == 0 {
                self.replan_after_bump(id);
            }
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
                    // Goal set: passable, non-structure tiles adjacent to
                    // the target (structures are solid, so "at" a printer
                    // or depot means standing beside it).
                    let structures = self.world.structure_tiles();
                    let mut goals = BTreeSet::new();
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let goal = TilePos::new(target_pos.x + dx, target_pos.y + dy);
                            if self.world.grid.get(goal).is_some_and(|t| t.move_ticks().is_some())
                                && !structures.contains(&goal)
                            {
                                goals.insert(goal);
                            }
                        }
                    }
                    match astar_avoiding(
                        &self.world.grid,
                        &self.world.overlays,
                        pos,
                        &goals,
                        &structures,
                    ) {
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
                            bot.data.action =
                                Some(Action::Move { path, ticks_left: first_cost, goals });
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
                ActionRequest::Wait(ticks) => {
                    bot.data.action = Some(Action::Wait { ticks_left: ticks });
                }
                ActionRequest::Build(blueprint) => {
                    bot.data.action = Some(Action::Build { blueprint });
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
            Action::Move { mut path, ticks_left, goals } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Move { path, ticks_left, goals });
                    return;
                }
                // Bots are solid. If the next tile is occupied, first try a
                // random sidestep among free neighbors that lose no ground
                // toward the goal; only when boxed in, bump-freeze (and
                // re-plan on thaw).
                let entered = path[0];
                if self.world.tile_occupied(entered, id) {
                    let from = self.world.bots[&id].data.pos;
                    let dodges = self.sidestep_candidates(id, from, entered, &goals);
                    if dodges.is_empty() {
                        let bot = self.world.bots.get_mut(&id).expect("bot exists");
                        bot.data.action = Some(Action::Move { path, ticks_left: 1, goals });
                        self.bump_both(id, entered, true);
                    } else {
                        let pick = (crate::world::next_rand(&mut self.world.rng.sidestep)
                            % dodges.len() as u64) as usize;
                        let step = dodges[pick];
                        let cost = self
                            .world
                            .grid
                            .get(step)
                            .and_then(|t| t.move_ticks())
                            .expect("candidates are passable");
                        let bot = self.world.bots.get_mut(&id).expect("bot exists");
                        // Single-step path; landing off-route triggers a
                        // re-plan (see the empty-path branch below).
                        bot.data.action =
                            Some(Action::Move { path: vec![step], ticks_left: cost, goals });
                    }
                    return;
                }
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                path.remove(0);
                bot.data.pos = entered;
                if path.is_empty() {
                    if goals.contains(&entered) {
                        self.finish_action(id, Ok(Value::Unit));
                    } else {
                        // A dodge landed us off-route: plan a fresh path,
                        // preferring one that threads around current bots.
                        self.replan_move(id, goals);
                    }
                } else {
                    let next_cost = self
                        .world
                        .grid
                        .get(path[0])
                        .and_then(|t| t.move_ticks())
                        .expect("path tiles are passable");
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.action = Some(Action::Move { path, ticks_left: next_cost, goals });
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
            Action::Build { blueprint } => {
                let pos = bot.data.pos;
                let Some(bp) = self.world.blueprints.get_mut(&blueprint) else {
                    // Someone else finished it: that's success.
                    self.finish_action(id, Ok(Value::Unit));
                    return;
                };
                if pos.chebyshev(bp.pos) > 1 {
                    self.finish_action(id, Err("build: blueprint out of range".into()));
                    return;
                }
                bp.progress += 1;
                let done = bp.progress >= bp.needed;
                let (site, kind) = (bp.pos, bp.kind);
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                bot.data.xp_building += 1;
                if done {
                    self.world.blueprints.remove(&blueprint);
                    match kind {
                        BlueprintKind::Bridge => self.world.grid.set(site, TileKind::Bridge),
                    }
                    self.finish_action(id, Ok(Value::Unit));
                } else {
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.action = Some(Action::Build { blueprint });
                }
            }
            Action::Wait { ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Wait { ticks_left });
                } else {
                    self.finish_action(id, Ok(Value::Unit));
                }
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
    pub(crate) fn finish_action(&mut self, id: BotId, result: Result<Value, String>) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.action = None;
        let mut vm = bot.vm.take().expect("vm present between phases");
        {
            let mut host = BotHost { world: &mut self.world, bot: id, tuning_handler_init_ticks: self.tuning.handler_init_ticks };
            vm.resolve_action(result, &mut host, &self.costs);
        }
        if let Some(bot) = self.world.bots.get_mut(&id) {
            bot.vm = Some(vm);
        }
    }

    /// Spilled cargo becomes a small ore node on a random adjacent
    /// passable tile (seeded RNG; falls back to the bot's own tile),
    /// merging with any node already there — closest(ore)/mine() recover it.
    pub(crate) fn drop_cargo_to_ground(&mut self, pos: TilePos, amount: u32) {
        if amount == 0 {
            return;
        }
        let mut candidates: Vec<TilePos> = [(0, -1), (1, 0), (0, 1), (-1, 0)]
            .iter()
            .map(|(dx, dy)| TilePos::new(pos.x + dx, pos.y + dy))
            .filter(|&p| self.world.grid.get(p).is_some_and(|t| t.move_ticks().is_some()))
            .collect();
        candidates.push(pos);
        // Spill scatter draws from rng.combat (deaths are what spill cargo);
        // if scatter grows beyond combat outcomes it earns its own stream in
        // docs/07's inventory.
        let drop_at = candidates[(crate::world::next_rand(&mut self.world.rng.combat)
            % candidates.len() as u64) as usize];
        let existing = self
            .world
            .ore_nodes
            .iter()
            .find(|(_, n)| n.pos == drop_at)
            .map(|(id, _)| *id);
        match existing {
            Some(id) => {
                self.world.ore_nodes.get_mut(&id).expect("just found").amount += amount;
            }
            None => {
                let id = self.world.alloc_entity();
                self.world.ore_nodes.insert(id, crate::world::OreNode { pos: drop_at, amount });
            }
        }
    }

    /// Phase 4.5: engine-driven state — boot countdowns and recall walks.
    pub(crate) fn advance_engine(&mut self, id: BotId) {
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
        if bot.data.bump_frozen > 0 {
            return; // decremented in resolve_bot
        }
        let Some(mut recall) = bot.data.recall.take() else { return };
        if !recall.path.is_empty() {
            recall.ticks_left -= 1;
            if recall.ticks_left > 0 {
                bot.data.recall = Some(recall);
                return;
            }
            let entered = recall.path[0];
            if self.world.tile_occupied(entered, id) {
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                recall.ticks_left = 1;
                bot.data.recall = Some(recall);
                self.bump_both(id, entered, false);
                return;
            }
            let bot = self.world.bots.get_mut(&id).expect("bot exists");
            recall.path.remove(0);
            bot.data.pos = entered;
            if let Some(next) = recall.path.first() {
                recall.ticks_left = self
                    .world
                    .grid
                    .get(*next)
                    .and_then(|t| t.move_ticks())
                    .expect("recall path tiles are passable");
            }
            // Stand on the arrival tile for one tick even when the path is
            // done — the walk's last step must be observable (the printer
            // starts its work next tick, not mid-stride).
            bot.data.recall = Some(recall);
            return;
        }
        // Arrived at the home printer.
        match recall.purpose {
            RecallPurpose::Recolor { dest } => {
                if !self.recolor_bot(id, dest) {
                    // No free tile beside the destination yet: hold here,
                    // recall intact, and retry next tick.
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.recall = Some(recall);
                }
            }
            RecallPurpose::Scrap => self.scrap_bot(id),
        }
    }
}
