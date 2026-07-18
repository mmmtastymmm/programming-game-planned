//! Procedural map generation (M14, docs/05 Map Generation, Q71).
//!
//! The generator is a setup-time producer, so these are ordinary unit tests
//! (no tick loop, no state hash). They pin the two properties the design
//! leans on — **seed-reproducibility** and the **playability floor** — plus
//! scaling, the `MapSpec` authoring validator, and that a generated spec
//! actually builds and steps a `Sim`.

use sim::map::{MapSpec, MapSpecError, PrinterSpec, TileKind, TilePos};
use sim::mapgen::{self, MapgenConfig};
use sim::resources::Resource;
use sim::sim::Sim;

// ------------------------------------------------------------ determinism

#[test]
fn same_seed_reproduces_byte_identical_spec() {
    let cfg = MapgenConfig::default();
    for players in 1..=6u32 {
        for seed in [0u64, 1, 7, 42, 1000, 0xDEAD_BEEF] {
            let a = mapgen::generate(&cfg, seed, players);
            let b = mapgen::generate(&cfg, seed, players);
            assert_eq!(a, b, "seed {seed}, {players}p must reproduce exactly");
        }
    }
}

#[test]
fn distinct_seeds_produce_distinct_maps() {
    let cfg = MapgenConfig::default();
    let base = mapgen::generate(&cfg, 100, 2);
    let mut differ = 0;
    for seed in 101..140u64 {
        if mapgen::generate(&cfg, seed, 2) != base {
            differ += 1;
        }
    }
    // Value-noise fill makes near-collisions vanishingly unlikely; require
    // the overwhelming majority to differ (all, in practice).
    assert!(differ >= 38, "expected distinct maps across seeds, only {differ}/39 differed");
}

#[test]
fn seed_varies_the_strategic_layout_not_just_the_fill() {
    // The seed must move the *skeleton* (start positions, veins, nests) —
    // not only the decorative rubble/mud/snow. Guards against the whole
    // strategic layout being derived from geometry alone.
    let cfg = MapgenConfig::default();
    // The strategic footprint = printer tiles + resource veins + nests.
    let strategic = |seed: u64| {
        let s = mapgen::generate(&cfg, seed, 3);
        let mut printers: Vec<_> = s.printers.iter().map(|p| (p.pos, p.faction, p.color)).collect();
        printers.sort();
        (printers, s.resource_tiles.clone(), s.nests.clone())
    };
    let a = strategic(1);
    let b = strategic(2);
    assert_ne!(a.0, b.0, "printer/start positions should vary by seed");
    assert_ne!(a.1, b.1, "resource-vein layout should vary by seed");
}

#[test]
fn huge_player_count_clamps_instead_of_panicking() {
    // A pathological MAPGEN_PLAYERS must degrade to a full map, never abort.
    let cfg = MapgenConfig::default();
    let cap = mapgen::max_supported_players(&cfg);
    let spec = mapgen::generate(&cfg, 3, 100_000);
    spec.validate().expect("clamped map is structurally valid");
    mapgen::playability_floor(&spec, &cfg, cap).expect("clamped map holds the floor");
    let factions = spec.printers.iter().map(|p| p.faction).max().map(|m| m as u32 + 1).unwrap_or(0);
    assert_eq!(factions, cap, "player count clamps to the ring capacity");
    assert!(cap >= 8, "the default config should seat a reasonable roster (got {cap})");
}

// ------------------------------------------------------------ the floor

#[test]
fn floor_holds_across_many_seeds_and_player_counts() {
    let cfg = MapgenConfig::default();
    for players in 1..=8u32 {
        for seed in 0..24u64 {
            let spec = mapgen::generate(&cfg, seed, players);
            // generate() asserts these internally; re-affirm at the seam so
            // a regression here fails loudly with the offending seed.
            spec.validate().unwrap_or_else(|e| {
                panic!("seed {seed}, {players}p failed authoring validate: {e:?}")
            });
            mapgen::playability_floor(&spec, &cfg, players).unwrap_or_else(|e| {
                panic!("seed {seed}, {players}p failed playability floor: {e:?}")
            });
        }
    }
}

#[test]
fn every_start_gets_an_ore_vein_in_sight() {
    let cfg = MapgenConfig::default();
    let spec = mapgen::generate(&cfg, 2024, 3);
    let sim = Sim::new(&spec);
    let sight = cfg.start_vein_sight as u32;

    for p in spec.printers.iter().filter(|p| p.color == 0) {
        let has = sim.world.nodes.values().any(|n| {
            n.amount > 0 && n.kind.is_ore_family() && p.pos.chebyshev(n.pos) <= sight
        });
        assert!(has, "faction {} start lacks an ore-family node in sight", p.faction);
    }
}

#[test]
fn generated_map_builds_and_steps() {
    let cfg = MapgenConfig::default();
    let spec = mapgen::generate(&cfg, 55, 2);
    let mut sim = Sim::new(&spec);
    // A generated map must survive the tick loop untouched (no deploy, no
    // input) — the point is that world-build accepts the spec.
    for _ in 0..40 {
        sim.step();
    }
    // Sanity: both factions have their remainder printer.
    for faction in 0..2u8 {
        assert!(
            sim.world.printers.values().any(|pr| pr.faction == faction),
            "faction {faction} should own printers"
        );
    }
}

// ------------------------------------------------------------ scaling

#[test]
fn map_size_scales_with_player_count() {
    let cfg = MapgenConfig::default();
    let one = mapgen::generate(&cfg, 9, 1).width;
    let four = mapgen::generate(&cfg, 9, 4).width;
    let eight = mapgen::generate(&cfg, 9, 8).width;
    assert!(four > one, "4p map ({four}) should be larger than 1p ({one})");
    assert!(eight >= four, "8p map ({eight}) should be at least as large as 4p ({four})");
    assert!(eight <= cfg.max_size, "map size must respect max_size");
}

#[test]
fn player_count_zero_is_clamped_to_one() {
    let cfg = MapgenConfig::default();
    let zero = mapgen::generate(&cfg, 3, 0);
    let one = mapgen::generate(&cfg, 3, 1);
    assert_eq!(zero, one, "0 players clamps to 1");
    assert!(!zero.printers.is_empty(), "a 1-player map has a start");
}

// ------------------------------------------------------------ floor rejects

#[test]
fn floor_rejects_a_sealed_start() {
    let cfg = MapgenConfig::default();
    let mut spec = mapgen::generate(&cfg, 7, 1);
    // Wall the remainder printer in on all four sides with Water — its kit
    // is now unreachable, so the floor must reject the layout.
    let printer = spec.printers.iter().find(|p| p.color == 0).unwrap().pos;
    for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
        spec.water.push(TilePos::new(printer.x + dx, printer.y + dy));
    }
    assert!(
        mapgen::playability_floor(&spec, &cfg, 1).is_err(),
        "a start sealed behind water must fail the floor"
    );
}

// ------------------------------------------------- MapSpec authoring validator

#[test]
fn validate_accepts_a_clean_generated_spec() {
    let cfg = MapgenConfig::default();
    let spec = mapgen::generate(&cfg, 12345, 4);
    assert_eq!(spec.validate(), Ok(()));
}

#[test]
fn validate_rejects_out_of_bounds() {
    let mut spec = MapSpec::empty(10, 10);
    spec.printers.push(PrinterSpec { pos: TilePos::new(50, 3), faction: 0, color: 0, ruined: false });
    assert!(matches!(spec.validate(), Err(MapSpecError::OutOfBounds { what: "printers", .. })));
}

#[test]
fn validate_rejects_printer_in_water() {
    let mut spec = MapSpec::empty(10, 10);
    spec.water.push(TilePos::new(3, 3));
    spec.printers.push(PrinterSpec { pos: TilePos::new(3, 3), faction: 0, color: 0, ruined: false });
    assert!(matches!(
        spec.validate(),
        Err(MapSpecError::NotSpawnable { what: "printer", kind: TileKind::Water, .. })
    ));
}

#[test]
fn validate_rejects_duplicate_printer_tile() {
    let mut spec = MapSpec::empty(10, 10);
    for color in [0u8, 1] {
        spec.printers.push(PrinterSpec { pos: TilePos::new(4, 4), faction: 0, color, ruined: false });
    }
    assert!(matches!(spec.validate(), Err(MapSpecError::DuplicatePrinter { .. })));
}

#[test]
fn validate_rejects_node_on_bare_ground() {
    let mut spec = MapSpec::empty(10, 10);
    // Plains yields no resource — a node here is a bug.
    spec.resource_tiles.push((TilePos::new(2, 2), TileKind::Plains));
    assert!(matches!(spec.validate(), Err(MapSpecError::NodeOnBareGround { .. })));
}

#[test]
fn validate_accepts_a_resource_ground_node() {
    let mut spec = MapSpec::empty(10, 10);
    spec.resource_tiles.push((TilePos::new(2, 2), TileKind::IronVein));
    assert_eq!(spec.validate(), Ok(()));
    assert!(Resource::for_tile(TileKind::IronVein).is_some());
}
