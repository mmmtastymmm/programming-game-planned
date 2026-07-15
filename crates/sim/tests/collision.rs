//! Bots are solid: one per tile. Bumping an occupied tile freezes the
//! mover (~5s), who then retries (docs/02-agents.md).

use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::Color;
use sim::TilePos;

const IDLER: &str = "log(1)\n";

fn spawn(sim: &mut Sim, pos: TilePos, source: &str) -> sim::BotId {
    sim.apply(&Command::SpawnBot {
        pos,
        source: source.into(),
        cpu: 4,
        cargo_cap: 1,
        faction: 0,
        hp: 100,
        color: Color::GREEN,
    })
    .unwrap()
    .unwrap()
}

#[test]
fn bots_never_overlap_and_bumps_freeze() {
    // One-tile corridor: the mover must pass the idler's tile to reach the
    // depot — impossible. It bumps, freezes, retries, forever.
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.bump_damage = 0; // freeze/replan semantics test
    let blocker = spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(4, 1), "move_to(closest(depot).expect())\n");

    let mut saw_bump_handler = false;
    for _ in 0..300 {
        sim.step();
        let a = sim.world.bots[&blocker].data.pos;
        let b = sim.world.bots[&mover].data.pos;
        assert_ne!(a, b, "two bots must never share a tile");
        if sim.world.bots[&mover].handler_name() == Some("bump") {
            saw_bump_handler = true;
            assert!(
                sim.world.bots[&mover].in_default_handler(),
                "no player handler installed: this is the engine default, as code"
            );
        }
    }
    assert!(saw_bump_handler, "bumping must drop the mover into the default bump handler");
    // The mover parked next to the blocker, still short of the depot.
    assert_eq!(sim.world.bots[&mover].data.pos, TilePos::new(2, 1));
}

#[test]
fn frozen_bots_do_not_think() {
    // Same corridor; the mover's freeze also stops its cycle grants, so a
    // bumped bot makes no VM progress while stunned.
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.bump_damage = 0; // frozen-brain semantics test
    spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(3, 1), "move_to(closest(depot).expect())\nlog(9)\n");

    for _ in 0..40 {
        sim.step();
    }
    let bot = &sim.world.bots[&mover];
    assert_eq!(bot.handler_name(), Some("bump"), "mid default bump handler at tick 40");
    assert!(
        bot.data.log_buf.is_empty(),
        "move_to never completed, so log(9) must not have run"
    );
}

#[test]
fn printed_bots_spread_to_free_tiles() {
    let mut spec = MapSpec::empty(8, 8);
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(4, 4),
        faction: 0,
        color: 0,
        ruined: false,
        desired_max: 4,
    });
    spec.starting_ore = 50;
    let mut sim = Sim::new(&spec);
    sim.apply(&Command::DeployProgram { faction: 0, color: Color::GREEN, source: IDLER.into() })
        .unwrap();
    for _ in 0..120 {
        sim.step();
    }
    assert_eq!(sim.world.bots.len(), 4);
    let mut positions: Vec<TilePos> = sim.world.bots.values().map(|b| b.data.pos).collect();
    positions.sort();
    positions.dedup();
    assert_eq!(positions.len(), 4, "printed idlers must occupy distinct tiles");
}

#[test]
fn collision_world_is_deterministic() {
    let build = || {
        let mut spec = MapSpec::empty(7, 3);
        for x in 0..7 {
            spec.water.push(TilePos::new(x, 0));
            spec.water.push(TilePos::new(x, 2));
        }
        spec.depots.push(TilePos::new(0, 1));
        let mut sim = Sim::new(&spec);
        spawn(&mut sim, TilePos::new(1, 1), IDLER);
        spawn(&mut sim, TilePos::new(4, 1), "move_to(closest(depot).expect())\n");
        spawn(&mut sim, TilePos::new(6, 1), "move_to(closest(depot).expect())\n");
        sim
    };
    let mut a = build();
    let mut b = build();
    for tick in 0..300 {
        a.step();
        b.step();
        assert_eq!(a.state_hash(), b.state_hash(), "desync at tick {tick}");
    }
}

#[test]
fn sidestep_dodges_blocker_without_freezing() {
    // Open ground: when the next tile is occupied the mover takes a free
    // sidestep (no freeze), re-plans, and arrives at the depot.
    let mut spec = MapSpec::empty(7, 3);
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    let blocker = spawn(&mut sim, TilePos::new(2, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(4, 1), "move_to(closest(depot).expect())\n");

    for _ in 0..150 {
        sim.step();
        let a = sim.world.bots[&blocker].data.pos;
        let b = sim.world.bots[&mover].data.pos;
        assert_ne!(a, b, "never overlap");
        assert_eq!(
            sim.world.bots[&mover].data.bump_frozen, 0,
            "open ground must dodge, not freeze"
        );
    }
    let end = sim.world.bots[&mover].data.pos;
    assert!(
        end.chebyshev(TilePos::new(0, 1)) <= 1,
        "the dodge + re-plan must reach the depot; ended at {end:?}"
    );
}

#[test]
fn wait_idles_for_the_requested_ticks() {
    let mut sim = Sim::new(&MapSpec::empty(4, 4));
    let bot = spawn(&mut sim, TilePos::new(1, 1), "wait(10)\nlog(1)\n");
    for _ in 0..8 {
        sim.step();
    }
    assert!(
        sim.world.bots[&bot].data.log_buf.is_empty(),
        "log must not run while waiting"
    );
    for _ in 0..12 {
        sim.step();
    }
    assert!(
        !sim.world.bots[&bot].data.log_buf.is_empty(),
        "log runs once the wait completes"
    );
}

#[test]
fn bumps_hurt_both_parties() {
    // One-tile corridor: mover repeatedly bumps the idler. Both bleed.
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    let blocker = spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let mover = spawn(&mut sim, TilePos::new(3, 1), "move_to(closest(depot).expect())\n");
    for _ in 0..200 {
        sim.step();
    }
    let blocker_hp = sim.world.bots[&blocker].data.hp;
    let mover_hp = sim.world.bots[&mover].data.hp;
    assert!(blocker_hp < 100, "the bumped bot takes damage too ({blocker_hp})");
    assert!(mover_hp < 100, "the bumper takes damage ({mover_hp})");
    assert_eq!(blocker_hp, mover_hp, "collisions are symmetric");
    // The blocker also recoils: it was frozen at some point.
    // (both frozen simultaneously right after each bump)
}

#[test]
fn corridor_deadlock_ends_in_mutual_destruction() {
    // Two movers head-on in a one-tile corridor, nowhere to dodge: they
    // grind each other down and both die — the deadlock self-clears the
    // expensive way (wrecks don't block).
    let mut spec = MapSpec::empty(8, 3);
    for x in 0..8 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    spec.ore_nodes.push((TilePos::new(7, 1), 50));
    let mut sim = Sim::new(&spec);
    sim.tuning.fault_damage = 0; // isolate bump damage
    let east = spawn(&mut sim, TilePos::new(2, 1), "move_to(closest(ore).expect())\nmine()\n");
    let west = spawn(&mut sim, TilePos::new(5, 1), "move_to(closest(depot).expect())\n");
    // Low hp so the grind finishes within the test.
    sim.world.bots.get_mut(&east).unwrap().data.hp = 10;
    sim.world.bots.get_mut(&east).unwrap().data.max_hp = 10;
    sim.world.bots.get_mut(&west).unwrap().data.hp = 10;
    sim.world.bots.get_mut(&west).unwrap().data.max_hp = 10;
    for _ in 0..1500 {
        sim.step();
        if !sim.world.bots.contains_key(&east) && !sim.world.bots.contains_key(&west) {
            break;
        }
    }
    assert!(
        !sim.world.bots.contains_key(&east) && !sim.world.bots.contains_key(&west),
        "head-on deadlock must end in mutual destruction"
    );
}

#[test]
fn rammer_freezes_longer_than_the_rammed() {
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    let blocker = spawn(&mut sim, TilePos::new(1, 1), IDLER);
    let rammer = spawn(&mut sim, TilePos::new(3, 1), "move_to(closest(depot).expect())\n");
    for tick in 0..200 {
        sim.step();
        if sim.world.bots[&rammer].handler_name() == Some("bump") {
            assert_eq!(
                sim.world.bots[&blocker].handler_name(),
                Some("bumped"),
                "victim handles `bumped` at the same impact"
            );
            // Asymmetric blame in code: wait(50) vs wait(15) — the victim
            // finishes long before the rammer.
            for _ in 0..25 {
                sim.step();
            }
            assert_eq!(
                sim.world.bots[&blocker].handler_name(),
                None,
                "victim's short default is done"
            );
            assert_eq!(
                sim.world.bots[&rammer].handler_name(),
                Some("bump"),
                "rammer still serving its long default at tick {tick}+25"
            );
            return;
        }
    }
    panic!("no bump observed");
}

#[test]
fn bump_handlers_replace_the_freeze() {
    // Rammer with `on bump:`: no stun — its handler logs, program restarts.
    // Victim with `on bumped:`: same, no stagger freeze.
    let rammer_src = "\
on bump:
    log(\"ow-my-front\")

move_to(closest(depot).expect())
";
    let victim_src = "\
on bumped:
    log(\"hey-watch-it\")

wait(3)
";
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    let victim = spawn(&mut sim, TilePos::new(1, 1), victim_src);
    let rammer = spawn(&mut sim, TilePos::new(3, 1), rammer_src);
    let mut rammer_handled = false;
    let mut victim_handled = false;
    for _ in 0..200 {
        sim.step();
        let r = &sim.world.bots[&rammer];
        let v = &sim.world.bots[&victim];
        assert_eq!(r.data.bump_frozen, 0, "handled bump must not freeze the rammer");
        assert_eq!(v.data.bump_frozen, 0, "handled bumped must not freeze the victim");
        rammer_handled |= r.data.log_buf.iter().any(|l| l.1.contains("ow-my-front"));
        victim_handled |= v.data.log_buf.iter().any(|l| l.1.contains("hey-watch-it"));
        if rammer_handled && victim_handled {
            return;
        }
    }
    panic!("both bump handlers must run (rammer {rammer_handled}, victim {victim_handled})");
}

#[test]
fn co_arriving_signals_resolve_by_severity_not_double_handle() {
    // A ram whose chip also crosses the hurt line raises bumped + hurt at
    // the same op boundary. Q81: the severest (hurt) enters its template
    // and the extra is dropped — co-arrival must NOT explode the bot as a
    // double-handle, even with a player handler installed (the double-
    // handle needs a template already *running*).
    let victim_src = "\
on hurt:
    log(\"hurt-wins\")

on bumped:
    log(\"bumped-ran\")

wait(600)
";
    let mut spec = MapSpec::empty(7, 3);
    for x in 0..7 {
        spec.water.push(TilePos::new(x, 0));
        spec.water.push(TilePos::new(x, 2));
    }
    spec.depots.push(TilePos::new(0, 1));
    let mut sim = Sim::new(&spec);
    sim.tuning.bump_damage = 60; // one ram crosses the victim's 50% line
    let victim = spawn(&mut sim, TilePos::new(1, 1), victim_src);
    let rammer = spawn(&mut sim, TilePos::new(3, 1), "move_to(closest(depot).expect())\n");
    // The rammer is too tough for its share of the crunch to cross ITS
    // hurt line: it gets a plain bump (default freeze), so the victim's
    // template finishes before any second ram lands.
    sim.world.bots.get_mut(&rammer).unwrap().data.hp = 1000;
    sim.world.bots.get_mut(&rammer).unwrap().data.max_hp = 1000;
    for _ in 0..120 {
        sim.step();
        let Some(v) = sim.world.bots.get(&victim) else {
            panic!("co-arrival is not a double-handle: the victim must survive");
        };
        if let Some(first) = v.data.log_buf.first() {
            assert!(
                first.1.contains("hurt-wins"),
                "the severest co-arriving signal wins the boundary, got {first:?}"
            );
            return;
        }
    }
    panic!("the ram never landed");
}

#[test]
fn bumped_during_a_handler_is_a_double_handle() {
    // The victim is mid-`on bumped:` (blocking wait) when a second bump
    // lands: any signal during a running template forces ABORT — the bot
    // drops into a wreck where it stands (no instant-destroy path, M3).
    let victim_src = "\
on bumped:
    wait(40)

wait(3)
";
    // A plus-shaped intersection: the victim sits at the crossing, and
    // both rammers' only routes to the depot pass through its tile.
    let mut spec = MapSpec::empty(5, 5);
    for x in 0..5 {
        for y in 0..5 {
            if x != 2 && y != 2 {
                spec.water.push(TilePos::new(x, y));
            }
        }
    }
    spec.depots.push(TilePos::new(0, 2));
    let mut sim = Sim::new(&spec);
    sim.tuning.bump_damage = 0; // isolate the signal mechanics
    let victim = spawn(&mut sim, TilePos::new(2, 2), victim_src);
    // Two rammers on different arms: the second bump lands mid-handler.
    spawn(&mut sim, TilePos::new(4, 2), "move_to(closest(depot).expect())\n");
    spawn(&mut sim, TilePos::new(2, 0), "move_to(closest(depot).expect())\n");
    for _ in 0..300 {
        sim.step();
        if !sim.world.bots.contains_key(&victim) {
            break;
        }
    }
    assert!(!sim.world.bots.contains_key(&victim), "second bump mid-handler must abort");
    assert!(
        sim.world.wrecks.contains_key(&victim),
        "double handle = abort = wreck (the rescue race), never vaporization"
    );
}
