//! Printers, colors, boot, and the recall interrupt
//! (docs/01 "Program Colors" + "The recall interrupt", docs/03).

use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::{Color, PrinterState};
use sim::{EntityId, TilePos};

const IDLER: &str = "log(1)\n";
const BRAWLER: &str = "attack(closest(enemy).expect())\n";

/// Base map: a working Green printer and a ruined Red one (the doc's
/// starting state), plus seed ore.
fn colony_map() -> MapSpec {
    let mut spec = MapSpec::empty(12, 8);
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 2),
        faction: 0,
        color: 0, // Green
        ruined: false,
        desired_max: 0,
    });
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(4, 2),
        faction: 0,
        color: 1, // Red
        ruined: true,
        desired_max: 0,
    });
    spec.starting_ore = 100;
    spec
}

fn printer_ids(sim: &Sim) -> Vec<EntityId> {
    sim.world.printers.keys().copied().collect()
}

#[test]
fn printer_prints_to_its_dial() {
    let mut sim = Sim::new(&colony_map());
    let green = printer_ids(&sim)[0];
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    sim.apply(&Command::SetDesiredMax { printer: green, value: 3 }).unwrap();
    for _ in 0..100 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 3, "population reaches the dial and stops");
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
fn ruined_printer_prints_only_after_repair() {
    let mut sim = Sim::new(&colony_map());
    let red = printer_ids(&sim)[1];
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::RED, source: IDLER.into() })
        .unwrap();
    sim.apply(&Command::SetDesiredMax { printer: red, value: 1 }).unwrap();
    for _ in 0..50 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 0, "ruined printers print nothing");

    // Repair prices in DATA now (docs/03): without Data it stays ruined.
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    assert_eq!(sim.world.printers[&red].state, PrinterState::Ruined, "no Data, no repair");
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();
    assert_eq!(sim.world.printers[&red].state, PrinterState::Working);
    assert_eq!(sim.world.data.get(&0).copied().unwrap_or(0), 0, "repair drained the Data");
    for _ in 0..50 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 1);
    assert_eq!(sim.world.bots.values().next().unwrap().data.color, Color::RED);
}

#[test]
fn recall_recolors_the_lowest_xp_bot_keeping_xp() {
    let mut sim = Sim::new(&colony_map());
    let [green, red] = printer_ids(&sim)[..] else { panic!() };
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::RED, source: IDLER.into() })
        .unwrap();
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();

    // Two green bots; give one of them XP by hand-spawning a veteran.
    let veteran = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 3),
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    let rookie = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(3, 3),
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    sim.world.bots.get_mut(&veteran).unwrap().data.xp_mining = 500;

    // Green over quota (2 > 1), Red has headroom (0 < 1): recall fires.
    sim.apply(&Command::SetDesiredMax { printer: green, value: 1 }).unwrap();
    sim.apply(&Command::SetDesiredMax { printer: red, value: 1 }).unwrap();
    for _ in 0..60 {
        sim.step();
    }

    let rookie_bot = &sim.world.bots[&rookie];
    let veteran_bot = &sim.world.bots[&veteran];
    assert_eq!(rookie_bot.data.color, Color::RED, "lowest-XP bot re-colors");
    assert_eq!(veteran_bot.data.color, Color::GREEN, "veteran keeps its color");
    assert_eq!(veteran_bot.data.xp_mining, 500);
    assert_eq!(sim.world.bots.len(), 2, "rebalancing loses nobody");
}

#[test]
fn no_recall_without_headroom() {
    // Over-quota green, but red is ruined: no destination → no recall,
    // surplus bots keep working (docs/01 dormant/ghost-fleet rule).
    let mut sim = Sim::new(&colony_map());
    let green = printer_ids(&sim)[0];
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    for pos in [TilePos::new(2, 3), TilePos::new(3, 3)] {
        sim.apply(&Command::SpawnBot {
            pos,
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap();
    }
    sim.apply(&Command::SetDesiredMax { printer: green, value: 1 }).unwrap();
    for _ in 0..50 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 2, "no destination with headroom → no recall");
    assert!(sim.world.bots.values().all(|b| b.data.recall.is_none()));
}

#[test]
fn damage_during_recall_walk_is_double_handle() {
    let mut sim = Sim::new(&colony_map());
    let [green, red] = printer_ids(&sim)[..] else { panic!() };
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::RED, source: IDLER.into() })
        .unwrap();
    sim.world.data.insert(0, sim.tuning.repair_cost_data);
    sim.apply(&Command::RepairPrinter { printer: red }).unwrap();

    // Victim far from home so the walk takes a while; brawler adjacent.
    // One hit kills: the blow lands in the resolve phase right after the
    // recall begins (the walk starts a phase later), so death arrives
    // mid-recall — an engine interrupt context.
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

    // Force the recall: green over quota (1 > 0), red has headroom.
    sim.apply(&Command::SetDesiredMax { printer: green, value: 0 }).unwrap();
    sim.apply(&Command::SetDesiredMax { printer: red, value: 1 }).unwrap();
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
fn over_capacity_scraps_lowest_xp_for_refund() {
    let mut sim = Sim::new(&colony_map());
    sim.tuning.capacity = 1;
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    let veteran = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 3),
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(3, 3),
        source: IDLER.into(),
        cpu: 2,
        cargo_cap: 1,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap();
    sim.world.bots.get_mut(&veteran).unwrap().data.xp_combat = 900;

    let ore_before = sim.world.stock_get(0, sim::resources::Resource::Iron);
    for _ in 0..60 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 1, "over-capacity colony scraps down");
    assert!(sim.world.bots.contains_key(&veteran), "the veteran survives");
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
    let mut sim = Sim::new(&colony_map());
    sim.tuning.capacity = 1;
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    let veteran = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 3),
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    // The victim starts across the map, so the recall is a real walk.
    let victim = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(10, 6),
            source: IDLER.into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    sim.world.bots.get_mut(&veteran).unwrap().data.xp_combat = 900;

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
fn printer_colony_is_deterministic() {
    let build = || {
        let mut sim = Sim::new(&colony_map());
        let [green, red] = printer_ids(&sim)[..] else { panic!() };
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
        sim.apply(&Command::SetDesiredMax { printer: green, value: 3 }).unwrap();
        sim.apply(&Command::SetDesiredMax { printer: red, value: 2 }).unwrap();
        sim
    };
    let mut a = build();
    let mut b = build();
    for tick in 0..300 {
        a.step();
        b.step();
        assert_eq!(a.state_hash(), b.state_hash(), "desync at tick {tick}");
    }
    assert_eq!(a.world.bots.len(), 5);
}
