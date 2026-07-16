//! Phase 5: the two-circle perception model (M7, docs/05 Q74).
//!
//! One stat, two concentric circles per perceiver: **seeing** (the sensor
//! stat — total information) and **hearing** (× `sense_factor`, movers
//! only — ONLY MOVING THINGS MAKE NOISE). Line of sight blocks both (v1:
//! High Ground blocks unless the perceiver is elevated — it sees over).
//! Signature offsets the heard-at distance; Snow mutes movement entirely;
//! Fords quiet the wader. Vision is the LIVE UNION of every friendly
//! perceiver (docs/05: the colony pools eyes), recomputed here every tick
//! from post-move positions; queries read last tick's compute by design.
//!
//! Distances are CHEBYSHEV (square circles — the grid's native metric for
//! adjacency); a diagonal sightline is as good as a straight one.
//! NEEDS DISCUSSION (TASKS.md): faction-union scoping, the metric, and
//! which tiles block LoS pre-M8 are all first-pass readings.

use crate::map::{Grid, TileKind, TilePos};
use crate::sim::Sim;
use crate::world::{BotId, EntityId, KnownNode, Perception, XpTrack};
use std::collections::{BTreeMap, BTreeSet};

/// Is the straight sightline `from` → `to` clear? Integer supercover walk
/// over intermediate tiles; symmetric by construction (the same set of
/// tiles both ways). High Ground blocks — unless the PERCEIVER stands on
/// High Ground ("height beats walls", docs/05); M8's Mountains and
/// Barricades join the blocker set with their tiles.
pub fn los_clear(grid: &Grid, from: TilePos, to: TilePos, perceiver_elevated: bool) -> bool {
    let blocks = |tile: TilePos| -> bool {
        match grid.get(tile) {
            // Mountains are High Ground's soft-slope sibling (M8):
            // elevation sees over both; ground level sees neither.
            Some(TileKind::HighGround) | Some(TileKind::Mountain) => !perceiver_elevated,
            // A terraformed wall blocks vision for EVERYONE (docs/05) —
            // it is built mass, not elevation.
            Some(TileKind::Barricade) => true,
            _ => false,
        }
    };
    // Supercover: sample the segment at 2× resolution — every tile the
    // line passes through appears; endpoints excluded (you always
    // perceive out of and into open positions). The walk always runs from
    // the lexicographically smaller endpoint: truncating division samples
    // DIFFERENT tiles for negative deltas, so a direction-dependent walk
    // made sight asymmetric (A sees B, B blind to A across a corner).
    // Canonicalizing the endpoints makes the sampled set identical both
    // ways by construction; the elevation exemption rides the blocks()
    // closure, not the walk direction.
    let (a, b) = if (from.x, from.y) <= (to.x, to.y) { (from, to) } else { (to, from) };
    let (dx, dy) = ((b.x - a.x) as i64, (b.y - a.y) as i64);
    let steps = dx.abs().max(dy.abs()) * 2;
    for i in 1..steps {
        let x = a.x as i64 * 2 + (dx * 2 * i) / steps;
        let y = a.y as i64 * 2 + (dy * 2 * i) / steps;
        let tile = TilePos::new((x / 2) as i32, (y / 2) as i32);
        if tile != from && tile != to && blocks(tile) {
            return false;
        }
    }
    true
}

impl Sim {
    /// Phase 5 (and the phase-0 seed): recompute every faction's
    /// perception, update permanent node knowledge, and settle detection
    /// episodes (→ Hiding XP).
    pub(crate) fn run_perception(&mut self) {
        let tick = self.world.tick;
        // Perceivers per faction: (pos, seeing, hearing, elevated).
        let mut perceivers: BTreeMap<u8, Vec<(TilePos, u32, u32, bool)>> = BTreeMap::new();
        let ctx = self.ctx();
        for bot in self.world.bots.values() {
            if bot.data.dying {
                continue;
            }
            let base_seeing = ctx.sensors_for(&bot.data)
                + high_ground_bonus(&self.world.grid, bot.data.pos);
            // Hearing derives from the BASE circle; the search stance then
            // widens actual seeing out to its current ring (docs/05 — the
            // survey ring is real sight, not just node discovery).
            let hearing = base_seeing * self.tuning.sense_factor_pct / 100;
            let seeing = match &bot.data.action {
                Some(crate::world::Action::Search { current, .. }) => base_seeing.max(*current),
                _ => base_seeing,
            };
            perceivers.entry(bot.data.faction).or_default().push((
                bot.data.pos,
                seeing,
                hearing,
                on_high_ground(&self.world.grid, bot.data.pos),
            ));
        }
        // Structures see and hear too (docs/05) — printers, depots, and
        // generic structures share the tuning base until Sentry/Lantern.
        let s = self.tuning.structure_sensors;
        let sh = s * self.tuning.sense_factor_pct / 100;
        for p in self.world.printers.values() {
            perceivers.entry(p.faction).or_default().push((p.pos, s, sh, false));
        }
        for st in self.world.structures.values() {
            perceivers.entry(st.faction).or_default().push((st.pos, s, sh, false));
        }
        // Depots are factionless (pre-M4 simplification) — they see for
        // faction 0 only to avoid granting everyone eyes; flagged with the
        // depot-faction discussion in TASKS.md.
        for d in self.world.depots.values() {
            perceivers.entry(0).or_default().push((d.pos, s, sh, false));
        }

        let factions: BTreeSet<u8> = perceivers.keys().copied().collect();
        let mut new_perception: BTreeMap<u8, Perception> = BTreeMap::new();
        for &faction in &factions {
            let mut seen: BTreeSet<EntityId> = BTreeSet::new();
            let mut heard: BTreeMap<EntityId, TilePos> = BTreeMap::new();
            let eyes = &perceivers[&faction];
            // Bots: seen in the inner circle; heard in the outer while
            // MOVING this tick — unless standing on Snow (mute) — at
            // hearing + signature (Ford-quieted), floored at 1.
            for target in self.world.bots.values() {
                if target.data.dying {
                    continue;
                }
                let tpos = target.data.pos;
                let entity = target.data.entity;
                let own = target.data.faction == faction;
                let moving = target.data.moved_tick == tick && tick > 0;
                let muted = self.world.grid.get(tpos) == Some(TileKind::Snow);
                // Ford wading quiets the mover (M8, Q38): the water
                // swallows the tread noise — subtracted from heard-at.
                let mut signature = ctx.signature_for(&target.data);
                if self.world.grid.get(tpos) == Some(TileKind::Ford) {
                    signature -= self.tuning.ford_quiet;
                }
                for &(pos, seeing, hearing, elevated) in eyes {
                    if own {
                        // Own units are always known to the colony cloud.
                        seen.insert(entity);
                        break;
                    }
                    let d = pos.chebyshev(tpos);
                    if d <= seeing && los_clear(&self.world.grid, pos, tpos, elevated) {
                        seen.insert(entity);
                        break;
                    }
                    if moving && !muted {
                        let heard_at = (hearing as i64 + signature).max(1) as u32;
                        if d <= heard_at && los_clear(&self.world.grid, pos, tpos, elevated) {
                            heard.insert(entity, tpos);
                            // keep scanning: a later eye may SEE it
                        }
                    }
                }
            }
            // Structures, printers, depots, nodes, wrecks: seen-only
            // (stationary things make no noise).
            let mut see_static = |entity: EntityId, tpos: TilePos, own: bool| {
                if own {
                    seen.insert(entity);
                    return;
                }
                for &(pos, seeing, _, elevated) in eyes {
                    if pos.chebyshev(tpos) <= seeing
                        && los_clear(&self.world.grid, pos, tpos, elevated)
                    {
                        seen.insert(entity);
                        return;
                    }
                }
            };
            for (id, p) in &self.world.printers {
                see_static(*id, p.pos, p.faction == faction);
            }
            for (id, st) in &self.world.structures {
                see_static(*id, st.pos, st.faction == faction);
            }
            for (id, d) in &self.world.depots {
                see_static(*id, d.pos, faction == 0); // factionless: see note above
            }
            for (id, n) in &self.world.nodes {
                see_static(*id, n.pos, false);
            }
            // Blight Cores (M8-C): nobody's own — seen like any other
            // stationary mass (attack's perception gate then just works).
            for (id, c) in &self.world.blight_cores {
                see_static(*id, c.pos, false);
            }
            // seen ∩ heard: sight is absolute — drop the blip.
            let heard: BTreeMap<EntityId, TilePos> =
                heard.into_iter().filter(|(e, _)| !seen.contains(e)).collect();
            new_perception.insert(faction, Perception { seen, heard });
        }

        // Permanent node knowledge (docs/05 Q70): seen nodes are
        // discovered forever; exhaustion updates only when observed.
        for &faction in &factions {
            let seen = &new_perception[&faction].seen;
            let known = self.world.known_nodes.entry(faction).or_default();
            for (id, node) in &self.world.nodes {
                if seen.contains(id) {
                    known.insert(
                        *id,
                        KnownNode { kind: node.kind, pos: node.pos, exhausted: node.amount == 0 },
                    );
                }
            }
        }

        self.world.perception = new_perception;
    }

    /// Phase 5b: detection episodes → Hiding XP (docs/05: per (bot,
    /// enemy faction); re-arm only after fully unobserved for the
    /// window). SEPARATE from the perception recompute so the phase-0 /
    /// SpawnBot seeds never advance re-arm counters — episode time is
    /// TICKS, not perception passes. Counters advance for every open
    /// episode, including against factions with no perceivers left (a
    /// wiped-out faction stops observing you; the episode still cools).
    pub(crate) fn settle_episodes(&mut self) {
        let bot_ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in bot_ids {
            let (entity, own_faction) = {
                let d = &self.world.bots[&id].data;
                (d.entity, d.faction)
            };
            // Factions observing this bot THIS tick.
            let detecting: Vec<u8> = self
                .world
                .perception
                .iter()
                .filter(|(f, p)| {
                    **f != own_faction
                        && (p.seen.contains(&entity) || p.heard.contains_key(&entity))
                })
                .map(|(f, _)| *f)
                .collect();
            let mut fresh = 0u32;
            {
                let bot = self.world.bots.get_mut(&id).expect("collected");
                for &faction in &detecting {
                    if bot.data.episodes.insert(faction, 0).is_none() {
                        fresh += 1; // a fresh episode: being caught teaches
                    }
                }
                let open: Vec<u8> = bot
                    .data
                    .episodes
                    .keys()
                    .copied()
                    .filter(|f| !detecting.contains(f))
                    .collect();
                for faction in open {
                    let counter = bot.data.episodes.get_mut(&faction).expect("just listed");
                    *counter += 1;
                    if *counter >= self.tuning.episode_rearm_ticks {
                        bot.data.episodes.remove(&faction);
                    }
                }
            }
            for _ in 0..fresh {
                self.world.pending_xp.push((id, XpTrack::Hiding, self.xp.hiding_episode_xp * 10));
            }
        }
    }
}

/// Standing on High Ground grants bonus sensor range (docs/05: +2; the
/// figure rides tuning with M8's full table — hardcoded ramp rules wait
/// there too, flagged).
pub fn high_ground_bonus(grid: &Grid, pos: TilePos) -> u32 {
    if on_high_ground(grid, pos) { 2 } else { 0 }
}

/// Mountain summits share the plateau's privileges (M8, docs/05): the
/// elevated-perceiver LoS exemption and the sensor bonus.
pub fn on_high_ground(grid: &Grid, pos: TilePos) -> bool {
    matches!(grid.get(pos), Some(TileKind::HighGround) | Some(TileKind::Mountain))
}
