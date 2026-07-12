//! Regression repro: the viewer demo map exactly — solid wall at x=16,
//! one-way bridge placed and built, miners must cross.

use sim::map::{Direction, MapSpec, PrinterSpec, TileKind};
use sim::sim::{Command, Sim};
use sim::world::{BlueprintKind, Color};
use sim::TilePos;

const MINER: &str = "if blueprint_exists():\n    move_to(nearest_blueprint())\n    build()\nmove_to(nearest_ore())\nmine()\nmove_to(nearest_depot())\ndeposit()\n";
const BUILDER: &str = "move_to(nearest_blueprint())\nbuild()\nmove_to(nearest_depot())\n";

fn viewer_map() -> MapSpec {
    let mut spec = MapSpec::empty(24, 14);
    for y in 2..9 {
        spec.rubble.push(TilePos::new(12, y));
    }
    for y in 0..14 {
        spec.water.push(TilePos::new(16, y));
    }
    spec.ore_nodes.push((TilePos::new(8, 3), 25));
    spec.ore_nodes.push((TilePos::new(20, 3), 60));
    spec.ore_nodes.push((TilePos::new(19, 11), 40));
    spec.depots.push(TilePos::new(3, 7));
    spec.printers.push(PrinterSpec { pos: TilePos::new(2, 5), faction: 0, color: 0, ruined: false, desired_max: 4 });
    spec.starting_ore = 30;
    spec
}

#[test]
fn viewer_demo_crossing_works() {
    let mut sim = Sim::new(&viewer_map());
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: MINER.into() }).unwrap();
    // Builder + return one-way west, outbound one-way east (return first).
    sim.apply(&Command::SpawnBot { pos: TilePos::new(4, 7), source: BUILDER.into(), cpu: 4, cargo_cap: 1, faction: 0, hp: 100, color: Color::GREEN }).unwrap();
    sim.apply(&Command::PlaceBlueprint { pos: TilePos::new(16, 8), kind: BlueprintKind::BridgeOneWay(Direction::West) }).unwrap();
    sim.apply(&Command::PlaceBlueprint { pos: TilePos::new(16, 5), kind: BlueprintKind::BridgeOneWay(Direction::East) }).unwrap();

    let mut east_built_at = None;
    for tick in 0..2500 {
        sim.step();
        if east_built_at.is_none()
            && sim.world.grid.get(TilePos::new(16, 5)) == Some(TileKind::BridgeOneWay(Direction::East))
        {
            east_built_at = Some(tick);
        }
    }
    println!("east bridge built at tick {east_built_at:?}");
    println!("stockpile {}", sim.world.stockpile_ore);
    println!("west ore left {:?}", sim.world.ore_nodes.values().map(|n| (n.pos, n.amount)).collect::<Vec<_>>());
    let east_of_wall = sim.world.bots.values().filter(|b| b.data.pos.x > 16).count();
    println!("bots east of wall: {east_of_wall}, total {}", sim.world.bots.len());
    assert!(east_built_at.is_some(), "bridge must get built");
    assert!(
        sim.world.ore_nodes.values().any(|n| n.pos.x > 16 && n.amount < 60),
        "eastern ore must get mined after the bridges exist"
    );
}

#[test]
fn bridges_added_long_after_pathfinding_failures_still_work() {
    // The user-reported scenario: miners fault "unreachable" for hundreds
    // of ticks FIRST; only then are blueprints placed and built. Retries
    // must pick up the new tiles — no stale pathfinding state.
    let mut sim = Sim::new(&viewer_map());
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: MINER.into() }).unwrap();
    let builder = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(4, 7),
            source: BUILDER.into(),
            cpu: 4,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    // Drain the west ore quickly so miners are already fault-looping on the
    // unreachable east nodes well before any blueprint exists.
    sim.world.ore_nodes.values_mut().find(|n| n.pos.x < 16).unwrap().amount = 2;

    for _ in 0..600 {
        sim.step();
    }
    let unreachable_faults = sim
        .world
        .archive
        .iter()
        .filter(|e| e.text.contains("unreachable"))
        .count();
    assert!(
        unreachable_faults > 5,
        "miners must have been failing for a while first ({unreachable_faults})"
    );

    // NOW the player bridges the wall (return lane first).
    sim.apply(&Command::PlaceBlueprint { pos: TilePos::new(16, 8), kind: BlueprintKind::BridgeOneWay(Direction::West) }).unwrap();
    sim.apply(&Command::PlaceBlueprint { pos: TilePos::new(16, 5), kind: BlueprintKind::BridgeOneWay(Direction::East) }).unwrap();

    for _ in 0..2500 {
        sim.step();
    }
    println!("stockpile {}", sim.world.stockpile_ore);
    println!("builder pos {:?}", sim.world.bots.get(&builder).map(|b| b.data.pos));
    println!("east ore {:?}", sim.world.ore_nodes.values().filter(|n| n.pos.x > 16).map(|n| n.amount).collect::<Vec<_>>());
    assert!(
        sim.world.ore_nodes.values().any(|n| n.pos.x > 16 && n.amount < 60),
        "late bridges must still unlock the east; east ore untouched"
    );
}
