//! Ferals (M12, docs/04): nests print archetype programs on the player's
//! VM, Harvesters feed the nest economy, Magician arcana mutate per
//! print, escalation follows footprint, defeated nests claim into
//! quadratic printer slots, and undefended claims are retaken.

use sim::feral::{mutates, printers_allowed, Archetype};
use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::{Color, NestState, FERAL_FACTION};
use sim::TilePos;

fn nest_spec(nests: Vec<(TilePos, u8)>) -> MapSpec {
    let mut spec = MapSpec::empty(14, 10);
    spec.quirk_permille = 0;
    spec.nests = nests;
    spec
}

fn first_nest(sim: &Sim) -> sim::EntityId {
    *sim.world.nests.keys().next().expect("a nest spawned")
}

fn feral_count(sim: &Sim) -> usize {
    sim.world.bots.values().filter(|b| b.data.faction == FERAL_FACTION).count()
}

#[test]
fn nests_print_ferals_from_their_own_stock() {
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(2, 2), 0)]));
    sim.tuning.nest_print_ticks = 3;
    sim.tuning.nest_income_deci = 0; // seed stock only: 300 = 3 prints
    for _ in 0..40 {
        sim.step();
    }
    assert_eq!(feral_count(&sim), 3, "the seed stock paid for exactly three prints");
    // Calm-tier mix: Drone, Drone, Harvester — archetype = color slot.
    let mut colors: Vec<u8> = sim
        .world
        .bots
        .values()
        .filter(|b| b.data.faction == FERAL_FACTION)
        .map(|b| b.data.color.0)
        .collect();
    colors.sort();
    assert_eq!(
        colors,
        vec![
            Archetype::Drone.color().0,
            Archetype::Drone.color().0,
            Archetype::Harvester.color().0
        ],
        "the calm-tier round-robin printed two Drones and a Harvester"
    );
}

#[test]
fn max_arcanum_is_a_match_option() {
    let mut spec =
        nest_spec(vec![(TilePos::new(2, 2), 0), (TilePos::new(10, 2), 18)]);
    spec.max_arcanum = 5;
    let sim = Sim::new(&spec);
    assert_eq!(sim.world.nests.len(), 1, "the arcanum-18 nest is beyond this match's cap");
    assert_eq!(sim.world.nests.values().next().unwrap().arcanum, 0);
}

#[test]
fn magician_nests_mutate_every_print() {
    assert!(mutates(1) && mutates(18) && !mutates(0) && !mutates(7));
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(2, 2), 1)]));
    sim.tuning.nest_print_ticks = 3;
    for _ in 0..30 {
        sim.step();
    }
    assert!(feral_count(&sim) >= 2, "the Magician printed");
    // Every deployed version enters the library; a mutated Drone differs
    // from the pristine source by one nudged integer literal.
    let pristine = Archetype::Drone.source();
    let mutated = sim
        .world
        .program_library
        .values()
        .any(|s| s.contains("wander()") && s.as_str() != pristine);
    assert!(mutated, "no two Magician prints ship the factory source");
    // Mutation keeps the program parse-valid by construction.
    for source in sim.world.program_library.values() {
        assert!(pyrite::parse(source, &pyrite::UnlockSet::all()).is_ok());
    }
}

#[test]
fn harvesters_feed_the_nest_economy() {
    let mut spec = nest_spec(vec![(TilePos::new(2, 2), 0)]);
    spec.ore_nodes.push((TilePos::new(5, 2), 10));
    let mut sim = Sim::new(&spec);
    sim.tuning.nest_print_ticks = 3;
    sim.tuning.nest_income_deci = 0; // isolate the harvest income
    sim.tuning.fault_damage = 0;
    for _ in 0..200 {
        sim.step();
    }
    let nest = &sim.world.nests[&first_nest(&sim)];
    // 300 seed − 3×100 prints = 0; anything above zero was hauled home
    // by the Harvester (deposit() treats the nest as its depot).
    assert!(
        nest.stock_deci > 0 || feral_count(&sim) > 3,
        "the Harvester's deposits fund the nest (stock {} / {} ferals)",
        nest.stock_deci,
        feral_count(&sim)
    );
}

#[test]
fn escalation_follows_footprint_not_the_clock() {
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(2, 2), 0)]));
    sim.tuning.nest_print_ticks = 1000; // no prints; escalation only
    sim.step();
    assert_eq!(sim.world.escalation, 0, "an empty map stays Calm");
    // A big body count raises the footprint metric on its own.
    sim.world.ferals_killed = 25;
    sim.step();
    assert_eq!(sim.world.escalation, 3, "kills push the tier to Overrun");
}

#[test]
fn defeated_nests_claim_into_quadratic_printer_slots() {
    // The curve (docs/01): 2 free, 3rd needs 1 nest, 4th needs 3, then 6.
    assert_eq!(printers_allowed(0), 2);
    assert_eq!(printers_allowed(1), 3);
    assert_eq!(printers_allowed(2), 3);
    assert_eq!(printers_allowed(3), 4);
    assert_eq!(printers_allowed(6), 5);

    let mut spec = nest_spec(vec![(TilePos::new(10, 6), 0)]);
    spec.printers.push(PrinterSpec { pos: TilePos::new(1, 1), faction: 0, color: 0, ruined: false });
    spec.printers.push(PrinterSpec { pos: TilePos::new(3, 1), faction: 0, color: 1, ruined: false });
    spec.fleet_cap_override = Some(0); // no player prints; command plumbing only
    spec.starting_stock = vec![(0, sim::resources::Resource::Steel, 200)];
    let mut sim = Sim::new(&spec);
    sim.tuning.nest_print_ticks = 1000;

    // Gate holds: two printers, zero claimed nests → the 3rd is refused.
    sim.apply(&Command::PlacePrinter { pos: TilePos::new(5, 1), faction: 0 }).unwrap();
    assert_eq!(sim.world.printers.len(), 2, "no controlled nest: the 3rd slot is gated");

    // Beat the nest down (dev kill: direct hp write, the attack path is
    // covered by combat tests) and claim it.
    let nid = first_nest(&sim);
    sim.world.nests.get_mut(&nid).unwrap().hp = 0;
    sim.world.nests.get_mut(&nid).unwrap().state = NestState::Defeated;
    sim.apply(&Command::ClaimNest { nest: nid, faction: 0 }).unwrap();
    assert_eq!(sim.world.nests[&nid].state, NestState::Claimed(0));

    sim.apply(&Command::PlacePrinter { pos: TilePos::new(5, 1), faction: 0 }).unwrap();
    assert_eq!(sim.world.printers.len(), 3, "one claimed nest unlocks the 3rd slot");
    let new = sim.world.printers.values().find(|p| p.pos == TilePos::new(5, 1)).unwrap();
    assert_eq!(new.color, Color(2), "the new slot takes the lowest unused color");
    assert!(new.rules.is_some(), "built printers are never the remainder bucket");

    // 4th slot needs 3 nests total — refused with 1.
    sim.apply(&Command::PlacePrinter { pos: TilePos::new(7, 1), faction: 0 }).unwrap();
    assert_eq!(sim.world.printers.len(), 3, "the 4th slot needs three claimed nests");
}

#[test]
fn razing_pays_the_data_bounty_instead() {
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(2, 2), 0)]));
    let nid = first_nest(&sim);
    // Razing an ACTIVE nest is refused — beat it first.
    sim.apply(&Command::RazeNest { nest: nid, faction: 0 }).unwrap();
    assert!(sim.world.nests.contains_key(&nid));
    sim.world.nests.get_mut(&nid).unwrap().state = NestState::Defeated;
    sim.apply(&Command::RazeNest { nest: nid, faction: 0 }).unwrap();
    assert!(!sim.world.nests.contains_key(&nid), "razed: the site is gone");
    assert_eq!(
        sim.world.data.get(&0).copied().unwrap_or(0),
        sim.tuning.nest_data_bounty,
        "the razer banks the bounty"
    );
}

#[test]
fn undefended_claims_are_retaken_by_feral_activity() {
    let mut spec = nest_spec(vec![(TilePos::new(6, 4), 0)]);
    spec.fleet_cap_override = Some(0);
    let mut sim = Sim::new(&spec);
    sim.tuning.nest_print_ticks = 1000;
    let nid = first_nest(&sim);
    sim.world.nests.get_mut(&nid).unwrap().state = NestState::Claimed(0);

    // A Feral straggler stands adjacent; the owner has nobody in the
    // guard radius. The loan is called in.
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(7, 4),
        source: "wait(600)\n".into(),
        cpu: 4,
        cargo_cap: 1,
        faction: FERAL_FACTION,
        hp: 50,
        color: Color(200),
    })
    .unwrap();
    for _ in 0..5 {
        sim.step();
    }
    assert_eq!(
        sim.world.nests[&nid].state,
        NestState::Active,
        "an undefended claim reverts to the Ferals"
    );

    // With a defender in radius, a fresh claim HOLDS.
    sim.world.nests.get_mut(&nid).unwrap().state = NestState::Claimed(0);
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(5, 4),
        source: "wait(600)\n".into(),
        cpu: 4,
        cargo_cap: 1,
        faction: 0,
        hp: 50,
        color: Color::GREEN,
    })
    .unwrap();
    for _ in 0..5 {
        sim.step();
    }
    assert_eq!(sim.world.nests[&nid].state, NestState::Claimed(0), "a defended claim holds");
}

#[test]
fn drones_hunt_what_they_see_and_players_can_fell_the_nest() {
    // See-first acquisition: the Feral perception cloud (nest eyes +
    // drone sensors) gates `exists(enemy)` like anyone else's.
    let mut spec = nest_spec(vec![(TilePos::new(2, 2), 0)]);
    spec.fleet_cap_override = Some(0);
    let mut sim = Sim::new(&spec);
    sim.tuning.nest_print_ticks = 3;
    sim.tuning.fault_damage = 0;
    // A player bot parked far outside every Feral eye: never engaged.
    let far = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(13, 9),
            source: "wait(600)\n".into(),
            cpu: 4,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    for _ in 0..60 {
        sim.step();
    }
    assert!(feral_count(&sim) > 0, "the nest printed hunters");
    assert_eq!(
        sim.world.bots[&far].data.hp,
        100,
        "unseen means unhunted (see-first acquisition)"
    );

    // The player storms the nest: attack() fells it into DEFEATED, not
    // off the map (raze-or-claim comes after).
    let nid = first_nest(&sim);
    sim.world.nests.get_mut(&nid).unwrap().hp = 15;
    // The nest is SOLID (its prints ring it), so the raider spawns clear
    // and walks in before swinging.
    let src = "\
if exists(nest):
    move_to(closest(nest).expect())
    attack(closest(nest).expect())
wait(1)
";
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(7, 6),
        source: src.into(),
        cpu: 8,
        cargo_cap: 1,
        faction: 0,
        hp: 400,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap();
    for _ in 0..120 {
        sim.step();
    }
    assert_eq!(sim.world.nests[&nid].state, NestState::Defeated, "beaten to a claimable site");
    assert_eq!(sim.world.nests[&nid].hp, 0);
}

#[test]
fn nests_are_solid_ground() {
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(4, 4), 0)]));
    sim.tuning.nest_print_ticks = 3;
    let pos = TilePos::new(4, 4);
    assert!(sim.world.structure_at(pos), "the nest occupies its tile");
    assert!(sim.world.structure_tiles().contains(&pos), "A* routes around it");
    // Nothing spawns ON the site — dev spawns are refused and Feral
    // prints ring it (review 2026-07-16: the first print landed on top).
    let refused = sim
        .apply(&Command::SpawnBot {
            pos,
            source: "wait(600)\n".into(),
            cpu: 4,
            cargo_cap: 1,
            faction: 0,
            hp: 50,
            color: Color::GREEN,
        })
        .unwrap();
    assert!(refused.is_none(), "a solid site rejects spawns");
    for _ in 0..30 {
        sim.step();
    }
    assert!(feral_count(&sim) > 0);
    assert!(
        sim.world.bots.values().all(|b| b.data.pos != pos),
        "every print landed AROUND the nest"
    );
}

#[test]
fn defeated_nests_are_not_an_xp_farm() {
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(4, 4), 0)]));
    sim.tuning.fault_damage = 0;
    sim.tuning.nest_print_ticks = 10_000;
    let nid = first_nest(&sim);
    sim.world.nests.get_mut(&nid).unwrap().hp = 5;
    let raider = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(5, 4),
            source: "if exists(nest):\n    attack(closest(nest).expect())\nwait(1)\n".into(),
            cpu: 8,
            cargo_cap: 1,
            faction: 0,
            hp: 200,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    for _ in 0..30 {
        sim.step();
    }
    assert_eq!(sim.world.nests[&nid].state, NestState::Defeated);
    let at_defeat = sim.world.bots[&raider].data.xp(sim::world::XpTrack::Combat);
    for _ in 0..40 {
        sim.step();
    }
    let later = sim.world.bots[&raider].data.xp(sim::world::XpTrack::Combat);
    assert_eq!(
        later, at_defeat,
        "swinging at a Defeated site (hp 0 forever) mints nothing (review 2026-07-16)"
    );
}

#[test]
fn feral_self_deaths_never_escalate() {
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(4, 4), 0)]));
    sim.tuning.nest_print_ticks = 10_000;
    sim.tuning.fault_damage = 60; // two faults kill
    // A Feral that faults itself to death (docs/04: escalation is PLAYER
    // footprint — self-inflicted attrition must not raise it).
    let lemming = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(7, 4),
            source: "x = closest(enemy).expect()\nwait(1)\n".into(),
            cpu: 8,
            cargo_cap: 1,
            faction: FERAL_FACTION,
            hp: 100,
            color: Color(200),
        })
        .unwrap()
        .unwrap();
    for _ in 0..40 {
        sim.step();
        if !sim.world.bots.contains_key(&lemming) {
            break;
        }
    }
    assert!(!sim.world.bots.contains_key(&lemming), "the fault loop was fatal");
    assert_eq!(sim.world.ferals_killed, 0, "self-deaths are not player footprint");

    // A player-attributed kill IS.
    sim.tuning.fault_damage = 0;
    let prey = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(8, 6),
            source: "wait(600)\n".into(),
            cpu: 4,
            cargo_cap: 1,
            faction: FERAL_FACTION,
            hp: 20,
            color: Color(200),
        })
        .unwrap()
        .unwrap();
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(9, 6),
        source: "if exists(enemy):\n    attack(closest(enemy).expect())\nwait(1)\n".into(),
        cpu: 8,
        cargo_cap: 1,
        faction: 0,
        hp: 200,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap();
    for _ in 0..40 {
        sim.step();
        if !sim.world.bots.contains_key(&prey) {
            break;
        }
    }
    assert_eq!(sim.world.ferals_killed, 1, "a player kill raises the footprint");
}

#[test]
fn dormant_nests_swallow_no_deposits() {
    let mut sim = Sim::new(&nest_spec(vec![(TilePos::new(4, 4), 0)]));
    sim.tuning.fault_damage = 0;
    sim.tuning.nest_print_ticks = 10_000;
    sim.tuning.nest_income_deci = 0;
    let nid = first_nest(&sim);
    // A Feral hauler parked beside its nest, holding cargo, VM idle.
    let hauler = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(5, 4),
            source: "wait(600)\n".into(),
            cpu: 4,
            cargo_cap: 4,
            faction: FERAL_FACTION,
            hp: 100,
            color: Color(202),
        })
        .unwrap()
        .unwrap();
    for _ in 0..3 {
        sim.step(); // park the VM in its wait first
    }
    sim.world
        .bots
        .get_mut(&hauler)
        .unwrap()
        .data
        .cargo
        .insert(sim::resources::Resource::Iron, 20);
    // Request lands while Active; the nest is beaten to Defeated before
    // the deposit settles — the manifest must NOT pre-fund the site.
    sim.world.bots.get_mut(&hauler).unwrap().data.requested =
        Some(sim::world::ActionRequest::Deposit { fault_on_fail: false });
    sim.step(); // resolve: acceptor picked (Active), 1-tick action parked
    sim.world.nests.get_mut(&nid).unwrap().hp = 0;
    sim.world.nests.get_mut(&nid).unwrap().state = NestState::Defeated;
    let stock_before = sim.world.nests[&nid].stock_deci;
    for _ in 0..3 {
        sim.step(); // the deposit settles against the dormant site
    }
    assert_eq!(
        sim.world.nests[&nid].stock_deci, stock_before,
        "a Defeated site absorbs nothing (review 2026-07-16)"
    );
    assert_eq!(
        sim.world.bots[&hauler].data.cargo.get(&sim::resources::Resource::Iron),
        Some(&20),
        "the manifest stays aboard"
    );
}
