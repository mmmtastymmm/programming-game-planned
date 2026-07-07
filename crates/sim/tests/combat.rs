//! Combat + signal wiring: damage, hurt/death handlers, double-handle,
//! black boxes, XP from fighting (docs/01-language.md, docs/02-agents.md).

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::ArchiveKind;
use sim::world::Color;
use sim::TilePos;

/// A stationary attacker: hits the nearest enemy forever.
const BRAWLER: &str = "attack(nearest_enemy())\n";

/// A harmless idler.
const IDLER: &str = "log(1)\n";

fn spawn(sim: &mut Sim, pos: TilePos, source: &str, faction: u8, hp: i64) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 4,
        cargo_cap: 1,
        faction,
        hp,
        color: Color::GREEN,
    })
    .expect("spawn")
    .expect("spawn returns id")
}

#[test]
fn attacker_kills_defenseless_bot_into_wreck() {
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    let attacker = spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    let victim = spawn(&mut sim, TilePos::new(2, 1), IDLER, 0, 30);
    for _ in 0..200 {
        sim.step();
    }
    assert!(!sim.world.bots.contains_key(&victim), "victim must die");
    assert!(sim.world.wrecks.contains_key(&victim), "no death handler → clean wreck");
    let attacker_bot = &sim.world.bots[&attacker];
    assert_eq!(attacker_bot.data.xp_combat, 30, "combat XP = damage dealt");
}

#[test]
fn hurt_handler_fires_below_default_threshold() {
    let victim_src = "\
on hurt:
    log(\"ouch\")
    upload_log()

log(1)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 100);

    // Track the victim's hp at the moment "ouch" lands in the cloud.
    let mut hp_at_ouch = None;
    for _ in 0..600 {
        sim.step();
        if hp_at_ouch.is_none()
            && sim.world.archive.iter().any(|e| e.text.contains("ouch"))
        {
            hp_at_ouch = sim.world.bots.get(&victim).map(|b| b.data.hp);
            break;
        }
    }
    let hp = hp_at_ouch.expect("hurt handler must fire and upload");
    assert!(hp < 50, "default threshold is 50% — fired at hp {hp}");
    assert!(hp > 0, "fired before death");
}

#[test]
fn custom_hurt_threshold_fires_later() {
    let victim_src = "\
on hurt(20):
    log(\"late\")
    upload_log()

log(1)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 100);

    let mut hp_at_fire = None;
    for _ in 0..600 {
        sim.step();
        if hp_at_fire.is_none() && sim.world.archive.iter().any(|e| e.text.contains("late")) {
            hp_at_fire = sim.world.bots.get(&victim).map(|b| b.data.hp);
            break;
        }
    }
    let hp = hp_at_fire.expect("custom-threshold hurt handler must fire");
    assert!(hp < 20, "on hurt(20) fires below 20% — fired at hp {hp}");
}

#[test]
fn death_handler_files_black_box_report_then_wrecks() {
    let victim_src = "\
on death:
    log(\"death report\")
    upload_log()

log(1)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 20);
    for _ in 0..300 {
        sim.step();
    }
    assert!(sim.world.wrecks.contains_key(&victim), "clean death → wreck");
    assert!(
        sim.world
            .archive
            .iter()
            .any(|e| e.kind == ArchiveKind::Log && e.text.contains("death report")),
        "the black-box budget covers one log + upload; archive: {:?}",
        sim.world.archive
    );
}

#[test]
fn fault_inside_hurt_handler_is_double_handle_no_wreck() {
    // The hurt handler calls nearest_depot() on a map with no depot: the
    // fault inside the handler is a double handle — instant destruction,
    // no wreck, but a Black Box with the cause.
    let victim_src = "\
on hurt:
    move_to(nearest_depot())

log(1)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 30);
    for _ in 0..300 {
        sim.step();
    }
    assert!(!sim.world.bots.contains_key(&victim), "victim destroyed");
    assert!(
        !sim.world.wrecks.contains_key(&victim),
        "double handle leaves NO wreck"
    );
    let bb = sim
        .world
        .black_boxes
        .iter()
        .find(|b| b.bot == victim)
        .expect("every destruction drops a black box");
    assert!(bb.cause.contains("no depot"), "cause records the fault: {}", bb.cause);
}

#[test]
fn lethal_damage_during_hurt_handler_explodes() {
    // Retreat program: the hurt handler blocks on a long move while the
    // brawler keeps swinging — death mid-handler is a double handle.
    let victim_src = "\
on hurt:
    move_to(nearest_depot())

log(1)
";
    let mut spec = MapSpec::empty(20, 3);
    spec.depots.push(TilePos::new(19, 1)); // far away: the retreat takes a while
    let mut sim = Sim::new(&spec);
    spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    // Victim stands next to the attacker; hurt fires at hp<15 (hp 30).
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 30);
    for _ in 0..300 {
        sim.step();
    }
    assert!(!sim.world.bots.contains_key(&victim));
    // Either the retreat outran the hits (then the victim would still be
    // alive — asserted above it isn't) or death landed mid-handler:
    assert!(
        !sim.world.wrecks.contains_key(&victim),
        "death during the hurt handler must explode, not wreck"
    );
    assert!(sim.world.black_boxes.iter().any(|b| b.bot == victim));
}

#[test]
fn combat_is_deterministic_tick_by_tick() {
    let build = || {
        let mut sim = Sim::new(&MapSpec::empty(6, 6));
        spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
        spawn(
            &mut sim,
            TilePos::new(2, 1),
            "on hurt:\n    log(\"h\")\n    upload_log()\n\nlog(1)\n",
            0,
            60,
        );
        spawn(&mut sim, TilePos::new(2, 2), BRAWLER, 0, 100); // fights back
        sim
    };
    let mut a = build();
    let mut b = build();
    for tick in 0..400 {
        a.step();
        b.step();
        assert_eq!(a.state_hash(), b.state_hash(), "desync at tick {tick}");
    }
}
