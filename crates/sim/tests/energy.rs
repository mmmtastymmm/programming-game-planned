//! Energy & upkeep (M5, docs/03 Q84): generation vs. draw, brownout, the
//! Fabricator trickle, fuel burning, and Steel-shortfall rust. Test maps
//! opt INTO the system with dev_free_power: false.

use sim::map::{MapSpec, PrinterSpec};
use sim::resources::Resource;
use sim::sim::{Command, Sim};
use sim::world::{Color, StructureKind};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 1,
        cargo_cap: 4,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

fn powered_map() -> MapSpec {
    let mut spec = MapSpec::empty(10, 6);
    spec.dev_free_power = false;
    // A stoked Generator: 200 deci Coal = 20 settlements of strong output.
    spec.structures.push((TilePos::new(7, 1), StructureKind::Generator));
    spec
}

#[test]
fn stoked_generator_prevents_brownout_until_fuel_runs_out() {
    let mut sim = Sim::new(&powered_map());
    sim.upkeep.interval_ticks = 10; // fast settlements for the test
    spawn(&mut sim, TilePos::new(2, 2), "wait(50)\n");
    // Stoked: draw (1 bot × 10) < coal output (200) — no brownout.
    for _ in 0..20 {
        sim.step();
    }
    assert!(!sim.world.brownout.contains(&0), "the stoked opening never brownouts (docs/03)");
    // 200 deci stoke / 10 per settlement = 20 settlements; run them dry.
    for _ in 0..300 {
        sim.step();
    }
    assert!(sim.world.brownout.contains(&0), "dry generator = brownout");
}

#[test]
fn geothermal_tap_is_free_steady_power_and_vents_only() {
    let mut spec = MapSpec::empty(10, 6);
    spec.dev_free_power = false;
    spec.vents.push(TilePos::new(6, 2));
    spec.starting_stock.push((0, Resource::Steel, 240));
    let mut sim = Sim::new(&spec);
    sim.upkeep.interval_ticks = 10;
    // Not on a vent: rejected (stock untouched).
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(3, 3),
        kind: StructureKind::GeothermalTap,
        faction: 0,
    })
    .unwrap();
    assert!(sim.world.structures.is_empty(), "taps place on vent tiles only");
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(6, 2),
        kind: StructureKind::GeothermalTap,
        faction: 0,
    })
    .unwrap();
    assert_eq!(sim.world.structures.len(), 1);
    spawn(&mut sim, TilePos::new(2, 2), "wait(600)\n");
    for _ in 0..100 {
        sim.step();
    }
    assert!(!sim.world.brownout.contains(&0), "a tap powers a small colony forever");
}

#[test]
fn brownout_halves_grants_but_the_printer_trickle_powers_one_bot() {
    let mut spec = MapSpec::empty(12, 6);
    spec.dev_free_power = false; // no generation at all
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(9, 2),
        faction: 0,
        color: 0,
        ruined: false,
        desired_max: 0,
    });
    let mut sim = Sim::new(&spec);
    sim.upkeep.interval_ticks = 5;
    let first = spawn(&mut sim, TilePos::new(2, 2), "wait(600)\n");
    let second = spawn(&mut sim, TilePos::new(4, 2), "wait(600)\n");
    for _ in 0..10 {
        sim.step();
    }
    assert!(sim.world.brownout.contains(&0), "zero generation = brownout");
    assert_eq!(
        sim.world.powered_bot.get(&0),
        Some(&first),
        "the Fabricator trickle picks the lowest id"
    );
    let d_first = &sim.world.bots[&first].data;
    let d_second = &sim.world.bots[&second].data;
    assert_eq!(sim::stats::cpu_centi(&sim.stats, d_first, true, true), 100);
    assert_eq!(sim::stats::cpu_centi(&sim.stats, d_second, true, false), 50);
}

#[test]
fn steel_shortfall_rusts_and_decays() {
    let mut spec = MapSpec::empty(10, 6);
    spec.dev_free_power = false;
    spec.structures.push((TilePos::new(7, 1), StructureKind::Generator));
    // No Steel in stock: maintenance goes unpaid from the first settlement.
    let mut sim = Sim::new(&spec);
    sim.upkeep.interval_ticks = 10;
    let bot = spawn(&mut sim, TilePos::new(2, 2), "wait(600)\n");
    sim.world.bots.get_mut(&bot).unwrap().data.hp = 90;
    let hp_before = sim.world.bots[&bot].data.hp;
    for _ in 0..60 {
        sim.step();
    }
    assert!(sim.world.rusting.contains(&0), "unpaid Steel = rust");
    let hp_after = sim.world.bots[&bot].data.hp;
    assert!(
        hp_after < hp_before,
        "rust decays hulls (self-repair halted): {hp_before} -> {hp_after}"
    );

    // Paying resumes: seed stock and settle again.
    sim.world.stock_add(0, Resource::Steel, 1000);
    for _ in 0..20 {
        sim.step();
    }
    assert!(!sim.world.rusting.contains(&0), "paid maintenance clears the rust flag");
}

#[test]
fn generators_burn_deposited_fuel_wood_weak_coal_strong() {
    let mut spec = MapSpec::empty(10, 6);
    spec.dev_free_power = false;
    spec.structures.push((TilePos::new(5, 2), StructureKind::Generator));
    let mut sim = Sim::new(&spec);
    sim.upkeep.interval_ticks = 10;
    // Drain the stoke so fuel accounting starts empty.
    let gen_id = *sim.world.structures.keys().next().unwrap();
    sim.world.structures.get_mut(&gen_id).unwrap().input.clear();
    // Ten idle bots: draw 100 > wood output 60, < coal output 200.
    for i in 0..10 {
        spawn(&mut sim, TilePos::new(1 + (i % 3), 1 + i / 3), "wait(600)\n");
    }
    // Wood only: browns out anyway (weak fuel).
    sim.world.structures.get_mut(&gen_id).unwrap().input.insert(Resource::Wood, 100);
    for _ in 0..12 {
        sim.step();
    }
    assert!(sim.world.brownout.contains(&0), "wood is the weak fuel (docs/03)");
    // Coal: strong — clears the brownout, and gets burned in preference.
    sim.world.structures.get_mut(&gen_id).unwrap().input.insert(Resource::Coal, 100);
    for _ in 0..12 {
        sim.step();
    }
    assert!(!sim.world.brownout.contains(&0), "coal covers ten idle bots comfortably");
    let st = &sim.world.structures[&gen_id];
    assert!(
        st.input.get(&Resource::Coal).copied().unwrap_or(0) < 100,
        "coal burned in preference: {:?}",
        st.input
    );
}

#[test]
fn browned_out_refineries_stand_idle() {
    let mut spec = MapSpec::empty(10, 6);
    spec.dev_free_power = false; // zero generation: permanent brownout
    spec.starting_stock.push((0, Resource::Steel, 100));
    let mut sim = Sim::new(&spec);
    sim.upkeep.interval_ticks = 5;
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(5, 2),
        kind: StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    let smelter = *sim.world.structures.keys().next().unwrap();
    sim.apply(&Command::SetRecipe { structure: smelter, recipe: Some(0) }).unwrap();
    spawn(&mut sim, TilePos::new(2, 2), "wait(600)\n"); // someone to draw power
    {
        let st = sim.world.structures.get_mut(&smelter).unwrap();
        st.input.insert(Resource::Iron, 20);
        st.input.insert(Resource::Coal, 10);
    }
    // Run past the first settlement, then note the batch state: whatever
    // slipped into the pre-settlement window must FREEZE — brownout stops
    // the timer, and no output ever emits.
    for _ in 0..10 {
        sim.step();
    }
    assert!(sim.world.brownout.contains(&0));
    let frozen = sim.world.structures[&smelter].batch;
    for _ in 0..120 {
        sim.step();
    }
    let st = &sim.world.structures[&smelter];
    assert_eq!(st.batch, frozen, "the batch timer stands still under brownout");
    assert!(st.output.is_empty(), "no energy, no smelting (docs/03 'needs energy'): {st:?}");
}
