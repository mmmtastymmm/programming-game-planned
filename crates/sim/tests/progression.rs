//! Template Caches & per-match function-block progression (M15, docs/06).
//!
//! Constructs (syntax) are covered elsewhere; these pin the OTHER axis —
//! function blocks LEARNED at Template Caches with `study()`, gating which
//! builtins a colony may call this match.

use sim::map::MapSpec;
use sim::progression::{block_of, FunctionBlock};
use sim::sim::{Command, Sim};
use sim::world::{studied_blocks, Color};
use sim::TilePos;

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 4,
        cargo_cap: 2,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .expect("spawn parses")
    .expect("spawn returns id")
}

// ---------------------------------------------------------- the block table

#[test]
fn every_block_builtin_is_a_real_registry_entry() {
    // A typo in a FunctionBlock's builtin list would gate a name no program
    // can call — silently un-gating the real verb. Cross-check every name
    // against builtins.ron.
    let costs = pyrite::CostTable::default();
    for block in FunctionBlock::ALL {
        for name in block.builtins() {
            assert!(
                costs.spec(name).is_some(),
                "{} lists {name}, which is not a real builtin",
                block.display_name()
            );
        }
    }
}

#[test]
fn block_of_gates_verbs_and_ignores_the_start_kit() {
    assert_eq!(block_of("attack"), Some(FunctionBlock::Attack));
    assert_eq!(block_of("scan_enemies"), Some(FunctionBlock::Scan));
    // Start-kit verbs are never gated.
    assert_eq!(block_of("move_to"), None);
    assert_eq!(block_of("study"), None);
    assert_eq!(block_of("closest"), None);
    // Every block builtin maps back to exactly its own block.
    for block in FunctionBlock::ALL {
        for name in block.builtins() {
            assert_eq!(block_of(name), Some(block), "{name} should map to {block:?}");
        }
    }
}

// ---------------------------------------------------------- deploy gating

#[test]
fn deploy_rejects_a_call_to_an_unstudied_function() {
    let mut spec = MapSpec::empty(6, 4);
    spec.dev_all_unlocks = false; // real-match gating on
    let mut sim = Sim::new(&spec);

    // attack() is behind F_ATK; the colony hasn't studied it.
    let err = sim
        .apply(&Command::DeployProgram {
            faction: 0,
            color: Color::GREEN,
            source: "attack(closest(enemy).expect())\n".into(),
        })
        .expect_err("attack() must be gated before its Cache is studied");
    assert!(
        matches!(
            err.kind,
            pyrite::PyriteErrorKind::LockedFunction { ref func, .. } if func == "attack"
        ),
        "expected a LockedFunction(attack) error, got {err:?}"
    );

    // Study F_ATK (directly), and the same deploy now goes through.
    sim.world.studied.entry(0).or_default().insert(FunctionBlock::Attack);
    assert!(
        sim.apply(&Command::DeployProgram {
            faction: 0,
            color: Color::GREEN,
            source: "attack(closest(enemy).expect())\n".into(),
        })
        .is_ok(),
        "studying F_ATK unlocks attack() colony-wide"
    );
}

#[test]
fn start_kit_deploys_without_any_study() {
    // A straight-line program using only start-kit verbs deploys on a real
    // (non-dev) map with nothing studied — the opening is never gated.
    let mut spec = MapSpec::empty(6, 4);
    spec.dev_all_unlocks = false;
    let mut sim = Sim::new(&spec);
    assert!(
        sim.apply(&Command::DeployProgram {
            faction: 0,
            color: Color::GREEN,
            source: "move_to(closest(ore).expect())\nmine()\ndeposit()\n".into(),
        })
        .is_ok(),
        "the start kit (move_to/closest/mine/deposit) needs no Cache"
    );
}

#[test]
fn dev_maps_bypass_function_gating() {
    // The dev sandbox (default) acts as if every Cache is studied, so existing
    // maps and tests deploy any verb freely.
    let mut sim = Sim::new(&MapSpec::empty(6, 4)); // dev_all_unlocks = true
    assert!(studied_blocks(&sim.world, 0).contains(&FunctionBlock::Hijack));
    assert!(
        sim.apply(&Command::DeployProgram {
            faction: 0,
            color: Color::GREEN,
            source: "attack(closest(enemy).expect())\n".into(),
        })
        .is_ok(),
        "dev sandboxes gate nothing"
    );
}

// ---------------------------------------------------------- study() end-to-end

#[test]
fn studying_a_cache_unlocks_its_block_colony_wide() {
    let mut spec = MapSpec::empty(8, 4);
    spec.dev_all_unlocks = false; // gating on, but power stays free (empty())
    spec.caches.push((TilePos::new(5, 1), FunctionBlock::Attack));
    let mut sim = Sim::new(&spec);

    // A bot walks to the (in-sight) Cache and studies it — all start-kit verbs.
    let bot = spawn(
        &mut sim,
        TilePos::new(1, 1),
        "move_to(closest(cache).expect())\nstudy()\nwait(100000)\n",
    );
    assert!(!studied_blocks(&sim.world, 0).contains(&FunctionBlock::Attack), "not studied yet");

    let mut reached_cache = false;
    for _ in 0..600 {
        sim.step();
        reached_cache |= sim.world.bots.get(&bot).is_some_and(|b| b.data.pos.x >= 4);
        if studied_blocks(&sim.world, 0).contains(&FunctionBlock::Attack) {
            break;
        }
    }
    assert!(reached_cache, "the bot must actually reach the Cache");
    assert!(
        studied_blocks(&sim.world, 0).contains(&FunctionBlock::Attack),
        "study() unlocks the Cache's block for the colony"
    );
    // Non-consumable: the Cache is still there for the next student.
    assert_eq!(sim.world.caches.len(), 1, "studying does not consume the Cache");
}

#[test]
fn study_faults_with_no_cache_in_range() {
    // study() with nothing adjacent faults gracefully (start_study), not a
    // crash — the verb needs a school to sit at.
    let mut sim = Sim::new(&MapSpec::empty(6, 4));
    let bot = spawn(&mut sim, TilePos::new(1, 1), "study()\nwait(5)\n");
    for _ in 0..30 {
        sim.step();
    }
    // The bot survives (fault → handler/normal flow), never a panic.
    assert!(sim.world.bots.contains_key(&bot) || sim.world.wrecks.contains_key(&bot));
}

// ---------------------------------------------------------- mapgen placement

#[test]
fn generated_maps_ring_starts_with_caches() {
    use sim::mapgen::{self, MapgenConfig};
    let cfg = MapgenConfig::default();
    let spec = mapgen::generate(&cfg, 7, 2);
    assert!(!spec.caches.is_empty(), "a generated map places Template Caches");
    // Every generated map is structurally valid with caches present.
    spec.validate().expect("generated caches are on valid ground");
    // Each start has a shallow Sense Cache within reach of its printer.
    for p in spec.printers.iter().filter(|p| p.color == 0) {
        let has_shallow = spec
            .caches
            .iter()
            .any(|(pos, block)| *block == FunctionBlock::Sense && p.pos.chebyshev(*pos) <= 6);
        assert!(has_shallow, "faction {} start lacks a nearby sensor Cache", p.faction);
    }
    // The deep blocks exist somewhere (the shared, contested core Caches).
    assert!(
        spec.caches.iter().any(|(_, b)| *b == FunctionBlock::Hijack),
        "the deep F_HIJACK Cache is placed"
    );
}
