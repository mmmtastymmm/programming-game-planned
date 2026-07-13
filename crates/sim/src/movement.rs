//! Collision handling and path replanning: bumps, sidesteps, and the
//! post-bump replan.

use crate::map::{astar, astar_avoiding, edge_allowed, TilePos};
use crate::sim::Sim;
use crate::world::{
    Action, BotId,
};
use pyrite::{RaiseOutcome, Signal, Value};
use std::collections::BTreeSet;

impl Sim {
    /// A collision, routed through the signal system (docs/01):
    /// the rammer gets `bump`, the victim `bumped` — signal first (impact),
    /// then chassis damage (crunch). A handler is the bot's own response;
    /// the engine's asymmetric freeze is only the UNHANDLED default. The
    /// double-handle rule applies as for any signal: colliding with (or as)
    /// a bot that's mid-handler/boot/recall explodes it.
    /// `raise_on_mover` is false for engine-driven walks (recall): the
    /// engine's own driving is not the program's exception.
    pub(crate) fn bump_both(&mut self, mover: BotId, tile: TilePos, raise_on_mover: bool) {
        let blocker = self
            .world
            .bots
            .values()
            .filter(|b| b.data.id != mover && !b.data.dying && b.data.pos == tile)
            .map(|b| b.data.id)
            .min();

        let mover_outcome = if raise_on_mover {
            self.raise_signal(mover, Signal::Bump)
        } else {
            RaiseOutcome::Ignored
        };
        if mover_outcome == RaiseOutcome::Ignored
            && let Some(bot) = self.world.bots.get_mut(&mover)
        {
            // Unhandled default: the long at-fault stun (never downgrades).
            bot.data.bump_frozen = bot.data.bump_frozen.max(self.tuning.bump_freeze_ticks);
        }

        if let Some(blocker) = blocker {
            let blocker_outcome = self.raise_signal(blocker, Signal::Bumped);
            if blocker_outcome == RaiseOutcome::Ignored
                && let Some(bot) = self.world.bots.get_mut(&blocker)
            {
                bot.data.bump_frozen =
                    bot.data.bump_frozen.max(self.tuning.bump_victim_freeze_ticks);
            }
        }

        let damage = self.tuning.bump_damage;
        self.apply_damage(mover, damage);
        if let Some(blocker) = blocker {
            self.apply_damage(blocker, damage);
        }
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
        let occupied: BTreeSet<TilePos> = self
            .world
            .bots
            .values()
            .filter(|b| b.data.id != id && !b.data.dying)
            .map(|b| b.data.pos)
            .collect();
        let path = astar_avoiding(&self.world.grid, &self.world.overlays, start, &goals, &occupied)
            .or_else(|| astar(&self.world.grid, &self.world.overlays, start, &goals));
        match path {
            Some(path) if path.is_empty() => self.finish_action(id, Ok(Value::Unit)),
            Some(path) => {
                let first_cost = self
                    .world
                    .grid
                    .get(path[0])
                    .and_then(|t| t.move_ticks())
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
        let occupied: BTreeSet<TilePos> = self
            .world
            .bots
            .values()
            .filter(|b| b.data.id != id && !b.data.dying)
            .map(|b| b.data.pos)
            .collect();

        // Program move.
        if let Some(Action::Move { goals, .. }) = &bot.data.action {
            let goals = goals.clone();
            match astar_avoiding(&self.world.grid, &self.world.overlays, start, &goals, &occupied) {
                Some(path) if path.is_empty() => {
                    // Already standing at a goal: the move is done.
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
                None => {} // no clear route: keep the old path, retry
            }
            return;
        }

        // Engine-driven recall walk.
        let Some(bot) = self.world.bots.get(&id) else { return };
        if let Some(recall) = &bot.data.recall {
            let home = recall.home;
            let Some(home_pos) = self.world.printers.get(&home).map(|p| p.pos) else { return };
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
            if let Some(path) =
                astar_avoiding(&self.world.grid, &self.world.overlays, start, &goals, &occupied)
            {
                let ticks_left = path
                    .first()
                    .map(|p| self.world.grid.get(*p).and_then(|t| t.move_ticks()).unwrap_or(1))
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
