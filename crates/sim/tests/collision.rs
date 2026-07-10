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
    let blocker = spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(4, 1), "move_to(nearest_depot())\n");

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
    spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(3, 1), "move_to(nearest_depot())\nlog(9)\n");

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
        spawn(&mut sim, TilePos::new(4, 1), "move_to(nearest_depot())\n");
        spawn(&mut sim, TilePos::new(6, 1), "move_to(nearest_depot())\n");
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
