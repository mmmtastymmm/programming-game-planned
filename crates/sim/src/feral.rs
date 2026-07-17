//! The Feral faction (M12, docs/04): nests print archetype programs on
//! the same VM as player bots — enemies whose code IS their stat block.
//!
//! Determinism: nests tick in entity-id order; the archetype mix is a
//! round-robin over the nest's lifetime print counter (no RNG spent on
//! the pick); only per-print MUTATION (Magician/Moon arcana) draws from
//! the dedicated `rng.feral_mutation` stream.

use std::rc::Rc;

use pyrite::{Value, Vm};

use crate::map::MapSpec;
use crate::sim::Sim;
use crate::world::{
    next_rand, ArchiveEntry, ArchiveKind, BotId, Color, EntityId, Nest, NestState, FERAL_FACTION,
};

/// The v1 archetype set (docs/04). Each maps to a fixed Feral color slot
/// so decryption — keyed (viewer, owner, color) — accrues PER ARCHETYPE,
/// exactly the "+N% of that nest's archetype program" rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archetype {
    Drone,
    Stinger,
    Harvester,
    Warden,
}

impl Archetype {
    pub fn color(self) -> Color {
        // High slots, clear of the player palette's named nine.
        match self {
            Archetype::Drone => Color(200),
            Archetype::Stinger => Color(201),
            Archetype::Harvester => Color(202),
            Archetype::Warden => Color(203),
        }
    }

    /// Threat level (docs/04) — scales the chassis over the universal
    /// floor via `feral_hp_per_threat_pct`.
    pub fn threat(self) -> i64 {
        match self {
            Archetype::Drone => 1,
            Archetype::Stinger | Archetype::Harvester => 2,
            Archetype::Warden => 3,
        }
    }

    /// The shipped source, in CURRENT builtins (docs/04's listings,
    /// adjusted: a `move_to` precedes each `attack` so the swing is in
    /// range, and the Harvester guards on `exists(ore)` instead of
    /// crash-looping on an empty map — flagged in TASKS.md).
    pub fn source(self) -> &'static str {
        match self {
            Archetype::Drone => "\
wander()
wander()
wait(3)
if exists(enemy):
    move_to(closest(enemy).expect())
    attack(closest(enemy).expect())
",
            Archetype::Stinger => "\
if health_low():
    move_to(home)
    wait(8)
if exists(enemy):
    move_to(closest(enemy).expect())
    attack(closest(enemy).expect())
wander()
",
            Archetype::Harvester => "\
if exists(ore):
    move_to(closest(ore).expect())
    mine()
    move_to(home)
    deposit()
wander()
wait(4)
",
            Archetype::Warden => "\
for spot in patrol_route:
    move_to(spot)
    if exists(enemy):
        try_broadcast(\"intruder\", closest(enemy).expect())
        move_to(closest(enemy).expect())
        attack(closest(enemy).expect())
wait(6)
",
        }
    }
}

/// Does this arcanum mutate per print? v1 flags (docs/04): 1 The Magician
/// (creative tweaks) and 18 The Moon (counter-intel — code written to be
/// decrypted differently every time). 10/21 join when they ship.
pub fn mutates(arcanum: u8) -> bool {
    matches!(arcanum, 1 | 18)
}

/// The archetype mix per escalation tier (docs/04's escalation ladder):
/// Calm is DRONES ONLY (the tutorial state — harmless), Probing adds
/// Stingers + Harvesters, Contested adds Wardens, Overrun drops the
/// Drones. Deterministic round-robin over `prints`.
fn mix(tier: u8) -> &'static [Archetype] {
    use Archetype::*;
    match tier {
        0 => &[Drone],
        1 => &[Drone, Stinger, Harvester],
        2 => &[Stinger, Harvester, Drone, Warden],
        _ => &[Stinger, Warden, Harvester],
    }
}

/// Minor procedural mutation (docs/04: "tweaked constants" — MUST stay
/// parse-valid): nudge one integer literal ±1 (floored at 1). String
/// literals are skipped so a digit in a channel name is never touched.
pub fn mutate_source(source: &str, stream: &mut u64) -> String {
    let bytes = source.as_bytes();
    let mut literals: Vec<(usize, usize)> = Vec::new(); // (start, len)
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            in_str = !in_str;
            i += 1;
            continue;
        }
        if !in_str && b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            // Digit runs glued to identifiers (name2) aren't literals.
            let prev_ident = start > 0
                && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
            if !prev_ident {
                literals.push((start, i - start));
            }
            continue;
        }
        i += 1;
    }
    if literals.is_empty() {
        return source.to_string();
    }
    let pick = (next_rand(stream) % literals.len() as u64) as usize;
    let (start, len) = literals[pick];
    let value: i64 = source[start..start + len].parse().unwrap_or(1);
    let tweaked = if next_rand(stream) % 2 == 0 { value + 1 } else { (value - 1).max(1) };
    format!("{}{}{}", &source[..start], tweaked, &source[start + len..])
}

/// The quadratic printer gate (docs/01): the first two printer slots are
/// free (Green + the repairable Red); the 3rd needs 1 controlled nest,
/// the 4th 3 total, then 6, 10, … (triangular numbers).
pub fn printers_allowed(claimed_nests: u32) -> u32 {
    let mut extra = 0u32;
    while (extra + 1) * (extra + 2) / 2 <= claimed_nests {
        extra += 1;
    }
    2 + extra
}

impl Sim {
    /// World-build step (called from `Sim::new`): place the spec's nests
    /// (filtered by the match's max-arcanum option), hand the Feral
    /// faction its channel construct (Wardens broadcast) and the map's
    /// node knowledge (they live here).
    pub(crate) fn build_nests(&mut self, spec: &MapSpec) {
        for &(pos, arcanum) in &spec.nests {
            if arcanum > spec.max_arcanum {
                continue;
            }
            let entity = self.world.alloc_entity();
            let hp = self.tuning.nest_hp + arcanum as i64 * self.tuning.nest_hp_per_arcanum;
            self.world.nests.insert(
                entity,
                Nest {
                    entity,
                    pos,
                    arcanum,
                    hp,
                    max_hp: hp,
                    state: NestState::Active,
                    stock_deci: self.tuning.nest_seed_stock_deci,
                    job: None,
                    prints: 0,
                },
            );
        }
        if self.world.nests.is_empty() {
            return;
        }
        // Ferals parse with everything unlocked (their code previews the
        // player's future power — docs/04) and know their own map.
        self.world.unlocks.insert(FERAL_FACTION, pyrite::UnlockSet::all());
        let nodes: Vec<(EntityId, crate::world::KnownNode)> = self
            .world
            .nodes
            .iter()
            .map(|(id, n)| {
                (*id, crate::world::KnownNode { kind: n.kind, pos: n.pos, exhausted: false })
            })
            .collect();
        self.world.known_nodes.entry(FERAL_FACTION).or_default().extend(nodes);
    }

    /// Phase 8, with the printers: escalation, nest economy, prints, and
    /// undefended-claim reclaims. Nest order = entity-id order.
    pub(crate) fn tick_nests(&mut self) {
        if self.world.nests.is_empty() {
            return;
        }
        // Escalation (docs/04): player FOOTPRINT, never wall-clock —
        // structures + printers + claimed nests + kills×weight.
        let claimed = self
            .world
            .nests
            .values()
            .filter(|n| matches!(n.state, NestState::Claimed(_)))
            .count() as u64;
        let footprint = self.world.structures.len() as u64
            + self.world.printers.len() as u64
            + claimed
            + self.world.ferals_killed as u64 * self.tuning.escalation_kill_weight;
        self.world.escalation = if footprint >= self.tuning.escalation_overrun {
            3
        } else if footprint >= self.tuning.escalation_contested {
            2
        } else if footprint >= self.tuning.escalation_probing {
            1
        } else {
            0
        };

        let ids: Vec<EntityId> = self.world.nests.keys().copied().collect();
        for id in ids {
            match self.world.nests[&id].state {
                NestState::Active => self.tick_active_nest(id),
                NestState::Claimed(owner) => self.check_reclaim(id, owner),
                NestState::Defeated => {}
            }
        }
    }

    fn tick_active_nest(&mut self, id: EntityId) {
        let tuning_income = self.tuning.nest_income_deci;
        let nest = self.world.nests.get_mut(&id).expect("nest exists");
        nest.stock_deci += tuning_income;
        match nest.job {
            Some(ticks) if ticks > 1 => {
                nest.job = Some(ticks - 1);
            }
            Some(_) => {
                // Print done — hold at 1 tick (like printers) until a
                // spawn tile frees up.
                if self.spawn_feral(id) {
                    let nest = self.world.nests.get_mut(&id).expect("nest exists");
                    nest.job = None;
                    nest.prints += 1;
                } else {
                    self.world.nests.get_mut(&id).expect("nest exists").job = Some(1);
                }
            }
            None => {
                if nest.stock_deci >= self.tuning.nest_print_cost_deci {
                    nest.stock_deci -= self.tuning.nest_print_cost_deci;
                    nest.job = Some(self.tuning.nest_print_ticks);
                }
            }
        }
    }

    /// Undefended claims are loans (docs/04): adjacent Feral activity
    /// with no owner bot inside the guard radius re-takes the site.
    fn check_reclaim(&mut self, id: EntityId, owner: u8) {
        let pos = self.world.nests[&id].pos;
        let radius = self.tuning.nest_guard_radius;
        let defended = self.world.bots.values().any(|b| {
            b.data.faction == owner && !b.data.dying && b.data.pos.chebyshev(pos) <= radius
        });
        if defended {
            return;
        }
        let feral_adjacent = self.world.bots.values().any(|b| {
            b.data.faction == FERAL_FACTION && !b.data.dying && b.data.pos.chebyshev(pos) <= 1
        });
        if !feral_adjacent {
            return;
        }
        let nest = self.world.nests.get_mut(&id).expect("nest exists");
        nest.state = NestState::Active;
        nest.hp = nest.max_hp / 2;
        let tick = self.world.tick;
        self.world.archive.push(ArchiveEntry {
            tick,
            bot: BotId(0),
            kind: ArchiveKind::Log,
            level: 0,
            line: 0,
            text: format!("nest at {},{} reclaimed by the Ferals", pos.x, pos.y),
        });
    }

    /// One Feral print: archetype from the tier mix (round-robin), source
    /// mutated for flagged arcana, `home`/`patrol_route` pre-bound as VM
    /// constants (Q79's kind-constant mechanism, nest-scoped). Returns
    /// false when no spawn tile is free (the job holds).
    fn spawn_feral(&mut self, nest_id: EntityId) -> bool {
        let nest = self.world.nests[&nest_id].clone();
        let Some(spawn_pos) = self.world.free_spawn_tile(nest.pos) else {
            return false;
        };
        let archetype = {
            let m = mix(self.world.escalation);
            m[(nest.prints as usize) % m.len()]
        };
        let source = if mutates(nest.arcanum) {
            mutate_source(archetype.source(), &mut self.world.rng.feral_mutation)
        } else {
            archetype.source().to_string()
        };
        // Every deployed version enters the library (the Codex reads it;
        // mutated variants get their own hash — the diff view's food).
        let hash = crate::world::program_hash(&source);
        self.world.program_library.entry(hash).or_insert_with(|| source.clone());

        let unlocks = crate::world::faction_unlocks(&self.world, FERAL_FACTION);
        let program = match pyrite::parse(&source, &unlocks) {
            Ok(p) => p,
            // Mutation must stay parse-valid by construction; if it ever
            // isn't, ship the pristine source rather than skip the print.
            Err(_) => pyrite::parse(archetype.source(), &unlocks)
                .expect("shipped archetype sources parse"),
        };
        let mut config = self.vm_config.clone();
        config.constants.insert("home".to_string(), Value::Entity(nest.entity.0));
        let route = self.patrol_route(&nest);
        config.constants.insert("patrol_route".to_string(), Value::List(route));
        let mut vm = Vm::new(Rc::new(program), config);
        vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot));

        let s = &self.stats;
        let hp = s.hp * (100 + (archetype.threat() - 1) * self.tuning.feral_hp_per_threat_pct)
            / 100;
        let (cpu, cap) = (s.cpu_centi, s.cargo_cap_deci);
        self.insert_bot(spawn_pos, FERAL_FACTION, archetype.color(), hp, cpu, cap, vm, true);
        true
    }

    /// The Warden's beat: the nest plus its nearest resource nodes
    /// (Manhattan, ties by entity id) — the ground worth defending.
    fn patrol_route(&self, nest: &Nest) -> Vec<Value> {
        let mut nodes: Vec<(u32, EntityId)> = self
            .world
            .nodes
            .iter()
            .map(|(id, n)| (nest.pos.manhattan(n.pos), *id))
            .collect();
        nodes.sort();
        let mut route = vec![Value::Entity(nest.entity.0)];
        route.extend(nodes.into_iter().take(3).map(|(_, id)| Value::Entity(id.0)));
        route
    }
}
