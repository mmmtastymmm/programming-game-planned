//! Action resolution: starting, advancing, and finishing the blocking
//! world actions bots issue (move, mine, build, ...).

use crate::host::BotHost;
use crate::map::{astar_avoiding, TileKind, TilePos};
use crate::sim::{Sim, ATTACK_DAMAGE};
use crate::world::{
    Action, ActionRequest, BlueprintKind, BotId, RecallPurpose, XpTrack,
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
                        .nodes
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
                ActionRequest::Deposit { fault_on_fail } => {
                    // Generalized acceptor (docs/03): an adjacent depot
                    // takes everything into colony stock; an adjacent
                    // refinery takes only its recipe's inputs into its
                    // physical input buffer. Depots first, then the
                    // lowest-id accepting structure.
                    let faction = bot.data.faction;
                    let carrying = bot.data.cargo.clone();
                    let empty = bot.data.cargo_total() == 0;
                    let depot = self
                        .world
                        .depots
                        .iter()
                        .filter(|(_, d)| pos.chebyshev(d.pos) <= 1)
                        .map(|(did, _)| *did)
                        .next();
                    let refinery = self
                        .world
                        .structures
                        .iter()
                        .filter(|(_, st)| {
                            st.faction == faction
                                && pos.chebyshev(st.pos) <= 1
                                && st.recipe.is_some_and(|idx| {
                                    crate::resources::RECIPES[idx as usize]
                                        .inputs
                                        .iter()
                                        .any(|(k, _)| carrying.contains_key(k))
                                })
                        })
                        .map(|(sid, _)| *sid)
                        .next();
                    let target = depot.or(refinery);
                    match target {
                        _ if empty => {
                            let outcome = if fault_on_fail {
                                Err("deposit: no cargo".into())
                            } else {
                                Ok(Value::Bool(false))
                            };
                            self.finish_action(id, outcome);
                        }
                        Some(target) => {
                            bot.data.action =
                                Some(Action::Deposit { depot: target, ticks_left: 1 });
                        }
                        None => {
                            let outcome = if fault_on_fail {
                                Err("deposit: no acceptor in range".into())
                            } else {
                                Ok(Value::Bool(false))
                            };
                            self.finish_action(id, outcome);
                        }
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
                path.remove(0);
                self.world.move_bot(id, entered);
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
                if bot.data.cargo_total() >= bot.data.cargo_cap {
                    self.finish_action(id, Err("mine: cargo full".into()));
                    return;
                }
                let Some(node_ref) = self.world.nodes.get_mut(&node) else {
                    self.finish_action(id, Err("mine: node gone".into()));
                    return;
                };
                if node_ref.amount == 0 {
                    self.finish_action(id, Err("mine: node depleted".into()));
                    return;
                }
                // Typed yield (docs/03): the node's kind, mine_yield_deci
                // per swing, clamped by what the node and the hold allow.
                let kind = node_ref.kind;
                let swing = self.tuning.mine_yield_deci.min(node_ref.amount);
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                let loaded = bot.data.cargo_add(kind, swing);
                self.world.nodes.get_mut(&node).expect("checked above").amount -= loaded;
                self.world.pending_xp.push((
                    id,
                    XpTrack::Mining,
                    (loaded / crate::resources::DECI) as u64,
                ));
                self.finish_action(id, Ok(Value::Unit));
            }
            Action::Attack { target, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Attack { target, ticks_left });
                    return;
                }
                let pos = bot.data.pos;
                // Structures are attackable (docs/03, M4): direct chassis
                // damage, no signals — a destroyed structure just falls
                // (its buffered contents are lost with it for now).
                if let Some(st) = self.world.structures.get_mut(&target) {
                    if pos.chebyshev(st.pos) > 1 {
                        self.finish_action(id, Err("attack: target out of range".into()));
                        return;
                    }
                    st.hp = (st.hp - ATTACK_DAMAGE).max(0);
                    let felled = st.hp == 0;
                    if felled {
                        self.world.structures.remove(&target);
                    }
                    self.world.pending_xp.push((id, XpTrack::Combat, ATTACK_DAMAGE as u64));
                    self.finish_action(id, Ok(Value::Unit));
                    return;
                }
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
                self.world.pending_xp.push((id, XpTrack::Combat, ATTACK_DAMAGE as u64));
                self.finish_action(id, Ok(Value::Unit));
                let attacker_faction = self.world.bots[&id].data.faction;
                self.queue_damage(target_bot, ATTACK_DAMAGE, Some(attacker_faction));
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
                self.world.pending_xp.push((id, XpTrack::Building, 1));
                if done {
                    self.world.blueprints.remove(&blueprint);
                    match kind {
                        BlueprintKind::Bridge => self.world.set_tile(site, TileKind::Bridge),
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
                let faction = bot.data.faction;
                let mut total = 0u32;
                if self.world.structures.contains_key(&depot) {
                    // Refinery feed: only the set recipe's inputs move.
                    let st = &self.world.structures[&depot];
                    let inputs: Vec<crate::resources::Resource> = st
                        .recipe
                        .map(|idx| {
                            crate::resources::RECIPES[idx as usize]
                                .inputs
                                .iter()
                                .map(|(k, _)| *k)
                                .collect()
                        })
                        .unwrap_or_default();
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    let mut moved: Vec<(crate::resources::Resource, u32)> = Vec::new();
                    for kind in inputs {
                        let deci = bot.data.cargo_remove(kind, u32::MAX);
                        if deci > 0 {
                            moved.push((kind, deci));
                            total += deci;
                        }
                    }
                    let st = self.world.structures.get_mut(&depot).expect("checked above");
                    for (kind, deci) in moved {
                        *st.input.entry(kind).or_insert(0) += deci;
                    }
                } else {
                    // Depot: the whole manifest enters colony stock
                    // (docs/03: payments draw from this abstract pool).
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    let manifest = std::mem::take(&mut bot.data.cargo);
                    total = manifest.values().sum();
                    for (kind, deci) in manifest {
                        self.world.stock_add(faction, kind, deci as u64);
                    }

                }
                self.world.pending_xp.push((
                    id,
                    XpTrack::Hauling,
                    (total / crate::resources::DECI) as u64,
                ));
                self.track_delivery_milestone(faction, total);
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
            let mut host = BotHost { world: &mut self.world, bot: id, tuning: &self.tuning };
            vm.resolve_action(result, &mut host, &self.costs);
        }
        if let Some(bot) = self.world.bots.get_mut(&id) {
            bot.vm = Some(vm);
        }
    }

    /// Spilled cargo becomes a small ore node on a random adjacent
    /// passable tile (seeded RNG; falls back to the bot's own tile),
    /// merging with any node already there — closest(ore)/mine() recover it.
    pub(crate) fn drop_cargo_to_ground(
        &mut self,
        pos: TilePos,
        manifest: std::collections::BTreeMap<crate::resources::Resource, u32>,
    ) {
        if manifest.values().all(|&d| d == 0) {
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
        for (kind, amount) in manifest {
            if amount == 0 {
                continue;
            }
            let existing = self
                .world
                .nodes
                .iter()
                .find(|(_, n)| n.pos == drop_at && n.kind == kind)
                .map(|(id, _)| *id);
            match existing {
                Some(id) => {
                    self.world.nodes.get_mut(&id).expect("just found").amount += amount;
                }
                None => {
                    let id = self.world.alloc_entity();
                    self.world.nodes.insert(
                        id,
                        crate::world::ResourceNode { kind, pos: drop_at, amount, regen: false },
                    );
                }
            }
        }
    }

    /// Data income: 20 Data per full `delivery_milestone_deci` delivered
    /// (docs/03: "deliver 500 ore" milestones — activity, not mining).
    pub(crate) fn track_delivery_milestone(&mut self, faction: u8, delivered_deci: u32) {
        let step = self.tuning.delivery_milestone_deci;
        if step == 0 {
            return;
        }
        let before = self.world.delivered.get(&faction).copied().unwrap_or(0);
        let after = before + delivered_deci as u64;
        self.world.delivered.insert(faction, after);
        let crossed = (after / step as u64) - (before / step as u64);
        if crossed > 0 {
            *self.world.data.entry(faction).or_insert(0) +=
                crossed * self.tuning.milestone_data;
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
            recall.path.remove(0);
            self.world.move_bot(id, entered);
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
            self.world.bots.get_mut(&id).expect("bot exists").data.recall = Some(recall);
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
