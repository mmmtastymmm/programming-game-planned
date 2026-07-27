//! Terraforming via blueprints: the player designates (PlaceBlueprint
//! command), bots do the labor (closest(blueprint).expect()/build()) — docs/05.
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
    spec.depots.push((TilePos::new(0, 2), 0));
    spec.starting_ore = 20;
    spec.starting_stock.push((0, sim::resources::Resource::Stone, 20));
    spec
}

// The trailing move_to matters: a builder that parks beside the finished
// bridge (crash-looping on closest(blueprint)) blocks the crossing it just
// built — the corridor problem, self-inflicted. Go home after work.
const BUILDER: &str = "\
move_to(closest(blueprint).expect())
build()
move_to(closest(depot).expect())
";

#[test]
fn builder_bot_bridges_the_wall() {
    let mut sim = Sim::new(&walled_map());
    let builder = spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    let site = TilePos::new(4, 2);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Bridge, faction: 0 }).unwrap();
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Stone),
        200 - sim.tuning.bridge_cost_stone,
        "placement charges Stone (docs/03: civil works)"
    );
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
        sim.world.bots[&builder].data.xp(sim::world::XpTrack::Building) >= sim.tuning.bridge_build_ticks as u64,
        "building earns Building XP"
    );
}

#[test]
fn bridge_opens_the_route_for_miners() {
    let mut sim = Sim::new(&walled_map());
    spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    // The miner faults ("unreachable") until the bridge exists, then works.
    spawn(&mut sim, TilePos::new(1, 1), "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n");
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(4, 2),
        kind: BlueprintKind::Bridge,
        faction: 0,
    })
    .unwrap();

    for _ in 0..800 {
        sim.step();
    }
    assert!(
        sim.world.stock_get(0, sim::resources::Resource::Iron) > 200,
        "ore must eventually cross the bridge and grow the seed stock; stockpile {}",
        sim.world.stock_get(0, sim::resources::Resource::Iron)
    );
}

#[test]
fn blueprint_placement_validates_site_and_funds() {
    let mut sim = Sim::new(&walled_map());
    // Not water: rejected.
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(1, 1),
        kind: BlueprintKind::Bridge,
        faction: 0,
    })
    .unwrap();
    assert!(sim.world.blueprints.is_empty());
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Stone),
        200,
        "invalid site must not charge Stone"
    );
    // Duplicate site: rejected.
    let site = TilePos::new(4, 1);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Bridge, faction: 0 }).unwrap();
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Bridge, faction: 0 }).unwrap();
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
        let v: i64 = entry.1.parse().expect("logged ints");
        assert!((0..100).contains(&v), "rng(100) out of range: {v}");
    }
}

#[test]
fn one_way_bridge_only_crosses_with_the_arrow() {
    // Solid wall; a bridge with an EAST arrow overlay. West bots can cross
    // to the ore; nothing can come back — including the loaded miner, whose
    // return trip faults unreachable. Directionality bites both ways.
    let mut sim = Sim::new(&walled_map());
    sim.tuning.fault_damage = 0; // stranded-miner routing test, not a health test
    spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    let miner = spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n",
    );
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(4, 2),
        kind: BlueprintKind::Bridge,
        faction: 0,
    })
    .unwrap();
    sim.apply(&Command::PlaceOverlay {
        pos: TilePos::new(4, 2),
        overlay: Some(OverlayKind::Arrow(Direction::East)),
        faction: 0,
    })
    .unwrap();

    for _ in 0..600 {
        sim.step();
    }
    let miner_bot = &sim.world.bots[&miner];
    assert!(miner_bot.data.xp(sim::world::XpTrack::Mining) > 0, "the miner must cross east and mine");
    assert!(miner_bot.data.pos.x > 4, "and be stranded east of the wall");
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Iron),
        200,
        "no ore comes back west (seed iron stock untouched)"
    );
    assert!(
        sim.world
            .archive_all()
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
        "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n",
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
        faction: 0,
        })
        .unwrap();
        sim.apply(&Command::PlaceOverlay {
            pos: TilePos::new(4, y),
            overlay: Some(OverlayKind::Arrow(dir)),
        faction: 0,
        })
        .unwrap();
    }

    for _ in 0..1000 {
        sim.step();
    }
    assert!(
        sim.world.stock_get(0, sim::resources::Resource::Iron) > 200,
        "ore must round-trip over the opposing one-ways; stockpile {}",
        sim.world.stock_get(0, sim::resources::Resource::Iron)
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
    spec.depots.push((TilePos::new(0, 1), 0));
    spec.starting_ore = 10;
    spec.starting_stock.push((0, sim::resources::Resource::Stone, 20));
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::PlaceOverlay {
        pos: TilePos::new(3, 1),
        overlay: Some(OverlayKind::Arrow(Direction::East)),
        faction: 0,
    })
    .unwrap();
    let bot = spawn(&mut sim, TilePos::new(5, 1), "move_to(closest(depot).expect())\n");
    for _ in 0..80 {
        sim.step();
    }
    assert!(
        sim.world.archive_all().any(|e| e.text.contains("unreachable") && e.bot == bot),
        "westbound travel over an east arrow must be unreachable"
    );
    // Clear the arrow: the road reopens.
    sim.apply(&Command::PlaceOverlay { pos: TilePos::new(3, 1), overlay: None, faction: 0 }).unwrap();
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
fn paint_is_labor_not_a_click() {
    // Q97: PlacePaint designates — a paint blueprint a bot must service;
    // nothing recolors until the labor lands.
    let mut sim = Sim::new(&MapSpec::empty(4, 4));
    let pos = TilePos::new(2, 2);
    sim.apply(&Command::PlacePaint { pos, color: Some(3), faction: 0 }).unwrap();
    assert!(sim.world.paint.is_empty(), "the command paints nothing by itself");
    assert!(
        sim.world.blueprints.values().any(|b| b.pos == pos && b.faction == 0),
        "a faction-attributed paint designation is placed"
    );
    let bot = spawn(&mut sim, TilePos::new(1, 2), "build()\n");
    for _ in 0..20 {
        sim.step();
    }
    assert_eq!(sim.world.paint.get(&pos), Some(&3), "the serviced designation recolors");
    let painted_hash = sim.state_hash();
    // Erasing is designating `unpainted` — labor again (Q96's eraser).
    sim.apply(&Command::PlacePaint { pos, color: None, faction: 0 }).unwrap();
    for _ in 0..20 {
        sim.step();
    }
    assert!(sim.world.paint.is_empty(), "unpainted designation erases");
    assert_ne!(sim.state_hash(), painted_hash, "paint is shared, replayed state");
    let _ = bot;
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
    assert!(logs.iter().any(|l| l.1 == "2"), "live bot must hot-swap; logs: {logs:?}");
}

#[test]
fn exists_blueprint_predicate() {
    let mut sim = Sim::new(&walled_map());
    let bot = spawn(&mut sim, TilePos::new(1, 1), "log(exists(blueprint))\nwait(2)\n");
    for _ in 0..12 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&bot].data.log_buf.first().map(|l| l.1.as_str()), Some("False"));
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(4, 1),
        kind: BlueprintKind::Bridge,
        faction: 0,
    })
    .unwrap();
    for _ in 0..30 {
        sim.step();
    }
    assert!(
        sim.world.bots[&bot].data.log_buf.iter().any(|l| l.1 == "True"),
        "predicate must flip once a blueprint exists: {:?}",
        sim.world.bots[&bot].data.log_buf
    );
}

#[test]
fn prebuilt_bridges_are_passable_from_the_start() {
    let mut spec = walled_map();
    spec.bridges.push(TilePos::new(4, 2));
    let mut sim = Sim::new(&spec);
    spawn(&mut sim, TilePos::new(1, 2), "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n");
    for _ in 0..400 {
        sim.step();
    }
    assert!(sim.world.stock_get(0, sim::resources::Resource::Iron) > 20, "miner crosses immediately; stockpile {}", sim.world.stock_get(0, sim::resources::Resource::Iron));
}

#[test]
fn kill_command_wrecks_bot_and_spills_cargo() {
    let mut sim = Sim::new(&MapSpec::empty(6, 6));
    let bot = spawn(&mut sim, TilePos::new(3, 3), "log(7)\nwait(3)\n");
    for _ in 0..6 {
        sim.step();
    }
    // Give it cargo by hand, then pull the plug.
    sim.world.bots.get_mut(&bot).unwrap().data.cargo = [(sim::resources::Resource::Iron, 20u32)].into_iter().collect();
    sim.apply(&Command::KillBot { bot }).unwrap();
    sim.step();

    assert!(!sim.world.bots.contains_key(&bot), "killed bot is gone");
    let wreck = sim.world.wrecks.get(&bot).expect("manual kill leaves a wreck");
    assert!(wreck.data.log_buf.iter().any(|l| l.1 == "7"), "logs ride in the wreck");
    let spilled: u32 = sim
        .world
        .nodes
        .values()
        .filter(|n| n.pos.chebyshev(TilePos::new(3, 3)) <= 1)
        .map(|n| n.amount)
        .sum();
    assert_eq!(spilled, 20, "cargo (2 units = 20 deci) spills next to the wreck");
}

#[test]
fn spilled_cargo_is_recoverable_by_miners() {
    // A dead hauler's spilled ore is just an ore node: another bot mines
    // it and delivers — closest(ore) finds it, no new mechanics needed.
    let mut spec = MapSpec::empty(8, 4);
    spec.depots.push((TilePos::new(0, 1), 0));
    let mut sim = Sim::new(&spec);
    let mule = spawn(&mut sim, TilePos::new(6, 2), "wait(50)\n");
    sim.world.bots.get_mut(&mule).unwrap().data.cargo = [(sim::resources::Resource::Iron, 20u32)].into_iter().collect();
    sim.apply(&Command::KillBot { bot: mule }).unwrap();
    sim.step();
    spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n",
    );
    for _ in 0..400 {
        sim.step();
    }
    assert!(sim.world.stock_get(0, sim::resources::Resource::Iron) >= 20, "spilled cargo (2 units = 20 deci) makes it home; got {}", sim.world.stock_get(0, sim::resources::Resource::Iron));
}

#[test]
fn repairing_a_full_target_earns_no_building_xp() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    spec.structures.push((TilePos::new(3, 1), sim::world::StructureKind::Smelter));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // The smelter is at FULL hp from birth: looping repair() on it must
    // mint nothing (review 2026-07-16: the per-tick grant paid before
    // checking whether any mending happened).
    let medic = spawn(
        &mut sim,
        TilePos::new(2, 1),
        "repair(closest(smelter).expect())\nwait(2)\n",
    );
    for _ in 0..60 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&medic].data.xp(sim::world::XpTrack::Building),
        0,
        "no work done, no Building XP"
    );
}

// --- M16 review: structure designations ----------------------------

/// A structure blueprint must cost the same no matter which command
/// variant wraps it. `cost_stone` is 0 for structures (their prices are
/// TYPED), so the generic Stone charge in `PlaceBlueprint` handed out free
/// Foundries to anyone who routed around `PlaceStructure`.
#[test]
fn place_blueprint_cannot_launder_a_free_structure() {
    let mut sim = Sim::new(&MapSpec::empty(8, 8));
    let site = TilePos::new(4, 4);
    // Empty stock: the faction cannot afford a Smelter's 10 Steel.
    sim.apply(&Command::PlaceBlueprint {
        pos: site,
        kind: BlueprintKind::Structure(sim::world::StructureKind::Smelter),
        faction: 0,
    })
    .unwrap();
    assert!(
        sim.world.blueprints.is_empty(),
        "an unaffordable structure must not be designated through PlaceBlueprint"
    );

    // With the materials, the SAME command works and actually charges.
    sim.world.stock_add(0, sim::resources::Resource::Steel, 10 * sim::resources::DECI as u64);
    sim.apply(&Command::PlaceBlueprint {
        pos: site,
        kind: BlueprintKind::Structure(sim::world::StructureKind::Smelter),
        faction: 0,
    })
    .unwrap();
    assert_eq!(sim.world.blueprints.len(), 1, "affordable: designated");
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Steel),
        0,
        "and the typed price was actually taken"
    );
}

/// A designation charges up front, so calling it off has to give the
/// materials back — otherwise one click on an unreachable tile destroys
/// them and bricks the ground forever.
#[test]
fn cancelling_a_structure_designation_refunds_it() {
    let mut sim = Sim::new(&MapSpec::empty(8, 8));
    let site = TilePos::new(4, 4);
    let steel = 10 * sim::resources::DECI as u64;
    sim.world.stock_add(0, sim::resources::Resource::Steel, steel);
    sim.apply(&Command::PlaceStructure {
        pos: site,
        kind: sim::world::StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    assert_eq!(sim.world.stock_get(0, sim::resources::Resource::Steel), 0, "charged");

    // A rival faction cannot cancel it.
    sim.apply(&Command::CancelBlueprint { pos: site, faction: 1 }).unwrap();
    assert_eq!(sim.world.blueprints.len(), 1, "only the owner may cancel");

    sim.apply(&Command::CancelBlueprint { pos: site, faction: 0 }).unwrap();
    assert!(sim.world.blueprints.is_empty(), "the designation is gone");
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Steel),
        steel,
        "and every unit came back"
    );
    // The tile is free to designate again.
    assert!(sim.world.blueprint_site_ok(
        BlueprintKind::Structure(sim::world::StructureKind::Smelter),
        site
    ));
}

/// Cancelling must not be able to yank a job out from under a crew that
/// has already put labor in: `build()`'s missing-blueprint arm reads a
/// vanished designation as "someone else finished it" and returns Ok, so
/// the bot would be told a structure exists that was never raised.
#[test]
fn cancel_refuses_a_designation_already_under_construction() {
    let mut spec = MapSpec::empty(9, 5);
    spec.depots.push((TilePos::new(0, 2), 0));
    spec.starting_stock.push((0, sim::resources::Resource::Steel, 20));
    let mut sim = Sim::new(&spec);
    let site = TilePos::new(5, 2);
    spawn(&mut sim, TilePos::new(1, 2), BUILDER);
    sim.apply(&Command::PlaceStructure {
        pos: site,
        kind: sim::world::StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();

    // Let the builder walk over and actually start swinging.
    for _ in 0..200 {
        sim.step();
        if sim.world.blueprints.values().any(|b| b.progress > 0) {
            break;
        }
    }
    assert!(
        sim.world.blueprints.values().any(|b| b.progress > 0),
        "the crew has invested labor"
    );

    let steel = sim.world.stock_get(0, sim::resources::Resource::Steel);
    sim.apply(&Command::CancelBlueprint { pos: site, faction: 0 }).unwrap();
    assert_eq!(sim.world.blueprints.len(), 1, "a started job cannot be cancelled");
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Steel),
        steel,
        "and nothing was refunded for it"
    );

    // It still completes normally.
    for _ in 0..400 {
        sim.step();
        if sim.world.structures.values().any(|s| s.pos == site) {
            break;
        }
    }
    assert!(
        sim.world.structures.values().any(|s| s.pos == site),
        "the Smelter the crew was building still gets raised"
    );
}

/// M16's headline rule is "every structure is BUILT BY LABOR" (Q105), and
/// until now nothing tested it end to end: the designation tests stop at
/// pricing, and every structure test elsewhere skips construction via
/// `finish_structure_for_test`. So the whole completion arm — site
/// re-check, refund, `structures.insert` — shipped with no coverage.
#[test]
fn a_bot_raises_a_designated_structure_by_labor() {
    let mut spec = MapSpec::empty(9, 5);
    spec.depots.push((TilePos::new(0, 2), 0));
    spec.starting_stock.push((0, sim::resources::Resource::Steel, 20));
    let mut sim = Sim::new(&spec);
    let site = TilePos::new(5, 2);
    let builder = spawn(&mut sim, TilePos::new(1, 2), BUILDER);

    sim.apply(&Command::PlaceStructure {
        pos: site,
        kind: sim::world::StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    assert_eq!(sim.world.blueprints.len(), 1, "designated, not raised");
    assert!(
        !sim.world.structures.values().any(|s| s.pos == site),
        "nothing appears on the click — labor is code"
    );

    for _ in 0..400 {
        sim.step();
        if sim.world.structures.values().any(|s| s.pos == site) {
            break;
        }
    }
    let raised = sim
        .world
        .structures
        .values()
        .find(|s| s.pos == site)
        .expect("the builder must actually raise the Smelter");
    assert_eq!(raised.kind, sim::world::StructureKind::Smelter);
    assert_eq!(raised.faction, 0, "the DESIGNATING faction owns it");
    assert_eq!(raised.hp, sim.tuning.structure_hp, "raised at full HP");
    assert!(sim.world.blueprints.is_empty(), "designation consumed");
    assert!(
        sim.world.bots[&builder].data.xp(sim::world::XpTrack::Building) > 0,
        "and the labor paid Building XP"
    );
}

/// A pending designation is not solid — nothing stops a bot walking over
/// it, or parking on it. Completing the build must cope with that rather
/// than destroying the job: the completion-time occupancy check was passed
/// `BotId(u32::MAX)`, a sentinel that excludes NOBODY, so any bot on the
/// tile (including the builder itself) voided the designation outright and
/// the up-front materials went back but ~25 ticks of labor did not.
#[test]
fn a_bot_parked_on_the_site_does_not_destroy_the_designation() {
    let mut spec = MapSpec::empty(9, 5);
    spec.depots.push((TilePos::new(0, 2), 0));
    spec.starting_stock.push((0, sim::resources::Resource::Steel, 20));
    let mut sim = Sim::new(&spec);
    let site = TilePos::new(5, 2);
    let builder = spawn(&mut sim, TilePos::new(1, 2), BUILDER);

    sim.apply(&Command::PlaceStructure {
        pos: site,
        kind: sim::world::StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    assert_eq!(sim.world.blueprints.len(), 1, "designated");
    // A bystander then parks ON the tile and never moves. Designation-time
    // occupancy already refuses a taken tile, so the interesting case is a
    // bot that ARRIVES afterwards — which nothing prevents, since pending
    // designations are not solid.
    let idler = spawn(&mut sim, site, "wait(900)\n");

    for _ in 0..200 {
        sim.step();
    }
    // The build must HOLD, not vanish: the site is only temporarily taken.
    assert_eq!(
        sim.world.blueprints.len(),
        1,
        "an occupied site must not delete the player's designation"
    );
    assert!(
        !sim.world.structures.values().any(|s| s.pos == site),
        "and must not entomb the bot standing there either"
    );

    // Move the bystander off; the held build then completes.
    sim.apply(&Command::KillBot { bot: idler }).unwrap();
    sim.world.wrecks.clear();
    for _ in 0..400 {
        sim.step();
        if sim.world.structures.values().any(|s| s.pos == site) {
            break;
        }
    }
    assert!(
        sim.world.structures.values().any(|s| s.pos == site),
        "once the site clears, the same designation finishes"
    );
    assert!(
        sim.world.bots[&builder].data.xp(sim::world::XpTrack::Building) > 0,
        "the builder's labor counted"
    );
}

/// A structure's typed cost is charged AT DESIGNATION, so if the ground
/// stops being buildable while the crew works, the materials have to come
/// back. They used to vanish: the blueprint was removed before the
/// `site_ok` re-check, the refund lived only in the occupancy branch, and
/// the action still reported `Ok` — so a Vent overrun by creep mid-build
/// silently ate the whole Geothermal Tap.
#[test]
fn an_unbuildable_site_refunds_instead_of_eating_the_materials() {
    let mut spec = MapSpec::empty(9, 5);
    spec.depots.push((TilePos::new(0, 2), 0));
    spec.starting_stock.push((0, sim::resources::Resource::Steel, 20));
    let site = TilePos::new(5, 2);
    spec.resource_tiles.push((site, TileKind::Vent));
    let mut sim = Sim::new(&spec);
    spawn(&mut sim, TilePos::new(1, 2), BUILDER);

    let before = sim.world.stock_get(0, sim::resources::Resource::Steel);
    sim.apply(&Command::PlaceStructure {
        pos: site,
        kind: sim::world::StructureKind::GeothermalTap,
        faction: 0,
    })
    .unwrap();
    let charged = before - sim.world.stock_get(0, sim::resources::Resource::Steel);
    assert!(charged > 0, "the Tap's typed cost was taken up front");

    // The Vent is lost while the build runs (creep, another crew's works).
    for _ in 0..12 {
        sim.step();
    }
    sim.set_tile_for_test(site, TileKind::Plains);

    for _ in 0..400 {
        sim.step();
        if sim.world.blueprints.is_empty() {
            break;
        }
    }
    assert!(sim.world.blueprints.is_empty(), "the build resolved");
    assert!(
        !sim.world.structures.values().any(|s| s.pos == site),
        "no Tap on ground that cannot host one"
    );
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Steel),
        before,
        "and every unit came back rather than evaporating"
    );
}
