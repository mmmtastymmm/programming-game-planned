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
    spec.depots.push((TilePos::new(0, 1), 0));
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
    spec.depots.push((TilePos::new(0, 1), 0));
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

/// Withdraw/deposit cycling at a depot is zero NET delivery — the Data
/// milestone must not be farmable by moving the same stock in a circle.
#[test]
fn withdraw_deposit_cycling_mints_no_data() {
    let mut spec = MapSpec::empty(8, 4);
    spec.depots.push((TilePos::new(2, 1), 0));
    spec.starting_stock.push((0, Resource::Iron, 4000));
    let mut sim = Sim::new(&spec);
    // Tight milestone: one cargo load (40 deci) = a whole milestone, so the
    // old gross accounting would mint on the very first cycle.
    sim.tuning.delivery_milestone_deci = 40;
    let bot = spawn(&mut sim, TilePos::new(1, 1), "try_withdraw(iron)\ndeposit()\n");
    let mut cycled = false;
    for _ in 0..300 {
        sim.step();
        // Liveness: the guarded behavior must actually EXECUTE — cargo
        // aboard at some point proves withdraw ran (a crash-looping bot
        // would leave every assertion below vacuously green).
        cycled |= sim.world.bots[&bot].data.cargo_total() > 0;
    }
    assert!(cycled, "the withdraw/deposit cycle must really run");
    // The cycle really ran: the seeded iron is split between stock and
    // the bot's hold mid-cycle, and nothing was created or destroyed
    // (starting_stock seeds in units — 4000 units = 40000 deci).
    let aboard = sim.world.bots[&bot].data.cargo_total() as u64;
    assert_eq!(
        sim.world.stock_get(0, Resource::Iron) + aboard,
        40_000,
        "conservation across withdraw/deposit cycling"
    );
    assert_eq!(
        sim.world.delivered.get(&0).copied().unwrap_or(0),
        0,
        "stock-withdrawn cargo re-deposited earns no delivery credit at all"
    );
    assert_eq!(
        sim.world.data.get(&0).copied().unwrap_or(0),
        0,
        "cycling withdraw/deposit at a depot mints no milestone Data"
    );
}

/// The provenance fix's other half: SPENDING seeded stock must never
/// suppress genuinely earned milestone Data (the old global net counter
/// subtracted seeded-stock withdrawals from future real deliveries).
#[test]
fn seeded_stock_withdrawals_do_not_suppress_milestones() {
    let mut spec = MapSpec::empty(10, 4);
    spec.ore_nodes.push((TilePos::new(6, 1), 100));
    spec.depots.push((TilePos::new(1, 1), 0));
    spec.starting_stock.push((0, Resource::Steel, 4000));
    let mut sim = Sim::new(&spec);
    sim.tuning.delivery_milestone_deci = 40;
    // A builder drains seeded Steel from stock (construction logistics —
    // it never re-deposits, so nothing is recycled OR delivered).
    let builder = spawn(&mut sim, TilePos::new(2, 1), "try_withdraw(steel)\ndrop_cargo()\n");
    // A miner genuinely delivers fresh ore.
    let _miner = spawn(
        &mut sim,
        TilePos::new(3, 1),
        "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n",
    );
    for _ in 0..400 {
        sim.step();
    }
    assert!(
        sim.world.bots[&builder].data.xp(sim::world::XpTrack::Hauling) == 0,
        "sanity: the builder never delivered"
    );
    assert!(
        sim.world.data.get(&0).copied().unwrap_or(0) >= sim.tuning.milestone_data,
        "real deliveries still pay despite seeded-stock spending: delivered {:?}, data {:?}",
        sim.world.delivered,
        sim.world.data
    );
}

/// The positive twin: mined ore delivered to a depot is net-new stock and
/// still crosses milestones.
#[test]
fn mined_deliveries_still_cross_milestones() {
    let mut spec = MapSpec::empty(8, 4);
    spec.ore_nodes.push((TilePos::new(4, 1), 100));
    spec.depots.push((TilePos::new(1, 1), 0));
    let mut sim = Sim::new(&spec);
    sim.tuning.delivery_milestone_deci = 40;
    let _bot = spawn(
        &mut sim,
        TilePos::new(2, 1),
        "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n",
    );
    for _ in 0..400 {
        sim.step();
    }
    assert!(
        sim.world.data.get(&0).copied().unwrap_or(0) >= sim.tuning.milestone_data,
        "net-new deliveries still pay milestone Data: delivered {:?}, data {:?}",
        sim.world.delivered,
        sim.world.data
    );
}

/// An acceptor destroyed between the deposit's start and its completion
/// moves nothing — the cargo must not teleport into colony stock as if a
/// depot were adjacent (the "phantom depot" path).
#[test]
fn acceptor_destroyed_mid_deposit_moves_nothing() {
    let mut spec = MapSpec::empty(8, 5);
    spec.starting_stock.push((0, Resource::Steel, 100));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0; // the retry loop crash-dumps harmlessly
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(4, 2),
        kind: StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    let smelter = *sim.world.structures.keys().next().expect("placed");
    sim.apply(&Command::SetRecipe { structure: smelter, recipe: Some(0) }).unwrap();
    let hauler = spawn(&mut sim, TilePos::new(3, 2), "deposit()\nwait(600)\n");
    sim.world.bots.get_mut(&hauler).unwrap().data.cargo.insert(Resource::Iron, 20);
    // Step until the 1-tick deposit action is in flight, then fell the
    // smelter before it completes.
    let mut armed = false;
    for _ in 0..20 {
        sim.step();
        if matches!(
            sim.world.bots[&hauler].data.action,
            Some(sim::world::Action::Deposit { .. })
        ) {
            armed = true;
            break;
        }
    }
    assert!(armed, "the deposit action must start");
    sim.world.structures.remove(&smelter);
    for _ in 0..3 {
        sim.step();
    }
    let bot = &sim.world.bots[&hauler];
    assert_eq!(
        bot.data.cargo.get(&Resource::Iron).copied().unwrap_or(0),
        20,
        "cargo stays aboard when the acceptor died mid-deposit"
    );
    assert_eq!(
        sim.world.stock_get(0, Resource::Iron),
        0,
        "no phantom depot: nothing teleports into colony stock"
    );
}

/// SetRecipe scraps the in-flight batch and its already-consumed inputs are
/// LOST — deliberate waste, pinned here so a silent refund never sneaks in.
#[test]
fn recipe_change_scraps_the_batch_and_its_inputs() {
    let mut spec = MapSpec::empty(8, 5);
    spec.starting_stock.push((0, Resource::Steel, 100));
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(4, 2),
        kind: StructureKind::Smelter,
        faction: 0,
    })
    .unwrap();
    let smelter = *sim.world.structures.keys().next().expect("placed");
    sim.apply(&Command::SetRecipe { structure: smelter, recipe: Some(0) }).unwrap();
    {
        let st = sim.world.structures.get_mut(&smelter).unwrap();
        st.input.insert(Resource::Iron, 20);
        st.input.insert(Resource::Coal, 10);
    }
    sim.step(); // phase 8 consumes the feed and starts the batch
    let st = &sim.world.structures[&smelter];
    assert!(st.batch.is_some() && st.input.is_empty(), "the batch consumed the feed");
    // Switch to glass (also a smelter recipe): the steel batch is scrapped.
    sim.apply(&Command::SetRecipe { structure: smelter, recipe: Some(2) }).unwrap();
    for _ in 0..200 {
        sim.step();
    }
    let st = &sim.world.structures[&smelter];
    assert!(st.batch.is_none(), "the scrapped batch never resumes");
    assert!(st.output.is_empty(), "the scrapped batch emits nothing");
    assert!(st.input.is_empty(), "the consumed inputs are lost, not refunded");
}

#[test]
fn hauling_xp_not_farmable_by_stock_cycling() {
    // Moving withdrawn colony stock out and back must mint NO Hauling XP
    // (docs/02: income is cargo-distance DELIVERED). The review found
    // credit_travel accrued from cargo_total regardless of provenance, so a
    // withdraw → lap → deposit loop farmed Hauling XP with zero net output.
    // (The prior guard tests never MOVED the bot between withdraw & deposit.)
    let mut spec = MapSpec::empty(12, 4);
    spec.depots.push((TilePos::new(2, 1), 0));
    spec.ore_nodes.push((TilePos::new(5, 1), 100)); // a lap waypoint (in sight), never mined
    spec.starting_stock.push((0, Resource::Iron, 400));
    let mut sim = Sim::new(&spec);
    // Withdraw at the depot, carry a lap to the node and back, then deposit —
    // repeat. The bot never calls mine(), so all cargo is recycled stock.
    let src = "try_withdraw(iron)\nmove_to(closest(ore).expect())\nmove_to(closest(depot).expect())\ndeposit()\n";
    let bot = spawn(&mut sim, TilePos::new(1, 1), src);
    let mut carried_far = false;
    for _ in 0..600 {
        sim.step();
        if let Some(b) = sim.world.bots.get(&bot) {
            // Liveness: prove the loop really hauls withdrawn stock a
            // distance — cargo aboard AND away from the depot.
            carried_far |= b.data.cargo_total() > 0 && b.data.pos.x >= 4;
        }
    }
    assert!(carried_far, "the bot must actually carry withdrawn stock on a lap");
    assert_eq!(
        sim.world.bots[&bot].data.xp(sim::world::XpTrack::Hauling),
        0,
        "cycling withdrawn stock mints no Hauling XP"
    );
}

#[test]
fn mined_cargo_still_earns_hauling_xp() {
    // The provenance guard must not zero LEGITIMATE hauling: a bot that
    // mines and delivers still earns Hauling XP (the control for the farm fix).
    let mut spec = MapSpec::empty(12, 4);
    spec.depots.push((TilePos::new(2, 1), 0));
    spec.ore_nodes.push((TilePos::new(5, 1), 100));
    let mut sim = Sim::new(&spec);
    let src = "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\n";
    let bot = spawn(&mut sim, TilePos::new(1, 1), src);
    for _ in 0..600 {
        sim.step();
    }
    assert!(
        sim.world.bots[&bot].data.xp(sim::world::XpTrack::Hauling) > 0,
        "mined cargo hauled to a depot still earns Hauling XP"
    );
}

#[test]
fn crystal_field_mines_and_the_chips_recipe_runs() {
    // The Crystal -> Chips compute loop (docs/03/05) had NO from-scratch test:
    // CrystalField was never mined and the chips recipe (Silver+Crystal+Wire)
    // never run — Chips only ever arrived via starting_stock. Do both here.
    let mut spec = MapSpec::empty(9, 5);
    spec.crystal.push(TilePos::new(2, 2)); // a CrystalField tile + Crystal node
    spec.starting_stock.push((0, Resource::Steel, 25)); // Foundry price
    spec.starting_stock.push((0, Resource::Bronze, 10));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;

    // Phase 1: a bot mines the CrystalField -> Crystal in its hold.
    let miner = spawn(
        &mut sim,
        TilePos::new(1, 2),
        "move_to(closest(crystal).expect())\nmine()\nmine()\nmine()\nwait(100000)\n",
    );
    for _ in 0..200 {
        sim.step();
    }
    let mined = sim.world.bots[&miner].data.cargo.get(&Resource::Crystal).copied().unwrap_or(0);
    assert!(mined >= 20, "the CrystalField must yield Crystal (got {mined} deci)");

    // Phase 2: top up the other two chip inputs by hand and run the recipe.
    sim.apply(&Command::PlaceStructure {
        pos: TilePos::new(5, 2),
        kind: StructureKind::Foundry,
        faction: 0,
    })
    .unwrap();
    let foundry = *sim
        .world
        .structures
        .iter()
        .find(|(_, s)| s.kind == StructureKind::Foundry)
        .map(|(id, _)| id)
        .expect("foundry placed");
    // chips = RECIPES[4] (Silver + 2 Crystal + Wire -> Chips).
    sim.apply(&Command::SetRecipe { structure: foundry, recipe: Some(4) }).unwrap();
    {
        let bot = sim.world.bots.get_mut(&miner).unwrap();
        bot.data.cargo.insert(Resource::Silver, 10);
        bot.data.cargo.insert(Resource::Wire, 10);
    }
    sim.apply(&Command::DeployProgram {
        faction: 0,
        color: Color::GREEN,
        source: "move_to(closest(foundry).expect())\ndeposit()\nwait(80)\ntry_withdraw(chips)\nwait(600)\n".into(),
    })
    .unwrap();
    for _ in 0..400 {
        sim.step();
    }
    let chips = sim.world.bots[&miner].data.cargo.get(&Resource::Chips).copied().unwrap_or(0);
    assert!(chips > 0, "the chips recipe produced withdrawable Chips (got {chips} deci)");
}
