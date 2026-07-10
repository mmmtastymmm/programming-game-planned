//! Terraforming via blueprints: the player designates (PlaceBlueprint
//! command), bots do the labor (nearest_blueprint()/build()) — docs/05.
//! Plus rng(n): sanctioned randomness from the sim's seeded stream.

use sim::map::{MapSpec, TileKind};
use sim::sim::{Command, Sim};
use sim::world::{BlueprintKind, Color};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 4,
        cargo_cap: 2,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

/// Map split by a water wall; ore is unreachable until a bridge exists.
fn walled_map() -> MapSpec {
    let mut spec = MapSpec::empty(9, 5);
    for y in 0..5 {
        spec.water.push(TilePos::new(4, y));
    }
    spec.ore_nodes.push((TilePos::new(7, 2), 10));
    spec.depots.push(TilePos::new(0, 2));
    spec.starting_ore = 20;
    spec
}

// The trailing move_to matters: a builder that parks beside the finished
// bridge (crash-looping on nearest_blueprint) blocks the crossing it just
// built — the corridor problem, self-inflicted. Go home after work.
const BUILDER: &str = "\
move_to(nearest_blueprint())
build()
move_to(nearest_depot())
";

#[test]
fn builder_bot_bridges_the_wall() {
    let mut sim = Sim::new(&walled_map());
    let builder = spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    let site = TilePos::new(4, 2);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Bridge }).unwrap();
    assert_eq!(sim.world.stockpile_ore, 20 - sim.tuning.bridge_cost_ore, "placement charges ore");
    assert_eq!(sim.world.blueprints.len(), 1);

    for _ in 0..200 {
        sim.step();
        if sim.world.grid.get(site) == Some(TileKind::Bridge) {
            break;
        }
    }
    assert_eq!(sim.world.grid.get(site), Some(TileKind::Bridge), "bridge must be built");
    assert!(sim.world.blueprints.is_empty(), "blueprint consumed");
    assert!(
        sim.world.bots[&builder].data.xp_building >= sim.tuning.bridge_build_ticks as u64,
        "building earns Building XP"
    );
}

#[test]
fn bridge_opens_the_route_for_miners() {
    let mut sim = Sim::new(&walled_map());
    spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    // The miner faults ("unreachable") until the bridge exists, then works.
    spawn(&mut sim, TilePos::new(1, 1), "move_to(nearest_ore())\nmine()\nmove_to(nearest_depot())\ndeposit()\n");
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(4, 2),
        kind: BlueprintKind::Bridge,
    })
    .unwrap();

    for _ in 0..800 {
        sim.step();
    }
    assert!(
        sim.world.stockpile_ore > 20 - sim.tuning.bridge_cost_ore,
        "ore must eventually cross the bridge; stockpile {}",
        sim.world.stockpile_ore
    );
}

#[test]
fn blueprint_placement_validates_site_and_funds() {
    let mut sim = Sim::new(&walled_map());
    // Not water: rejected.
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(1, 1),
        kind: BlueprintKind::Bridge,
    })
    .unwrap();
    assert!(sim.world.blueprints.is_empty());
    assert_eq!(sim.world.stockpile_ore, 20, "invalid site must not charge");
    // Duplicate site: rejected.
    let site = TilePos::new(4, 1);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Bridge }).unwrap();
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Bridge }).unwrap();
    assert_eq!(sim.world.blueprints.len(), 1, "one blueprint per tile");
}

#[test]
fn rng_is_bounded_and_deterministic() {
    let run = || {
        let mut sim = Sim::new(&MapSpec::empty(4, 4));
        let bot = spawn(&mut sim, TilePos::new(1, 1), "log(rng(100))\nwait(rng(5) + 1)\n");
        for _ in 0..60 {
            sim.step();
        }
        sim.world.bots[&bot].data.log_buf.clone()
    };
    let a = run();
    let b = run();
    assert!(!a.is_empty());
    assert_eq!(a, b, "seeded rng must replay identically");
    for entry in &a {
        let v: i64 = entry.parse().expect("logged ints");
        assert!((0..100).contains(&v), "rng(100) out of range: {v}");
    }
}
