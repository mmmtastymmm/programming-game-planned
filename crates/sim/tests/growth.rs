//! XP v2 & quirks (M6, docs/02 + docs/09): the quadratic curve, incomes,
//! the Learning feed, total-XP milestones, latent quirk rolls, and
//! manifestation.

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::{Color, XpTrack};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 2,
        cargo_cap: 4,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

#[test]
fn the_quadratic_curve_levels_where_the_doc_says() {
    let sim = Sim::new(&MapSpec::empty(4, 4));
    // docs/02: cumulative 100/300/600/1000/1500 whole XP, cap L5.
    for (deci, level) in [
        (0, 0),
        (999, 0),
        (1000, 1),
        (2999, 1),
        (3000, 2),
        (6000, 3),
        (10_000, 4),
        (15_000, 5),
        (99_999, 5),
    ] {
        assert_eq!(sim.xp.level(deci), level, "{deci} deci-XP");
    }
    assert_eq!(sim.xp.track_cap_deci(), 15_000, "the L5 boundary is the track cap");
}

#[test]
fn age_drips_and_learning_feeds_on_it() {
    let mut spec = MapSpec::empty(4, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    for _ in 0..200 {
        sim.step();
    }
    let data = &sim.world.bots[&bot].data;
    assert_eq!(data.xp(XpTrack::Age), 200, "1 deci-XP per tick survived");
    // Learning = 10% of the drip, accrued through the fractional carry.
    assert_eq!(data.xp(XpTrack::Learning), 20, "10% of 200 deci");
}

#[test]
fn hauling_pays_cargo_distance_at_delivery_and_mileage_per_tile() {
    let mut spec = MapSpec::empty(12, 4);
    spec.quirk_permille = 0;
    spec.ore_nodes.push((TilePos::new(9, 1), 100));
    spec.depots.push(TilePos::new(1, 1));
    let mut sim = Sim::new(&spec);
    sim.stats.move_rate_deci = 10; // 1 tick/tile: pacing isn't under test
    let bot = spawn(
        &mut sim,
        TilePos::new(2, 1),
        "move_to(closest(ore).expect())\nmine()\nmove_to(closest(depot).expect())\ndeposit()\nwait(600)\n",
    );
    for _ in 0..120 {
        sim.step();
    }
    let data = &sim.world.bots[&bot].data;
    assert!(data.xp(XpTrack::Mileage) >= 120, "1 XP (10 deci) per tile, both legs");
    // One swing = 2 units carried ~6 tiles home: 2 deci-XP per tile.
    let hauled = data.xp(XpTrack::Hauling);
    assert!(
        (10..=16).contains(&hauled),
        "cargo-distance delivered: 2 units x ~6 tiles = ~12 deci, got {hauled}"
    );
    assert_eq!(data.haul_accum, 0, "the accumulator paid out at the depot");
}

#[test]
fn flinches_train_only_from_hostile_sources() {
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0;
    let victim = spawn(&mut sim, TilePos::new(2, 1), "wait(600)\n");
    // Hostile damage crossing the hurt line = one hostile flinch.
    let enemy = sim
        .apply(&Command::SpawnBot {
            pos: TilePos::new(3, 1),
            source: "wait(600)\n".into(),
            cpu: 2,
            cargo_cap: 1,
            faction: 1,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
    sim.world.pending_damage.push((victim, 60, Some((enemy, 1))));
    for _ in 0..30 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&victim].data.xp(XpTrack::Flinch),
        100,
        "10 XP (100 deci) per hostile flinch"
    );
    // Self-inflicted (no attacker tag): the flinch happens, no XP.
    let loner = spawn(&mut sim, TilePos::new(5, 1), "wait(600)\n");
    sim.world.pending_damage.push((loner, 60, None));
    for _ in 0..30 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&loner].data.xp(XpTrack::Flinch),
        0,
        "self-inflicted signals grant nothing (docs/02 source filter)"
    );
}

#[test]
fn total_xp_milestones_grow_module_slots() {
    let mut spec = MapSpec::empty(4, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    assert_eq!(sim.world.bots[&bot].data.module_slots, 1);
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Mining, 10_000);
    sim.step();
    assert_eq!(sim.world.bots[&bot].data.module_slots, 2, "+1 at 1000 total XP");
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Combat, 20_000);
    sim.step();
    assert_eq!(sim.world.bots[&bot].data.module_slots, 3, "+1 at 3000, cap 3");
}

#[test]
fn quirks_roll_latent_and_manifest_at_the_threshold() {
    let mut spec = MapSpec::empty(4, 4);
    spec.quirk_permille = 2000; // both latent slots certain
    let mut sim = Sim::new(&spec);
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    {
        let data = &sim.world.bots[&bot].data;
        assert_eq!(data.latent_quirks.len(), 2, "the dial at 2000 rolls both slots");
        assert!(data.quirks.is_empty(), "latent quirks do not exist to the world");
    }
    sim.step();
    assert!(sim.world.bots[&bot].data.quirks.is_empty(), "rookies stay quirk-free");
    // Cross 300 total XP (3000 deci): the first roll comes alive.
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Mining, 3000);
    sim.step();
    let data = &sim.world.bots[&bot].data;
    assert_eq!(data.quirks.len(), 1, "first manifestation at 300 XP");
    assert_eq!(data.latent_quirks.len(), 1);
    // Cross 900: the second.
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Mining, 9000);
    sim.step();
    assert_eq!(sim.world.bots[&bot].data.quirks.len(), 2, "second at 900 XP");
}

#[test]
fn quirk_rolls_are_deterministic_and_gated_by_the_dial() {
    let roll = |permille: u32| -> Vec<Vec<u8>> {
        let mut spec = MapSpec::empty(6, 4);
        spec.seed = 0xDECAF;
        spec.quirk_permille = permille;
        let mut sim = Sim::new(&spec);
        (0..4)
            .map(|i| {
                let id = spawn(&mut sim, TilePos::new(1 + i, 1), "wait(9)\n");
                sim.world.bots[&id].data.latent_quirks.clone()
            })
            .collect()
    };
    assert_eq!(roll(500), roll(500), "same seed, same rolls (rng.quirk_roll)");
    assert!(roll(0).iter().all(|l| l.is_empty()), "0 = quirks off (docs/09)");
    assert!(roll(2000).iter().all(|l| l.len() == 2), "2000 = both slots certain");
}

#[test]
fn manifested_quirk_effects_reach_the_pipeline_and_introspection() {
    let mut spec = MapSpec::empty(4, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    let overclocked = sim.quirks.by_name("overclocked").expect("in the catalog");
    // Hand-manifest for the effect test (the roll paths are covered above).
    sim.world.bots.get_mut(&bot).unwrap().data.quirks.push(overclocked);
    let data = &sim.world.bots[&bot].data;
    assert_eq!(
        sim::stats::cpu_centi(sim.ctx(), data, false, false),
        300,
        "spawn cpu 2 (200 centi) + Overclocked (+1 cycle)"
    );
    // Introspection reads only MANIFESTED quirks.
    let latent_only = sim.quirks.by_name("memory_leak").unwrap();
    sim.world.bots.get_mut(&bot).unwrap().data.latent_quirks.push(latent_only);
    let data = &sim.world.bots[&bot].data;
    assert!(data.quirks.contains(&overclocked));
    assert!(!data.quirks.contains(&latent_only));
}

#[test]
fn policy_quirks_shift_defaults_and_clamp_setenv() {
    let mut spec = MapSpec::empty(4, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    let defensive = sim.quirks.by_name("defensive_programming").expect("in the catalog");
    sim.world.bots.get_mut(&bot).unwrap().data.quirks.push(defensive);
    // Temperament: the unset key reads the quirk's default (60, not 50).
    let data = &sim.world.bots[&bot].data;
    assert_eq!(
        sim::world::env_read(data, "hurt_line", &sim.tuning, &sim.quirks),
        60,
        "temperament shifts the default"
    );
    // Compulsion: a stored value past the clamp CLIPS on read — the
    // hardware refuses; getenv reports where it landed (docs/09 Q60).
    sim.world.bots.get_mut(&bot).unwrap().data.env.insert("hurt_line".into(), 20);
    let data = &sim.world.bots[&bot].data;
    assert_eq!(
        sim::world::env_read(data, "hurt_line", &sim.tuning, &sim.quirks),
        55,
        "compulsion clamps to 55..=99"
    );
}
