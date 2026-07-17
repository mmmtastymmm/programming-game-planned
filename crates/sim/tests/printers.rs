//! Printers v2 (M9): target shares, the allocation, recall dispatch,
//! ghosts, hardware bars, and scrap
//! (docs/01 "Program Colors" + "Target shares" + "The recall interrupt").

use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::{Color, PrintTarget, PrinterState, SelectKey};
use sim::{EntityId, TilePos};

const IDLER: &str = "log(1)\n";
const BRAWLER: &str = "attack(closest(enemy).expect())\n";

/// Base map: a working Green printer (the remainder bucket — first-born)
/// and a ruined Red one (the doc's starting state), plus seed ore.
fn colony_map() -> MapSpec {
    let mut spec = MapSpec::empty(12, 8);
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 2),
        faction: 0,
        color: 0, // Green — first-born: the remainder bucket
        ruined: false,
    });
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(4, 2),
        faction: 0,
        color: 1, // Red
        ruined: true,
    });
    spec.starting_ore = 100;
    spec
}

fn printer_ids(sim: &Sim) -> Vec<EntityId> {
    sim.world.printers.keys().copied().collect()
}

fn spawn_green(sim: &mut Sim, pos: TilePos) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: IDLER.into(),
        cpu: 2,
        cargo_cap: 1,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

/// Dial a printer: target + key, defaults elsewhere.
fn dial(sim: &mut Sim, printer: EntityId, target: u32, key: SelectKey, best_first: bool) {
    sim.apply(&Command::EditPrinterRules {
        printer,
        target: PrintTarget::Count(target),
        key,
        best_first,
        priority: 0,
        check_interval: None,
    })
    .unwrap();
}

#[test]
fn remainder_prints_to_the_fleet_cap() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(3);
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    for _ in 0..100 {
        sim.step();
    }
    // Cap = 3 × 1 working printer (the ruined Red contributes nothing).
    assert_eq!(sim.world.bots.len(), 3, "the remainder prints to the cap and stops");
    assert!(sim.world.bots.values().all(|b| b.data.color == Color::GREEN));
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Iron),
        1000,
        "prints are free by default (print_cost_steel 0) — iron stock untouched"
    );
    // Printed bots booted and are running their program (logging).
    assert!(sim.world.bots.values().all(|b| !b.data.log_buf.is_empty()));
}

#[test]
fn dialed_printer_prints_its_own_color_after_repair() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(1);
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::RED, source: IDLER.into() })
        .unwrap();
    dial(&mut sim, red, 1, SelectKey::TotalXp, true);
    for _ in 0..50 {
        sim.step();
    }
    assert!(
        sim.world.bots.values().all(|b| b.data.color != Color::RED),
        "ruined printers print nothing"
    );

    // Repair prices in DATA (docs/03): without Data it stays ruined.
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    assert_eq!(sim.world.printers[&red].state, PrinterState::Ruined, "no Data, no repair");
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    assert_eq!(sim.world.printers[&red].state, PrinterState::Working);
    assert_eq!(sim.world.data.get(&0).copied().unwrap_or(0), 0, "repair drained the Data");
    for _ in 0..60 {
        sim.step();
    }
    // Cap grew to 2; the dialed printer prints FIRST (priority before the
    // remainder), so exactly one Red exists.
    assert_eq!(
        sim.world.bots.values().filter(|b| b.data.color == Color::RED).count(),
        1,
        "the dialed printer prints its own color when short of its target"
    );
}

#[test]
fn rule_edit_recolors_by_key_keeping_xp() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(1); // cap 2 once red works: no prints
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();

    let veteran = spawn_green(&mut sim, TilePos::new(2, 3));
    let rookie = spawn_green(&mut sim, TilePos::new(3, 3));
    sim.world.bots.get_mut(&veteran).unwrap().data.xp.insert(sim::world::XpTrack::Mining, 500);

    // Red claims ONE bot, worst-first on total XP: the rookie.
    dial(&mut sim, red, 1, SelectKey::TotalXp, false);
    for _ in 0..80 {
        sim.step();
    }
    let rookie_bot = &sim.world.bots[&rookie];
    let veteran_bot = &sim.world.bots[&veteran];
    assert_eq!(rookie_bot.data.color, Color::RED, "worst-first claim takes the rookie");
    assert_eq!(veteran_bot.data.color, Color::GREEN, "the veteran stays remainder");
    assert_eq!(veteran_bot.data.xp(sim::world::XpTrack::Mining), 500, "XP rides the bot");
    assert_eq!(sim.world.bots.len(), 2, "rebalancing loses nobody");
}

#[test]
fn best_first_claim_takes_the_veteran() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(1);
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    let veteran = spawn_green(&mut sim, TilePos::new(2, 3));
    let _rookie = spawn_green(&mut sim, TilePos::new(3, 3));
    sim.world.bots.get_mut(&veteran).unwrap().data.xp.insert(sim::world::XpTrack::Combat, 700);

    dial(&mut sim, red, 1, SelectKey::Xp(sim::world::XpTrack::Combat), true);
    // The walk may bump the parked rookie (50-tick freeze) and detour.
    for _ in 0..400 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&veteran].data.color,
        Color::RED,
        "best-first on Combat XP keeps the fighters Red"
    );
}

#[test]
fn cap_pct_targets_read_the_cap_not_the_fleet() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(4); // cap = 4 (green) + 4 (red once repaired)
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::RED, source: IDLER.into() })
        .unwrap();
    // 25% of the 8-bot cap = 2, floored — regardless of live fleet size.
    sim.apply(&Command::EditPrinterRules {
        printer: red,
        target: PrintTarget::CapPct(25),
        key: SelectKey::TotalXp,
        best_first: true,
        priority: 0,
        check_interval: None,
    })
    .unwrap();
    for _ in 0..300 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots.values().filter(|b| b.data.color == Color::RED).count(),
        2,
        "CapPct(25) of an 8 cap = 2 Reds, floored"
    );
    assert_eq!(sim.world.bots.len(), 8, "the remainder fills the rest of the cap");
}

#[test]
fn ghost_machines_orphan_and_rejoin_on_repair() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(2);
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    // Two RED bots while the Red printer is ruined: ghosts — no working
    // printer owns their color.
    for pos in [TilePos::new(6, 5), TilePos::new(7, 5)] {
        sim.apply(&Command::SpawnBot {
            pos,
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::RED,
        })
        .unwrap()
        .unwrap();
    }
    let ghosts: Vec<_> = sim.world.bots.keys().copied().collect();
    assert!(sim.world.bots.values().all(|b| sim.world.is_ghost(&b.data)));
    for _ in 0..50 {
        sim.step();
    }
    // Ghosts are outside the allocation and exempt from scrap: nobody was
    // recalled, nobody re-colored, nobody scrapped (fleet size is ZERO —
    // ghosts aren't fleet — while prints filled the cap alongside them).
    for id in &ghosts {
        let bot = &sim.world.bots[id];
        assert_eq!(bot.data.color, Color::RED, "ghosts keep their frozen color");
        assert!(bot.data.recall.is_none(), "nobody force-marches ghosts home");
    }
    // Retake (repair) the printer: the ghosts are uploaded again — the
    // allocation claims them like any member (Red has no target, so the
    // remainder Green absorbs them via recalls).
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    assert!(
        sim.world.bots.values().filter(|b| ghosts.contains(&b.data.id)).all(
            |b| !sim.world.is_ghost(&b.data)
        ),
        "a working printer re-uploads its survivors"
    );
}

#[test]
fn hardware_bar_gates_claims_and_the_remainder_deploy() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(1);
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    let bot = spawn_green(&mut sim, TilePos::new(2, 3));

    // A 33-line artifact exceeds the stock 32-line memory: the REMAINDER
    // (Green) must refuse it outright — it has to fit any bot.
    let long_program = "log(1)\n".repeat(33);
    let refused = sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::GREEN,
        source: long_program.clone(),
    });
    assert!(refused.is_err(), "the remainder program must fit stock hardware");

    // Deployed to RED (a dialed color) it's legal — but Red then claims
    // only bots whose bought hardware fits, which stock bots don't.
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::RED, source: long_program })
        .unwrap();
    dial(&mut sim, red, 1, SelectKey::TotalXp, true);
    for _ in 0..60 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&bot].data.color,
        Color::GREEN,
        "an over-bar color claims no stock bots (Q52: the bar filters before the key)"
    );
    // And it never prints either: fresh prints are stock machines.
    assert!(
        sim.world.bots.values().all(|b| b.data.color != Color::RED),
        "above-stock-bar printers don't print"
    );
}

#[test]
fn deploy_drops_land_politely_as_lame_ducks() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(1);
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    let bot = spawn_green(&mut sim, TilePos::new(2, 3));
    dial(&mut sim, red, 1, SelectKey::TotalXp, true);
    for _ in 0..80 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&bot].data.color, Color::RED, "claimed by the dial");

    // A fatter RED deploy raises the bar over the bot's stock hardware:
    // the drop is assignment-at-once but the recall lands POLITELY — via
    // the pending queue, never a signal.
    sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::RED,
        source: "log(1)\n".repeat(33),
    })
    .unwrap();
    assert!(
        sim.world.pending_recalls.contains_key(&bot),
        "the dropped member queues politely (the lame-duck rule)"
    );
    // The walk home may bump idle prints parked at the doorstep (50-tick
    // freeze each) and replan around them.
    for _ in 0..400 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&bot].data.color,
        Color::GREEN,
        "the lame duck lands at the remainder"
    );
}

#[test]
fn damage_during_recall_walk_is_double_handle() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(1);
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();

    // Victim far from home so the walk takes a while; brawler adjacent.
    let victim = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(10, 6),
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 10,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(11, 6),
        source: BRAWLER.into(),
        cpu: 8,
        cargo_cap: 1,
        faction: 1,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap();

    // Claim the victim Red: the rule edit fires a signal-like recall.
    dial(&mut sim, red, 1, SelectKey::TotalXp, true);
    for _ in 0..100 {
        sim.step();
        if !sim.world.bots.contains_key(&victim) {
            break;
        }
    }
    assert!(!sim.world.bots.contains_key(&victim), "victim downed mid-recall");
    assert!(
        sim.world.wrecks.contains_key(&victim),
        "signal during recall = double handle = abort → wreck (M3: no instant destroy)"
    );
}

#[test]
fn over_capacity_scraps_lowest_total_xp_for_refund() {
    // Scrap is an ECONOMY event only (docs/01: a shrunken cap stops
    // prints, never scraps) — the surviving trigger is sustained Steel
    // shortfall with `rust_scraps` on (M5 Q84).
    let mut spec = colony_map();
    spec.dev_free_power = false; // no Steel in stock: rust from the start
    spec.fleet_cap_override = Some(0); // no prints: scrap semantics only
    let mut sim = Sim::new(&spec);
    sim.upkeep.interval_ticks = 5;
    sim.upkeep.rust_scraps = true;
    let veteran = spawn_green(&mut sim, TilePos::new(2, 3));
    spawn_green(&mut sim, TilePos::new(3, 3));
    // TOTAL XP decides (M9: every track counts — Building included).
    sim.world.bots.get_mut(&veteran).unwrap().data.xp.insert(sim::world::XpTrack::Building, 900);

    let ore_before = sim.world.stock_get(0, sim::resources::Resource::Iron);
    // The valve fires per sustained settlement; disarm it after the first
    // recall so the SELECTION (lowest total XP) is what this test proves.
    let mut fired = false;
    for _ in 0..200 {
        sim.step();
        if !fired
            && sim.world.bots.values().any(|b| {
                matches!(
                    b.data.recall,
                    Some(sim::world::Recall { purpose: sim::world::RecallPurpose::Scrap, .. })
                )
            })
        {
            sim.upkeep.rust_scraps = false;
            fired = true;
        }
        if sim.world.bots.len() == 1 {
            break;
        }
    }
    assert_eq!(sim.world.bots.len(), 1, "sustained rust scraps the colony down");
    assert!(sim.world.bots.contains_key(&veteran), "the veteran survives (Building XP counts)");
    assert_eq!(
        sim.world.stock_get(0, sim::resources::Resource::Iron),
        ore_before + sim.tuning.scrap_refund_steel,
        "scrap refunds partial ore"
    );
    assert!(sim.world.wrecks.is_empty(), "scrapping is recycling, not destruction");
}

#[test]
fn scrap_walk_ends_beside_the_printer_for_a_visible_tick() {
    // The walk's last step must be observable from outside the sim: the
    // viewer plays the disassembly wherever it last saw the bot, so being
    // consumed in the same tick as the final step reads as a bot scrapped
    // mid-stride, tiles away from the printer.
    // Driven by the economy valve (sustained rust), like every scrap.
    let mut spec = colony_map();
    spec.dev_free_power = false;
    spec.fleet_cap_override = Some(0); // no prints: scrap semantics only
    let mut sim = Sim::new(&spec);
    sim.upkeep.interval_ticks = 5;
    sim.upkeep.rust_scraps = true;
    let veteran = spawn_green(&mut sim, TilePos::new(2, 3));
    // The victim starts across the map, so the recall is a real walk.
    let victim = spawn_green(&mut sim, TilePos::new(10, 6));
    sim.world.bots.get_mut(&veteran).unwrap().data.xp.insert(sim::world::XpTrack::Combat, 900);

    let printer_pos = TilePos::new(2, 2);
    let mut last_seen = TilePos::new(10, 6);
    for _ in 0..200 {
        sim.step();
        match sim.world.bots.get(&victim) {
            Some(bot) => last_seen = bot.data.pos,
            None => break,
        }
    }
    assert!(!sim.world.bots.contains_key(&victim), "victim never scrapped");
    assert_eq!(
        last_seen.manhattan(printer_pos),
        1,
        "the victim's last observable position must be directly (orthogonally) \
         beside the printer — no diagonal corner-touch, no mid-stride vanish \
         (last seen {last_seen:?})"
    );
}

#[test]
fn reprint_queue_counts_down_as_jobs_start() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(2);
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::QueuePrint { faction: 0 }).unwrap();
    sim.apply(&Command::QueuePrint { faction: 0 }).unwrap();
    assert_eq!(sim.world.reprint_queue.get(&0), Some(&2));
    for _ in 0..60 {
        sim.step();
    }
    assert!(
        sim.world.reprint_queue.is_empty(),
        "queued reprints are consumed as print jobs start"
    );
    assert_eq!(sim.world.bots.len(), 2, "a reprint IS a fresh print — the cap still rules");
}

#[test]
fn printers_are_born_with_an_empty_program_file() {
    let sim = Sim::new(&colony_map());
    for slot in [(0u8, 0u8), (0, 1)] {
        let cp = sim.world.color_programs.get(&slot).expect("born with a file");
        assert_eq!(cp.source, "", "the birth file is empty (Q85)");
    }
}

#[test]
fn printer_colony_is_deterministic() {
    let build = || {
        let mut spec = colony_map();
        spec.fleet_cap_override = Some(3); // cap 6 once red works
        let mut sim = Sim::new(&spec);
        let red = printer_ids(&sim)[1];
        sim.apply(&Command::DeployProgram {
            faction: 0,
            color: Color::GREEN,
            source: IDLER.into(),
        })
        .unwrap();
        sim.apply(&Command::DeployProgram { faction: 0, color: Color::RED, source: IDLER.into() })
            .unwrap();
        sim.world.data.insert(0, sim.tuning.repair_cost_data);
        sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
        dial(&mut sim, red, 2, SelectKey::TotalXp, true);
        sim.apply(&Command::QueuePrint { faction: 0 }).unwrap();
        sim
    };
    let mut a = build();
    let mut b = build();
    for tick in 0..300 {
        a.step();
        b.step();
        assert_eq!(a.state_hash(), b.state_hash(), "desync at tick {tick}");
    }
    assert_eq!(a.world.bots.len(), 6, "red prints its 2, the remainder fills to the cap");
}

/// A recall target with NO route to it must not start the walk: an empty
/// path reads as "arrived", which would scrap the victim in place across
/// the map (or teleport a recolor). Unreachable home = no recall at all.
#[test]
fn unreachable_home_printer_never_scraps_in_place() {
    let mut spec = MapSpec::empty(10, 5);
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(8, 2),
        faction: 0,
        color: 0,
        ruined: false,
    });
    spec.fleet_cap_override = Some(1); // 2 live bots → capacity scrap wants a victim
    // Wall the printer's side of the map off with water.
    for y in 0..5 {
        spec.water.push(TilePos::new(6, y));
    }
    let mut sim = Sim::new(&spec);
    for pos in [TilePos::new(1, 1), TilePos::new(1, 3)] {
        sim.apply(&Command::SpawnBot {
            pos,
            source: IDLER.into(),
            cpu: 4,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    }
    for _ in 0..100 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 2, "no bot may be scrapped in place across the water");
    assert!(
        sim.world.bots.values().all(|b| b.data.recall.is_none()),
        "no recall walk may start toward an unreachable printer"
    );
}


/// Review fix (M9): a dial nudge must never wreck bots in ENGINE states —
/// booting (fresh prints) and pad-sitting defer to the polite queue
/// instead of being signal-recalled into an abort.
#[test]
fn rule_edits_defer_for_booting_bots_instead_of_wrecking() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(2);
    let mut sim = Sim::new(&spec);
    sim.tuning.boot_ticks = 40; // a long, observable boot window
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    // Let the remainder print one bot and catch it MID-BOOT.
    let mut booting: Option<sim::BotId> = None;
    for _ in 0..30 {
        sim.step();
        if let Some((id, _)) =
            sim.world.bots.iter().find(|(_, b)| b.data.booting.is_some())
        {
            booting = Some(*id);
            break;
        }
    }
    let booting = booting.expect("a print must boot");
    // Claim everything for Red while the bot is booting.
    dial(&mut sim, red, 2, SelectKey::TotalXp, true);
    assert!(
        sim.world.bots.contains_key(&booting),
        "the dial nudge must not wreck a booting bot"
    );
    assert!(sim.world.wrecks.is_empty(), "no wreck from a rule edit landing mid-boot");
    // The deferred recall lands politely once the boot finishes.
    for _ in 0..400 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&booting].data.color,
        Color::RED,
        "the deferred claim lands after the boot"
    );
}

/// Review fix (M9): a ruined REMAINDER printer receives nobody — surplus
/// bots of a WORKING color keep that color instead of being marched to
/// the ruin, re-colored, and turned into permanent ghosts.
#[test]
fn ruined_remainder_never_liquidates_the_fleet_into_ghosts() {
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(2);
    // Ruin the FIRST-BORN (remainder, Green); keep Red working.
    spec.printers[0].ruined = true;
    spec.printers[1].ruined = false;
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    // Two RED bots: the dial claims one; pre-fix the surplus one was
    // assigned to the ruined remainder and ghosted on arrival.
    let mut spawn_red = |sim: &mut Sim, pos| -> sim::BotId {
        sim.apply(&Command::SpawnBot {
            pos,
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::RED,
        })
        .unwrap()
        .unwrap()
    };
    let a = spawn_red(&mut sim, TilePos::new(6, 3));
    let b = spawn_red(&mut sim, TilePos::new(7, 3));
    dial(&mut sim, red, 1, SelectKey::TotalXp, true);
    for _ in 0..300 {
        sim.step();
    }
    for id in [a, b] {
        let bot = &sim.world.bots[&id];
        assert_eq!(bot.data.color, Color::RED, "nobody re-colors at a ruin");
        assert!(!sim.world.is_ghost(&bot.data), "no manufactured ghosts");
        assert!(bot.data.recall.is_none(), "the surplus bot stays put");
    }
}

/// Review fix (M9): pre-M9 replay files still DESERIALIZE — the legacy
/// SetDesiredMax variant is kept as an alias into the v2 rules.
#[test]
fn legacy_set_desired_max_still_parses_and_applies() {
    let ron = r#"SetDesiredMax(printer: (3), value: 2)"#;
    let command: Command = ron::from_str(ron).expect("legacy command deserializes");
    let mut spec = colony_map();
    spec.fleet_cap_override = Some(3);
    let mut sim = Sim::new(&spec);
    let red = printer_ids(&sim)[1];
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    sim.apply(&Command::SetDesiredMax { printer: red, value: 2 }).unwrap();
    assert!(
        matches!(
            sim.world.printers[&red].rules,
            Some(sim::world::PrinterRules { target: PrintTarget::Count(2), .. })
        ),
        "the legacy dial maps to a Count target"
    );
    let _ = command;
}
