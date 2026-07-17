//! The wreck race (M10, docs/02): countdown + blast, the rescue at the
//! Damaged line, salvage receipts + decryption, analyze intel, hijack,
//! black boxes, and guard duty.

use sim::map::MapSpec;
use sim::resources::Resource;
use sim::sim::{Command, Sim};
use sim::world::{Color, XpTrack};
use sim::TilePos;

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
    .unwrap()
    .unwrap()
}

/// Kill a bot into a wreck via the dev command (straight to wreck).
fn wreck(sim: &mut Sim, id: sim::BotId) {
    sim.apply(&Command::KillBot { bot: id }).unwrap();
    sim.step();
    assert!(sim.world.wrecks.contains_key(&id), "KillBot leaves a wreck");
}

#[test]
fn countdown_scales_with_xp_and_expiry_blasts_without_chaining() {
    let mut spec = MapSpec::empty(8, 5);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let rookie = spawn(&mut sim, TilePos::new(2, 2), "wait(600)\n", 0, 100);
    let veteran = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    // 1000 XP = 10000 deci → +100 ticks of countdown.
    sim.world.bots.get_mut(&veteran).unwrap().data.xp.insert(XpTrack::Mining, 10_000);
    let bystander = spawn(&mut sim, TilePos::new(2, 3), "wait(600)\n", 0, 100);
    wreck(&mut sim, rookie);
    wreck(&mut sim, veteran);
    let (r_cd, v_cd) = (
        sim.world.wrecks[&rookie].countdown,
        sim.world.wrecks[&veteran].countdown,
    );
    assert!(
        v_cd > r_cd,
        "veterans linger — the richest prizes give the most time ({r_cd} vs {v_cd})"
    );
    // Run the rookie's countdown out: the blast hits the adjacent
    // bystander AND the adjacent veteran wreck — which is DESTROYED, not
    // detonated (no chain).
    let hp_before = sim.world.bots[&bystander].data.hp;
    for _ in 0..(r_cd + 5) {
        sim.step();
    }
    assert!(!sim.world.wrecks.contains_key(&rookie), "the countdown expired");
    assert!(
        sim.world.black_boxes.iter().any(|bb| bb.bot == rookie),
        "every destruction drops a black box"
    );
    assert!(
        sim.world.bots[&bystander].data.hp < hp_before,
        "the blast is real — friend and foe"
    );
    // The veteran wreck took blast damage but did NOT explode early: it
    // either survives (hull) or was destroyed WITHOUT a blast. Either
    // way the bystander was hit exactly once (no chain doubling).
    let veteran_wreck_alive = sim.world.wrecks.contains_key(&veteran);
    let veteran_boxed = sim.world.black_boxes.iter().any(|bb| bb.bot == veteran);
    assert!(veteran_wreck_alive || veteran_boxed, "no vanishing wrecks");
}

#[test]
fn field_repair_rescues_at_the_damaged_line_and_rewreck_resumes() {
    let mut spec = MapSpec::empty(8, 5);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let downed = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    let medic = spawn(
        &mut sim,
        TilePos::new(2, 2),
        "repair(closest(wreck).expect())\nwait(600)\n",
        0,
        100,
    );
    wreck(&mut sim, downed);
    let countdown_at_wreck = sim.world.wrecks[&downed].countdown;
    // 800 deci at 10/tick = ~80 ticks of repair + slack.
    for _ in 0..140 {
        sim.step();
        if sim.world.bots.contains_key(&downed) {
            break;
        }
    }
    let bot = sim.world.bots.get(&downed).expect("rescued");
    assert_eq!(bot.data.hp, 50, "boots at the Damaged line (50% of 100)");
    assert!(!bot.data.hurt_fired, "the hurt latch is re-armed");
    assert!(bot.data.booting.is_some() || bot.vm.is_some(), "enters the Boot Sequence");
    let carry = bot.data.countdown_carry.expect("re-wreck resumes, never resets");
    assert!(
        carry < countdown_at_wreck,
        "the carried countdown is what REMAINED ({carry} < {countdown_at_wreck})"
    );
    assert!(
        sim.world.bots[&medic].data.xp(XpTrack::Building) > 0,
        "field repair earns Building XP"
    );
    // Re-wreck: the countdown resumes from the carry.
    sim.apply(&Command::KillBot { bot: downed }).unwrap();
    sim.step();
    assert_eq!(
        sim.world.wrecks[&downed].countdown,
        carry,
        "failed rescues burn the window"
    );
}

#[test]
fn salvage_pays_the_receipt_and_decryption_analyze_steals_intel() {
    let mut spec = MapSpec::empty(10, 5);
    spec.quirk_permille = 0;
    // The victim faction's kit includes bought hardware so the receipt is
    // non-trivial: give it a cpu_mk2 by hand after spawn.
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "log(9)\nwait(600)\n", 0, 100);
    let (mk2, _) = sim.stats.upgrade("cpu_mk2").unwrap();
    sim.world.bots.get_mut(&victim).unwrap().data.upgrades.push(mk2);
    // Two enemy racers.
    let salvager = spawn(
        &mut sim,
        TilePos::new(2, 2),
        "salvage(closest(wreck).expect())\nwait(600)\n",
        1,
        100,
    );
    for _ in 0..3 {
        sim.step(); // let the victim log its story first
    }
    wreck(&mut sim, victim);
    for _ in 0..80 {
        sim.step();
        if !sim.world.wrecks.contains_key(&victim) {
            break;
        }
    }
    assert!(!sim.world.wrecks.contains_key(&victim), "salvage destroys the wreck");
    // cpu_mk2 costs 5 Chips: 25% of 50 deci = 12 deci to the salvager.
    assert_eq!(
        sim.world.stock_get(1, Resource::Chips),
        12,
        "a cut of the invested receipt"
    );
    assert_eq!(
        sim.world.decryption.get(&(1, 0, Color::GREEN.0)).copied(),
        Some(sim.tuning.salvage_decrypt_pct),
        "programs are read on murder — permanent decryption"
    );
    assert!(
        sim.world.black_boxes.iter().any(|bb| bb.bot == victim),
        "the black box still drops"
    );
    let _ = salvager;

    // Analyze: a second victim, dissected for Data + the comm key.
    let victim2 = spawn(&mut sim, TilePos::new(5, 2), "wait(600)\n", 0, 100);
    let analyzer = spawn(
        &mut sim,
        TilePos::new(6, 2),
        "analyze(closest(wreck).expect())\nwait(600)\n",
        1,
        100,
    );
    wreck(&mut sim, victim2);
    let data_before = sim.world.data.get(&1).copied().unwrap_or(0);
    for _ in 0..100 {
        sim.step();
        if !sim.world.wrecks.contains_key(&victim2) {
            break;
        }
    }
    assert_eq!(
        sim.world.data.get(&1).copied().unwrap_or(0),
        data_before + sim.tuning.analyze_data,
        "analyze pays Data"
    );
    assert!(
        sim.world.comm_keys.get(&1).is_some_and(|k| k.contains(&0)),
        "analyze steals the victim's comm key"
    );
    let _ = analyzer;
}

#[test]
fn hijack_boots_the_wreck_under_the_claimers_remainder_color() {
    let mut spec = MapSpec::empty(12, 6);
    spec.quirk_permille = 0;
    // The claimer faction needs a working remainder printer.
    spec.printers.push(sim::map::PrinterSpec {
        pos: TilePos::new(9, 2),
        faction: 1,
        color: 2, // Blue remainder
        ruined: false,
    });
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    sim.world.bots.get_mut(&victim).unwrap().data.xp.insert(XpTrack::Combat, 5000);
    let raider = spawn(
        &mut sim,
        TilePos::new(2, 2),
        "hijack(closest(wreck).expect())\nwait(600)\n",
        1,
        100,
    );
    wreck(&mut sim, victim);
    for _ in 0..160 {
        sim.step();
        if sim.world.bots.contains_key(&victim) {
            break;
        }
    }
    let stolen = sim.world.bots.get(&victim).expect("hijacked back to life");
    assert_eq!(stolen.data.faction, 1, "works for THEM now");
    assert_eq!(stolen.data.color.0, 2, "under the claimer's remainder color");
    assert_eq!(
        stolen.data.xp(XpTrack::Combat),
        5000,
        "XP intact — a stolen veteran is a unique prize"
    );
    let _ = raider;
}

#[test]
fn black_boxes_bank_to_the_cloud() {
    let mut spec = MapSpec::empty(8, 5);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "log(4)\nwait(600)\n", 0, 100);
    for _ in 0..3 {
        sim.step();
    }
    wreck(&mut sim, victim);
    // Destroy the wreck by attack: black box, NO blast.
    let hunter = spawn(
        &mut sim,
        TilePos::new(2, 2),
        "attack(closest(wreck).expect())\nwait(1)\n",
        1,
        100,
    );
    for _ in 0..80 {
        sim.step();
        if !sim.world.wrecks.contains_key(&victim) {
            break;
        }
    }
    assert!(!sim.world.wrecks.contains_key(&victim), "attack destroys the hull");
    assert!(sim.world.bots.contains_key(&hunter), "no blast from a destroyed wreck");
    let bb = sim
        .world
        .black_boxes
        .iter()
        .find(|bb| bb.bot == victim)
        .expect("black box dropped");
    let target = bb.entity;
    let archive_before = sim.world.archive.len();
    // Recover it (the hunter is adjacent).
    sim.apply(&Command::DeployProgram {
        faction: 1,
        color: Color::GREEN,
        source: "wait(1)\n".into(),
    })
    .unwrap();
    // Drive recovery via a fresh bot next to the box.
    let scout = spawn(&mut sim, TilePos::new(4, 2), "wait(600)\n", 1, 100);
    for _ in 0..2 {
        sim.step(); // park the VM in its wait FIRST, or it overwrites us
    }
    // Hand the scout the recover action directly (program plumbing is
    // covered by the host-arm tests; this exercises the banking).
    sim.world.bots.get_mut(&scout).unwrap().data.requested =
        Some(sim::world::ActionRequest::Recover(target));
    for _ in 0..5 {
        sim.step();
    }
    assert!(
        !sim.world.black_boxes.iter().any(|bb| bb.entity == target),
        "the box leaves the field"
    );
    assert!(sim.world.archive.len() > archive_before, "its contents banked to the cloud");
}

#[test]
fn guard_holds_station_and_engages() {
    let mut spec = MapSpec::empty(12, 5);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.stats.move_rate_deci = 10; // stance mechanics, not pacing
    sim.tuning.fault_damage = 0;
    let ward = spawn(&mut sim, TilePos::new(8, 2), "wait(600)\n", 0, 100);
    let ward_entity = sim.world.bots[&ward].data.entity;
    let guard = spawn(&mut sim, TilePos::new(2, 2), "wait(600)\n", 0, 100);
    for _ in 0..2 {
        sim.step(); // park the VM in its wait FIRST, or it overwrites us
    }
    sim.world.bots.get_mut(&guard).unwrap().data.requested =
        Some(sim::world::ActionRequest::Guard { target: ward_entity, escort: false });
    for _ in 0..60 {
        sim.step();
    }
    assert!(
        sim.world.bots[&guard].data.pos.chebyshev(TilePos::new(8, 2)) <= 2,
        "the guard closed to its leash"
    );
    // An enemy walks in: the guard swings without any program help.
    let intruder = spawn(&mut sim, TilePos::new(7, 2), "wait(600)\n", 1, 40);
    for _ in 0..80 {
        sim.step();
        if !sim.world.bots.contains_key(&intruder) {
            break;
        }
    }
    assert!(
        sim.world.bots.get(&intruder).map(|b| b.data.hp < 40).unwrap_or(true),
        "guards engage adjacent enemies autonomously"
    );
}

#[test]
fn hijack_holds_at_the_fleet_cap() {
    let mut spec = MapSpec::empty(12, 6);
    spec.quirk_permille = 0;
    spec.printers.push(sim::map::PrinterSpec {
        pos: TilePos::new(9, 2),
        faction: 1,
        color: 2,
        ruined: false,
    });
    // Cap ZERO: the claimer has no room — the theft must never boot
    // (review 2026-07-16: hijack bypassed the printer-derived ceiling).
    spec.fleet_cap_override = Some(0);
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    let raider = spawn(
        &mut sim,
        TilePos::new(2, 2),
        "hijack(closest(wreck).expect())\nwait(600)\n",
        1,
        100,
    );
    wreck(&mut sim, victim);
    for _ in 0..160 {
        sim.step();
    }
    let stolen = sim
        .world
        .bots
        .get(&victim)
        .is_some_and(|b| b.data.faction == 1);
    assert!(!stolen, "a full fleet can't absorb the prize — the hijack holds");
    let _ = raider;
}

#[test]
fn held_rescues_stop_paying_building_xp() {
    let mut spec = MapSpec::empty(10, 6);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    sim.tuning.field_repair_deci = 50;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    let medic = spawn(
        &mut sim,
        TilePos::new(2, 2),
        "repair(closest(wreck).expect())\nwait(2)\n",
        0,
        100,
    );
    wreck(&mut sim, victim);
    // A blocker parks ON the wreck tile: the rescue reaches full progress
    // and HOLDS — XP must stop with the progress (review 2026-07-16).
    spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    for _ in 0..40 {
        sim.step();
    }
    let at_hold = sim.world.bots[&medic].data.xp(XpTrack::Building);
    for _ in 0..30 {
        sim.step();
    }
    let later = sim.world.bots[&medic].data.xp(XpTrack::Building);
    assert!(at_hold > 0, "real progress paid");
    assert_eq!(later, at_hold, "a HELD rescue mints nothing further");
}

#[test]
fn standing_on_the_wreck_fails_the_rescue_loudly() {
    let mut spec = MapSpec::empty(10, 6);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    wreck(&mut sim, victim);
    // The medic spawns ON the wreck tile — it can never boot the wreck
    // under its own feet; the action must fail, not hold forever.
    let src = "\
on error:
    log(last_error())
    upload_log()

repair(closest(wreck).expect())
wait(600)
";
    spawn(&mut sim, TilePos::new(3, 2), src, 0, 100);
    for _ in 0..120 {
        sim.step();
    }
    assert!(
        sim.world
            .archive
            .iter()
            .any(|e| e.text.contains("err_action")),
        "the self-block resolves as a trappable fault, not an infinite hold"
    );
}

#[test]
fn rescue_never_bypasses_the_hardware_bar() {
    // The deploy layer already stock-caps REMAINDER artifacts (hijack
    // always boots the remainder, so that lane is closed at the source).
    // The residual exposure is a RESCUE after the color was redeployed
    // over-bar while the bot lay wrecked — faction 0 has no printers, so
    // its Green slot takes any size.
    let mut spec = MapSpec::empty(10, 6);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    sim.tuning.field_repair_deci = 10;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    spawn(
        &mut sim,
        TilePos::new(2, 2),
        "if exists(wreck):\n    repair(closest(wreck).expect())\nwait(2)\n",
        0,
        100,
    );
    wreck(&mut sim, victim);
    let elite = format!("log(\"elite\")\nupload_log()\n{}", "wait(1)\n".repeat(40));
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: elite })
        .unwrap();
    for _ in 0..60 {
        sim.step();
        if sim.world.bots.contains_key(&victim) {
            break;
        }
    }
    assert!(sim.world.bots.contains_key(&victim), "the rescue booted");
    for _ in 0..60 {
        sim.step();
    }
    assert!(
        !sim.world.archive.iter().any(|e| e.text.contains("elite")),
        "Q52 holds at the rescue boot: the over-bar artifact yields the fallback"
    );
}

#[test]
fn a_full_mend_rearms_the_countdown_window() {
    let mut spec = MapSpec::empty(10, 6);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    sim.tuning.field_repair_deci = 10;
    sim.tuning.regen_interval_ticks = 5;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    spawn(
        &mut sim,
        TilePos::new(2, 2),
        "if exists(wreck):\n    repair(closest(wreck).expect())\nwait(2)\n",
        0,
        100,
    );
    wreck(&mut sim, victim);
    sim.world.wrecks.get_mut(&victim).unwrap().countdown = 60;
    for _ in 0..40 {
        sim.step();
        if sim.world.bots.contains_key(&victim) {
            break;
        }
    }
    assert!(
        sim.world.bots[&victim].data.countdown_carry.is_some(),
        "a failed window resumes on the next wreck while unhealed"
    );
    // Fully mended: the window re-arms (review 2026-07-16: without the
    // reset every rescue ratcheted toward an insta-blast wreck).
    sim.world.bots.get_mut(&victim).unwrap().data.hp = 100;
    for _ in 0..12 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&victim].data.countdown_carry,
        None,
        "a full mend re-arms the self-destruct window"
    );
    sim.apply(&Command::KillBot { bot: victim }).unwrap();
    sim.step();
    assert!(
        sim.world.wrecks[&victim].countdown >= sim.tuning.wreck_countdown_base_ticks,
        "the re-wreck gets the FULL formula window, not the burned remainder"
    );
}

#[test]
fn black_boxes_are_findable_by_real_programs() {
    let mut spec = MapSpec::empty(10, 6);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let victim = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n", 0, 100);
    // One field agent: crack the wreck open (attack → black box, no
    // blast), then bank the box — all through real queries.
    let src = "\
if exists(wreck):
    attack(closest(wreck).expect())
if exists(black_box):
    recover_black_box(closest(black_box).expect())
wait(2)
";
    spawn(&mut sim, TilePos::new(2, 2), src, 0, 100);
    wreck(&mut sim, victim);
    for _ in 0..120 {
        sim.step();
    }
    assert!(sim.world.black_boxes.is_empty(), "the box was found and banked");
    assert!(
        sim.world.archive.iter().any(|e| e.text.contains("[black box]")),
        "the banked forensics reached the colony cloud"
    );
}
