//! Channels (M11, docs/01): rendezvous handoffs, longest-blocked-receiver
//! selection, broadcasts, try_* message-lost variants, err_timeout, the
//! Corruption jam, and comm-key-gated foreign namespaces.

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::{ArchiveKind, Color};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str, faction: u8) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 8,
        cargo_cap: 1,
        faction,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

fn logged(sim: &Sim, needle: &str) -> bool {
    sim.world
        .archive
        .iter()
        .any(|e| e.kind == ArchiveKind::Log && e.text.contains(needle))
}

#[test]
fn rendezvous_hands_the_value_to_the_receiver() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    spawn(&mut sim, TilePos::new(1, 1), "send(\"orders\", 42)\nwait(600)\n", 0);
    let receiver = spawn(
        &mut sim,
        TilePos::new(3, 1),
        "x = receive(\"orders\")\nlog(x)\nupload_log()\nwait(600)\n",
        0,
    );
    for _ in 0..40 {
        sim.step();
    }
    assert!(logged(&sim, "42"), "the handoff delivered the value");
    let _ = receiver;
}

#[test]
fn longest_blocked_receiver_wins_the_handoff() {
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // The early receiver logs "early", the late one "late".
    spawn(
        &mut sim,
        TilePos::new(1, 1),
        "x = receive(\"t\")\nlog(\"early\")\nupload_log()\nwait(600)\n",
        0,
    );
    for _ in 0..20 {
        sim.step(); // the early receiver blocks and waits
    }
    spawn(
        &mut sim,
        TilePos::new(3, 1),
        "x = receive(\"t\")\nlog(\"late\")\nupload_log()\nwait(600)\n",
        0,
    );
    for _ in 0..10 {
        sim.step();
    }
    spawn(&mut sim, TilePos::new(5, 1), "send(\"t\", 1)\nwait(600)\n", 0);
    for _ in 0..40 {
        sim.step();
    }
    assert!(logged(&sim, "early"), "the longest-blocked receiver won");
    assert!(!logged(&sim, "late"), "the late receiver keeps waiting (no queues)");
}

#[test]
fn broadcast_reaches_every_blocked_receiver() {
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    for (i, tag) in ["a", "b", "c"].iter().enumerate() {
        spawn(
            &mut sim,
            TilePos::new(1 + i as i32 * 2, 1),
            &format!("x = receive(\"all\")\nlog(\"{tag}\")\nupload_log()\nwait(600)\n"),
            0,
        );
    }
    for _ in 0..20 {
        sim.step();
    }
    spawn(&mut sim, TilePos::new(8, 1), "broadcast(\"all\", 7)\nwait(600)\n", 0);
    for _ in 0..40 {
        sim.step();
    }
    for tag in ["a", "b", "c"] {
        assert!(logged(&sim, tag), "every blocked receiver got a copy ({tag})");
    }
}

#[test]
fn timeouts_fault_err_timeout_and_try_variants_never_block() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // Timeout: nobody sends; the trap logs the fault id.
    let src = "\
on error:
    log(last_error())
    upload_log()

x = receive(\"quiet\", 20)
wait(600)
";
    spawn(&mut sim, TilePos::new(1, 1), src, 0);
    for _ in 0..80 {
        sim.step();
    }
    assert!(logged(&sim, "err_timeout"), "the expiry is a trappable err_timeout fault");

    // try_send with no receiver: the message is LOST, not queued.
    spawn(
        &mut sim,
        TilePos::new(3, 1),
        "x = try_send(\"nobody\", 5)\nlog(x)\nupload_log()\nwait(600)\n",
        0,
    );
    for _ in 0..30 {
        sim.step();
    }
    assert!(logged(&sim, "False"), "fire-and-forget: nobody blocked, message lost");
}

#[test]
fn corruption_jams_the_radio_but_timeouts_still_run() {
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    spec.corruption.push(TilePos::new(2, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // The receiver stands IN the static: nothing reaches it.
    spawn(
        &mut sim,
        TilePos::new(2, 1),
        "x = receive(\"ops\", 30)\nlog(\"heard\")\nupload_log()\nwait(600)\n",
        0,
    );
    spawn(&mut sim, TilePos::new(5, 1), "send(\"ops\", 9, 40)\nwait(600)\n", 0);
    for _ in 0..80 {
        sim.step();
    }
    assert!(!logged(&sim, "heard"), "blocked receivers inside the jam never wake");
}

#[test]
fn foreign_namespaces_need_the_comm_key() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let src = "\
on error:
    log(last_error())
    upload_log()

x = receive(\"intel\", 10, 1)
wait(600)
";
    let spy = spawn(&mut sim, TilePos::new(1, 1), src, 0);
    for _ in 0..30 {
        sim.step();
    }
    assert!(logged(&sim, "err_action"), "no comm key: the foreign channel refuses");
    // Hand faction 0 the key (analyze does this in play): now it blocks
    // properly on faction 1's namespace instead of faulting.
    sim.world.comm_keys.entry(0).or_default().insert(1);
    let _ = spy;
    let listener = spawn(
        &mut sim,
        TilePos::new(3, 1),
        "x = receive(\"intel\", 60, 1)\nlog(\"keyed\")\nupload_log()\nwait(600)\n",
        0,
    );
    let speaker = spawn(
        &mut sim,
        TilePos::new(5, 1),
        "send(\"intel\", 3)\nwait(600)\n",
        1,
    );
    for _ in 0..60 {
        sim.step();
    }
    assert!(logged(&sim, "keyed"), "with the key, the foreign namespace works");
    let _ = (listener, speaker);
}

#[test]
fn the_jam_blocks_try_verbs_both_ways() {
    let mut spec = MapSpec::empty(10, 4);
    spec.quirk_permille = 0;
    spec.corruption.push(TilePos::new(2, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // A receiver blocks OUTSIDE the static; the sender stands IN it.
    spawn(
        &mut sim,
        TilePos::new(6, 1),
        "x = receive(\"ops\", 200)\nlog(\"heard\")\nupload_log()\nwait(600)\n",
        0,
    );
    for _ in 0..10 {
        sim.step();
    }
    spawn(
        &mut sim,
        TilePos::new(2, 1),
        "x = try_send(\"ops\", 9)\nlog(x)\nupload_log()\nwait(600)\n",
        0,
    );
    for _ in 0..30 {
        sim.step();
    }
    assert!(
        logged(&sim, "False"),
        "a caller inside Corruption transmits nothing (jammed both ways)"
    );
    assert!(!logged(&sim, "heard"), "the message never escaped the static");
}

#[test]
fn faction_zero_namespaces_are_addressable() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // Faction 1 holds faction 0's comm key (analyze does this in play).
    sim.world.comm_keys.entry(1).or_default().insert(0);
    spawn(
        &mut sim,
        TilePos::new(1, 1),
        "x = receive(\"intel\", 60, 0)\nlog(\"zero\")\nupload_log()\nwait(600)\n",
        1,
    );
    spawn(&mut sim, TilePos::new(3, 1), "send(\"intel\", 3)\nwait(600)\n", 0);
    for _ in 0..60 {
        sim.step();
    }
    assert!(
        logged(&sim, "zero"),
        "faction 0 is a real namespace — its stolen key must work (review 2026-07-16)"
    );
}
