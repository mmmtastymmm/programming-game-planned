//! Combat + signal wiring: damage, hurt windows, the double-handle →
//! abort rule, forced log uploads, XP from fighting (docs/01, docs/02).

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
on hurt:
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
fn abort_files_the_logged_story_then_wrecks() {
    // No death handler exists any more (M3): your black box is whatever
    // you logged while alive. The victim logs during normal operation;
    // hp 0 → abort → the forced upload_log() sends the story home, then
    // become_disabled() drops the wreck.
    let victim_src = "\
log(\"day report\")
wait(60)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    spawn(&mut sim, TilePos::new(1, 1), BRAWLER, 1, 100);
    let victim = spawn(&mut sim, TilePos::new(2, 1), victim_src, 0, 20);
    for _ in 0..300 {
        sim.step();
    }
    assert!(sim.world.wrecks.contains_key(&victim), "every death is a wreck now");
    assert!(
        sim.world
            .archive
            .iter()
            .any(|e| e.kind == ArchiveKind::Log && e.text.contains("day report")),
        "abort's forced upload always sends the logs home; archive: {:?}",
        sim.world.archive
    );
}

#[test]
fn fault_inside_hurt_window_is_double_handle_abort() {
    // The hurt window calls closest(depot).expect() on a map with no
    // depot: the fault inside the PLAYER window is a double handle —
    // abort, not vaporization: the bot drops into a WRECK (the rescue
    // race), and its logs go home via the forced upload (Q50/M3 — the
    // explosion path is gone).
    let victim_src = "\
on hurt:
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
    assert!(!sim.world.bots.contains_key(&victim), "victim downed");
    assert!(
        sim.world.wrecks.contains_key(&victim),
        "double handle aborts into a wreck — no instant-destroy path exists"
    );
    assert!(
        sim.world.archive.iter().any(|e| e.kind == ArchiveKind::Log),
        "the forced upload_log sent the buffer home"
    );
}

#[test]
fn lethal_damage_during_hurt_window_aborts() {
    // Retreat program: the hurt window blocks on a long move while the
    // brawler keeps swinging — hp 0 mid-template is a double handle →
    // abort: the retreat is over, the wreck drops where the bot fell.
    let victim_src = "\
on hurt:
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
    // alive — asserted above it isn't) or death landed mid-template:
    assert!(
        sim.world.wrecks.contains_key(&victim),
        "death during the hurt window aborts into a wreck where it stood"
    );
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
    assert!(sim.world.wrecks.contains_key(&bot), "every death is a wreck now");
    // Fewer dumps than crashes: the hurt crossing (or the fatal chip)
    // lands mid-factory-window eventually — a double handle → abort. The
    // dumps that completed are in the cloud; the rest ride the wreck.
    assert!(
        sim.world.archive.iter().filter(|e| e.kind == ArchiveKind::CrashDump).count() >= 1,
        "at least the first crash filed its dump"
    );
}

#[test]
fn error_handlers_are_armor() {
    // Same faulting call, but handled: no crashes, no chassis damage.
    let src = "\
on error:
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
fn hurt_latch_fires_and_rearms_on_the_same_moved_line() {
    // The edge trigger must fire AND re-arm against the bot's own
    // hurt_line env: with the line raised to 80, a regen tick that lifts
    // hp over tuning's 50 but not over 80 must NOT clear the latch (the
    // old split let the next hit re-fire Hurt mid-template — an unearned
    // double-handle abort).
    let hurt_src = "\
on hurt:
    log(\"ouch\")

wait(2)
";
    let mut sim = Sim::new(&MapSpec::empty(5, 5));
    sim.tuning.regen_interval_ticks = 5;
    let bot = spawn(&mut sim, TilePos::new(2, 2), hurt_src, 0, 100);
    sim.world.bots.get_mut(&bot).unwrap().data.env.insert("hurt_line".into(), 80);
    let ouches = |sim: &Sim| {
        sim.world.bots[&bot].data.log_buf.iter().filter(|l| l.1 == "\"ouch\"").count()
    };
    for _ in 0..4 {
        sim.step();
    }
    sim.apply_damage_for_test(bot, 25); // 75 < 80% line: fires
    for _ in 0..18 {
        sim.step();
    }
    assert_eq!(ouches(&sim), 1, "hurt fired once at the moved line");
    // Regen has lifted hp over 50 (tuning) but not over 80 (env): the
    // latch must still be armed, so another dip must NOT re-fire.
    let hp = sim.world.bots[&bot].data.hp;
    assert!(hp > 50 && hp < 80, "test setup: hp {hp} between the two lines");
    sim.apply_damage_for_test(bot, 4);
    for _ in 0..20 {
        sim.step();
    }
    assert_eq!(ouches(&sim), 1, "latched below the env line: no re-fire");
    // Only once regen crosses THE BOT'S line does the latch re-arm.
    for _ in 0..200 {
        sim.step();
        if sim.world.bots[&bot].data.hp >= 85 {
            break;
        }
    }
    sim.apply_damage_for_test(bot, 15);
    for _ in 0..40 {
        sim.step();
    }
    assert_eq!(ouches(&sim), 2, "re-armed above the env line: fires again");
}

#[test]
fn bots_regenerate_and_hurt_rearms() {
    let hurt_src = "\
on hurt:
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
    assert_eq!(logs.iter().filter(|l| l.1 == "\"ouch\"").count(), 1, "hurt fired once: {logs:?}");
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
    assert_eq!(logs.iter().filter(|l| l.1 == "\"ouch\"").count(), 2, "hurt re-fired: {logs:?}");
}
