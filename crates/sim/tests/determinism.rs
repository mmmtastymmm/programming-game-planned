//! Determinism guards for RNG streams the golden fixture never exercises
//! (07/08, CLAUDE.md). The golden replay pins a straight-line miner with no
//! nests, so `rng.feral_mutation`, `rng.wander`, `rng.explore`, and
//! `rng.quirk_roll` advance in NO hash-comparing test — a nondeterminism
//! regression in any of those paths (a HashMap iteration, a wall-clock read)
//! would desync a live match yet sail through the whole replay suite.
//!
//! This runs a scenario that drives all four streams and asserts two
//! independent runs produce identical per-tick state hashes. (Same process,
//! but std HashMap seeds differ per instance, so an accidental HashMap in any
//! exercised path makes the two runs diverge — exactly what we want to catch.)

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::{Color, FERAL_FACTION};
use sim::TilePos;

/// Build and run the shared scenario, collecting the phase-9 state hash after
/// every tick. Returns (per-tick hashes, feral count) for a liveness check.
fn run() -> (Vec<u64>, usize) {
    let mut spec = MapSpec::empty(16, 12);
    spec.quirk_permille = 500; // exercise rng.quirk_roll
    // A Magician nest (arcanum 1) mutates every print via rng.feral_mutation,
    // and its Drones run wander() — so rng.wander advances too.
    spec.nests = vec![(TilePos::new(3, 3), 1)];
    spec.ore_nodes.push((TilePos::new(10, 6), 100));
    let mut sim = Sim::new(&spec);
    sim.tuning.nest_print_ticks = 3;
    // Player bots driving the explore and wander streams directly.
    for (x, src) in [(1i32, "explore()\n"), (2, "wander()\n")] {
        sim.apply(&Command::SpawnBot {
            pos: TilePos::new(x, 1),
            source: src.into(),
            cpu: 4,
            cargo_cap: 2,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap();
    }
    let mut hashes = Vec::new();
    for _ in 0..150 {
        sim.step();
        hashes.push(sim.state_hash());
    }
    let ferals = sim.world.bots.values().filter(|b| b.data.faction == FERAL_FACTION).count();
    (hashes, ferals)
}

#[test]
fn feral_mutation_wander_and_explore_streams_are_deterministic() {
    let (a, ferals_a) = run();
    let (b, ferals_b) = run();
    assert_eq!(
        a, b,
        "two runs of a feral-mutation + wander + explore + quirk scenario must \
         produce identical state hashes at EVERY tick"
    );
    // Liveness: the scenario actually drove the streams (ferals printed &
    // mutated), so the equality above is not comparing two empty worlds.
    assert!(
        ferals_a >= 1 && ferals_a == ferals_b,
        "the Magician nest must have printed ferals (got {ferals_a})"
    );
}

/// Q102: phase 4 resolves in three passes (move → combat → work), so
/// **combat sees a settled world** — whether a fleeing bot escapes a
/// swing is a rule, not a function of entity id.
///
/// The probe runs one fight twice, swapping only SPAWN ORDER (which is
/// what entity id is), across a sweep of head-start delays so the
/// decisive alignment — a swing landing on the very tick the victim's
/// traverse completes — is covered without hand-timing it. Under the old
/// one-bot-at-a-time loop `delay = 0` disagreed outright: attacker-first
/// landed the hit (victim 90 hp), victim-first let the escape work
/// (100 hp). Verified to fail against that code.
#[test]
fn combat_outcome_does_not_depend_on_spawn_order() {
    /// Victim HP per tick for a chase where it walks away while the
    /// attacker swings; `attacker_first` flips which bot gets the lower
    /// entity id, `delay` slides the victim's departure.
    fn fight(attacker_first: bool, delay: u32) -> Vec<i64> {
        let mut spec = MapSpec::empty(20, 3);
        spec.depots.push((TilePos::new(18, 1), 1)); // something to run at
        let mut sim = Sim::new(&spec);
        sim.stats.move_rate_deci = 10; // 1 tick/tile: keep the chase tight
        let flee = format!("wait({delay})\nmove_to(closest(depot).expect())\nwait(500)\n");
        let hunt = "attack(closest(enemy).expect())\nwait(500)\n"; // one swing
        let spawn = |sim: &mut Sim, pos: TilePos, faction: u8, src: &str| {
            sim.apply(&Command::SpawnBot {
                pos,
                source: src.into(),
                cpu: 4,
                cargo_cap: 2,
                faction,
                hp: 100,
                color: Color::GREEN,
            })
            .unwrap()
            .unwrap()
        };
        let victim = if attacker_first {
            spawn(&mut sim, TilePos::new(1, 1), 0, hunt);
            spawn(&mut sim, TilePos::new(2, 1), 1, &flee)
        } else {
            let v = spawn(&mut sim, TilePos::new(2, 1), 1, &flee);
            spawn(&mut sim, TilePos::new(1, 1), 0, hunt);
            v
        };
        (0..25)
            .map(|_| {
                sim.step();
                sim.world.bots.get(&victim).map(|b| b.data.hp).unwrap_or(-1)
            })
            .collect()
    }
    for delay in 0..10 {
        assert_eq!(
            fight(true, delay),
            fight(false, delay),
            "delay {delay}: the same fight must resolve the same way \
             whichever bot was printed first"
        );
    }
}
