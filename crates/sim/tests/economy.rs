//! The typed economy (M4, docs/03): typed nodes and cargo, refinery
//! feeds, Data + research, and the new verbs.

use sim::map::MapSpec;
use sim::resources::Resource;
use sim::sim::{Command, Sim};
use sim::world::{Color, StructureKind};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 4,
        cargo_cap: 4,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

#[test]
fn typed_nodes_ride_their_ground_tiles_and_mine_typed() {
    let mut spec = MapSpec::empty(8, 4);
    spec.resource_tiles.push((TilePos::new(4, 1), sim::TileKind::CoalSeam));
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    let miner = spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(closest(coal).expect())\nmine()\nwait(600)\n",
    );
    for _ in 0..200 {
        sim.step();
    }
    let bot = &sim.world.bots[&miner];
    assert_eq!(
        bot.data.cargo.get(&Resource::Coal).copied().unwrap_or(0),
        sim.tuning.mine_yield_deci,
        "mine() yields the NODE's kind, one swing's worth: {:?}",
        bot.data.cargo
    );
}

#[test]
fn groves_regenerate_other_nodes_do_not() {
    let mut spec = MapSpec::empty(6, 4);
    spec.resource_tiles.push((TilePos::new(2, 1), sim::TileKind::Grove));
    spec.resource_tiles.push((TilePos::new(4, 1), sim::TileKind::CoalSeam));
    let mut sim = Sim::new(&spec);
    sim.tuning.regen_interval_ticks = 5;
    // Drain both a little by hand.
    for node in sim.world.nodes.values_mut() {
        node.amount -= 30;
    }
    let amounts = |sim: &Sim| -> Vec<(Resource, u32)> {
        sim.world.nodes.values().map(|n| (n.kind, n.amount)).collect()
    };
    let before = amounts(&sim);
    for _ in 0..20 {
        sim.step();
    }
    let after = amounts(&sim);
    let wood_before = before.iter().find(|(k, _)| *k == Resource::Wood).unwrap().1;
    let wood_after = after.iter().find(|(k, _)| *k == Resource::Wood).unwrap().1;
    let coal_before = before.iter().find(|(k, _)| *k == Resource::Coal).unwrap().1;
    let coal_after = after.iter().find(|(k, _)| *k == Resource::Coal).unwrap().1;
    assert!(wood_after > wood_before, "the Grove regenerates (docs/03 flagship)");
    assert_eq!(coal_after, coal_before, "seams are finite");
}

#[test]
fn smelter_refines_fed_inputs_into_withdrawable_steel() {
    let mut spec = MapSpec::empty(8, 5);
    spec.starting_stock.push((0, Resource::Steel, 10));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0; // the looping program crash-loops harmlessly
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(4, 2),
        kind: StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    let smelter = *sim.world.structures.keys().next().expect("placed");
    assert_eq!(
        sim.world.stock_get(0, Resource::Steel),
        0,
        "the Smelter's 10-Steel price drew from stock (abstract payment)"
    );
    // steel recipe = RECIPES[0].
    sim.apply(&Command::SetRecipe { structure: smelter, recipe: Some(0) }).unwrap();

    // A hauler carrying iron+coal feeds the smelter physically, then
    // withdraws the steel batch.
    let hauler = spawn(
        &mut sim,
        TilePos::new(3, 2),
        "wait(2)\n", // program idles; the test drives cargo by hand
    );
    let bot = sim.world.bots.get_mut(&hauler).unwrap();
    bot.data.cargo.insert(Resource::Iron, 20);
    bot.data.cargo.insert(Resource::Coal, 10);
    // deposit() feeds the adjacent refinery (only recipe inputs move).
    sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::GREEN,
        source: "deposit()\nwait(60)\ntry_withdraw(steel)\nwait(600)\n".into(),
    })
    .unwrap();
    for _ in 0..300 {
        sim.step();
    }
    let bot = &sim.world.bots[&hauler];
    assert_eq!(
        bot.data.cargo.get(&Resource::Steel).copied().unwrap_or(0),
        10,
        "2 Fe + 1 Coal became 1 Steel (10 deci), withdrawn from the output; cargo {:?} structure {:?}",
        bot.data.cargo,
        sim.world.structures.values().next()
    );
}

#[test]
fn research_spends_data_and_gates_deploys() {
    let mut spec = MapSpec::empty(4, 4);
    spec.dev_all_unlocks = false;
    let mut sim = Sim::new(&spec);
    // Tier-0 programs deploy fine without any unlocks.
    sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::GREEN,
        source: "mine()\n".into(),
    })
    .expect("tier-0 needs no unlocks");
    // Variables are locked until researched.
    let locked = sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::GREEN,
        source: "x = 1\n".into(),
    });
    assert!(locked.is_err(), "locked construct must fail the deploy");
    // Research without Data: no-op.
    sim.apply(&Command::Research { faction: 0, construct: pyrite::Construct::Variables })
        .unwrap();
    assert!(sim
        .apply(&Command::DeployProgram {
            faction: 0,
            color: Color::GREEN,
            source: "x = 1\n".into(),
        })
        .is_err());
    // Fund it: research succeeds, deploy passes, Data is spent.
    sim.world.data.insert(0, 10);
    sim.apply(&Command::Research { faction: 0, construct: pyrite::Construct::Variables })
        .unwrap();
    assert_eq!(sim.world.data.get(&0).copied().unwrap_or(0), 0);
    sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::GREEN,
        source: "x = 1\n".into(),
    })
    .expect("researched construct deploys");
}

#[test]
fn structures_are_attackable_and_fall_at_zero() {
    let mut spec = MapSpec::empty(6, 4);
    spec.starting_stock.push((0, Resource::Steel, 10));
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(3, 1),
        kind: StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    let smelter = *sim.world.structures.keys().next().unwrap();
    spawn(&mut sim, TilePos::new(2, 1), "attack(closest(smelter).expect())\n");
    let hp0 = sim.world.structures[&smelter].hp;
    for _ in 0..600 {
        sim.step();
        if !sim.world.structures.contains_key(&smelter) {
            break;
        }
    }
    assert!(
        !sim.world.structures.contains_key(&smelter),
        "sustained attack fells the structure (started at {hp0} hp)"
    );
}

#[test]
fn delivery_milestones_mint_data() {
    let mut spec = MapSpec::empty(8, 4);
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.delivery_milestone_deci = 30; // tiny milestone for the test
    let mule = spawn(&mut sim, TilePos::new(1, 1), "deposit()\nwait(600)\n");
    sim.world.bots.get_mut(&mule).unwrap().data.cargo.insert(Resource::Iron, 40);
    for _ in 0..60 {
        sim.step();
    }
    assert_eq!(
        sim.world.data.get(&0).copied().unwrap_or(0),
        sim.tuning.milestone_data,
        "40 deci delivered crossed the 30-deci milestone once"
    );
}
