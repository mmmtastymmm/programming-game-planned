//! Terraforming via blueprints: the player designates (PlaceBlueprint
//! command), bots do the labor (nearest_blueprint()/build()) — docs/05.
//! Plus rng(n): sanctioned randomness from the sim's seeded stream.

use sim::map::{Direction, MapSpec, OverlayKind, TileKind};
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

#[test]
fn one_way_bridge_only_crosses_with_the_arrow() {
    // Solid wall; a bridge with an EAST arrow overlay. West bots can cross
    // to the ore; nothing can come back — including the loaded miner, whose
    // return trip faults unreachable. Directionality bites both ways.
    let mut sim = Sim::new(&walled_map());
    spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    let miner = spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(nearest_ore())\nmine()\nmove_to(nearest_depot())\ndeposit()\n",
    );
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(4, 2),
        kind: BlueprintKind::Bridge,
    })
    .unwrap();
    sim.apply(&Command::PlaceOverlay {
        pos: TilePos::new(4, 2),
        overlay: Some(OverlayKind::Arrow(Direction::East)),
    })
    .unwrap();

    for _ in 0..600 {
        sim.step();
    }
    let miner_bot = &sim.world.bots[&miner];
    assert!(miner_bot.data.xp_mining > 0, "the miner must cross east and mine");
    assert!(miner_bot.data.pos.x > 4, "and be stranded east of the wall");
    assert_eq!(
        sim.world.stockpile_ore,
        20 - sim.tuning.bridge_cost_ore - sim.tuning.overlay_cost_ore,
        "no ore comes back west"
    );
    assert!(
        sim.world
            .archive
            .iter()
            .any(|e| e.text.contains("unreachable") && e.bot == miner),
        "the return trip must fault unreachable"
    );
}

#[test]
fn opposing_one_way_bridges_make_a_round_trip() {
    // Two one-ways, opposite arrows: a deadlock-free crossing. The miner
    // does full loops and ore lands in the depot.
    let mut sim = Sim::new(&walled_map());
    spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(nearest_ore())\nmine()\nmove_to(nearest_depot())\ndeposit()\n",
    );
    // Return bridge FIRST (placement order = build order here): if the
    // outbound bridge finishes first, the miner crosses, fills its cargo,
    // and livelocks — mine() faults "cargo full" on every restart, so the
    // return lines never run again. A straight-line program meeting a
    // one-way world: the Tier-2 `if cargo_full():` unlock in miniature.
    for (y, dir) in [(3, Direction::West), (1, Direction::East)] {
        sim.apply(&Command::PlaceBlueprint {
            pos: TilePos::new(4, y),
            kind: BlueprintKind::Bridge,
        })
        .unwrap();
        sim.apply(&Command::PlaceOverlay {
            pos: TilePos::new(4, y),
            overlay: Some(OverlayKind::Arrow(dir)),
        })
        .unwrap();
    }

    for _ in 0..1000 {
        sim.step();
    }
    assert!(
        sim.world.stockpile_ore
            > 20 - 2 * (sim.tuning.bridge_cost_ore + sim.tuning.overlay_cost_ore),
        "ore must round-trip over the opposing one-ways; stockpile {}",
        sim.world.stockpile_ore
    );
}

#[test]
fn arrow_overlay_works_on_plain_ground() {
    // Overlays are terrain-independent: an EAST arrow on open plains
    // blocks westbound crossing of that tile; clearing it reopens it.
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    spec.starting_ore = 10;
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::PlaceOverlay {
        pos: TilePos::new(3, 1),
        overlay: Some(OverlayKind::Arrow(Direction::East)),
    })
    .unwrap();
    let bot = spawn(&mut sim, TilePos::new(5, 1), "move_to(nearest_depot())\n");
    for _ in 0..80 {
        sim.step();
    }
    assert!(
        sim.world.archive.iter().any(|e| e.text.contains("unreachable") && e.bot == bot),
        "westbound travel over an east arrow must be unreachable"
    );
    // Clear the arrow: the road reopens.
    sim.apply(&Command::PlaceOverlay { pos: TilePos::new(3, 1), overlay: None }).unwrap();
    for _ in 0..120 {
        sim.step();
    }
    assert!(
        sim.world.bots[&bot].data.pos.chebyshev(TilePos::new(0, 1)) <= 1,
        "clearing the overlay must reopen the road; at {:?}",
        sim.world.bots[&bot].data.pos
    );
}

#[test]
fn paint_is_stored_and_cleared() {
    let mut sim = Sim::new(&MapSpec::empty(4, 4));
    let pos = TilePos::new(2, 2);
    sim.apply(&Command::PlacePaint { pos, color: Some(3) }).unwrap();
    assert_eq!(sim.world.paint.get(&pos), Some(&3));
    let painted_hash = sim.state_hash();
    sim.apply(&Command::PlacePaint { pos, color: None }).unwrap();
    assert!(sim.world.paint.is_empty());
    assert_ne!(sim.state_hash(), painted_hash, "paint is shared, replayed state");
}

#[test]
fn deploy_hot_swaps_live_bots() {
    // The viewer regression: bots exist at their dial, the player deploys
    // new code — live bots must pick it up at their next loop boundary,
    // without waiting for a fresh print.
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    let bot = spawn(&mut sim, TilePos::new(2, 2), "log(1)\n");
    for _ in 0..10 {
        sim.step();
    }
    sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::GREEN,
        source: "log(2)\n".into(),
    })
    .unwrap();
    for _ in 0..20 {
        sim.step();
    }
    let logs = &sim.world.bots[&bot].data.log_buf;
    assert!(logs.iter().any(|l| l == "2"), "live bot must hot-swap; logs: {logs:?}");
}

#[test]
fn blueprint_exists_predicate() {
    let mut sim = Sim::new(&walled_map());
    let bot = spawn(&mut sim, TilePos::new(1, 1), "log(blueprint_exists())\nwait(2)\n");
    for _ in 0..12 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&bot].data.log_buf.first().map(String::as_str), Some("False"));
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(4, 1),
        kind: BlueprintKind::Bridge,
    })
    .unwrap();
    for _ in 0..30 {
        sim.step();
    }
    assert!(
        sim.world.bots[&bot].data.log_buf.iter().any(|l| l == "True"),
        "predicate must flip once a blueprint exists: {:?}",
        sim.world.bots[&bot].data.log_buf
    );
}
