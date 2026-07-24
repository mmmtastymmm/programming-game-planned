//! Action resolution: starting, advancing, and finishing the blocking
//! world actions bots issue (move, mine, build, ...).

use crate::map::{astar_avoiding, edge_allowed, TileKind, TilePos};
use crate::sim::Sim;
use crate::world::{
    Action, ActionRequest, BlueprintKind, BotId, RecallPurpose, XpTrack,
};
use pyrite::Value;
use std::collections::BTreeSet;

/// One tick of mending: raise `hp` toward `max_hp` by the per-tick heal
/// (deci-rate ÷ 10, floored at 1), clamped so it never overshoots.
/// Returns (hp restored this tick, whether the target is now full) — the
/// shared arithmetic behind repairing a structure or a friendly bot.
fn mend(hp: &mut i64, max_hp: i64, rate: u32) -> (i64, bool) {
    let mended = (rate as i64 / 10).max(1).min(max_hp - *hp);
    *hp += mended;
    (mended, *hp == max_hp)
}

impl Sim {
    /// Phase 4 for one bot: start its requested action or advance the
    /// in-flight one; on completion, resume the VM with the result.
    pub(crate) fn resolve_bot(&mut self, id: BotId) {
        let tick = self.world.tick;
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
                            let bot = self.world.bot_mut(id);
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
                            let bot = self.world.bot_mut(id);
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
                    let bot = self.world.bot_mut(id);
                    bot.data.action = Some(Action::Attack { target, ticks_left: 1 });
                }
                ActionRequest::Wait(ticks) => {
                    bot.data.action = Some(Action::Wait { ticks_left: ticks });
                }
                ActionRequest::Build(blueprint) => {
                    bot.data.action = Some(Action::Build { blueprint });
                }
                // --- the wreck race + guard duty (M10) ---
                ActionRequest::Repair(target) => {
                    let Some(tpos) = self.world.entity_pos(target) else {
                        self.finish_action(id, Err("repair: no such target".into()));
                        return;
                    };
                    if pos.chebyshev(tpos) > 1 {
                        self.finish_action(id, Err("repair: target out of range".into()));
                        return;
                    }
                    let bot = self.world.bot_mut(id);
                    bot.data.action = Some(Action::Repair { target, done_deci: 0 });
                }
                ActionRequest::Salvage(target)
                | ActionRequest::Analyze(target)
                | ActionRequest::Hijack(target) => {
                    let Some(wreck) = self.world.wreck_of(target) else {
                        self.finish_action(id, Err("wreck race: target is not a wreck".into()));
                        return;
                    };
                    let wpos = self.world.wrecks[&wreck].data.pos;
                    if pos.chebyshev(wpos) > 1 {
                        self.finish_action(id, Err("wreck race: out of range".into()));
                        return;
                    }
                    let (kind, ticks) = match request {
                        ActionRequest::Salvage(_) => {
                            (crate::world::RaceKind::Salvage, self.tuning.salvage_ticks)
                        }
                        ActionRequest::Analyze(_) => {
                            // Your own wrecks yield you nothing (Q76).
                            if self.world.wrecks[&wreck].data.faction
                                == self.world.bots[&id].data.faction
                            {
                                self.finish_action(
                                    id,
                                    Err("analyze: your own wrecks yield nothing".into()),
                                );
                                return;
                            }
                            (crate::world::RaceKind::Analyze, self.tuning.analyze_ticks)
                        }
                        _ => (crate::world::RaceKind::Hijack, self.tuning.hijack_ticks),
                    };
                    let bot = self.world.bot_mut(id);
                    bot.data.action =
                        Some(Action::Race { wreck, kind, ticks_left: ticks.max(1) });
                }
                ActionRequest::Recover(target) => {
                    let is_box = self.world.black_boxes.iter().any(|bb| bb.entity == target);
                    let Some(tpos) = self.world.entity_pos(target).filter(|_| is_box) else {
                        self.finish_action(id, Err("recover: no black box there".into()));
                        return;
                    };
                    if pos.chebyshev(tpos) > 1 {
                        self.finish_action(id, Err("recover: out of range".into()));
                        return;
                    }
                    let bot = self.world.bot_mut(id);
                    bot.data.action = Some(Action::Recover { target, ticks_left: 1 });
                }
                ActionRequest::Guard { target, escort } => {
                    if self.world.entity_pos(target).is_none() {
                        self.finish_action(id, Err("guard: no such target".into()));
                        return;
                    }
                    let bot = self.world.bot_mut(id);
                    bot.data.action =
                        Some(Action::Guard { target, escort, step_wait: 0, cooldown: 0 });
                }
                ActionRequest::Channel { op, ch, namespace, timeout } => {
                    // The rendezvous block (M11): parked here; the settle
                    // pairs participants. Standing IN Corruption still
                    // blocks — the jam means nobody can reach you, and
                    // your timeout is your way out.
                    bot.data.action = Some(Action::Channel {
                        op,
                        ch,
                        namespace,
                        waited: 0,
                        timeout,
                        delivered: None,
                    });
                }
                ActionRequest::Search => self.start_search(id),
                ActionRequest::Study => self.start_study(id),
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
                    // Feral logistics (M12, docs/04): a Harvester's nest
                    // IS its depot — the whole manifest feeds the nest's
                    // print stock.
                    let nest = (faction == crate::world::FERAL_FACTION)
                        .then(|| {
                            self.world
                                .nests
                                .iter()
                                .filter(|(_, n)| {
                                    n.state == crate::world::NestState::Active
                                        && pos.chebyshev(n.pos) <= 1
                                })
                                .map(|(nid, _)| *nid)
                                .next()
                        })
                        .flatten();
                    let target = nest.or(depot).or(refinery);
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
                    // Advancing the traverse counts as moving for hearing
                    // (docs/05: "moving = advanced its traverse") — on every
                    // in-between tick of a multi-tick crossing, not only when
                    // the tile changes, so a mover on Rubble/Ford stays
                    // audible the whole way.
                    bot.data.moved_tick = tick;
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
                        let bot = self.world.bot_mut(id);
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
                        let bot = self.world.bot_mut(id);
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
                        let bot = self.world.bot_mut(id);
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
                            let bot = self.world.bot_mut(id);
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
                let bot = self.world.bot_mut(id);
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
                // Non-PvP servers (M13, docs/08): direct harm to another
                // PLAYER's property is refused — your own things and the
                // Ferals are always fair game (blasts stay indiscriminate;
                // they never route through attack()).
                let attacker = self.world.bots[&id].data.faction;
                let victim = self
                    .world
                    .structures
                    .get(&target)
                    .map(|s| s.faction)
                    .or_else(|| {
                        self.world.wreck_of(target).map(|w| self.world.wrecks[&w].data.faction)
                    })
                    .or_else(|| {
                        // A claimed nest is the claimant's property; Active
                        // and Defeated sites belong to the Ferals (always
                        // fair game).
                        self.world.nests.get(&target).map(|n| n.owner())
                    })
                    .or_else(|| {
                        self.world
                            .bot_entities
                            .get(&target)
                            .and_then(|b| self.world.bots.get(b))
                            .map(|b| b.data.faction)
                    });
                if let Some(v) = victim
                    && !self.world.harm_allowed(attacker, v)
                {
                    self.finish_action(id, Err("attack: harm disabled on this server".into()));
                    return;
                }
                if let Some(st) = self.world.structures.get_mut(&target) {
                    if pos.chebyshev(st.pos) > 1 {
                        self.finish_action(id, Err("attack: target out of range".into()));
                        return;
                    }
                    // Combat XP pays for damage DEALT, clamped to the hp
                    // actually removed (review 2026-07-17: an over-kill
                    // swing must not over-credit — matches the nest rule).
                    let dealt = damage.max(0).min(st.hp);
                    st.hp -= dealt;
                    let felled = st.hp == 0;
                    if felled {
                        self.world.structures.remove(&target);
                    }
                    self.world.pending_xp.push((id, XpTrack::Combat, dealt as u64));
                    self.finish_action(id, Ok(Value::Unit));
                    return;
                }
                // Nests are attackable masses (M12): hitting 0 doesn't
                // remove the site — it goes DEFEATED, awaiting the raze-
                // or-claim choice (docs/04).
                if self.world.nests.contains_key(&target) {
                    let npos = self.world.nests[&target].pos;
                    if pos.chebyshev(npos) > 1 {
                        self.finish_action(id, Err("attack: target out of range".into()));
                        return;
                    }
                    let nest = self.world.nests.get_mut(&target).expect("checked");
                    // XP pays for damage DEALT: a Defeated site sits at 0
                    // hp forever, so swinging at it must mint nothing
                    // (review 2026-07-16: the unconditional grant was an
                    // unbounded Combat XP farm).
                    let dealt = damage.max(0).min(nest.hp);
                    nest.hp -= dealt;
                    let mut lost_owner = None;
                    if nest.hp == 0 && nest.state != crate::world::NestState::Defeated {
                        // A claimed nest beaten down is a lost nest (Q87): its
                        // former owner's bound printer must go Dormant, same as
                        // a Feral reclaim — not just Feral reclaims.
                        if let crate::world::NestState::Claimed(owner) = nest.state {
                            lost_owner = Some(owner);
                        }
                        nest.state = crate::world::NestState::Defeated;
                        nest.job = None;
                    }
                    if dealt > 0 {
                        self.world.pending_xp.push((id, XpTrack::Combat, dealt as u64));
                    }
                    if let Some(owner) = lost_owner {
                        self.reconcile_dormancy(owner);
                    }
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
                    // XP pays for HP actually removed, clamped like the
                    // structure/nest/wreck paths — an over-kill swing on a
                    // low-HP Core must not over-credit Combat XP.
                    let dealt = damage.max(0).min(core.hp);
                    core.hp -= dealt;
                    if core.hp == 0 {
                        self.world.blight_cores.remove(&target);
                    }
                    self.world.pending_xp.push((id, XpTrack::Combat, dealt as u64));
                    self.finish_action(id, Ok(Value::Unit));
                    return;
                }
                // Wrecks are attackable hulls (M10): damage falls on the
                // wreck directly; zero destroys it — black box, NO blast
                // (expiry is the only explosion).
                if let Some(wreck) = self.world.wreck_of(target) {
                    let wpos = self.world.wrecks[&wreck].data.pos;
                    if pos.chebyshev(wpos) > 1 {
                        self.finish_action(id, Err("attack: target out of range".into()));
                        return;
                    }
                    let w = self.world.wrecks.get_mut(&wreck).expect("checked");
                    // XP for damage DEALT, clamped to the hull removed.
                    let dealt = damage.max(0).min(w.hp);
                    w.hp -= dealt;
                    let felled = w.hp == 0;
                    if felled {
                        self.destroy_wreck(wreck, "destroyed by attack");
                    }
                    self.world.pending_xp.push((id, XpTrack::Combat, dealt as u64));
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
                // Combat income (per-damage XP AND the +25/kill) settles in
                // settle_damage from the attacker tag, credited on the HP
                // actually removed so overkill/ganks can't over-credit.
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
                    let bot = self.world.bot_mut(id);
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
            Action::Repair { target, done_deci } => {
                let pos = bot.data.pos;
                // Rate through the pipeline; Building L3 repairs +25%.
                let ctx = crate::stats::StatCtx {
                    stats: &self.stats,
                    xp: &self.xp,
                    quirks: &self.quirks,
                    tuning: &self.tuning,
                };
                let mut rate = ctx.build_rate_for(&bot.data);
                if self.xp.level(bot.data.xp(XpTrack::Building)) >= 3 {
                    rate += rate * self.xp.building_l3_repair_pct / 100;
                }
                let Some(tpos) = self.world.entity_pos(target) else {
                    // Rescued/salvaged/destroyed by someone else mid-work.
                    self.finish_action(id, Err("repair: target gone".into()));
                    return;
                };
                if pos.chebyshev(tpos) > 1 {
                    self.finish_action(id, Err("repair: target moved out of range".into()));
                    return;
                }
                // Building XP pays for WORK DONE, per lane below — a
                // repair loop on a full-HP target earns nothing (review
                // 2026-07-16: the unconditional per-tick grant minted
                // free XP forever).
                let was_deci = done_deci;
                let done_deci = done_deci + rate;
                if let Some(wreck) = self.world.wreck_of(target) {
                    // A rescuer standing ON the wreck tile blocks its own
                    // boot forever — fail loudly instead of holding (and
                    // minting XP) for eternity (review 2026-07-16).
                    if pos == tpos {
                        self.finish_action(
                            id,
                            Err("repair: move off the wreck to boot it".into()),
                        );
                        return;
                    }
                    // Field repair — the rescue race's rescue lane. XP
                    // flows only while progress accrues; a rescue HELD at
                    // full progress (blocked tile, full fleet) earns
                    // nothing further.
                    if was_deci < self.tuning.field_repair_deci {
                        self.world
                            .pending_xp
                            .push((id, XpTrack::Building, (rate / 10).max(1) as u64));
                    }
                    if done_deci >= self.tuning.field_repair_deci {
                        if !self.rescue_wreck(wreck) {
                            // Tile blocked: hold at full progress.
                            let bot = self.world.bot_mut(id);
                            bot.data.action = Some(Action::Repair {
                                target,
                                done_deci: self.tuning.field_repair_deci,
                            });
                            return;
                        }
                        self.finish_action(id, Ok(Value::Unit));
                    } else {
                        let bot = self.world.bot_mut(id);
                        bot.data.action = Some(Action::Repair { target, done_deci });
                    }
                } else {
                    // Mending lane — a structure or a friendly bot at the
                    // tile. Both heal identically (only the hp field
                    // differs), so compute the mend, drop the target
                    // borrow, then run the shared XP-and-repark tail once.
                    let healed = if let Some(st) =
                        self.world.structures.values_mut().find(|s| s.pos == tpos)
                    {
                        Some(mend(&mut st.hp, st.max_hp, rate))
                    } else if let Some(other) = self
                        .world
                        .bot_entities
                        .get(&target)
                        .copied()
                        .filter(|b| self.world.bots.contains_key(b))
                    {
                        let b = self.world.bots.get_mut(&other).expect("checked");
                        Some(mend(&mut b.data.hp, b.data.max_hp, rate))
                    } else {
                        None
                    };
                    match healed {
                        Some((mended, full)) => {
                            // XP pays for HP actually restored (a full
                            // target earns nothing — review 2026-07-16).
                            if mended > 0 {
                                self.world.pending_xp.push((
                                    id,
                                    XpTrack::Building,
                                    (rate / 10).max(1) as u64,
                                ));
                            }
                            if full {
                                self.finish_action(id, Ok(Value::Unit));
                            } else {
                                let bot = self.world.bot_mut(id);
                                bot.data.action = Some(Action::Repair { target, done_deci });
                            }
                        }
                        None => {
                            self.finish_action(id, Err("repair: nothing repairable there".into()))
                        }
                    }
                }
            }
            Action::Race { wreck, kind, ticks_left } => {
                let Some(w) = self.world.wrecks.get(&wreck) else {
                    self.finish_action(id, Err("wreck race: lost — the wreck is gone".into()));
                    return;
                };
                if bot.data.pos.chebyshev(w.data.pos) > 1 {
                    self.finish_action(id, Err("wreck race: out of range".into()));
                    return;
                }
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Race { wreck, kind, ticks_left });
                    return;
                }
                let faction = bot.data.faction;
                match kind {
                    crate::world::RaceKind::Salvage => {
                        self.salvage_wreck(wreck, faction);
                        self.finish_action(id, Ok(Value::Unit));
                    }
                    crate::world::RaceKind::Analyze => {
                        self.analyze_wreck(wreck, faction);
                        self.finish_action(id, Ok(Value::Unit));
                    }
                    crate::world::RaceKind::Hijack => {
                        if self.hijack_wreck(wreck, faction) {
                            self.finish_action(id, Ok(Value::Unit));
                        } else {
                            // No working remainder printer / blocked tile:
                            // hold at the threshold and retry.
                            let bot = self.world.bot_mut(id);
                            bot.data.action =
                                Some(Action::Race { wreck, kind, ticks_left: 1 });
                        }
                    }
                }
            }
            Action::Recover { target, ticks_left } => {
                let ticks_left = ticks_left.saturating_sub(1);
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Recover { target, ticks_left });
                    return;
                }
                let faction = bot.data.faction;
                self.recover_black_box(target, faction);
                self.finish_action(id, Ok(Value::Unit));
            }
            Action::Guard { target, escort, step_wait, cooldown } => {
                let pos = bot.data.pos;
                let faction = bot.data.faction;
                let Some(tpos) = self.world.entity_pos(target) else {
                    // The charge is gone: the stance resolves.
                    self.finish_action(id, Ok(Value::Unit));
                    return;
                };
                let mut cooldown = cooldown.saturating_sub(1);
                let mut step_wait = step_wait.saturating_sub(1);
                // Swing at an adjacent perceived enemy (lowest entity id).
                // The same gates as attack() (M13): no harm to other
                // players on Non-PvP servers, never at declared allies,
                // and only at contacts the colony actually perceives.
                if cooldown == 0 {
                    let victim = self
                        .world
                        .bots
                        .iter()
                        .filter(|(_, b)| {
                            b.data.faction != faction
                                && !b.data.dying
                                && b.data.pos.chebyshev(pos) <= 1
                                && self.world.harm_allowed(faction, b.data.faction)
                                && !self.world.allied(faction, b.data.faction)
                                && self.world.perception.get(&faction).is_some_and(|p| {
                                    p.seen.contains(&b.data.entity)
                                })
                        })
                        .map(|(bid, b)| (b.data.entity, *bid))
                        .min();
                    if let Some((_, victim)) = victim {
                        let damage = crate::stats::StatCtx {
                            stats: &self.stats,
                            xp: &self.xp,
                            quirks: &self.quirks,
                            tuning: &self.tuning,
                        }
                        .attack_damage_for(&self.world.bots[&id].data, self.tuning.attack_damage);
                        // Combat XP settles in settle_damage on HP removed
                        // (same as attack()), not the full swing here.
                        self.queue_damage(victim, damage, Some((id, faction)));
                        cooldown = self.tuning.guard_swing_ticks.max(1);
                    }
                }
                // Keep station: escort hugs, guard holds a short leash
                // (both from tuning); step one tile when out of range.
                let leash = if escort { self.tuning.escort_leash } else { self.tuning.guard_leash };
                if pos.chebyshev(tpos) > leash && step_wait == 0 {
                    let structures = self.world.structure_tiles();
                    let mut goals = BTreeSet::new();
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let g = TilePos::new(tpos.x + dx, tpos.y + dy);
                            if self.world.grid.get(g).is_some_and(|t| t.passable())
                                && !structures.contains(&g)
                            {
                                goals.insert(g);
                            }
                        }
                    }
                    if let Some(path) = astar_avoiding(
                        &self.world.grid,
                        &self.world.overlays,
                        &self.tuning.tile_costs,
                        pos,
                        &goals,
                        &structures,
                    ) && let Some(&next) = path.first()
                    {
                        if !self.world.tile_occupied(next, id) && !self.world.structure_at(next) {
                            step_wait = crate::stats::step_ticks(
                                crate::stats::StatCtx {
                                    stats: &self.stats,
                                    xp: &self.xp,
                                    quirks: &self.quirks,
                                    tuning: &self.tuning,
                                },
                                &self.world.grid,
                                &self.world.bots[&id].data,
                                next,
                            )
                            .unwrap_or(1);
                            self.world.move_bot(id, next);
                            self.credit_travel(id);
                        } else {
                            step_wait = 1; // blocked: retry next tick
                        }
                    }
                }
                let bot = self.world.bot_mut(id);
                bot.data.action = Some(Action::Guard { target, escort, step_wait, cooldown });
            }
            Action::Channel { op, ch, namespace, waited, timeout, delivered } => {
                // Rendezvous state is settled by settle_channels; the
                // advance just keeps the block parked.
                bot.data.action =
                    Some(Action::Channel { op, ch, namespace, waited, timeout, delivered });
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
                    // Mountain summits are elevated too (docs/05) — the survey
                    // must see over walls just like passive perception does,
                    // via the same on_high_ground predicate as los_clear.
                    crate::perception::on_high_ground(&self.world.grid, bot.data.pos),
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
                    let bot = self.world.bot_mut(id);
                    bot.data.action =
                        Some(Action::Search { reach, current, ticks_left: interval });
                }
            }
            Action::Study { cache, ticks_left } => {
                let ticks_left = ticks_left - 1;
                if ticks_left > 0 {
                    bot.data.action = Some(Action::Study { cache, ticks_left });
                    return;
                }
                // Study complete (M15, docs/06): the Cache must still be in
                // range (it's non-consumable, so it only vanishes if the map
                // changed under us). Unlock its block colony-wide + pay a
                // little Learning XP (studying IS learning).
                let pos = self.world.bots[&id].data.pos;
                let faction = self.world.bots[&id].data.faction;
                match self.world.caches.get(&cache) {
                    Some(c) if pos.chebyshev(c.pos) <= 1 => {
                        let block = c.block;
                        self.world.studied.entry(faction).or_default().insert(block);
                        self.world.pending_xp.push((
                            id,
                            XpTrack::Learning,
                            self.xp.scouting_survey_xp * 10,
                        ));
                        self.finish_action(id, Ok(Value::Unit));
                    }
                    _ => self.finish_action(id, Err("study: Cache no longer in range".into())),
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
                // A nest beaten to Defeated (or claimed) mid-deposit is a
                // dead acceptor — the manifest must not pre-fund a dormant
                // site's reclaim (review 2026-07-16). State re-checked at
                // settle, like the destroyed-depot arm below.
                if self
                    .world
                    .nests
                    .get(&depot)
                    .is_some_and(|n| n.state == crate::world::NestState::Active)
                {
                    // Nest feed (M12): everything aboard becomes print
                    // stock. No colony-stock or milestone bookkeeping —
                    // the Feral economy is the nest's own.
                    let bot = self.world.bot_mut(id);
                    let manifest = std::mem::take(&mut bot.data.cargo);
                    total = manifest.values().sum();
                    bot.data.withdrawn_aboard = 0;
                    let nest = self.world.nests.get_mut(&depot).expect("checked above");
                    nest.stock_deci += total as u64;
                } else if self.world.structures.contains_key(&depot) {
                    // Structure feed: only what it feeds on moves (recipe
                    // inputs, Generator fuel, Station coolant). No
                    // delivery-milestone credit — feeding a station is
                    // production logistics, not delivery (and counting it
                    // would double-pay a mine→smelt→deliver chain).
                    let inputs = self.world.structures[&depot].accepted_feed();
                    let bot = self.world.bot_mut(id);
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
                    let bot = self.world.bot_mut(id);
                    bot.data.withdrawn_aboard =
                        bot.data.withdrawn_aboard.min(bot.data.cargo_total());
                } else if self.world.depots.contains_key(&depot) {
                    // Depot: the whole manifest enters colony stock
                    // (docs/03: payments draw from this abstract pool).
                    // Milestone credit excludes the stock-withdrawn share
                    // (cargo provenance): recycling stock is zero NET
                    // delivery, but it can never suppress real income.
                    let bot = self.world.bot_mut(id);
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
                    let bot = self.world.bot_mut(id);
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
                    // Mountain summits see over walls too (docs/05), matching
                    // passive perception's elevation rule.
                    crate::perception::on_high_ground(&self.world.grid, bot.data.pos),
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
            || self.world.depots.values().any(|d| d.faction == faction && sees(d.pos))
    }

    /// Start a walk to a specific tile (the stances' mover): A* around
    /// structures; `survey` chains the scouting stance onto arrival.
    fn walk_to_tile(&mut self, id: BotId, tile: TilePos, survey: bool) {
        let mut goals = BTreeSet::new();
        goals.insert(tile);
        if survey {
            self.world.bot_mut(id).data.survey_after_move = true;
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
        let bot = self.world.bot_mut(id);
        bot.data.action =
            Some(Action::Search { reach, current: seeing, ticks_left: interval.max(1) });
    }

    /// Begin studying an adjacent Template Cache (M15, docs/06): root for
    /// `study_ticks`, then unlock its function block colony-wide. Faults if no
    /// Cache is within reach — the study verb has to have a school to sit at.
    pub(crate) fn start_study(&mut self, id: BotId) {
        let pos = self.world.bots[&id].data.pos;
        // Nearest adjacent Cache (chebyshev <= 1), lowest entity id on a tie.
        let cache = self
            .world
            .caches
            .iter()
            .filter(|(_, c)| pos.chebyshev(c.pos) <= 1)
            .map(|(id, _)| *id)
            .min();
        match cache {
            None => self
                .finish_action(id, Err("study: no Template Cache in range".into())),
            Some(cache) => {
                let ticks = self.tuning.study_ticks.max(1);
                self.world.bot_mut(id).data.action =
                    Some(Action::Study { cache, ticks_left: ticks });
            }
        }
    }

    /// A move just reached its goal: explore() chains into the survey,
    /// everyone else simply resolves.
    pub(crate) fn complete_move(&mut self, id: BotId) {
        let chained = {
            let bot = self.world.bot_mut(id);
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
        let bot = self.world.bot_mut(id);
        // Hauling income is DELIVERED production only (docs/02): accrue on the
        // MINED share of the load, never stock withdrawn from the colony and
        // cycled back (`withdrawn_aboard` is that recycled share). Without
        // this, a withdraw → lap → deposit loop farms Hauling XP with zero
        // net production — matching the Data milestone's provenance exclusion.
        let mined = bot.data.cargo_total().saturating_sub(bot.data.withdrawn_aboard);
        let load_units = (mined / crate::resources::DECI) as u64;
        if load_units > 0 {
            bot.data.haul_accum += load_units;
        }
    }

    /// Resume a bot's VM with an action result (fault path may run handlers
    /// or force a crash dump — hence the host).
    /// Like finish_action, but with a TYPED host-domain fault id (M11:
    /// err_timeout — the generic finish path faults err_action).
    pub(crate) fn finish_action_fault(&mut self, id: BotId, fault: pyrite::Fault) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.action = None;
        bot.data.survey_after_move = false;
        let mut vm = bot.vm.take().expect("vm present between phases");
        self.with_host(id, |host, costs| vm.resolve_action_fault(fault, host, costs));
        if let Some(bot) = self.world.bots.get_mut(&id) {
            bot.vm = Some(vm);
        }
    }

    pub(crate) fn finish_action(&mut self, id: BotId, result: Result<Value, String>) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.action = None;
        // A finished (or failed) action never leaves a pending survey
        // chain behind — complete_move consumed it already if it applied.
        bot.data.survey_after_move = false;
        let mut vm = bot.vm.take().expect("vm present between phases");
        self.with_host(id, |host, costs| vm.resolve_action(result, host, costs));
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
                self.world.bot_mut(id).data.recall = Some(recall);
                self.replan_after_bump(id);
                return;
            }
            // Same solidity rule as the program walk: a structure placed
            // mid-route blocks the step (the post-bump replan threads
            // around it).
            if self.world.tile_occupied(entered, id) || self.world.structure_at(entered) {
                let bot = self.world.bot_mut(id);
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
                    self.world.bot_mut(id).data.recall =
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
            self.world.bot_mut(id).data.recall = Some(recall);
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
            self.world.bot_mut(id).data.recall = Some(recall);
            self.replan_after_bump(id);
            return;
        }
        match recall.purpose {
            RecallPurpose::Recolor { dest } => {
                if !self.recolor_bot(id, dest) {
                    // No free tile beside the destination yet: hold here,
                    // recall intact, and retry next tick.
                    let bot = self.world.bot_mut(id);
                    bot.data.recall = Some(recall);
                }
            }
            RecallPurpose::Scrap => self.scrap_bot(id),
        }
    }
}
