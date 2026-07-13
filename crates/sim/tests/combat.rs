//! Combat + signal wiring: damage, hurt/death handlers, double-handle,
//! black boxes, XP from fighting (docs/01-language.md, docs/02-agents.md).

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::ArchiveKind;
use sim::world::Color;
use sim::TilePos;

/// A stationary attacker: hits the nearest enemy forever.
const BRAWLER: &str = "attack(closest(enemy).expect())\n";

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
    sim.tuning.regen_amount = 0; // exact XP math needs static hp
    let attacker = spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    let victim = spawn(&mut sim, TilePos::new(2, 1), IDLER, 0, 30);
    for _ in 0..200 {
        sim.step();
        if !sim.world.bots.contains_key(&victim) {
            break; // stop before the now-idle brawler crash-loops to death
        }
    }
    assert!(!sim.world.bots.contains_key(&victim), "victim must die");
    assert!(sim.world.wrecks.contains_key(&victim), "no death handler → clean wreck");
    let attacker_bot = &sim.world.bots[&attacker];
    assert_eq!(attacker_bot.data.xp_combat, 30, "combat XP = damage dealt");
}

#[test]
fn hurt_handler_fires_below_default_threshold() {
    let victim_src = "\
on signal(s):
    log(\"ouch\")
    upload_log()

log(1)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    // Generous hp: the forced handler_init ritual (15 ticks) must survive
    // sustained fire before the handler body can upload.
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 400);

    // Track the victim's hp at the moment "ouch" lands in the cloud.
    let mut hp_at_ouch = None;
    for _ in 0..900 {
        sim.step();
        if hp_at_ouch.is_none()
            && sim.world.archive.iter().any(|e| e.text.contains("ouch"))
        {
            hp_at_ouch = sim.world.bots.get(&victim).map(|b| b.data.hp);
            break;
        }
    }
    let hp = hp_at_ouch.expect("hurt handler must fire and upload");
    assert!(hp * 2 < 400, "default threshold is 50% — fired at hp {hp}");
    assert!(hp > 0, "fired before death");
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
    // The hurt handler calls closest(depot).expect() on a map with no
    // depot: the fault inside the PLAYER handler is a double handle —
    // instant destruction, no wreck, black box with the cause. (A single
    // wound, not sustained fire: the handler must survive its own
    // handler_init ritual to reach the faulting line.)
    let victim_src = "\
on signal(s):
    move_to(closest(depot).expect())

log(1)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 30);
    for _ in 0..4 {
        sim.step();
    }
    sim.apply_damage_for_test(victim, 20); // 10/30: below the 50% threshold
    for _ in 0..120 {
        sim.step();
        if !sim.world.bots.contains_key(&victim) {
            break;
        }
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
on signal(s):
    move_to(closest(depot).expect())

log(1)
";
    let mut spec = MapSpec::empty(20, 3);
    spec.depots.push(TilePos::new(19, 1)); // far away: the retreat takes a while
    let mut sim = Sim::new(&spec);
    sim.tuning.regen_amount = 0; // the double-handle race needs static hp
    sim.tuning.fault_damage = 0; // and the chasing brawler must not crash out
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
            "on signal(s):\n    log(\"h\")\n    upload_log()\n\nlog(1)\n",
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

#[test]
fn crash_loops_are_lethal() {
    // No ore anywhere: .expect() faults every program pass. Each unhandled
    // crash chips the chassis; the bot dies into a wreck.
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    let bot = spawn(&mut sim, TilePos::new(2, 2), "move_to(closest(ore).expect())\n", 0, 20);
    for _ in 0..400 {
        sim.step();
        if !sim.world.bots.contains_key(&bot) {
            break;
        }
    }
    assert!(!sim.world.bots.contains_key(&bot), "crash loop must eventually kill");
    assert!(sim.world.wrecks.contains_key(&bot), "fault death is a clean death: wreck");
    // Fewer dumps than crashes: the fatal fault kills the bot inside the
    // default handler before its dump pays off, and the hurt crossing can
    // humbly interrupt another crash's report mid-upload. The survivors
    // are in the cloud; the rest are only in the wreck's black box.
    assert!(
        sim.world.archive.iter().filter(|e| e.kind == ArchiveKind::CrashDump).count() >= 2,
        "the earlier crashes are in the cloud"
    );
}

#[test]
fn error_handlers_are_armor() {
    // Same faulting call, but handled: no crashes, no chassis damage.
    let src = "\
on signal(s):
    wait(1)

move_to(closest(ore).expect())
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    let bot = spawn(&mut sim, TilePos::new(2, 2), src, 0, 20);
    for _ in 0..400 {
        sim.step();
    }
    let b = &sim.world.bots[&bot];
    assert_eq!(b.data.hp, 20, "handled faults must cost no health");
}

#[test]
fn bots_regenerate_and_hurt_rearms() {
    let hurt_src = "\
on signal(s):
    log(\"ouch\")

wait(2)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    sim.tuning.regen_interval_ticks = 5; // fast regen: this test pins the mechanism, not the rate
    let bot = spawn(&mut sim, TilePos::new(2, 2), hurt_src, 0, 100);
    for _ in 0..4 {
        sim.step();
    }
    // First wound below 50%: hurt fires once.
    sim.world.bots.get_mut(&bot).unwrap().data.hp = 60;
    sim.apply_damage_for_test(bot, 20); // 40 -> below threshold
    for _ in 0..40 {
        sim.step();
    }
    let logs = sim.world.bots[&bot].data.log_buf.clone();
    assert_eq!(logs.iter().filter(|l| *l == "\"ouch\"").count(), 1, "hurt fired once: {logs:?}");
    // Regen climbs hp back over 50% (re-arming) and toward max.
    for _ in 0..400 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&bot].data.hp, 100, "regen must reach max");
    assert!(!sim.world.bots[&bot].data.hurt_fired, "hurt re-armed above threshold");
    // A second dip below threshold fires hurt again.
    sim.apply_damage_for_test(bot, 60);
    for _ in 0..40 {
        sim.step();
    }
    let logs = sim.world.bots[&bot].data.log_buf.clone();
    assert_eq!(logs.iter().filter(|l| *l == "\"ouch\"").count(), 2, "hurt re-fired: {logs:?}");
}
