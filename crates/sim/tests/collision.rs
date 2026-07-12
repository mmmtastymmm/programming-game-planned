//! Bots are solid: one per tile. Bumping an occupied tile freezes the
//! mover (~5s), who then retries (docs/02-agents.md).

use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::Color;
use sim::TilePos;

const IDLER: &str = "log(1)\n";

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 4,
        cargo_cap: 1,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

#[test]
fn bots_never_overlap_and_bumps_freeze() {
    // One-tile corridor: the mover must pass the idler's tile to reach the
    // depot — impossible. It bumps, freezes, retries, forever.
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.bump_damage = 0; // freeze/replan semantics test
    let blocker = spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(4, 1), "move_to(closest(depot).expect())\n");

    let mut saw_freeze = false;
    for _ in 0..300 {
        sim.step();
        let a = sim.world.bots[&blocker].data.pos;
        let b = sim.world.bots[&mover].data.pos;
        assert_ne!(a, b, "two bots must never share a tile");
        if sim.world.bots[&mover].data.bump_frozen > 0 {
            saw_freeze = true;
        }
    }
    assert!(saw_freeze, "bumping must freeze the mover");
    // The mover parked next to the blocker, still short of the depot.
    assert_eq!(sim.world.bots[&mover].data.pos, TilePos::new(2, 1));
}

#[test]
fn frozen_bots_do_not_think() {
    // Same corridor; the mover's freeze also stops its cycle grants, so a
    // bumped bot makes no VM progress while stunned.
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.bump_damage = 0; // frozen-brain semantics test
    spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(3, 1), "move_to(closest(depot).expect())\nlog(9)\n");

    for _ in 0..40 {
        sim.step();
    }
    let bot = &sim.world.bots[&mover];
    assert!(bot.data.bump_frozen > 0, "should be mid-freeze at tick 40");
    assert!(
        bot.data.log_buf.is_empty(),
        "move_to never completed, so log(9) must not have run"
    );
}

#[test]
fn printed_bots_spread_to_free_tiles() {
    let mut spec = MapSpec::empty(8, 8);
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(4, 4),
        faction: 0,
        color: 0,
        ruined: false,
        desired_max: 4,
    });
    spec.starting_ore = 50;
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    for _ in 0..120 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 4);
    let mut positions: Vec<TilePos> = sim.world.bots.values().map(|b| b.data.pos).collect();
    positions.sort();
    positions.dedup();
    assert_eq!(positions.len(), 4, "printed idlers must occupy distinct tiles");
}

#[test]
fn collision_world_is_deterministic() {
    let build = || {
        let mut spec = MapSpec::empty(7, 3);
        for x in 0..7 {
            spec.water.push(TilePos::new(x, 0));
            spec.water.push(TilePos::new(x, 2));
        }
        spec.depots.push(TilePos::new(0, 1));
        let mut sim = Sim::new(&spec);
        spawn(&mut sim, TilePos::new(1, 1), IDLER);
        spawn(&mut sim, TilePos::new(4, 1), "move_to(closest(depot).expect())\n");
        spawn(&mut sim, TilePos::new(6, 1), "move_to(closest(depot).expect())\n");
        sim
    };
    let mut a = build();
    let mut b = build();
    for tick in 0..300 {
        a.step();
        b.step();
        assert_eq!(a.state_hash(), b.state_hash(), "desync at tick {tick}");
    }
}

#[test]
fn sidestep_dodges_blocker_without_freezing() {
    // Open ground: when the next tile is occupied the mover takes a free
    // sidestep (no freeze), re-plans, and arrives at the depot.
    let mut spec = MapSpec::empty(7, 3);
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    let blocker = spawn(&mut sim, TilePos::new(2, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(4, 1), "move_to(closest(depot).expect())\n");

    for _ in 0..150 {
        sim.step();
        let a = sim.world.bots[&blocker].data.pos;
        let b = sim.world.bots[&mover].data.pos;
        assert_ne!(a, b, "never overlap");
        assert_eq!(
            sim.world.bots[&mover].data.bump_frozen, 0,
            "open ground must dodge, not freeze"
        );
    }
    let end = sim.world.bots[&mover].data.pos;
    assert!(
        end.chebyshev(TilePos::new(0, 1)) <= 1,
        "the dodge + re-plan must reach the depot; ended at {end:?}"
    );
}

#[test]
fn wait_idles_for_the_requested_ticks() {
    let mut sim = Sim::new(&MapSpec::empty(4, 4));
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(10)\nlog(1)\n");
    for _ in 0..8 {
        sim.step();
    }
    assert!(
        sim.world.bots[&bot].data.log_buf.is_empty(),
        "log must not run while waiting"
    );
    for _ in 0..12 {
        sim.step();
    }
    assert!(
        !sim.world.bots[&bot].data.log_buf.is_empty(),
        "log runs once the wait completes"
    );
}

#[test]
fn bumps_hurt_both_parties() {
    // One-tile corridor: mover repeatedly bumps the idler. Both bleed.
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    let blocker = spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(3, 1), "move_to(closest(depot).expect())\n");
    for _ in 0..200 {
        sim.step();
    }
    let blocker_hp = sim.world.bots[&blocker].data.hp;
    let mover_hp = sim.world.bots[&mover].data.hp;
    assert!(blocker_hp < 100, "the bumped bot takes damage too ({blocker_hp})");
    assert!(mover_hp < 100, "the bumper takes damage ({mover_hp})");
    assert_eq!(blocker_hp, mover_hp, "collisions are symmetric");
    // The blocker also recoils: it was frozen at some point.
    // (both frozen simultaneously right after each bump)
}

#[test]
fn corridor_deadlock_ends_in_mutual_destruction() {
    // Two movers head-on in a one-tile corridor, nowhere to dodge: they
    // grind each other down and both die — the deadlock self-clears the
    // expensive way (wrecks don't block).
    let mut spec = MapSpec::empty(8, 3);
    for x in 0..8 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    spec.ore_nodes.push((TilePos::new(7, 1), 50));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0; // isolate bump damage
    let east = spawn(&mut sim, TilePos::new(2, 1), "move_to(closest(ore).expect())\nmine()\n");
    let west = spawn(&mut sim, TilePos::new(5, 1), "move_to(closest(depot).expect())\n");
    // Low hp so the grind finishes within the test.
    sim.world.bots.get_mut(&east).unwrap().data.hp = 10;
    sim.world.bots.get_mut(&east).unwrap().data.max_hp = 10;
    sim.world.bots.get_mut(&west).unwrap().data.hp = 10;
    sim.world.bots.get_mut(&west).unwrap().data.max_hp = 10;
    for _ in 0..1500 {
        sim.step();
        if !sim.world.bots.contains_key(&east) && !sim.world.bots.contains_key(&west) {
            break;
        }
    }
    assert!(
        !sim.world.bots.contains_key(&east) && !sim.world.bots.contains_key(&west),
        "head-on deadlock must end in mutual destruction"
    );
}
