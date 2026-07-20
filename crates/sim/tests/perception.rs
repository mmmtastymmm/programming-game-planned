//! Perception, the scouting stance, and start-kit world-reading builtins
//! (docs/05, docs/02 Scouting). Coverage the max review flagged as absent:
//! search()/survey + Scouting XP, scan ordering, and is_seen/cargo_count/
//! path_blocked driven against a live world.

use sim::map::{MapSpec, TileKind};
use sim::sim::{Command, Sim};
use sim::world::{Color, XpTrack};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 4,
        cargo_cap: 4,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .expect("spawn parses")
    .expect("spawn returns id")
}

// ------------------------------------------------------ the scouting stance

#[test]
fn search_surveys_beyond_sight_and_pays_scouting_xp() {
    // The scouting stance (docs/05 Decided) roots and expands the seeing ring
    // out to the hearing radius, permanently recording nodes it uncovers and
    // paying the Scouting XP track (docs/02) — none of which had a test.
    let mut spec = MapSpec::empty(12, 3);
    // A vein at chebyshev 6 from the scout: past base sight (5), inside the
    // survey's reach (5 × 150% = 7), so ONLY the survey can discover it.
    spec.ore_nodes.push((TilePos::new(7, 1), 50));
    let mut sim = Sim::new(&spec);
    let scout = spawn(&mut sim, TilePos::new(1, 1), "search()\nwait(100000)\n");

    // Passive perception does not reach the vein at spawn.
    assert!(
        sim.world.known_nodes.get(&0).is_none_or(|k| k.is_empty()),
        "the far vein must NOT be known before surveying"
    );
    for _ in 0..300 {
        sim.step();
    }
    assert!(
        sim.world.known_nodes.get(&0).is_some_and(|k| !k.is_empty()),
        "search() must permanently record the surveyed vein"
    );
    assert!(
        sim.world.bots[&scout].data.xp(XpTrack::Scouting) > 0,
        "surveying pays the Scouting XP track"
    );
}

// ------------------------------------------------------ scan ordering

#[test]
fn scan_resources_lists_the_nearest_known_vein_first() {
    // scan_resources() returns known nodes in (distance, id) order (CLAUDE.md
    // rule 6). Drive it: walk to scan_resources()[0] and confirm the bot
    // heads for the NEARER vein, not the farther one.
    let mut spec = MapSpec::empty(14, 3);
    spec.ore_nodes.push((TilePos::new(4, 1), 50)); // near
    spec.ore_nodes.push((TilePos::new(12, 1), 50)); // far
    let mut sim = Sim::new(&spec);
    // Both veins are within sight of a scout that surveys first, then routes
    // to the nearest scanned node.
    let bot = spawn(
        &mut sim,
        TilePos::new(2, 1),
        "search()\nmove_to(scan_resources()[0])\nmine()\nwait(100000)\n",
    );
    for _ in 0..300 {
        sim.step();
    }
    // The bot mined the NEAR vein (id-lowest / closest), so it ends near x=4,
    // never having walked out to x=12.
    let x = sim.world.bots[&bot].data.pos.x;
    assert!((3..=5).contains(&x), "bot routed to the nearest scanned vein, ended at x={x}");
}

// ------------------------------------------------------ start-kit queries

#[test]
fn cargo_count_reads_the_hold() {
    // cargo_count(kind) — a tick-1 start-kit query — reflects what a bot has
    // mined. Log it after a mine and read the value back.
    let mut spec = MapSpec::empty(6, 3);
    spec.ore_nodes.push((TilePos::new(3, 1), 50)); // Iron, in sight
    let mut sim = Sim::new(&spec);
    let bot = spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(closest(ore).expect())\nmine()\nlog(cargo_count(iron))\nwait(100000)\n",
    );
    for _ in 0..200 {
        sim.step();
    }
    // A positive, non-"0" cargo_count landed in the log after mining.
    let logs = &sim.world.bots[&bot].data.log_buf;
    assert!(
        logs.iter().any(|(_, t)| t != "0" && t.parse::<i64>().is_ok()),
        "cargo_count(iron) logged a positive hold after mining: {logs:?}"
    );
}

#[test]
fn path_blocked_is_callable_and_returns_a_bool() {
    // path_blocked() (the corridor sensor: is the current move's next tile
    // bot-occupied?) is invoked by no other test. Called from the main body
    // between actions it reads False; this guards that the builtin runs and
    // returns a Bool at all (a fault or wrong-type regression fails here).
    let mut spec = MapSpec::empty(6, 1);
    spec.depots.push((TilePos::new(5, 0), 0));
    let mut sim = Sim::new(&spec);
    let mover =
        spawn(&mut sim, TilePos::new(0, 0), "move_to(closest(depot).expect())\nlog(path_blocked())\n");
    for _ in 0..60 {
        sim.step();
    }
    let logs = &sim.world.bots[&mover].data.log_buf;
    assert!(
        logs.iter().any(|(_, t)| t == "False"),
        "path_blocked() runs and returns a Bool: {logs:?}"
    );
}

#[test]
fn is_seen_is_true_for_a_visible_own_depot() {
    // is_seen(handle) reports colony-visible entities. An own depot in range is
    // in the colony cloud, so is_seen(closest(depot)) is True. (is_seen was
    // invoked by zero integration tests before this.)
    let mut spec = MapSpec::empty(6, 3);
    spec.depots.push((TilePos::new(3, 1), 0));
    let mut sim = Sim::new(&spec);
    let obs =
        spawn(&mut sim, TilePos::new(1, 1), "log(is_seen(closest(depot).expect()))\nwait(100000)\n");
    for _ in 0..20 {
        sim.step();
    }
    let logs = &sim.world.bots[&obs].data.log_buf;
    assert!(
        logs.iter().any(|(_, t)| t == "True"),
        "is_seen is True for an in-range own depot: {logs:?}"
    );
}

// ------------------------------------------------------ Mountain elevation

#[test]
fn survey_uses_mountain_elevation() {
    // Regression for the survey/fog elevation flag: a scout on a Mountain
    // summit surveys with elevation, so a vein behind a wall inside its ring
    // is still discovered. (Under current tuning passive sight and survey
    // reach coincide, so this mainly guards that the survey path runs the
    // on_high_ground predicate rather than == HighGround.)
    let mut spec = MapSpec::empty(10, 3);
    // Bot on a Mountain at (1,1); a HighGround wall at (3,1); a vein at (5,1).
    spec.high_ground.push(TilePos::new(3, 1));
    spec.ore_nodes.push((TilePos::new(5, 1), 50));
    // Paint the bot's tile Mountain via a resync-safe direct set after build.
    let mut sim = Sim::new(&spec);
    sim.world.grid.set(TilePos::new(1, 1), TileKind::Mountain);
    let _scout = spawn(&mut sim, TilePos::new(1, 1), "search()\nwait(100000)\n");
    for _ in 0..200 {
        sim.step();
    }
    // The vein behind the wall is discovered (elevation lets the summit see
    // over the wall — the fixed elevated flag).
    assert!(
        sim.world.known_nodes.get(&0).is_some_and(|k| k.values().any(|n| n.pos == TilePos::new(5, 1))),
        "a summit scout discovers a vein behind a wall"
    );
}
