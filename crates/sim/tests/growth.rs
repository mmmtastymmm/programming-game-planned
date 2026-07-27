//! XP v2 & quirks (M6, docs/02 + docs/09): the quadratic curve, incomes,
//! the Learning feed, total-XP milestones, latent quirk rolls, and
//! manifestation.

use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::{DamageTarget, Color, XpTrack};
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
    spec.depots.push((TilePos::new(1, 1), 0));
    let mut sim = Sim::new(&spec);
    sim.stats.move_rate_deci = 10; // 1 tick/tile: pacing isn't under test
    sim.stats.sensors = 12; // start-zone sight guarantee (docs/03): the
    // node is discovered from the spawn tile; vision isn't under test
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
    let hauled = data.xp(XpTrack::Hauling).raw_unscaled();
    assert!(
        (10..=16).contains(&hauled),
        "cargo-distance delivered: 2 units x ~6 tiles = ~12 deci, got {hauled}"
    );
    assert_eq!(data.haul_accum, 0, "the accumulator paid out at the depot");
}

#[test]
fn co_arriving_bumps_grant_flinch_if_any_source_is_hostile() {
    // A victim rammed by an enemy AND a friendly in the same tick earns the
    // flinch: the hostile ram happened regardless of which duplicate was
    // pushed last. Before the fix, eligibility read only the winning signal's
    // single (last-pushed) source, so a friendly ram arriving after an enemy
    // one robbed the XP (whole-codebase review 2026-07-23).
    let mut spec = MapSpec::empty(8, 4);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    let victim = spawn(&mut sim, TilePos::new(2, 1), "on bumped:\n    wait(1)\n\nwait(600)\n");
    // Let the bot finish booting and settle into its idle wait.
    for _ in 0..20 {
        sim.step();
    }
    // Enemy (faction 1) pushed FIRST, friendly (faction 0) LAST — the order
    // whose last-wins source used to deny the flinch XP.
    sim.world.pending_signals.push((victim, pyrite::Signal::Bumped, Some(1)));
    sim.world.pending_signals.push((victim, pyrite::Signal::Bumped, Some(0)));
    for _ in 0..5 {
        sim.step();
    }
    assert_eq!(
        sim.world.bots[&victim].data.xp(XpTrack::Flinch),
        100,
        "a co-arriving enemy ram grants the flinch even if a friendly ram was pushed last"
    );
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
    sim.world.pending_damage.push((DamageTarget::Bot(victim), 60, Some((enemy, 1))));
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
    sim.world.pending_damage.push((DamageTarget::Bot(loner), 60, None));
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
    // Q105 ruling (a): manifestation reads the AGE track, not total XP.
    // Tier-scaled task XP would cross the old total thresholds within a
    // few units of work and pop every latent quirk at once; Age is
    // tier-independent and unfarmable. Task XP must therefore do nothing.
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Mining, sim::world::StoredXp::from_scaled(900_000));
    sim.step();
    assert!(
        sim.world.bots[&bot].data.quirks.is_empty(),
        "task XP never manifests a quirk — only time survived does"
    );
    // Thresholds come from the catalog, not from literals: they are
    // tuning constants and were retuned once already (M16 review) —
    // hardcoding them here just moves the breakage into the test.
    let first = sim.quirks.manifest_at[0] * 10;
    let second = sim.quirks.manifest_at[1] * 10;
    // Land just BELOW the first threshold — minus 2, because the step
    // itself drips one more deci of Age before manifestation is checked.
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Age, sim::world::StoredXp::from_scaled(first - 2));
    sim.step();
    assert!(
        sim.world.bots[&bot].data.quirks.is_empty(),
        "one deci short of the threshold manifests nothing"
    );
    // Cross the first Age threshold: the first roll comes alive.
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Age, sim::world::StoredXp::from_scaled(first));
    sim.step();
    let data = &sim.world.bots[&bot].data;
    assert_eq!(data.quirks.len(), 1, "first manifestation at manifest_at[0]");
    assert_eq!(data.latent_quirks.len(), 1);
    // Cross the second Age threshold: the second.
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Age, sim::world::StoredXp::from_scaled(second));
    sim.step();
    assert_eq!(sim.world.bots[&bot].data.quirks.len(), 2, "second at manifest_at[1]");
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

#[test]
fn detection_by_an_enemy_pays_the_hiding_track() {
    // docs/05: a fresh detection episode (an enemy faction newly sees/hears
    // you) pays the Hiding XP track — "being caught teaches". Two adjacent
    // bots of different factions detect each other on the first perception
    // pass. The whole Hiding track (settle_episodes) had no test.
    let mut spec = MapSpec::empty(6, 3);
    spec.quirk_permille = 0;
    let mut sim = Sim::new(&spec);
    let hider = spawn(&mut sim, TilePos::new(2, 1), "wait(100000)\n"); // faction 0
    sim.apply(&Command::SpawnBot {
        pos: TilePos::new(3, 1),
        source: "wait(100000)\n".into(),
        cpu: 4,
        cargo_cap: 1,
        faction: 1,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap();
    for _ in 0..5 {
        sim.step();
    }
    assert!(
        sim.world.bots[&hider].data.xp(XpTrack::Hiding) > 0,
        "being detected by an enemy opens an episode and pays Hiding XP"
    );
}

/// Q105: buying a tier effectively RESETS that capability's level — by
/// arithmetic, not a reset branch. Each tier multiplies the level
/// thresholds and the XP gain by the same factor, so carried XP falls
/// below the new L1 while re-climbing costs the same *work*, and no
/// number ever decreases (which is what keeps a veteran reading as a
/// veteran to the scrap valve).
#[test]
fn a_tier_purchase_resets_the_level_without_erasing_xp() {
    use sim::world::Capability;
    let mut sim = Sim::new(&MapSpec::empty(6, 6));
    let bot = spawn(&mut sim, TilePos::new(2, 2), "wait(600)\n");

    // Max the Mining track at tier 1.
    let maxed = sim.xp.track_cap_deci();
    sim.world.bots.get_mut(&bot).unwrap().data.xp.insert(XpTrack::Mining, sim::world::StoredXp::from_scaled(maxed));
    let data = &sim.world.bots[&bot].data;
    assert_eq!(sim.ctx().capability_level(data, Capability::Mining), sim.xp.level_cap, "L5 at tier 1");

    // Buy tier 2 by hand (the Station path is covered in station.rs).
    sim.world.bots.get_mut(&bot).unwrap().data.tiers[Capability::Mining.idx()] = 2;
    let data = &sim.world.bots[&bot].data;
    assert_eq!(sim.ctx().capability_level(data, Capability::Mining), 0, "the new tier starts green");
    assert_eq!(
        data.xp(XpTrack::Mining),
        maxed,
        "and yet nothing was erased — the XP is untouched, only outscaled"
    );
    // Untouched XP is exactly why the fleet still reads this bot as
    // experienced (Q105-R3's other half is tier_value).
    assert!(data.xp_total() >= maxed, "total XP never decreases");
}

// --- M16 review: the tier-scale leaks ------------------------------
//
// Capability XP is STORED tier-scaled (Q105), so every consumer has to
// divide it back down before reading a level. Six of them did not. Each
// test below fails against the pre-fix code.

/// Give a bot a capability tier and enough stored XP that the RAW number
/// reads at the level cap while the EFFECTIVE level is still 0 — the exact
/// shape that fooled the perk gates and the energy bill.
fn tier_up(sim: &mut Sim, bot: sim::BotId, cap: sim::world::Capability, tier: u8, stored: u64) {
    let data = &mut sim.world.bots.get_mut(&bot).unwrap().data;
    data.tiers[cap.idx()] = tier;
    data.xp.insert(cap.track(), sim::world::StoredXp::from_scaled(stored));
}

#[test]
fn a_tier_purchase_does_not_hand_out_the_l3_perks_for_free() {
    // Asserts on the PERK, not on the helper: the Scouting-L3 immunity is
    // observable as the corruption compute tax the VM does or does not
    // pay, and an earlier version of this test only compared
    // `track_level(..) < 3` against itself (M16 max review).
    let taxed = |tier: u8| {
        let mut spec = MapSpec::empty(6, 6);
        spec.quirk_permille = 0;
        spec.corruption.push(TilePos::new(1, 1));
        let mut sim = Sim::new(&spec);
        let bot = spawn(&mut sim, TilePos::new(1, 1), "x = 1\n");
        // The SAME stored magnitude either way — a Scouting-L4 scout that
        // then buys Optics tier 2. Storage never decreases; only the
        // scale it is read at changes, which is the whole of Q105.
        tier_up(&mut sim, bot, sim::world::Capability::Optics, tier, 10_000);
        // Past the boot ritual: phase 2 skips a booting bot entirely, so
        // the overlay is not set until the VM actually runs.
        for _ in 0..10 {
            sim.step();
        }
        sim.world.bots[&bot]
            .vm
            .as_ref()
            .map(|vm| vm.cost_overlay_centi())
            .unwrap_or(0)
    };
    // Tier 1 with 10,000 deci really is Scouting L4 — genuinely immune.
    assert_eq!(taxed(1), 0, "a real L3+ scout runs clean inside Corruption");
    // The SAME work at tier 2 stores 100x and raw-levels as L4 too, but
    // the bot's effective level is 0: it must still pay the tax.
    assert!(
        taxed(2) > 0,
        "a freshly-tiered bot has NOT earned the L3 immunity — the raw \
         stored number only looks like it has"
    );
}

#[test]
fn tier_scaled_storage_does_not_inflate_the_energy_bill() {
    // Asserts on the OBSERVABLE — whether the colony browns out — not on
    // the helper the fix happens to use. The first version of this test
    // re-implemented the upkeep sum from `StatCtx::track_level` and
    // compared it against itself, so reverting the actual call site in
    // `settle_upkeep` left it green (M16 max review).
    // Track LEVELS are the only thing drawing power here, so the colony
    // browns out exactly when the sim thinks the bot has levels.
    let browns_out = |tier: u8| {
        let mut sim = Sim::new(&MapSpec::empty(6, 6));
        sim.world.dev_free_power = false; // empty maps run on free power
        sim.upkeep.interval_ticks = 1;
        sim.upkeep.base_draw = 0;
        sim.upkeep.draw_per_upgrade = 0;
        sim.upkeep.draw_per_module = 0;
        sim.upkeep.draw_per_track_level = 2;
        let cap = sim.xp.track_cap_deci();
        let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
        // The SAME stored magnitude either way — a bot maxed at tier 1
        // that then buys tier 2. Storage never decreases; only the scale
        // it is read at changes.
        tier_up(&mut sim, bot, sim::world::Capability::Mining, tier, cap);
        for _ in 0..5 {
            sim.step();
        }
        sim.world.brownout.contains(&0)
    };
    assert!(
        browns_out(1),
        "sanity: genuine veteran levels DO cost power — otherwise this \
         test could not tell the two cases apart"
    );
    assert!(
        !browns_out(2),
        "a tier purchase must not bill the colony for levels the bot no \
         longer has"
    );
}

#[test]
fn buying_the_processor_tier_is_never_a_cycle_downgrade() {
    // Q105-R1, the invariant validate_against_xp now asserts at load.
    let mut sim = Sim::new(&MapSpec::empty(6, 6));
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    let cap = sim.xp.track_cap_deci();
    tier_up(&mut sim, bot, sim::world::Capability::Processor, 1, cap);
    let maxed_at_t1 = {
        let data = &sim.world.bots[&bot].data;
        sim::stats::cpu_centi(sim.ctx(), data, false, false)
    };
    // Buy the next tier: the level resets to 0, the flat grant replaces it.
    tier_up(&mut sim, bot, sim::world::Capability::Processor, 2, cap);
    let fresh_at_t2 = {
        let data = &sim.world.bots[&bot].data;
        sim::stats::cpu_centi(sim.ctx(), data, false, false)
    };
    assert!(
        fresh_at_t2 >= maxed_at_t1,
        "paying for a Processor tier must not make the bot slower \
         (was {maxed_at_t1} centicycles, became {fresh_at_t2})"
    );
}

#[test]
fn one_bought_tier_outranks_a_fully_maxed_rookie() {
    // Q105-R3: the scrap valve ranks on investment(), and a Backup-Core
    // reprint (every tier, zero XP) must never read as the cheapest
    // machine in the fleet.
    let mut sim = Sim::new(&MapSpec::empty(6, 6));
    let reprint = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    let rookie = spawn(&mut sim, TilePos::new(2, 1), "wait(600)\n");
    {
        let data = &mut sim.world.bots.get_mut(&reprint).unwrap().data;
        data.tiers[sim::world::Capability::Mining.idx()] = 2;
    }
    {
        // The rookie maxes EVERY track — the best an untiered bot can do.
        let cap = sim.xp.track_cap_deci();
        let data = &mut sim.world.bots.get_mut(&rookie).unwrap().data;
        for t in XpTrack::ALL {
            data.xp.insert(t, sim::world::StoredXp::from_scaled(cap));
        }
    }
    let ctx = sim.ctx();
    let a = sim.world.bots[&reprint].data.investment(ctx);
    let b = sim.world.bots[&rookie].data.investment(ctx);
    assert!(a > b, "one bought tier ({a}) must outweigh a maxed rookie ({b})");

    // ...and the case the first version of this test MISSED: a rookie
    // whose XP is stored at a high tier's scale. Stored magnitudes are
    // multiplied by M^(tier-1), so a tier-3 specialist banks 10,000x the
    // deci — summing storage instead of effective XP let one track dwarf
    // every tier a Backup Core could carry, and the scrap valve ate the
    // reprint regardless (M16 max review).
    let specialist = spawn(&mut sim, TilePos::new(3, 1), "wait(600)\n");
    {
        let cap = sim.xp.track_cap_deci();
        let data = &mut sim.world.bots.get_mut(&specialist).unwrap().data;
        data.tiers[sim::world::Capability::Mining.idx()] = 3;
        // Maxed WITHIN tier 3, i.e. cap * 100^2 in STORED deci — which is
        // still just one maxed track of real proficiency.
        data.xp.insert(
            XpTrack::Mining,
            sim::world::StoredXp::from_scaled(cap.saturating_mul(10_000)),
        );
    }
    // A real Backup-Core reprint: every capability at max tier, zero XP.
    {
        let data = &mut sim.world.bots.get_mut(&reprint).unwrap().data;
        for c in sim::world::Capability::ALL {
            data.tiers[c.idx()] = 3;
        }
        data.xp.clear();
    }
    let ctx = sim.ctx();
    let reprint_v = sim.world.bots[&reprint].data.investment(ctx);
    let specialist_v = sim.world.bots[&specialist].data.investment(ctx);
    assert!(
        reprint_v > specialist_v,
        "a fully-tiered reprint ({reprint_v}) must outrank a single-track \
         specialist ({specialist_v}) — summing STORED magnitudes let one \
         tier-3 track dwarf every tier a Backup Core can carry"
    );
}

/// The tier reset is arithmetic, so it lives or dies by `track_scale`.
/// `tier_xp_scale_pct / 100` in integer arithmetic truncated every dial
/// below 200 to 1, silently turning the reset into a no-op — a maxed
/// scout would keep its L5 sensors AND collect the new tier's flat grant.
#[test]
fn the_tier_reset_holds_at_every_tier() {
    let mut sim = Sim::new(&MapSpec::empty(6, 6));
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(600)\n");
    let cap = sim.xp.track_cap_deci();
    let scale_pct = sim.stats.tier_xp_scale_pct;
    assert!(scale_pct > 100, "the dial itself must be a real multiplier");

    // At EVERY tier the catalog sells, a bot maxed within the tier below
    // must land under the new L1 — that is what "the reset is arithmetic"
    // means, and it has to hold past the 1 -> 2 step.
    let top = sim.stats.tier_cap(sim::world::Capability::Mining);
    assert!(top >= 2, "the catalog must sell at least one Mining tier");
    for tier in 2..=top {
        let scale_below = (0..tier - 2).fold(1u64, |a, _| a * scale_pct / 100);
        let maxed_below = cap.saturating_mul(scale_below);
        {
            let data = &mut sim.world.bots.get_mut(&bot).unwrap().data;
            data.tiers[sim::world::Capability::Mining.idx()] = tier;
            data.xp.insert(XpTrack::Mining, sim::world::StoredXp::from_scaled(maxed_below));
        }
        let data = &sim.world.bots[&bot].data;
        assert_eq!(
            sim.ctx().capability_level(data, sim::world::Capability::Mining),
            0,
            "buying tier {tier} must drop a bot maxed at tier {} to level 0",
            tier - 1
        );
    }
}

