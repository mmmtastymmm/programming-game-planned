//! Spec-conformance regressions (review 2026-07-17): entity property
//! reads (`t.distance`), the comm-key hash length prefix, and the
//! study()/attack-XP fault behaviors.

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
fn entity_distance_reads_work_for_seen_contacts() {
    // docs/01: `t.distance` is priced core language. BotHost must
    // implement attr or every read faults (the feature was dead).
    let mut spec = MapSpec::empty(12, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    // A reader adjacent to a seen enemy: distance resolves to a number.
    let src = "\
t = closest(enemy).expect()
log(t.distance)
upload_log()
wait(600)
";
    spawn(&mut sim, TilePos::new(2, 1), src, 0);
    spawn(&mut sim, TilePos::new(4, 1), "wait(600)\n", 1);
    for _ in 0..30 {
        sim.step();
    }
    // Chebyshev distance between (2,1) and (4,1) is 2.
    assert!(logged(&sim, "2"), "t.distance returns the chebyshev tile distance");
    assert!(
        !logged(&sim, "err_name") && !logged(&sim, "unknown attribute"),
        "a documented property read does not fault"
    );
}

#[test]
fn unknown_property_faults_err_name() {
    let mut spec = MapSpec::empty(12, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let src = "\
on error:
    log(last_error())
    upload_log()

t = closest(enemy).expect()
x = t.nonsense
wait(600)
";
    spawn(&mut sim, TilePos::new(2, 1), src, 0);
    spawn(&mut sim, TilePos::new(4, 1), "wait(600)\n", 1);
    for _ in 0..30 {
        sim.step();
    }
    assert!(logged(&sim, "err_name"), "an unknown property faults err_name");
}

#[test]
fn study_faults_gracefully_not_unknown_function() {
    // study() is an advertised start-kit verb; Template Caches aren't
    // built yet, so it must fault err_action, never err_unknown_function.
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let src = "\
on error:
    log(last_error())
    upload_log()

study()
wait(600)
";
    spawn(&mut sim, TilePos::new(2, 1), src, 0);
    for _ in 0..20 {
        sim.step();
    }
    assert!(logged(&sim, "err_action"), "study with no Cache faults err_action");
    assert!(
        !logged(&sim, "err_unknown_function"),
        "an advertised builtin never reports unknown_function"
    );
}

#[test]
fn comm_key_states_hash_distinctly() {
    // The phase-9 hash must separate {1:{2},5:{6}} from {1:{2,5,6}}
    // (review 2026-07-17: a missing inner length prefix collided them,
    // blinding the desync detector).
    let spec = MapSpec::empty(6, 4);
    let mut a = Sim::new(&spec);
    a.world.comm_keys.entry(1).or_default().insert(2);
    a.world.comm_keys.entry(5).or_default().insert(6);
    let mut b = Sim::new(&spec);
    b.world.comm_keys.entry(1).or_default().extend([2, 5, 6]);
    assert_ne!(
        a.state_hash(),
        b.state_hash(),
        "distinct comm-key distributions must not collide in the snapshot hash"
    );
}
