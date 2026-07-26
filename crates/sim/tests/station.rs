//! The Upgrade Station (M5, docs/03+06): pad-pull queues, payment at
//! mount, coolant feeds, catalog effects, module slots and swaps.

use sim::map::MapSpec;
use sim::resources::Resource;
use sim::sim::{Command, Sim};
use sim::world::{Capability, DamageTarget, Color, StructureKind};
use sim::TilePos;

const STATION_POS: TilePos = TilePos { x: 4, y: 2 };

/// A powered-off sandbox with a bought-and-placed station, seeded stock,
/// and a wet coolant buffer.
fn station_sim(stock: &[(Resource, u64)]) -> (Sim, sim::EntityId) {
    let mut spec = MapSpec::empty(9, 5);
    spec.starting_stock.push((0, Resource::Steel, 100));
    spec.starting_stock.push((0, Resource::Chips, 50));
    spec.starting_stock.push((0, Resource::Wire, 30));
    for (k, deci) in stock {
        spec.starting_stock.push((0, *k, *deci));
    }
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::PlaceStructure {
        pos: STATION_POS,
        kind: StructureKind::UpgradeStation,
        faction: 0,
    })
    .unwrap();
    sim.finish_structure_for_test(STATION_POS);
    let sid = *sim.world.structures.keys().next().expect("station placed");
    // Coolant is a physical feed — wet the buffer by hand (a Pump/It's
    // hauled in real play; see TASKS.md water-source discussion).
    sim.world.structures.get_mut(&sid).unwrap().input.insert(Resource::Water, 100);
    (sim, sid)
}

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 1,
        cargo_cap: 1,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

#[test]
fn cpu_upgrade_full_cycle() {
    let (mut sim, sid) = station_sim(&[]);
    let bot = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n");
    let chips_before = sim.world.stock_get(0, Resource::Chips);
    sim.apply(&Command::QueueUpgrade { bot, order: "cpu_mk2".into(), replace: None }).unwrap();
    // Pull happens in phase 8; the sit is 40 ticks; step-off restarts.
    let mut mounted = false;
    for _ in 0..60 {
        sim.step();
        if sim.world.bots[&bot].data.pad_sit {
            mounted = true;
            assert_eq!(sim.world.bots[&bot].data.pos, STATION_POS, "mounted ON the pad");
        }
    }
    assert!(mounted, "the pad must pull the adjacent queued bot");
    let data = &sim.world.bots[&bot].data;
    assert!(!data.pad_sit, "sit over; stepped off");
    assert_ne!(data.pos, STATION_POS, "stepped off the pad");
    assert_eq!(data.upgrades.len(), 1, "upgrade recorded on the chassis");
    assert_eq!(
        sim::stats::cpu_centi(sim.ctx(), data, false, false),
        200,
        "CPU Mk2: 2 cycles/tick"
    );
    assert_eq!(
        sim.world.stock_get(0, Resource::Chips),
        chips_before - 50,
        "5 Chips paid at mount"
    );
    assert_eq!(
        sim.world.structures[&sid].input.get(&Resource::Water).copied().unwrap_or(0),
        90,
        "1 Water coolant consumed"
    );
}

#[test]
fn unaffordable_orders_skip_and_rearm() {
    // No GoldChips in stock: a queued Backup Core can't mount; the pad
    // skips to the next queued bot rather than wedging.
    let (mut sim, _sid) = station_sim(&[]);
    let rich = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n");
    let poor = spawn(&mut sim, TilePos::new(5, 2), "wait(600)\n");
    // Lowest entity id would normally win the pull — queue the pricy
    // order on the LOWER id so the skip is observable.
    sim.apply(&Command::QueueUpgrade { bot: rich, order: "backup_core".into(), replace: None })
        .unwrap();
    sim.apply(&Command::QueueUpgrade { bot: poor, order: "cpu_mk2".into(), replace: None })
        .unwrap();
    for _ in 0..80 {
        sim.step();
    }
    assert!(sim.world.bots[&rich].data.upgrades.is_empty(), "backup core skipped (no gold)");
    assert!(
        !sim.world.bots[&rich].data.upgrade_queue.is_empty(),
        "skipped order re-arms, never drops"
    );
    assert_eq!(sim.world.bots[&poor].data.upgrades.len(), 1, "the pad moved on to the next bot");
    // Stock arrives: the skipped order finally mounts. (The Backup Core
    // prices in Chips AND Gold Chips — Q100's late, gilded insurance.)
    // stock_add takes DECI: the Backup Core prices at 12 Chips + 4 Gold
    // Chips, i.e. 120 + 40 deci.
    sim.world.stock_add(0, Resource::GoldChip, 100);
    sim.world.stock_add(0, Resource::Chips, 200);
    for _ in 0..120 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&rich].data.upgrades.len(), 1, "re-armed order served");
}

#[test]
fn pad_pull_skips_mid_template_bots() {
    let (mut sim, _sid) = station_sim(&[]);
    // The queued bot is mid-template the whole test: give it a long bump
    // window via a self-bump... simpler: put it in a hurt window by
    // damaging it under a written handler that waits.
    let src = "on hurt:\n    drop_cargo()\n    upload_log()\n\nwait(600)\n";
    let templated = spawn(&mut sim, TilePos::new(3, 2), src);
    sim.apply(&Command::QueueUpgrade {
        bot: templated,
        order: "cpu_mk2".into(),
        replace: None,
    })
    .unwrap();
    // Freeze the bot mid-template by hand: raise hurt via damage BELOW the
    // line, then check the pull never fires while the template runs.
    // (Cheap deterministic stand-in: bump_frozen also blocks nothing here —
    // the honest signal is vm phase, so drive damage.)
    sim.world.bots.get_mut(&templated).unwrap().data.hp = 100;
    sim.world.pending_damage.push((DamageTarget::Bot(templated), 60, None));
    let mut was_pulled_mid_template = false;
    for _ in 0..12 {
        sim.step();
        let b = &sim.world.bots[&templated];
        let mid_template =
            b.vm.as_ref().is_some_and(|vm| vm.phase() != pyrite::Phase::Main);
        if b.data.pad_sit && mid_template {
            was_pulled_mid_template = true;
        }
    }
    assert!(!was_pulled_mid_template, "the pull itself must never create a double-handle");
    // Once the window finishes, the pull proceeds normally.
    for _ in 0..80 {
        sim.step();
    }
    assert_eq!(sim.world.bots[&templated].data.upgrades.len(), 1);
}

#[test]
fn buying_a_capability_tier_raises_it_and_grants_its_bonus() {
    // Q105: capability tiers replace generic module slots. Optics is the
    // clean probe — its tier grant is a flat sensor bonus.
    let (mut sim, _sid) = station_sim(&[(Resource::Lens, 100), (Resource::Bronze, 100)]);
    let bot = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n");
    let base = sim.ctx().sensors_for(&sim.world.bots[&bot].data);
    assert_eq!(sim.world.bots[&bot].data.tier(Capability::Optics), 1, "free base tier");

    sim.apply(&Command::QueueUpgrade { bot, order: "optics".into(), replace: None }).unwrap();
    for _ in 0..60 {
        sim.step();
    }
    let data = &sim.world.bots[&bot].data;
    assert_eq!(data.tier(Capability::Optics), 2, "the tier rose");
    assert_eq!(
        sim.ctx().sensors_for(data),
        base + sim.stats.tier_sensors,
        "the tier's flat grant lands"
    );
    // Every OTHER capability is untouched — slots are per-capability.
    assert_eq!(data.tier(Capability::Mining), 1);
    assert_eq!(data.tier_value(), 1, "one tier above base was bought");
}

/// Q105-R1: a tier's grant must DOMINATE the level bonus its purchase
/// resets, or buying an upgrade would leave a maxed bot worse at the very
/// stat it paid for. Validated at load; asserted here on the shipped data.
#[test]
fn a_tier_grant_dominates_the_levels_it_resets() {
    let sim = Sim::new(&MapSpec::empty(4, 4));
    let l5_sensors = sim.xp.scouting_sensors_per_level * 5;
    assert!(
        sim.stats.tier_sensors >= l5_sensors,
        "Optics tier grant {} must clear the Scouting L5 bonus {}",
        sim.stats.tier_sensors,
        l5_sensors
    );
}

#[test]
fn memory_bank_grows_the_log_cap_and_duplicate_cpu_tiers_drop() {
    let (mut sim, _sid) = station_sim(&[]);
    let bot = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n");
    let cap_before = sim.world.bots[&bot].data.log_cap;
    sim.apply(&Command::QueueUpgrade { bot, order: "memory_bank".into(), replace: None })
        .unwrap();
    sim.apply(&Command::QueueUpgrade { bot, order: "cpu_mk2".into(), replace: None }).unwrap();
    sim.apply(&Command::QueueUpgrade { bot, order: "cpu_mk2".into(), replace: None }).unwrap();
    for _ in 0..200 {
        sim.step();
    }
    let data = &sim.world.bots[&bot].data;
    assert_eq!(data.log_cap, cap_before + sim.stats.memory_bank_log, "+8 log entries");
    let (mk2, _) = sim.stats.upgrade("cpu_mk2").unwrap();
    assert_eq!(
        data.upgrades.iter().filter(|&&u| u == mk2).count(),
        1,
        "a duplicate CPU tier is dropped at mount, unpaid"
    );
    assert!(data.upgrade_queue.is_empty(), "the dropped duplicate leaves the queue");
}

/// A lockstep command must never panic the sim: replace=Some(255) queues
/// (the hash writes presence + raw value, no arithmetic) and then gets
/// dropped at mount as an invalid slot.
#[test]
fn hostile_replace_slot_neither_panics_nor_mounts() {
    let (mut sim, _sid) = station_sim(&[(Resource::Lens, 100), (Resource::Bronze, 100)]);
    let bot = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n");
    sim.apply(&Command::QueueUpgrade { bot, order: "optics".into(), replace: Some(255) })
        .unwrap();
    let _ = sim.state_hash(); // used to overflow on slot + 1
    for _ in 0..60 {
        sim.step();
    }
    let data = &sim.world.bots[&bot].data;
    // Q105: a tier order never names a slot, so a replace= target makes
    // the order unresolvable — it is refused at the command, not mounted.
    assert_eq!(data.tier(Capability::Optics), 1, "a slot-naming order never mounts");
    assert!(data.upgrade_queue.is_empty(), "the invalid order is dropped, not wedged");
}

/// CPU tiers SET the cycle grant in purchase order — buying a lower tier
/// after a higher one would be a PAID DOWNGRADE. It drops at mount, unpaid.
#[test]
fn lower_cpu_tier_after_higher_is_dropped_unpaid() {
    let (mut sim, _sid) = station_sim(&[]);
    sim.world.stock_add(0, Resource::Chips, 500);
    let bot = spawn(&mut sim, TilePos::new(3, 2), "wait(600)\n");
    sim.apply(&Command::QueueUpgrade { bot, order: "cpu_mk3".into(), replace: None }).unwrap();
    for _ in 0..100 {
        sim.step();
    }
    assert_eq!(sim::stats::cpu_centi(sim.ctx(), &sim.world.bots[&bot].data, false, false), 400);
    let chips_after_mk3 = sim.world.stock_get(0, Resource::Chips);
    sim.apply(&Command::QueueUpgrade { bot, order: "cpu_mk2".into(), replace: None }).unwrap();
    for _ in 0..100 {
        sim.step();
    }
    let data = &sim.world.bots[&bot].data;
    assert_eq!(
        sim::stats::cpu_centi(sim.ctx(), data, false, false),
        400,
        "the grant never downgrades"
    );
    assert_eq!(
        sim.world.stock_get(0, Resource::Chips),
        chips_after_mk3,
        "the dropped order charges nothing"
    );
    assert!(data.upgrade_queue.is_empty(), "dropped, not wedged");
}
