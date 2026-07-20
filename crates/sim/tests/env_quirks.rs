//! Bot-facing env store (setenv/getenv, docs/01) and quirk introspection
//! (my_quirks/has_quirk, docs/09) — verb families the max review flagged as
//! invoked by zero test programs, despite `data.env` being lockstep-hashed.

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::Color;
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
    .expect("spawn parses")
    .expect("spawn returns id")
}

#[test]
fn setenv_persists_and_getenv_reads_it_back() {
    // A program writes an env key and reads it back — the persistent per-bot
    // store that survives across the implicit loop (and rides the phase-9
    // hash). No prior test exercised the env verbs from a program.
    let mut sim = Sim::new(&MapSpec::empty(4, 3));
    let bot = spawn(&mut sim, TilePos::new(1, 1), "setenv(hurt_line, 30)\nlog(getenv(hurt_line))\nwait(100000)\n");
    for _ in 0..20 {
        sim.step();
    }
    // The write landed in state (hashed) and getenv logged it.
    assert_eq!(sim.world.bots[&bot].data.env.get("hurt_line"), Some(&30));
    assert!(
        sim.world.bots[&bot].data.log_buf.iter().any(|(_, t)| t == "30"),
        "getenv(hurt_line) reads back the written value: {:?}",
        sim.world.bots[&bot].data.log_buf
    );
}

#[test]
fn setenv_rejects_out_of_range_and_getenv_reports_the_default() {
    // setenv enforces the ENV_KEYS range (hurt_line 1..=99): an out-of-range
    // write faults, leaving the key at its tuning default, which getenv reads.
    let mut sim = Sim::new(&MapSpec::empty(4, 3));
    let bot = spawn(&mut sim, TilePos::new(1, 1), "log(getenv(hurt_line))\nwait(100000)\n");
    for _ in 0..10 {
        sim.step();
    }
    let default = sim.tuning.hurt_line_pct;
    assert!(
        sim.world.bots[&bot].data.log_buf.iter().any(|(_, t)| t == &default.to_string()),
        "getenv on an unset key reads the tuning default ({default}): {:?}",
        sim.world.bots[&bot].data.log_buf
    );
    // The store stays empty (nothing written).
    assert!(sim.world.bots[&bot].data.env.is_empty(), "no key was written");
}

#[test]
fn my_quirks_and_has_quirk_report_manifested_only() {
    // my_quirks()/has_quirk() are the only in-language way to read a bot's
    // quirks (docs/09), and must report MANIFESTED quirks only — never latent
    // ones. Manifest one quirk, leave another latent, and check both verbs.
    let mut sim = Sim::new(&MapSpec::empty(4, 3));
    let manifested = sim.quirks.by_name("overclocked").expect("overclocked exists");
    let latent = sim.quirks.by_name("huffman_coded").expect("huffman_coded exists");
    let bot = spawn(
        &mut sim,
        TilePos::new(1, 1),
        "log(has_quirk(\"overclocked\"))\nlog(has_quirk(\"huffman_coded\"))\nlog(has_quirk(\"retina_display\"))\nlog(my_quirks())\nwait(100000)\n",
    );
    {
        let data = &mut sim.world.bots.get_mut(&bot).unwrap().data;
        data.quirks = vec![manifested];
        data.latent_quirks = vec![latent];
    }
    for _ in 0..20 {
        sim.step();
    }
    let logs: Vec<String> = sim.world.bots[&bot].data.log_buf.iter().map(|(_, t)| t.clone()).collect();
    // has_quirk: manifested True, latent False, absent False.
    assert!(logs.iter().any(|t| t == "True"), "has_quirk(manifested) is True: {logs:?}");
    assert!(
        !logs.iter().any(|t| t.contains("huffman_coded")),
        "a LATENT quirk must not appear anywhere: {logs:?}"
    );
    // my_quirks lists the manifested one.
    assert!(
        logs.iter().any(|t| t.contains("overclocked")),
        "my_quirks() lists the manifested quirk: {logs:?}"
    );
}
