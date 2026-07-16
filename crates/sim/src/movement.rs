//! Collision handling and path replanning: bumps, sidesteps, and the
//! post-bump replan.

use crate::map::{astar_avoiding, edge_allowed, OverlayKind, TileKind, TilePos};
use crate::sim::Sim;
use crate::world::{
    Action, BotId,
};
use pyrite::Signal;
use std::collections::BTreeSet;

impl Sim {
    /// A collision, routed through the signal system (docs/01):
    /// the rammer gets `bump`, the victim `bumped` — signal first (impact),
    /// then chassis damage (crunch). A handler is the bot's own response;
    /// the engine's asymmetric freeze is only the UNHANDLED default. Both
    /// the signals and the damage are QUEUED here and land in phase 6 —
    /// co-arrivals resolve by severity there, and the double-handle rule
    /// still applies at dispatch: colliding with (or as) a bot that's
    /// mid-handler/boot/recall from an earlier boundary explodes it.
    /// `raise_on_mover` is false for engine-driven walks (recall): the
    /// engine's own driving is not the program's exception.
    pub(crate) fn bump_both(&mut self, mover: BotId, tile: TilePos, raise_on_mover: bool) {
        // Lowest blocking occupant of the rammed tile, off the spatial
        // index (which holds only live, non-dying bots).
        let blocker =
            self.world.occupancy.get(&tile).into_iter().flatten().copied().find(|b| *b != mover);

        if raise_on_mover {
            // The rammer's own driving: never a hostile source (docs/02 —
            // a two-bot mosh pit in your base earns no Flinch XP).
            self.world.pending_signals.push((mover, Signal::Bump, None));
        } else if let Some(bot) = self.world.bots.get_mut(&mover) {
            // Engine walks raise nothing on the mover — but the long
            // at-fault stun still applies (never downgrades).
            bot.data.bump_frozen = bot.data.bump_frozen.max(self.tuning.bump_freeze_ticks);
        }

        if let Some(blocker) = blocker {
            // The victim was rammed: the mover's faction is the source —
            // enemy rams train the Flinch track, friendly ones don't.
            let mover_faction = self.world.bots.get(&mover).map(|b| b.data.faction);
            self.world.pending_signals.push((blocker, Signal::Bumped, mover_faction));
        }

        let damage = self.tuning.bump_damage;
        self.queue_damage(mover, damage, None);
        if let Some(blocker) = blocker {
            self.queue_damage(blocker, damage, None);
        }
    }

    /// Ice slides (M8, Q37): where momentum carries a bot that just
    /// stepped onto `entered` (moving from `from`) — an arrow painted on
    /// the ice redirects the slide, and a blocked or impassable edge lets
    /// the bot stop (`None`). Occupancy is the CALLER's problem: a slide
    /// into an occupant is a collision, and whether the mover eats a
    /// `bump` depends on who is driving (programs do, engine walks don't).
    pub(crate) fn slide_target(&self, entered: TilePos, from: TilePos) -> Option<TilePos> {
        if self.world.grid.get(entered) != Some(TileKind::Ice) {
            return None;
        }
        let delta = match self.world.overlays.get(&entered) {
            Some(OverlayKind::Arrow(d)) => d.delta(),
            None => (entered.x - from.x, entered.y - from.y),
        };
        if delta == (0, 0) {
            return None; // spawned/placed on ice: no momentum
        }
        let target = TilePos::new(entered.x + delta.0, entered.y + delta.1);
        if !edge_allowed(&self.world.grid, &self.world.overlays, entered, target)
            || self.world.structure_at(target)
        {
            return None;
        }
        Some(target)
    }

    /// Free, passable neighbor tiles of `from` (excluding the blocked
    /// `avoid` tile) that are no farther from `goals` than `from` is —
    /// dodges may not lose ground, so corridors still queue and freeze.
    pub(crate) fn sidestep_candidates(
        &self,
        id: BotId,
        from: TilePos,
        avoid: TilePos,
        goals: &BTreeSet<TilePos>,
    ) -> Vec<TilePos> {
        let dist = |p: TilePos| goals.iter().map(|g| p.manhattan(*g)).min().unwrap_or(u32::MAX);
        let here = dist(from);
        [(0, -1), (1, 0), (0, 1), (-1, 0)]
            .iter()
            .map(|(dx, dy)| TilePos::new(from.x + dx, from.y + dy))
            .filter(|&p| {
                p != avoid
                    && edge_allowed(&self.world.grid, &self.world.overlays, from, p)
                    && !self.world.structure_at(p)
                    && !self.world.tile_occupied(p, id)
                    && dist(p) <= here
            })
            .collect()
    }

    /// Fresh route to `goals`: prefer threading around current bot
    /// positions, fall back to terrain-only, fault if truly unreachable.
    pub(crate) fn replan_move(&mut self, id: BotId, goals: BTreeSet<TilePos>) {
        let Some(bot) = self.world.bots.get(&id) else { return };
        let start = bot.data.pos;
        let structures = self.world.structure_tiles();
        let mut occupied: BTreeSet<TilePos> = self.world.occupied_tiles(id);
        occupied.extend(structures.iter().copied());
        let path = astar_avoiding(&self.world.grid, &self.world.overlays, &self.tuning.tile_costs, start, &goals, &occupied)
            .or_else(|| {
                astar_avoiding(&self.world.grid, &self.world.overlays, &self.tuning.tile_costs, start, &goals, &structures)
            });
        match path {
            Some(path) if path.is_empty() => self.complete_move(id),
            Some(path) => {
                let first_cost = crate::stats::step_ticks(
                    crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning },
                    &self.world.grid,
                    &self.world.bots[&id].data,
                    path[0],
                )
                .expect("path tiles are passable");
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                bot.data.action = Some(Action::Move { path, ticks_left: first_cost, goals });
            }
            None => self.finish_action(id, Err("move_to: unreachable".into())),
        }
    }

    /// As a bump-freeze ends: re-run A* to the same goals, treating other
    /// bots' current tiles as obstacles. Falls back to the old path when no
    /// clear route exists (true corridors keep jamming, visibly).
    pub(crate) fn replan_after_bump(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get(&id) else { return };
        let start = bot.data.pos;
        let mut occupied: BTreeSet<TilePos> = self.world.occupied_tiles(id);
        occupied.extend(self.world.structure_tiles());

        // Program move.
        if let Some(Action::Move { goals, .. }) = &bot.data.action {
            let goals = goals.clone();
            match astar_avoiding(&self.world.grid, &self.world.overlays, &self.tuning.tile_costs, start, &goals, &occupied) {
                Some(path) if path.is_empty() => {
                    // Already standing at a goal: the move is done.
                    self.complete_move(id);
                }
                Some(path) => {
                    let first_cost = crate::stats::step_ticks(
                        crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning },
                        &self.world.grid,
                        &self.world.bots[&id].data,
                        path[0],
                    )
                    .expect("path tiles are passable");
                    let bot = self.world.bots.get_mut(&id).expect("bot exists");
                    bot.data.action =
                        Some(Action::Move { path, ticks_left: first_cost, goals });
                }
                None => {} // no clear route: keep the old path, retry
            }
            return;
        }

        // Engine-driven recall walk.
        let Some(bot) = self.world.bots.get(&id) else { return };
        if let Some(recall) = &bot.data.recall {
            let home = recall.home;
            let Some(home_pos) = self.world.printers.get(&home).map(|p| p.pos) else { return };
            // Goals: the passable, non-structure tiles ORTHOGONALLY beside
            // home — same arrival rule as begin_recall_walk.
            let mut goals = BTreeSet::new();
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let g = TilePos::new(home_pos.x + dx, home_pos.y + dy);
                if self.world.grid.get(g).is_some_and(|t| t.move_ticks().is_some())
                    && !self.world.structure_at(g)
                {
                    goals.insert(g);
                }
            }
            if let Some(path) =
                astar_avoiding(&self.world.grid, &self.world.overlays, &self.tuning.tile_costs, start, &goals, &occupied)
            {
                let ticks_left = path
                    .first()
                    .map(|p| {
                        crate::stats::step_ticks(
                            crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning },
                            &self.world.grid,
                            &self.world.bots[&id].data,
                            *p,
                        )
                        .unwrap_or(1)
                    })
                    .unwrap_or(0);
                let bot = self.world.bots.get_mut(&id).expect("bot exists");
                if let Some(recall) = bot.data.recall.as_mut() {
                    recall.path = path;
                    recall.ticks_left = ticks_left;
                }
            }
        }
    }
}
