//! The universal chassis (M5, docs/02): floor statline on every print,
//! and the modifier pipeline's state layer (Damaged, brownout).

use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::stats;
use sim::world::Color;
use sim::TilePos;

#[test]
fn printed_bots_get_the_floor_statline() {
    let mut spec = MapSpec::empty(8, 5);
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 2),
        faction: 0,
        color: 0,
        ruined: false,
        desired_max: 1,
    });
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: "log(1)\n".into() })
        .unwrap();
    for _ in 0..30 {
        sim.step();
    }
    let bot = sim.world.bots.values().next().expect("printer printed");
    let s = &sim.stats;
    assert_eq!(bot.data.max_hp, s.hp, "floor HP (docs/02: 40)");
    assert_eq!(bot.data.cpu_centi, s.cpu_centi, "floor CPU (1 cycle/tick)");
    assert_eq!(bot.data.cargo_cap, s.cargo_cap_deci, "floor cargo (4 units)");
    assert_eq!(bot.data.move_rate_deci, s.move_rate_deci, "floor move rate (14 t/t)");
    assert_eq!(bot.data.sensors, s.sensors);
    assert_eq!(bot.data.module_slots, s.module_slots);
    assert_eq!(bot.data.log_cap, s.log_buffer);
    assert!(bot.data.upgrades.is_empty() && bot.data.modules.is_empty(), "identical rookies");
}

#[test]
fn damaged_bots_think_and_move_slower() {
    let spec = MapSpec::empty(6, 4);
    let mut sim = Sim::new(&spec);
    let id = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: "wait(5)\n".into(),
            cpu: 1,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    let healthy_step =
        stats::step_ticks(&sim.stats, &sim.world.grid, &sim.world.bots[&id].data, TilePos::new(3, 2))
            .unwrap();
    let healthy_cpu = stats::cpu_centi(&sim.stats, &sim.world.bots[&id].data, false, false);
    assert_eq!(healthy_step, 14, "140 deci-ticks on a 1x tile");
    assert_eq!(healthy_cpu, 100);

    // Below the fixed 50% line: Damaged — speed and cycles lose 25%
    // (pessimistic: the move-rate penalty ceils, the cycle loss ceils).
    sim.world.bots.get_mut(&id).unwrap().data.hp = 49;
    let d = &sim.world.bots[&id].data;
    assert!(stats::is_damaged(d));
    assert_eq!(
        stats::step_ticks(&sim.stats, &sim.world.grid, d, TilePos::new(3, 2)).unwrap(),
        18,
        "140 + ceil(25%) = 175 deci -> 18 ticks"
    );
    assert_eq!(stats::cpu_centi(&sim.stats, d, false, false), 75);
}

#[test]
fn brownout_halves_cycles_but_the_trickle_exempts() {
    let spec = MapSpec::empty(6, 4);
    let mut sim = Sim::new(&spec);
    let id = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: "wait(5)\n".into(),
            cpu: 1,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    let d = &sim.world.bots[&id].data;
    assert_eq!(stats::cpu_centi(&sim.stats, d, true, false), 50, "brownout: -50%");
    assert_eq!(stats::cpu_centi(&sim.stats, d, true, true), 100, "the Fabricator trickle pick");
    // Damaged then brownout, each off the running subtotal, both ceils:
    // 100 - 25 = 75; 75 - ceil(37.5) = 37.
    let data = &mut sim.world.bots.get_mut(&id).unwrap().data;
    data.hp = 40;
    let d = &sim.world.bots[&id].data;
    assert_eq!(stats::cpu_centi(&sim.stats, d, true, false), 37);
}

/// Lockstep commands never panic the sim: absurd dev-spawn numbers clamp
/// and saturate instead of overflowing the unit conversions.
#[test]
fn hostile_spawn_values_clamp_instead_of_overflowing() {
    let spec = MapSpec::empty(6, 4);
    let mut sim = Sim::new(&spec);
    let id = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: "wait(5)\n".into(),
            cpu: u64::MAX / 2,
            cargo_cap: u32::MAX,
            faction: 0,
            hp: i64::MAX,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    sim.step(); // grant + hash paths must survive the extremes
    let data = &sim.world.bots[&id].data;
    assert!(data.hp <= 1_000_000_000, "hp clamps to a sane ceiling");
    assert_eq!(data.cargo_cap, u32::MAX, "cargo saturates, never wraps");
}
