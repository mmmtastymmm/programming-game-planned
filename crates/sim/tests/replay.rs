//! Golden-replay style tests: (map, command log) → exact world state, and
//! bit-identical hashes across runs (docs/07-architecture.md testing
//! strategy, day one).

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::Color;
use sim::TilePos;

/// The doc's Tier-0 starter program, verbatim (docs/01-language.md).
const MINER: &str = "\
move_to(closest(ore).expect())
mine()
move_to(closest(depot).expect())
deposit()
";

fn mining_map() -> MapSpec {
    let mut spec = MapSpec::empty(10, 6);
    spec.ore_nodes.push((TilePos::new(8, 2), 10));
    spec.depots.push((TilePos::new(1, 2), 0));
    // A rubble ridge the path must cross.
    spec.rubble.push(TilePos::new(5, 1));
    spec.rubble.push(TilePos::new(5, 2));
    spec.rubble.push(TilePos::new(5, 3));
    spec
}

fn run_mining_sim(ticks: u64) -> Sim {
    let mut sim = Sim::new(&mining_map());
    // Start-zone sight guarantee (docs/03) — the map is a corridor, not a
    // vision test; M5's floor sensors (5) would leave the node fogged.
    sim.stats.sensors = 12;
    // Beside the depot, not ON it — depot tiles are solid (bots can
    // neither stand on nor spawn onto them).
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(2, 2),
        source: MINER.into(),
        cpu: 2,
        cargo_cap: 3,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .expect("spawn");
    for _ in 0..ticks {
        sim.step();
    }
    sim
}

#[test]
fn tier0_miner_delivers_ore() {
    let sim = run_mining_sim(400);
    assert!(
        sim.world.stock_get(0, sim::resources::Resource::Iron) > 0,
        "the starter program must produce ore; archive: {:?}",
        sim.world.archive
    );
    // The bot is alive and working, not crash-looping.
    assert_eq!(sim.world.bots.len(), 1);
    let bot = sim.world.bots.values().next().unwrap();
    assert!(bot.data.xp(sim::world::XpTrack::Mining) > 0, "mining XP accrues from doing");
    assert!(bot.data.xp(sim::world::XpTrack::Hauling) > 0, "hauling XP accrues from delivering");
}

#[test]
fn identical_runs_hash_identically() {
    let a = run_mining_sim(300);
    let b = run_mining_sim(300);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.world.stock_get(0, sim::resources::Resource::Iron), b.world.stock_get(0, sim::resources::Resource::Iron));
}

#[test]
fn hash_sequences_match_tick_by_tick() {
    // The stronger property lockstep needs: state is identical at EVERY
    // tick, not just the end.
    let mut a = Sim::new(&mining_map());
    let mut b = Sim::new(&mining_map());
    let cmd = Command::SpawnBot {
        pos: TilePos::new(2, 2),
        source: MINER.into(),
        cpu: 2,
        cargo_cap: 3,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    };
    a.apply(&cmd).unwrap();
    b.apply(&cmd).unwrap();
    for tick in 0..300 {
        a.step();
        b.step();
        assert_eq!(a.state_hash(), b.state_hash(), "desync at tick {tick}");
    }
}

#[test]
fn ore_depletes_then_program_faults_into_crash_dumps() {
    // 10 ore, cargo trips of 3: eventually closest(ore).expect() faults ("no ore
    // anywhere") → forced crash dump in the archive. Requirement: the
    // failure is *visible*, not silent.
    let sim = run_mining_sim(3000);
    assert_eq!(sim.world.stock_get(0, sim::resources::Resource::Iron), 100, "all ore (10 units = 100 deci) ends up in stock");
    assert!(
        sim.world
            .archive_all()
            .any(|e| e.kind == sim::world::ArchiveKind::CrashDump && e.text.contains("no ore")),
        "depletion must surface as crash dumps; archive: {:?}",
        sim.world.archive
    );
}

#[test]
fn unreachable_target_faults() {
    // Ore on an island surrounded by water: move_to must fault, the bot
    // crash-dumps and keeps retrying (fault loops are legal and visible).
    let mut spec = MapSpec::empty(9, 7);
    spec.ore_nodes.push((TilePos::new(5, 3), 5));
    for dy in -2..=2_i32 {
        for dx in -2..=2_i32 {
            if dx.abs() == 2 || dy.abs() == 2 {
                spec.water.push(TilePos::new(5 + dx, 3 + dy));
            }
        }
    }
    spec.depots.push((TilePos::new(0, 0), 0));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0; // unreachable-loop routing test, not a health test
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(1, 0),
        source: MINER.into(),
        cpu: 4,
        cargo_cap: 2,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap();
    for _ in 0..200 {
        sim.step();
    }
    assert!(
        sim.world
            .archive_all()
            .any(|e| e.text.contains("unreachable")),
        "archive: {:?}",
        sim.world.archive
    );
    assert_eq!(sim.world.stock_get(0, sim::resources::Resource::Iron), 0);
}

#[test]
fn abort_is_the_only_scuttle_and_become_disabled_is_engine_only() {
    // abort() is the ONLY deliberate way down (Q76): the forced sequence
    // uploads the buffer to the archive, then wrecks the bot.
    let src = "\
log(123)
abort()
";
    let mut sim = Sim::new(&MapSpec::empty(4, 4));
    let id = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: src.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    for _ in 0..10 {
        sim.step();
    }
    assert!(!sim.world.bots.contains_key(&id), "bot must be wrecked");
    assert!(sim.world.wrecks.contains_key(&id), "abort exits into a wreck");
    assert!(
        sim.world.archive_all().any(|e| e.text.contains("123")),
        "abort's forced upload sent the log home"
    );

    // become_disabled() must NOT be player-callable — it faults like any
    // unknown name (the VM blocks it before the host's engine-only arm),
    // so the bot crash-loops instead of silently wrecking itself.
    let mut sim = Sim::new(&MapSpec::empty(4, 4));
    let id = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: "become_disabled()\n".into(),
            cpu: 8,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    for _ in 0..4 {
        sim.step();
    }
    let bot = &sim.world.bots[&id];
    assert!(!bot.data.dying, "the call must not scuttle the bot");
    assert!(
        bot.vm.as_ref().is_some_and(|vm| vm.crash_count() >= 1),
        "player become_disabled() faults err_unknown_function"
    );
}

#[test]
fn rubble_slows_movement() {
    // Same trip with and without a rubble wall across the corridor: the
    // rubble run needs more ticks to make its first delivery.
    let deliver_tick = |rubble: bool| -> u64 {
        let mut spec = MapSpec::empty(12, 3);
        spec.ore_nodes.push((TilePos::new(10, 1), 50));
        spec.depots.push((TilePos::new(0, 1), 0));
        if rubble {
            for y in 0..3 {
                spec.rubble.push(TilePos::new(5, y));
            }
        }
        let mut sim = Sim::new(&spec);
        // Start-zone sight guarantee (docs/03): the node sits 9 tiles out
        // and this map tests rubble PACING, not fog.
        sim.stats.sensors = 14;
        sim.apply(&Command::SpawnBot {
            pos: TilePos::new(1, 1),
            source: MINER.into(),
            cpu: 4,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap();
        for tick in 1..=2000 {
            sim.step();
            if sim.world.stock_get(0, sim::resources::Resource::Iron) > 0 {
                return tick;
            }
        }
        panic!("never delivered");
    };
    let fast = deliver_tick(false);
    let slow = deliver_tick(true);
    assert!(slow > fast, "rubble must slow the trip: {slow} vs {fast}");
}
