//! Terrain v2 & terraforming (M8, docs/05): the ×2 cost scale and edge
//! costs, the misbehaving tiles (Ice, Dunes, Scree, Ford), Corruption
//! dynamics (tax + Blight Cores), and the terraform blueprint set.

use sim::map::{edge_allowed, MapSpec, TileKind};
use sim::sim::{Command, Sim};
use sim::stats;
use sim::world::{BlueprintKind, Color};
use sim::TilePos;
use std::collections::{BTreeMap, BTreeSet};

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

const IDLER: &str = "log(1)\n";

// ------------------------------------------------------------ edge costs

#[test]
fn step_ticks_follow_the_x2_cost_table() {
    let spec = MapSpec::empty(10, 6);
    let mut sim = Sim::new(&spec);
    let g = &mut sim.world.grid;
    g.set(TilePos::new(2, 1), TileKind::Road);
    g.set(TilePos::new(3, 1), TileKind::Ford);
    g.set(TilePos::new(4, 1), TileKind::Mud);
    let id = spawn(&mut sim, TilePos::new(1, 1), IDLER);

    let step = |sim: &Sim, id: &sim::BotId, to: TilePos| {
        stats::step_ticks(sim.ctx(), &sim.world.grid, &sim.world.bots[id].data, to)
    };
    // 140 deci-ticks/tile × (cost_x2 / 2): Road halves, Ford quadruples.
    assert_eq!(
        step(&sim, &id, TilePos::new(1, 2)).unwrap(),
        14,
        "plains stays the M5 pace (the ×2 scale is transparent)"
    );
    assert_eq!(step(&sim, &id, TilePos::new(2, 1)).unwrap(), 7, "road is half plains");
    assert_eq!(step(&sim, &id, TilePos::new(3, 1)).unwrap(), 56, "ford is 4x");

    // Mud: 3× empty, 4× loaded (per-bot state rides step_ticks, not A*).
    let miner = spawn(&mut sim, TilePos::new(5, 1), IDLER);
    assert_eq!(step(&sim, &miner, TilePos::new(4, 1)).unwrap(), 42, "mud empty = 3x");
    sim.world
        .bots
        .get_mut(&miner)
        .unwrap()
        .data
        .cargo
        .insert(sim::resources::Resource::Stone, 10);
    assert_eq!(step(&sim, &miner, TilePos::new(4, 1)).unwrap(), 56, "mud loaded = 4x");
}

#[test]
fn mountain_edges_price_the_climb_not_the_tile() {
    let spec = MapSpec::empty(10, 6);
    let mut sim = Sim::new(&spec);
    let g = &mut sim.world.grid;
    g.set(TilePos::new(2, 2), TileKind::Mountain);
    g.set(TilePos::new(3, 2), TileKind::Mountain);

    let step = |sim: &Sim, id: &sim::BotId, to: TilePos| {
        stats::step_ticks(sim.ctx(), &sim.world.grid, &sim.world.bots[id].data, to).unwrap()
    };
    let climber = spawn(&mut sim, TilePos::new(1, 2), IDLER);
    assert_eq!(step(&sim, &climber, TilePos::new(2, 2)), 42, "climbing on: 6/2 = 3x");
    let ridge_runner = spawn(&mut sim, TilePos::new(2, 2), IDLER);
    assert_eq!(step(&sim, &ridge_runner, TilePos::new(3, 2)), 14, "ridge-running: 1x");
    let descender = spawn(&mut sim, TilePos::new(3, 2), IDLER);
    assert_eq!(step(&sim, &descender, TilePos::new(4, 2)), 28, "descending off: 4/2 = 2x");
}

#[test]
fn high_ground_is_entered_only_via_ramp_or_mountain() {
    let spec = MapSpec::empty(8, 4);
    let mut sim = Sim::new(&spec);
    let g = &mut sim.world.grid;
    let hg = TilePos::new(3, 1);
    g.set(hg, TileKind::HighGround);
    g.set(TilePos::new(2, 1), TileKind::Ramp);
    g.set(TilePos::new(3, 2), TileKind::Mountain);
    // (4,1) stays Plains.
    let overlays = BTreeMap::new();
    let g = &sim.world.grid;
    assert!(!edge_allowed(g, &overlays, TilePos::new(4, 1), hg), "plains -> mesa: walled");
    assert!(edge_allowed(g, &overlays, TilePos::new(2, 1), hg), "ramp -> mesa: the doorway");
    assert!(edge_allowed(g, &overlays, TilePos::new(3, 2), hg), "mountain -> mesa: soft slope");
    assert!(edge_allowed(g, &overlays, hg, TilePos::new(2, 1)), "mesa -> ramp: back down");
    assert!(!edge_allowed(g, &overlays, hg, TilePos::new(4, 1)), "mesa -> plains: no cliff dives");
}

#[test]
fn astar_detours_onto_roads_when_cheaper() {
    let spec = MapSpec::empty(6, 3);
    let mut sim = Sim::new(&spec);
    for x in 0..6 {
        sim.world.grid.set(TilePos::new(x, 1), TileKind::Road);
    }
    // Direct along y=0: 5 plains steps = 10. Via the road: down (1) +
    // 4 road steps (4) + up (2) = 7.
    let goals: BTreeSet<TilePos> = [TilePos::new(5, 0)].into();
    let path = sim::map::astar(
        &sim.world.grid,
        &sim.world.overlays,
        &sim.tuning.tile_costs,
        TilePos::new(0, 0),
        &goals,
    )
    .expect("route exists");
    assert!(
        path.iter().any(|p| p.y == 1),
        "the planner must take the road: {path:?}"
    );
}

// ------------------------------------------------------- misbehaving tiles

#[test]
fn ice_slides_carry_bots_past_their_turn() {
    // Corridor along y=2 walled so the route turns south at x=3 — but
    // (4,2) and (3,2) are ice: momentum carries the mover W onto (2,2),
    // which no un-slid plan ever visits, and the walk replans from there.
    let mut spec = MapSpec::empty(7, 6);
    for x in 2..7 {
        spec.water.push(TilePos::new(x, 1));
    }
    for x in 4..7 {
        spec.water.push(TilePos::new(x, 3));
    }
    spec.depots.push((TilePos::new(3, 4), 0));
    let mut sim = Sim::new(&spec);
    sim.world.grid.set(TilePos::new(4, 2), TileKind::Ice);
    sim.world.grid.set(TilePos::new(3, 2), TileKind::Ice);
    let mover = spawn(&mut sim, TilePos::new(6, 2), "move_to(closest(depot).expect())\nwait(500)\n");

    let mut overshot = false;
    for _ in 0..400 {
        sim.step();
        if sim.world.bots[&mover].data.pos == TilePos::new(2, 2) {
            overshot = true;
        }
    }
    assert!(overshot, "entering ice heading W must slide the bot past its planned turn");
    let arrived = sim.world.bots[&mover].data.pos;
    assert!(
        arrived.chebyshev(TilePos::new(3, 4)) <= 1,
        "the replanned walk still reaches the depot (at {arrived:?})"
    );
}

#[test]
fn dunes_swallow_idlers() {
    let spec = MapSpec::empty(8, 4);
    let mut sim = Sim::new(&spec);
    sim.world.grid.set(TilePos::new(2, 2), TileKind::Dunes);
    let id = spawn(&mut sim, TilePos::new(2, 2), IDLER);
    let exit = TilePos::new(1, 2);

    let fresh = stats::step_ticks(sim.ctx(), &sim.world.grid, &sim.world.bots[&id].data, exit)
        .unwrap();
    assert_eq!(fresh, 14, "a fresh exit prices the plains it steps onto");
    for _ in 0..65 {
        sim.step();
    }
    let idle = sim.world.bots[&id].data.dune_idle;
    assert!(idle >= 60, "standing still on sand sinks: {idle}");
    let sunk = stats::step_ticks(sim.ctx(), &sim.world.grid, &sim.world.bots[&id].data, exit)
        .unwrap();
    assert!(sunk > fresh, "the exit step must cost more after idling ({sunk} vs {fresh})");

    // The surcharge caps: buried, never trapped.
    sim.world.bots.get_mut(&id).unwrap().data.dune_idle = 100_000;
    let capped = stats::step_ticks(sim.ctx(), &sim.world.grid, &sim.world.bots[&id].data, exit)
        .unwrap();
    let cap_cost = (2 + sim.tuning.dune_sink_cap_x2) as u64; // plains 2 + capped sink
    assert_eq!(capped as u64, (140 * cap_cost).div_ceil(20), "sink surcharge is capped");
}

#[test]
fn moving_shakes_the_sand_off() {
    let mut spec = MapSpec::empty(8, 3);
    spec.depots.push((TilePos::new(6, 1), 0));
    let mut sim = Sim::new(&spec);
    sim.world.grid.set(TilePos::new(2, 1), TileKind::Dunes);
    let id = spawn(&mut sim, TilePos::new(2, 1), "wait(30)\nmove_to(closest(depot).expect())\nwait(500)\n");
    for _ in 0..40 {
        sim.step();
    }
    assert!(sim.world.bots[&id].data.dune_idle > 0, "the wait sank in");
    for _ in 0..200 {
        sim.step();
    }
    let bot = &sim.world.bots[&id].data;
    assert_ne!(bot.pos, TilePos::new(2, 1), "the walk happened");
    assert_eq!(bot.dune_idle, 0, "every move resets the sink counter");
}

#[test]
fn scree_collapses_to_rubble_after_enough_crossings() {
    // A miner shuttles ore across the only crossing — a scree tile —
    // wearing it down to rubble.
    let mut spec = MapSpec::empty(8, 3);
    for x in 0..8 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push((TilePos::new(0, 1), 0));
    spec.ore_nodes.push((TilePos::new(6, 1), 50));
    let mut sim = Sim::new(&spec);
    sim.world.grid.set(TilePos::new(3, 1), TileKind::Scree);
    sim.tuning.scree_crossings = 3;
    let site = TilePos::new(3, 1);
    spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n",
    );
    let mut collapsed_at = None;
    for tick in 0..3000 {
        sim.step();
        if sim.world.grid.get(site) == Some(TileKind::Rubble) {
            collapsed_at = Some(tick);
            break;
        }
    }
    assert!(collapsed_at.is_some(), "three crossings must collapse the scree");
    assert!(
        !sim.world.scree_wear.contains_key(&site),
        "the collapsed tile's wear counter is dropped"
    );
}

#[test]
fn ford_wading_quiets_the_mover() {
    // Same walk twice: the listener hears the walker exactly at hearing
    // range (d=3) on plains; with a ford under that step, ford_quiet
    // shrinks heard-at below it and the step goes silent.
    // The depot (the walk's target) sits far east: depots carry faction-0
    // structure eyes, so it must be nowhere near the sample point or its
    // SEEING swallows the listener's hearing test (seen beats heard).
    let heard_at_three = |ford: bool| -> bool {
        // One row: A*'s tie-breaks would otherwise let the walker pass
        // the ford's COLUMN on the row beside it.
        let mut spec = MapSpec::empty(16, 1);
        spec.depots.push((TilePos::new(15, 0), 0));
        let mut sim = Sim::new(&spec);
        if ford {
            sim.world.grid.set(TilePos::new(3, 0), TileKind::Ford);
        }
        sim.stats.sensors = 2; // seeing 2, hearing 2 * 150% = 3
        let listener = spawn(&mut sim, TilePos::new(0, 0), IDLER);
        let _ = listener;
        let walker = sim
            .apply(&Command::SpawnBot {
                pos: TilePos::new(2, 0),
                source: "move_to(closest(depot).expect())\nwait(500)\n".into(),
                cpu: 4,
                cargo_cap: 1,
                faction: 1,
                hp: 100,
                color: Color::GREEN,
            })
            .unwrap()
            .unwrap();
        let entity = sim.world.bots[&walker].data.entity;
        let mut heard = false;
        for _ in 0..400 {
            sim.step();
            let data = &sim.world.bots[&walker].data;
            if TilePos::new(0, 0).chebyshev(data.pos) == 3
                && sim
                    .world
                    .perception
                    .get(&0)
                    .is_some_and(|p| p.heard.contains_key(&entity))
            {
                heard = true;
            }
        }
        heard
    };
    assert!(heard_at_three(false), "a mover at hearing range on plains is heard");
    assert!(!heard_at_three(true), "the same step wading a ford is quiet");
}

// ------------------------------------------------------------- corruption

#[test]
fn corruption_taxes_every_charged_op() {
    let mut spec = MapSpec::empty(8, 4);
    spec.corruption.push(TilePos::new(5, 2));
    let mut sim = Sim::new(&spec);
    // cpu: 1 cycle/tick. Each `log(1)` iteration costs log (1cy) + the
    // wrap statement (1cy) = 2cy on plains, 4cy under the +1/op tax.
    let clean = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: IDLER.into(),
            cpu: 1,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    let taxed = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(5, 2),
            source: IDLER.into(),
            cpu: 1,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    for _ in 0..12 {
        sim.step();
    }
    let clean_logs = sim.world.bots[&clean].data.log_buf.len();
    let taxed_logs = sim.world.bots[&taxed].data.log_buf.len();
    assert!(taxed_logs >= 1, "taxed bots still think, just slower");
    assert!(
        taxed_logs < clean_logs,
        "the same program must run slower on corruption ({taxed_logs} vs {clean_logs})"
    );
}

#[test]
fn blight_cores_spread_and_die() {
    let mut spec = MapSpec::empty(9, 5);
    spec.blight_cores.push((TilePos::new(4, 2), 3, 30));
    let mut sim = Sim::new(&spec);
    sim.tuning.corruption_spread_ticks = 10;
    assert_eq!(
        sim.world.grid.get(TilePos::new(4, 2)),
        Some(TileKind::Corruption),
        "the core squats on corrupted ground from tick 0"
    );

    for _ in 0..10 {
        sim.step();
    }
    // Nearest clean passable tile by (chebyshev, y, x): the d=1 ring's
    // smallest y then x — (3,1).
    assert_eq!(
        sim.world.grid.get(TilePos::new(3, 1)),
        Some(TileKind::Corruption),
        "the creep front advances deterministically"
    );

    // Re-corruption: cleansed ground inside the radius is simply the
    // nearest clean tile again while the source lives.
    sim.world.grid.set(TilePos::new(3, 1), TileKind::Plains);
    for _ in 0..10 {
        sim.step();
    }
    assert_eq!(
        sim.world.grid.get(TilePos::new(3, 1)),
        Some(TileKind::Corruption),
        "a living core re-corrupts cleansed ground"
    );

    // Kill the source: the creep stops advancing (what exists, stays).
    let hunter = spawn(&mut sim, TilePos::new(3, 2), "attack(closest(blight).expect())\n");
    let _ = hunter;
    for _ in 0..60 {
        sim.step();
        if sim.world.blight_cores.is_empty() {
            break;
        }
    }
    assert!(sim.world.blight_cores.is_empty(), "cores are attackable and killable");
    sim.world.grid.set(TilePos::new(3, 1), TileKind::Plains);
    for _ in 0..40 {
        sim.step();
    }
    assert_eq!(
        sim.world.grid.get(TilePos::new(3, 1)),
        Some(TileKind::Plains),
        "with the source dead, cleansed ground stays clean"
    );
}

// ---------------------------------------------------------- terraforming

const BUILDER: &str = "\
move_to(closest(blueprint).expect())
build()
move_to(closest(depot).expect())
";

fn terraform_map() -> MapSpec {
    let mut spec = MapSpec::empty(9, 5);
    spec.depots.push((TilePos::new(0, 2), 0));
    spec.starting_stock.push((0, sim::resources::Resource::Stone, 20));
    spec
}

/// Run the sim until `site` becomes `want` (or panic after the budget).
fn build_until(sim: &mut Sim, site: TilePos, want: TileKind) {
    for _ in 0..600 {
        sim.step();
        if sim.world.grid.get(site) == Some(want) {
            return;
        }
    }
    panic!("site {site:?} never became {want:?} (now {:?})", sim.world.grid.get(site));
}

#[test]
fn clear_yields_stone_from_rubble() {
    let mut sim = Sim::new(&terraform_map());
    let site = TilePos::new(4, 2);
    sim.world.grid.set(site, TileKind::Rubble);
    spawn(&mut sim, TilePos::new(2, 2), BUILDER);
    let before = sim.world.stock_get(0, sim::resources::Resource::Stone);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Clear, faction: 0 })
        .unwrap();
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Stone),
        before,
        "clearing is labor-only: placement charges nothing"
    );
    build_until(&mut sim, site, TileKind::Plains);
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Stone),
        before + sim.tuning.clear_yield_stone,
        "cleared rubble pays out Stone"
    );
}

#[test]
fn barricade_walls_movement_and_sight_and_demolish_undoes_it() {
    let mut sim = Sim::new(&terraform_map());
    let site = TilePos::new(4, 2);
    spawn(&mut sim, TilePos::new(2, 2), BUILDER);
    let before = sim.world.stock_get(0, sim::resources::Resource::Stone);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Barricade, faction: 0 })
        .unwrap();
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Stone),
        before - sim.tuning.barricade_cost_stone,
        "walls price in Stone"
    );
    build_until(&mut sim, site, TileKind::Barricade);

    // Solid: no edge admits it. Opaque: LoS through it is cut both ways.
    assert!(!edge_allowed(
        &sim.world.grid,
        &sim.world.overlays,
        TilePos::new(3, 2),
        site
    ));
    assert!(!sim::perception::los_clear(
        &sim.world.grid,
        TilePos::new(2, 2),
        TilePos::new(6, 2),
        false,
    ));
    assert!(
        !sim::perception::los_clear(
            &sim.world.grid,
            TilePos::new(2, 2),
            TilePos::new(6, 2),
            true,
        ),
        "built mass blocks even elevated eyes (it is not elevation)"
    );

    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Demolish, faction: 0 })
        .unwrap();
    build_until(&mut sim, site, TileKind::Plains);
}

#[test]
fn demolish_returns_a_bridge_to_water() {
    let mut spec = terraform_map();
    spec.water.push(TilePos::new(4, 2));
    spec.bridges.push(TilePos::new(4, 2));
    let mut sim = Sim::new(&spec);
    let site = TilePos::new(4, 2);
    spawn(&mut sim, TilePos::new(2, 2), BUILDER);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Demolish, faction: 0 })
        .unwrap();
    build_until(&mut sim, site, TileKind::Water);
}

#[test]
fn cleanse_and_road_pave_the_ground() {
    let mut sim = Sim::new(&terraform_map());
    let creep = TilePos::new(4, 2);
    let pave = TilePos::new(5, 2);
    sim.world.grid.set(creep, TileKind::Corruption);
    spawn(&mut sim, TilePos::new(2, 2), BUILDER);
    sim.apply(&Command::PlaceBlueprint { pos: creep, kind: BlueprintKind::Cleanse, faction: 0 })
        .unwrap();
    build_until(&mut sim, creep, TileKind::Plains);

    sim.apply(&Command::PlaceBlueprint { pos: pave, kind: BlueprintKind::Road, faction: 0 })
        .unwrap();
    build_until(&mut sim, pave, TileKind::Road);
    let id = spawn(&mut sim, TilePos::new(5, 1), IDLER);
    assert_eq!(
        stats::step_ticks(sim.ctx(), &sim.world.grid, &sim.world.bots[&id].data, pave).unwrap(),
        7,
        "the paved tile moves at road pace"
    );
}

#[test]
fn terraform_sites_are_validated() {
    let mut sim = Sim::new(&terraform_map());
    let plains = TilePos::new(4, 2);
    // Clear needs rubble; Cleanse needs corruption; Demolish needs works.
    for kind in [BlueprintKind::Clear, BlueprintKind::Cleanse, BlueprintKind::Demolish] {
        sim.apply(&Command::PlaceBlueprint { pos: plains, kind, faction: 0 }).unwrap();
    }
    assert!(sim.world.blueprints.is_empty(), "wrong ground: all rejected");
    // Building under a structure is refused (walling a depot in stone).
    sim.apply(&Command::PlaceBlueprint {
        pos: TilePos::new(0, 2),
        kind: BlueprintKind::Barricade,
        faction: 0,
    })
    .unwrap();
    assert!(sim.world.blueprints.is_empty(), "structure tiles refuse terraform works");
}

// -------------------------------------------------- review 2026-07-16 fixes

#[test]
fn ground_hardening_under_a_plan_replans_instead_of_panicking() {
    // A barricade lands on the mover's only route mid-walk: the step must
    // re-plan (and fault "unreachable"), never panic on the unpriced tile
    // or walk through the new wall.
    let mut spec = MapSpec::empty(8, 3);
    for x in 0..8 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push((TilePos::new(0, 1), 0));
    spec.starting_stock.push((0, sim::resources::Resource::Stone, 20));
    let mut sim = Sim::new(&spec);
    let wall = TilePos::new(3, 1);
    let builder = spawn(&mut sim, TilePos::new(2, 1), BUILDER);
    let _ = builder;
    // The handler matters: once the wall lands, move_to faults
    // "unreachable" every loop — unhandled, that crash loop would chip
    // the mover to death (docs/01), which is correct but not this test.
    let mover = spawn(
        &mut sim,
        TilePos::new(6, 1),
        "move_to(closest(depot).expect())\nwait(500)\non error:\n  wait(50)\n",
    );
    sim.apply(&Command::PlaceBlueprint { pos: wall, kind: BlueprintKind::Barricade, faction: 0 })
        .unwrap();
    for _ in 0..400 {
        sim.step();
        let pos = sim.world.bots[&mover].data.pos;
        let tile = sim.world.grid.get(pos).unwrap();
        assert_ne!(tile, TileKind::Water, "never walk into water");
        assert_ne!(tile, TileKind::Barricade, "never walk into the wall");
    }
    assert_eq!(sim.world.grid.get(wall), Some(TileKind::Barricade), "the wall landed");
    assert!(
        sim.world.bots[&mover].data.pos.x >= 4,
        "the mover is stuck on its own side of the wall, alive"
    );
}

#[test]
fn corruption_spares_ramps_and_roads() {
    let mut spec = MapSpec::empty(9, 5);
    spec.blight_cores.push((TilePos::new(4, 2), 2, 1000));
    let mut sim = Sim::new(&spec);
    let ramp = TilePos::new(3, 2);
    let road = TilePos::new(5, 2);
    sim.world.grid.set(ramp, TileKind::Ramp);
    sim.world.grid.set(road, TileKind::Road);
    sim.tuning.corruption_spread_ticks = 5;
    for _ in 0..300 {
        sim.step();
    }
    assert_eq!(sim.world.grid.get(ramp), Some(TileKind::Ramp), "the plateau doorway survives");
    assert_eq!(sim.world.grid.get(road), Some(TileKind::Road), "paid civil works survive");
    assert_eq!(
        sim.world.grid.get(TilePos::new(3, 1)),
        Some(TileKind::Corruption),
        "ordinary ground still corrupts"
    );
}

#[test]
fn nothing_materializes_on_high_ground() {
    let spec = MapSpec::empty(8, 6);
    let mut sim = Sim::new(&spec);
    let hg = TilePos::new(3, 2);
    sim.world.grid.set(hg, TileKind::HighGround);
    // Dev spawn straight onto the plateau: rejected.
    let rejected = sim
        .apply(&Command::SpawnBot {
            pos: hg,
            source: IDLER.into(),
            cpu: 4,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap();
    assert!(rejected.is_none(), "spawns may not materialize on High Ground");
    // Print placement skips plateau tiles: ring the center with mesa,
    // leave one plains gap — the pick lands there.
    let center = TilePos::new(2, 4);
    for dy in -1..=1 {
        for dx in -1..=1 {
            sim.world.grid.set(TilePos::new(center.x + dx, center.y + dy), TileKind::HighGround);
        }
    }
    let gap = TilePos::new(3, 5); // ORDER offset (1,1)
    sim.world.grid.set(gap, TileKind::Plains);
    assert_eq!(
        sim.world.free_spawn_tile(center),
        Some(gap),
        "free_spawn_tile must skip the plateau"
    );
}

#[test]
fn a_build_voided_by_changed_ground_stamps_nothing() {
    let mut sim = Sim::new(&terraform_map());
    let site = TilePos::new(4, 2);
    spawn(&mut sim, TilePos::new(2, 2), BUILDER);
    sim.apply(&Command::PlaceBlueprint { pos: site, kind: BlueprintKind::Road, faction: 0 })
        .unwrap();
    // Corruption claims the site mid-build (what spread does).
    sim.world.grid.set(site, TileKind::Corruption);
    for _ in 0..400 {
        sim.step();
        if sim.world.blueprints.is_empty() {
            break;
        }
    }
    assert!(sim.world.blueprints.is_empty(), "the labor still completes");
    assert_eq!(
        sim.world.grid.get(site),
        Some(TileKind::Corruption),
        "a 10-tick road must not erase creep the 40-tick cleanse exists for"
    );
}

#[test]
fn blueprint_kind_is_pinned_by_the_state_hash() {
    // Two worlds identical except for the KIND of one in-progress
    // blueprint must hash apart immediately, not at completion.
    let build = |kind: BlueprintKind| -> u64 {
        let mut spec = terraform_map();
        spec.starting_stock.clear();
        let mut spec = spec;
        let site = TilePos::new(4, 2);
        spec.resource_tiles.clear();
        let mut sim = Sim::new(&spec);
        sim.world.grid.set(site, TileKind::Rubble);
        sim.tuning.road_cost_stone = 0; // isolate the kind: equal price,
        sim.tuning.road_build_ticks = sim.tuning.clear_ticks; // equal needed
        sim.apply(&Command::PlaceBlueprint { pos: site, kind, faction: 0 }).unwrap();
        assert_eq!(sim.world.blueprints.len(), 1, "placement accepted");
        sim.state_hash()
    };
    assert_ne!(
        build(BlueprintKind::Clear),
        build(BlueprintKind::Road),
        "kind divergence must desync NOW"
    );
}

#[test]
fn a_multi_tick_mover_stays_audible_between_tiles() {
    // Crossing Mud takes several ticks/tile: the mover advances its traverse
    // on the in-between ticks WITHOUT changing tile. docs/05 says a moving bot
    // registers as a contact, so it must stay heard throughout the crossing —
    // not flicker audible only on the tile-change ticks. (Regression: the
    // review found `moved_tick` was stamped only on tile change.)
    // The depot (the walk's target) sits far east: its structure-eyes would
    // otherwise SEE the walker in the sample zone (seen beats heard). Placed
    // at x=18 so at x=3 the walker is out of the depot's sight AND hearing —
    // a genuine heard-only-from-the-listener gap.
    let mut spec = MapSpec::empty(20, 1);
    spec.depots.push((TilePos::new(18, 0), 0));
    for x in 1..=10 {
        spec.mud.push(TilePos::new(x, 0)); // the path is slow (multi-tick) mud
    }
    let mut sim = Sim::new(&spec);
    sim.stats.sensors = 2; // seeing 2, hearing 2*150% = 3
    let _listener = spawn(&mut sim, TilePos::new(0, 0), IDLER);
    let walker = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 0),
            source: "move_to(closest(depot).expect())\nwait(500)\n".into(),
            cpu: 4,
            cargo_cap: 1,
            faction: 1,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    let entity = sim.world.bots[&walker].data.entity;
    let mut prev = sim.world.bots[&walker].data.pos;
    let mut heard_mid_traverse = false;
    for _ in 0..400 {
        sim.step();
        let Some(w) = sim.world.bots.get(&walker) else { break };
        let pos = w.data.pos;
        let per = sim.world.perception.get(&0);
        let heard = per.is_some_and(|p| p.heard.contains_key(&entity))
            && !per.is_some_and(|p| p.seen.contains(&entity));
        // A tick where the walker HELD its tile (mid-traverse) yet is heard
        // (heard-only, in the hearing-not-seeing ring at chebyshev 3): exactly
        // the in-between-tick case the fix restores.
        if pos == prev && TilePos::new(0, 0).chebyshev(pos) == 3 && heard {
            heard_mid_traverse = true;
        }
        prev = pos;
    }
    assert!(heard_mid_traverse, "a bot mid-traverse on multi-tick terrain stays audible");
}
