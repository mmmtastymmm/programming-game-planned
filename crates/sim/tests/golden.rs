//! Stored golden-replay fixtures (docs/07 testing strategy; CLAUDE.md).
//!
//! Unlike the paired-run tests in `replay.rs`, these compare against
//! CHECKED-IN hashes, so any behavior change — deterministic or not —
//! fails CI until the fixture is regenerated on purpose:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test -p sim --test golden
//! ```
//!
//! A PR that regenerates the fixture must explain why (CLAUDE.md: "a PR
//! that changes a replay hash must explain why").

use sim::map::{MapSpec, PrinterSpec};
use sim::sim::Command;
use sim::world::{BlueprintKind, BotId, Color, EntityId};
use sim::map::{Direction, OverlayKind};
use sim::{Replay, TilePos, TimedCommand};
use std::path::PathBuf;

/// The doc's Tier-0 starter program (docs/01-language.md).
const MINER: &str = "\
move_to(closest(ore).expect())
mine()
move_to(closest(depot).expect())
deposit()
";

/// Hot-swapped v2: same loop with a polite pause (exercises redeploy).
const MINER_V2: &str = "\
move_to(closest(ore).expect())
mine()
move_to(closest(depot).expect())
deposit()
wait(5)
";

/// The canonical golden scenario. Exercises every Command variant, printer
/// prints with boot sequences, a mid-run hot-swap, sidestep RNG (two bots
/// share a corridor), fault paths (ore eventually far), and a kill.
fn golden_replay() -> Replay {
    let mut spec = MapSpec::empty(14, 8);
    spec.seed = 0x5EED_601D; // fixed match seed for the fixture
    spec.starting_ore = 10;
    spec.starting_stock.push((0, sim::resources::Resource::Stone, 50));
    // A worked node WITHIN the start cluster's floor sight (M5: sensors
    // 5 — printed miners must discover it from the printer/depot), plus
    // the far one that keeps the fault-path flavor once the near node
    // drains (blind faults are still faults).
    spec.ore_nodes.push((TilePos::new(5, 3), 30));
    spec.ore_nodes.push((TilePos::new(11, 3), 30));
    spec.depots.push(TilePos::new(2, 3));
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 5),
        faction: 0,
        color: 0,
        ruined: false,
        desired_max: 0,
    });
    // A water inlet with a bridge-able gap (blueprint placed mid-run).
    for y in 0..3 {
        spec.water.push(TilePos::new(7, y));
    }
    // Entity IDs from from_spec order: ore nodes = 1,2, depot = 3, printer = 4.
    let printer = EntityId(4);
    let commands = vec![
        TimedCommand {
            tick: 0,
            command: Command::DeployProgram { faction: 0, color: Color::GREEN, source: MINER.into() },
        },
        TimedCommand { tick: 0, command: Command::SetDesiredMax { printer, value: 2 } },
        TimedCommand {
            tick: 40,
            command: Command::PlaceOverlay {
                pos: TilePos::new(6, 3),
                overlay: Some(OverlayKind::Arrow(Direction::East)),
                faction: 0,
            },
        },
        TimedCommand {
            tick: 60,
            command: Command::PlacePaint { pos: TilePos::new(5, 5), color: Some(2) },
        },
        TimedCommand {
            tick: 100,
            command: Command::DeployProgram { faction: 0, color: Color::GREEN, source: MINER_V2.into() },
        },
        TimedCommand {
            tick: 150,
            command: Command::PlaceBlueprint { pos: TilePos::new(7, 1), kind: BlueprintKind::Bridge, faction: 0 },
        },
        TimedCommand { tick: 220, command: Command::KillBot { bot: BotId(1) } },
    ];
    // 1500 ticks: at M5's 14-ticks/tile floor statline a depot round trip
    // is ~250 ticks, so the scenario still sees several deliveries.
    Replay { spec, commands, ticks: 1500 }
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn hashes_to_text(hashes: &[u64]) -> String {
    let mut out = String::new();
    for h in hashes {
        out.push_str(&format!("{h:016x}\n"));
    }
    out
}

/// The fixture is only worth its bytes if the scenario genuinely exercises
/// the sim: printed bots, mined ore, the deploy swap, and the kill.
#[test]
fn golden_scenario_is_alive() {
    let replay = golden_replay();
    let mut sim = sim::Sim::new(&replay.spec);
    let mut next = 0;
    for tick in 0..replay.ticks {
        while next < replay.commands.len() && replay.commands[next].tick == tick {
            sim.apply(&replay.commands[next].command).expect("command accepted");
            next += 1;
        }
        sim.step();
    }
    assert!(!sim.world.bots.is_empty(), "printer must have printed bots");
    // starting_ore 10 seeds 100 deci of Iron — the miners must OUT-EARN
    // it, not just exist (the old `> 10` compared deci against units and
    // was trivially true).
    assert!(
        sim.world.stock_get(0, sim::resources::Resource::Iron) > 100,
        "miners must out-earn the seeded stock; got {}",
        sim.world.stock_get(0, sim::resources::Resource::Iron)
    );
    assert!(sim.world.wrecks.contains_key(&BotId(1)), "KillBot(1) must leave a wreck");
    assert_eq!(sim.world.program_library.len(), 2, "both deployed versions retained");
    assert!(!sim.world.blueprints.is_empty(), "bridge blueprint placed");
}

#[test]
fn golden_replay_round_trips_through_ron() {
    let replay = golden_replay();
    let text = replay.to_ron();
    let parsed = Replay::from_ron(&text).expect("fixture RON parses");
    assert_eq!(replay, parsed, "replay must survive serialization byte-exactly");
}

#[test]
fn golden_replay_matches_stored_fixture() {
    let dir = fixture_dir();
    let replay_path = dir.join("showcase.replay.ron");
    let hashes_path = dir.join("showcase.hashes.txt");
    let replay = golden_replay();
    let hashes = replay.run();

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(&dir).expect("fixture dir");
        std::fs::write(&replay_path, replay.to_ron()).expect("write replay fixture");
        std::fs::write(&hashes_path, hashes_to_text(&hashes)).expect("write hash fixture");
        eprintln!("golden fixtures regenerated — explain the hash change in the PR (CLAUDE.md)");
        return;
    }

    let stored_replay = Replay::from_ron(
        &std::fs::read_to_string(&replay_path).expect("stored replay fixture exists"),
    )
    .expect("stored replay parses");
    assert_eq!(
        stored_replay, replay,
        "the in-code scenario and the stored artifact diverged — \
         regenerate with UPDATE_GOLDEN=1 and explain why in the PR"
    );
    let stored_hashes = std::fs::read_to_string(&hashes_path).expect("stored hashes exist");
    assert_eq!(
        stored_hashes,
        hashes_to_text(&hashes),
        "replay hash drift: sim behavior changed. If intentional, regenerate \
         fixtures with UPDATE_GOLDEN=1 and explain the change in the PR (CLAUDE.md)"
    );
}

/// Child half of the cross-process check: prints the final hash. Ignored in
/// normal runs; the parent test invokes it in a fresh process.
#[test]
#[ignore = "spawned by cross_process_replay_matches"]
fn emit_golden_final_hash() {
    let hashes = golden_replay().run();
    println!("GOLDEN_FINAL_HASH={:016x}", hashes.last().expect("nonempty run"));
}

/// The actual lockstep guarantee: a SEPARATE PROCESS reaches bit-identical
/// state (same-process pairs can't catch e.g. ASLR-dependent iteration).
#[test]
fn cross_process_replay_matches() {
    let exe = std::env::current_exe().expect("test binary path");
    let output = std::process::Command::new(exe)
        .args(["--ignored", "--exact", "emit_golden_final_hash", "--nocapture"])
        .env_remove("UPDATE_GOLDEN")
        .output()
        .expect("spawn child test process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "child process failed: {stdout}");
    let child_hash = stdout
        .lines()
        .find_map(|l| l.strip_prefix("GOLDEN_FINAL_HASH="))
        .unwrap_or_else(|| panic!("no hash line in child output: {stdout}"))
        .to_string();
    let local = golden_replay().run();
    let local_hash = format!("{:016x}", local.last().expect("nonempty run"));
    assert_eq!(child_hash, local_hash, "cross-process desync — determinism violation");
}
