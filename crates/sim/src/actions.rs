//! Action resolution: starting, advancing, and finishing the blocking
//! world actions bots issue (move, mine, build, ...).

use crate::host::BotHost;
use crate::map::{astar_avoiding, edge_allowed, TileKind, TilePos};
use crate::sim::Sim;
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
                            if self.world.grid.get(goal).is_some_and(|t| t.passable())
                                && !structures.contains(&goal)
                            {
                                goals.insert(goal);
                            }
                        }
                    }
                    match astar_avoiding(
                        &self.world.grid,
                        &self.world.overlays,
                        &self.tuning.tile_costs,
                        pos,
                        &goals,
                        &structures,
                    ) {
                        Some(path) if path.is_empty() => {
                            self.finish_action(id, Ok(Value::Unit));
                        }
                        Some(path) => {
                            // Ticks per step come from the move-rate stat
                            // through the pipeline; terrain multiplies (M5).
                            let first_cost = crate::stats::step_ticks(
                                self.ctx(),
                                &self.world.grid,
                                &self.world.bots[&id].data,
                                path[0],
                            )
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
                            // Mining L3 swings −25% (the pipeline).
                            let ticks_left = self.ctx().mine_swing_for(
                                &self.world.bots[&id].data,
                                self.tuning.mine_swing_ticks,
                            );
                            let bot = self.world.bots.get_mut(&id).expect("bot exists");
                            bot.data.action = Some(Action::Mine { node, ticks_left });
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
                ActionRequest::Search => self.start_search(id),
                ActionRequest::Wander => {
                    // A seeded random walk leg: a random passable free
                    // tile within the leg length; unreachable picks are a
                    // completed (empty) leg, not a fault.
                    let leg = self.tuning.wander_leg as i32;
                    let mut candidates: Vec<TilePos> = Vec::new();
                    for dy in -leg..=leg {
                        for dx in -leg..=leg {
                            let t = TilePos::new(pos.x + dx, pos.y + dy);
                            if t != pos
                                && self
                                    .world
                                    .grid
                                    .get(t)
                                    .is_some_and(|k| k.passable())
                                && !self.world.structure_at(t)
                                && !self.world.tile_occupied(t, id)
                            {
                                candidates.push(t);
                            }
                        }
                    }
                    if candidates.is_empty() {
                        self.finish_action(id, Ok(Value::Unit));
                        return;
                    }
                    let pick = (crate::world::next_rand(&mut self.world.rng.wander)
                        % candidates.len() as u64) as usize;
                    self.walk_to_tile(id, candidates[pick], false);
                }
                ActionRequest::Explore => {
                    // The smart explorer (Q79): a random CURRENTLY-FOGGED
                    // passable tile within the radius; walk there, then
                    // drop into the scouting stance (survey_after_move).
                    let radius = self.tuning.explore_radius as i32;
                    let faction = self.world.bots[&id].data.faction;
                    let mut candidates: Vec<TilePos> = Vec::new();
                    for dy in -radius..=radius {
                        for dx in -radius..=radius {
                            let t = TilePos::new(pos.x + dx, pos.y + dy);
                            if t != pos
                                && self
                                    .world
                                    .grid
                                    .get(t)
                                    .is_some_and(|k| k.passable())
                                && !self.world.structure_at(t)
                                && !self.world.tile_occupied(t, id)
                                && !self.tile_visible(faction, t)
                            {
                                candidates.push(t);
                            }
                        }
                    }
                    if candidates.is_empty() {
                        // Nothing fogged in reach: explore degrades to a
                        // completed no-op (the map is known here).
                        self.finish_action(id, Ok(Value::Unit));
                        return;
                    }
                    let pick = (crate::world::next_rand(&mut self.world.rng.explore)
                        % candidates.len() as u64) as usize;
                    self.walk_to_tile(id, candidates[pick], true);
                }
                ActionRequest::Deposit { fault_on_fail } => {
                    // Generalized acceptor (docs/03): an adjacent depot
                    // takes everything into colony stock; an adjacent
                    // structure takes only what it FEEDS on — recipe
                    // inputs, Generator fuel, Station coolant. Depots
                    // first, then the lowest-id accepting structure.
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
                                && st.accepted_feed().iter().any(|k| carrying.contains_key(k))
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
                            bot.data.action = Some(Action::Deposit {
                                depot: target,
                                ticks_left: 1,
                                fault_on_fail,
                            });
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
                let from = self.world.bots[&id].data.pos;
                // Terraforming can HARDEN ground under an in-flight plan
                // (M8: a demolished bridge, a finished barricade): re-check
                // the step's edge and re-plan around works that landed
                // since — never walk into water or panic on an unpriced
                // tile (review 2026-07-16).
                if !edge_allowed(&self.world.grid, &self.world.overlays, from, entered) {
                    self.replan_move(id, goals);
                    return;
                }
                // Structures are solid too: one placed on the route AFTER
                // this path was planned blocks the step exactly like a bot
                // (plan-time A* only sees structures that already exist).
                if self.world.tile_occupied(entered, id) || self.world.structure_at(entered) {
                    let dodges = self.sidestep_candidates(id, from, entered, &goals);
                    if dodges.is_empty() {
                        let bot = self.world.bots.get_mut(&id).expect("bot exists");
                        bot.data.action = Some(Action::Move { path, ticks_left: 1, goals });
                        self.bump_both(id, entered, true);
                    } else {
                        let pick = (crate::world::next_rand(&mut self.world.rng.sidestep)
                            % dodges.len() as u64) as usize;
                        let step = dodges[pick];
                        let cost = crate::stats::step_ticks(
                            self.ctx(),
                            &self.world.grid,
                            &self.world.bots[&id].data,
                            step,
                        )
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
                self.credit_travel(id);
                // Ice slides (M8, Q37): momentum carries the mover one
                // more tile — chaining across ice — until solid ground or
                // a blocked edge. Sliding into an occupant is a collision
                // WITH THE SLIDER AT FAULT; the crunch spends the
                // momentum and the plan resumes.
                if let Some(target) = self.slide_target(entered, from) {
                    if self.world.tile_occupied(target, id) {
                        self.bump_both(id, target, true);
                    } else {
                        let cost = crate::stats::step_ticks(
                            self.ctx(),
                            &self.world.grid,
                            &self.world.bots[&id].data,
                            target,
                        )
                        .expect("slide targets are passable");
                        let bot = self.world.bots.get_mut(&id).expect("bot exists");
                        // Single-step override; landing off-route triggers
                        // the same re-plan as a dodge (empty-path branch).
                        bot.data.action =
                            Some(Action::Move { path: vec![target], ticks_left: cost, goals });
                        return;
                    }
                }
                if path.is_empty() {
                    if goals.contains(&entered) {
                        self.complete_move(id);
                    } else {
                        // A dodge landed us off-route: plan a fresh path,
                        // preferring one that threads around current bots.
                        self.replan_move(id, goals);
                    }
                } else {
                    // The rest of the plan can harden too — a None here
                    // means terrain changed since A* ran: re-plan.
                    match crate::stats::step_ticks(
                        self.ctx(),
                        &self.world.grid,
                        &self.world.bots[&id].data,
                        path[0],
                    ) {
                        Some(next_cost) => {
                            let bot = self.world.bots.get_mut(&id).expect("bot exists");
                            bot.data.action =
                                Some(Action::Move { path, ticks_left: next_cost, goals });
                        }
                        None => self.replan_move(id, goals),
                    }
                }
            }
            Action::Mine { node, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Mine { node, ticks_left });
                    return;
                }
                // Field-precise ctx literal: `bot` still borrows the world
                // mutably here, and `Sim::ctx()` would borrow all of self.
                let ctx = crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning };
                let cap = ctx.cargo_cap_for(&bot.data);
                if bot.data.cargo_total() >= cap {
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
                // per swing through the Mining perk (+10%/level), clamped
                // by what the node and the hold allow.
                let kind = node_ref.kind;
                let base = ctx
                    .mine_yield_for(&self.world.bots[&id].data, self.tuning.mine_yield_deci);
                let swing = base.min(self.world.nodes[&node].amount);
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                let loaded = bot.data.cargo_add(kind, swing, cap);
                self.world.nodes.get_mut(&node).expect("checked above").amount -= loaded;
                // Mining income: 1 XP/unit = 1 deci-XP per deci-unit.
                self.world.pending_xp.push((id, XpTrack::Mining, loaded as u64));
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
                let damage = self.ctx()
                .attack_damage_for(&self.world.bots[&id].data, self.tuning.attack_damage);
                if let Some(st) = self.world.structures.get_mut(&target) {
                    if pos.chebyshev(st.pos) > 1 {
                        self.finish_action(id, Err("attack: target out of range".into()));
                        return;
                    }
                    st.hp = (st.hp - damage).max(0);
                    let felled = st.hp == 0;
                    if felled {
                        self.world.structures.remove(&target);
                    }
                    // Combat income: 1 XP per 10 damage = 1 deci per point.
                    self.world.pending_xp.push((id, XpTrack::Combat, damage.max(0) as u64));
                    self.finish_action(id, Ok(Value::Unit));
                    return;
                }
                // Blight Cores are attackable like structures (M8-C):
                // direct damage, no signals. Killing one stops the spread;
                // the creep it made stays until cleansed.
                if let Some(core) = self.world.blight_cores.get_mut(&target) {
                    if pos.chebyshev(core.pos) > 1 {
                        self.finish_action(id, Err("attack: target out of range".into()));
                        return;
                    }
                    core.hp = (core.hp - damage).max(0);
                    if core.hp == 0 {
                        self.world.blight_cores.remove(&target);
                    }
                    self.world.pending_xp.push((id, XpTrack::Combat, damage.max(0) as u64));
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
                // Combat income: 1 XP per 10 damage = 1 deci per point;
                // the +25/kill lands in settle_damage via the attacker tag.
                self.world.pending_xp.push((id, XpTrack::Combat, damage.max(0) as u64));
                self.finish_action(id, Ok(Value::Unit));
                let attacker_faction = self.world.bots[&id].data.faction;
                self.queue_damage(target_bot, damage, Some((id, attacker_faction)));
            }
            Action::Build { blueprint } => {
                let pos = bot.data.pos;
                // Build rate through the pipeline (Building +10%/level),
                // in deci-progress per tick (Q56 fine-grained units).
                let rate = crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }
                .build_rate_for(&bot.data);
                let Some(bp) = self.world.blueprints.get_mut(&blueprint) else {
                    // Someone else finished it: that's success.
                    self.finish_action(id, Ok(Value::Unit));
                    return;
                };
                if pos.chebyshev(bp.pos) > 1 {
                    self.finish_action(id, Err("build: blueprint out of range".into()));
                    return;
                }
                bp.progress += rate;
                let done = bp.progress >= bp.needed;
                let (site, kind) = (bp.pos, bp.kind);
                // Building income: 1 XP per 10 progress units = deci/10.
                self.world.pending_xp.push((id, XpTrack::Building, (rate / 10).max(1) as u64));
                if done {
                    self.world.blueprints.remove(&blueprint);
                    // The ground may have changed under a slow build
                    // (corruption spread, another crew's works): EVERY
                    // kind re-checks its site rule at completion, and
                    // void work stamps nothing — a 10-tick Road build
                    // must not erase creep 4× faster than the 40-tick
                    // Cleanse (review 2026-07-16). The labor still ends
                    // Ok: the crew finished the job it was given.
                    if kind.site_ok(self.world.grid.get(site)) {
                        match kind {
                            BlueprintKind::Bridge => {
                                self.world.set_tile(site, TileKind::Bridge)
                            }
                            // Cleared rubble yields Stone to the BUILDER's
                            // faction (the blueprint records none) —
                            // salvage pays whoever swings the shovel.
                            BlueprintKind::Clear => {
                                self.world.set_tile(site, TileKind::Plains);
                                let faction = self.world.bots[&id].data.faction;
                                self.world.stock_add(
                                    faction,
                                    crate::resources::Resource::Stone,
                                    self.tuning.clear_yield_stone,
                                );
                            }
                            // A bot standing on the site is NOT trapped:
                            // only entering a Barricade is blocked,
                            // leaving isn't.
                            BlueprintKind::Barricade => {
                                self.world.set_tile(site, TileKind::Barricade)
                            }
                            // Un-build to what the works stand on: planks
                            // over water, a wall over plains.
                            BlueprintKind::Demolish => {
                                match self.world.grid.get(site) {
                                    Some(TileKind::Bridge) => {
                                        self.world.set_tile(site, TileKind::Water)
                                    }
                                    Some(TileKind::Barricade) => {
                                        self.world.set_tile(site, TileKind::Plains)
                                    }
                                    _ => unreachable!("site_ok pinned the kind"),
                                }
                            }
                            // Cleanse yields PLAINS — the pre-creep kind
                            // is not preserved (flagged in TASKS.md).
                            BlueprintKind::Cleanse => {
                                self.world.set_tile(site, TileKind::Plains)
                            }
                            BlueprintKind::Road => {
                                self.world.set_tile(site, TileKind::Road)
                            }
                        }
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
            Action::Search { reach, current, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Search { reach, current, ticks_left });
                    return;
                }
                // One ring further out: full sight at range — new nodes
                // become permanent map knowledge and pay Scouting XP.
                let current = current + 1;
                let (pos, faction, elevated) = (
                    bot.data.pos,
                    bot.data.faction,
                    self.world.grid.get(bot.data.pos) == Some(TileKind::HighGround),
                );
                let discovered: Vec<(crate::world::EntityId, crate::world::KnownNode)> = self
                    .world
                    .nodes
                    .iter()
                    .filter(|(nid, n)| {
                        pos.chebyshev(n.pos) <= current
                            && crate::perception::los_clear(&self.world.grid, pos, n.pos, elevated)
                            && !self
                                .world
                                .known_nodes
                                .get(&faction)
                                .is_some_and(|k| k.contains_key(nid))
                    })
                    .map(|(nid, n)| {
                        (
                            *nid,
                            crate::world::KnownNode {
                                kind: n.kind,
                                pos: n.pos,
                                exhausted: n.amount == 0,
                            },
                        )
                    })
                    .collect();
                for (nid, known) in discovered {
                    self.world.known_nodes.entry(faction).or_default().insert(nid, known);
                    self.world.pending_xp.push((
                        id,
                        XpTrack::Scouting,
                        self.xp.scouting_node_xp * 10,
                    ));
                }
                if current >= reach {
                    // Full reach: the survey resolves (docs/01).
                    self.world.pending_xp.push((
                        id,
                        XpTrack::Scouting,
                        self.xp.scouting_survey_xp * 10,
                    ));
                    self.finish_action(id, Ok(Value::Unit));
                } else {
                    let interval = self.tuning.search_ring_ticks.max(1);
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.action =
                        Some(Action::Search { reach, current, ticks_left: interval });
                }
            }
            Action::Deposit { depot, ticks_left, fault_on_fail } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action =
                        Some(Action::Deposit { depot, ticks_left, fault_on_fail });
                    return;
                }
                let faction = bot.data.faction;
                let mut total = 0u32;
                if self.world.structures.contains_key(&depot) {
                    // Structure feed: only what it feeds on moves (recipe
                    // inputs, Generator fuel, Station coolant). No
                    // delivery-milestone credit — feeding a station is
                    // production logistics, not delivery (and counting it
                    // would double-pay a mine→smelt→deliver chain).
                    let inputs = self.world.structures[&depot].accepted_feed();
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
                    // The feed may have carried stock-withdrawn cargo out:
                    // provenance never exceeds what's still aboard.
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.withdrawn_aboard =
                        bot.data.withdrawn_aboard.min(bot.data.cargo_total());
                } else if self.world.depots.contains_key(&depot) {
                    // Depot: the whole manifest enters colony stock
                    // (docs/03: payments draw from this abstract pool).
                    // Milestone credit excludes the stock-withdrawn share
                    // (cargo provenance): recycling stock is zero NET
                    // delivery, but it can never suppress real income.
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    let manifest = std::mem::take(&mut bot.data.cargo);
                    total = manifest.values().sum();
                    let recycled = bot.data.withdrawn_aboard.min(total);
                    bot.data.withdrawn_aboard = 0;
                    for (kind, deci) in manifest {
                        self.world.stock_add(faction, kind, deci as u64);
                    }
                    self.track_delivery_milestone(faction, total - recycled);
                } else {
                    // The acceptor picked at request time was destroyed
                    // before the action completed — the cargo goes nowhere
                    // (nothing may teleport into abstract stock without a
                    // live depot adjacent).
                    let outcome = if fault_on_fail {
                        Err("deposit: acceptor destroyed".into())
                    } else {
                        Ok(Value::Bool(false))
                    };
                    self.finish_action(id, outcome);
                    return;
                }
                // Hauling income is CARGO-DISTANCE DELIVERED (docs/02):
                // the accumulator filled per loaded tile pays out here.
                if total > 0 {
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    let earned = std::mem::take(&mut bot.data.haul_accum);
                    if earned > 0 {
                        self.world.pending_xp.push((id, XpTrack::Hauling, earned));
                    }
                }
                self.finish_action(id, Ok(Value::Unit));
            }
        }
    }

    /// Is this tile inside any friendly perceiver's SEEING circle right
    /// now? (Tile-level visibility for explore()'s fogged-tile pick and
    /// the fog renderer; entities go through `world.perception`.)
    pub fn tile_visible(&self, faction: u8, tile: TilePos) -> bool {
        let ctx = self.ctx();
        for bot in self.world.bots.values() {
            if bot.data.faction != faction || bot.data.dying {
                continue;
            }
            let seeing = ctx.sensors_for(&bot.data);
            if bot.data.pos.chebyshev(tile) <= seeing
                && crate::perception::los_clear(
                    &self.world.grid,
                    bot.data.pos,
                    tile,
                    self.world.grid.get(bot.data.pos)
                        == Some(crate::map::TileKind::HighGround),
                )
            {
                return true;
            }
        }
        let s = self.tuning.structure_sensors;
        let sees = |pos: TilePos| {
            pos.chebyshev(tile) <= s
                && crate::perception::los_clear(&self.world.grid, pos, tile, false)
        };
        self.world.printers.values().any(|p| p.faction == faction && sees(p.pos))
            || self.world.structures.values().any(|st| st.faction == faction && sees(st.pos))
            || (faction == 0 && self.world.depots.values().any(|d| sees(d.pos)))
    }

    /// Start a walk to a specific tile (the stances' mover): A* around
    /// structures; `survey` chains the scouting stance onto arrival.
    fn walk_to_tile(&mut self, id: BotId, tile: TilePos, survey: bool) {
        let mut goals = BTreeSet::new();
        goals.insert(tile);
        if survey {
            self.world.bots.get_mut(&id).expect("bot exists").data.survey_after_move = true;
        }
        self.replan_move(id, goals);
    }

    /// Enter the scouting stance (M7, docs/05): root in place; the seeing
    /// circle expands one ring per interval out to the HEARING radius.
    pub(crate) fn start_search(&mut self, id: BotId) {
        let ctx = self.ctx();
        let data = &self.world.bots[&id].data;
        let seeing = ctx.sensors_for(data);
        let reach = seeing * self.tuning.sense_factor_pct / 100;
        if seeing >= reach {
            // Nothing to expand into: the survey completes immediately.
            self.world.pending_xp.push((id, XpTrack::Scouting, self.xp.scouting_survey_xp * 10));
            self.finish_action(id, Ok(Value::Unit));
            return;
        }
        let interval = self.tuning.search_ring_ticks;
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.action =
            Some(Action::Search { reach, current: seeing, ticks_left: interval.max(1) });
    }

    /// A move just reached its goal: explore() chains into the survey,
    /// everyone else simply resolves.
    pub(crate) fn complete_move(&mut self, id: BotId) {
        let chained = {
            let bot = self.world.bots.get_mut(&id).expect("bot exists");
            std::mem::take(&mut bot.data.survey_after_move)
        };
        if chained {
            self.start_search(id);
        } else {
            self.finish_action(id, Ok(Value::Unit));
        }
    }

    /// Per-tile travel income (M6): Mileage for every tile actually
    /// walked, Hauling accrual while loaded (1 XP per unit per 10 tiles
    /// = one deci-XP per unit-tile) — paid out at delivery.
    pub(crate) fn credit_travel(&mut self, id: BotId) {
        self.world.pending_xp.push((id, XpTrack::Mileage, self.xp.mileage_deci_per_tile));
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        let load_units = (bot.data.cargo_total() / crate::resources::DECI) as u64;
        if load_units > 0 {
            bot.data.haul_accum += load_units;
        }
    }

    /// Resume a bot's VM with an action result (fault path may run handlers
    /// or force a crash dump — hence the host).
    pub(crate) fn finish_action(&mut self, id: BotId, result: Result<Value, String>) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.action = None;
        // A finished (or failed) action never leaves a pending survey
        // chain behind — complete_move consumed it already if it applied.
        bot.data.survey_after_move = false;
        let mut vm = bot.vm.take().expect("vm present between phases");
        {
            let mut host = BotHost { world: &mut self.world, bot: id, tuning: &self.tuning, ctx: crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning } };
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
            // Spawnable, not merely passable: spills don't fly up cliffs.
            .filter(|&p| self.world.grid.get(p).is_some_and(|t| t.spawnable()))
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
    /// The caller already excluded stock-recycled cargo (per-bot
    /// provenance), so `delivered` is NET by construction; the paid
    /// high-water mark guards re-crossing regardless.
    pub(crate) fn track_delivery_milestone(&mut self, faction: u8, delivered_deci: u32) {
        let step = self.tuning.delivery_milestone_deci;
        if step == 0 || delivered_deci == 0 {
            return;
        }
        let net = {
            let d = self.world.delivered.entry(faction).or_insert(0);
            *d += delivered_deci as u64;
            *d
        };
        let owed = net / step as u64;
        // No entry until a milestone actually pays — the paid map is
        // hashed, and a spurious 0 entry would move every replay hash.
        let paid = self.world.milestones_paid.get(&faction).copied().unwrap_or(0);
        if owed > paid {
            self.world.milestones_paid.insert(faction, owed);
            *self.world.data.entry(faction).or_insert(0) +=
                (owed - paid) * self.tuning.milestone_data;
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
            // Terraforming can harden ground under the walk (M8): a
            // blocked edge is not a collision — hold a tick, re-plan.
            let from = self.world.bots[&id].data.pos;
            if !edge_allowed(&self.world.grid, &self.world.overlays, from, entered) {
                recall.ticks_left = 1;
                self.world.bots.get_mut(&id).expect("bot exists").data.recall = Some(recall);
                self.replan_after_bump(id);
                return;
            }
            // Same solidity rule as the program walk: a structure placed
            // mid-route blocks the step (the post-bump replan threads
            // around it).
            if self.world.tile_occupied(entered, id) || self.world.structure_at(entered) {
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                recall.ticks_left = 1;
                bot.data.recall = Some(recall);
                self.bump_both(id, entered, false);
                return;
            }
            recall.path.remove(0);
            self.world.move_bot(id, entered);
            self.credit_travel(id); // Mileage counts engine walks too
            // Ice slides carry engine walks too (M8, Q37) — but the
            // engine's own driving raises no bump on the mover. The slide
            // preempts the plan; the arrival guard below replans when the
            // ice lets go somewhere off the doorstep.
            if let Some(target) = self.slide_target(entered, from) {
                if self.world.tile_occupied(target, id) {
                    self.bump_both(id, target, false);
                } else {
                    recall.ticks_left = crate::stats::step_ticks(
                        self.ctx(),
                        &self.world.grid,
                        &self.world.bots[&id].data,
                        target,
                    )
                    .expect("slide targets are passable");
                    recall.path = vec![target];
                    self.world.bots.get_mut(&id).expect("bot exists").data.recall =
                        Some(recall);
                    return;
                }
            }
            if let Some(next) = recall.path.first() {
                // A hardened next tile costs None: hold one tick — the
                // entry re-check above replans on the next advance.
                recall.ticks_left = crate::stats::step_ticks(
                    self.ctx(),
                    &self.world.grid,
                    &self.world.bots[&id].data,
                    *next,
                )
                .unwrap_or(1);
            }
            // Stand on the arrival tile for one tick even when the path is
            // done — the walk's last step must be observable (the printer
            // starts its work next tick, not mid-stride).
            self.world.bots.get_mut(&id).expect("bot exists").data.recall = Some(recall);
            return;
        }
        // Arrived at the home printer — unless an ice slide carried the
        // walk past the doorstep (M8): scrap/recolor must happen AT home,
        // so an off-doorstep arrival replans instead of settling here.
        let pos = self.world.bots[&id].data.pos;
        let at_doorstep = self
            .world
            .printers
            .get(&recall.home)
            .is_none_or(|p| (p.pos.x - pos.x).abs() + (p.pos.y - pos.y).abs() == 1);
        if !at_doorstep {
            self.world.bots.get_mut(&id).expect("bot exists").data.recall = Some(recall);
            self.replan_after_bump(id);
            return;
        }
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
