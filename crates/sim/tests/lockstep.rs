//! The lockstep relay scaffold (M13, docs/08): two peers exchanging
//! per-tick command sets over a transport, comparing phase-9 hashes,
//! and surfacing desyncs.

use sim::lockstep::{LocalHub, LockstepPeer};
use sim::map::MapSpec;
use sim::sim::{Command, Sim};
use sim::world::Color;
use sim::TilePos;

fn spec() -> MapSpec {
    let mut spec = MapSpec::empty(10, 6);
    spec.quirk_permille = 0;
    spec
}

#[test]
fn peers_stay_in_lockstep_through_command_traffic() {
    let mut hub = LocalHub::new();
    let mut t0 = hub.endpoint(0);
    let mut t1 = hub.endpoint(1);
    let mut p0 = LockstepPeer::new(0, Sim::new(&spec()), 3, vec![0, 1]);
    let mut p1 = LockstepPeer::new(1, Sim::new(&spec()), 3, vec![0, 1]);

    for frame in 0..50u64 {
        // Peer 0 deploys a bot on frame 5; everything else is silence —
        // still submitted every frame (the barrier needs the explicit
        // "nothing from me").
        let cmds = if frame == 5 {
            vec![Command::SpawnBot {
                pos: TilePos::new(2, 2),
                source: "move_to(closest(depot).expect())\nwait(5)\n".into(),
                cpu: 8,
                cargo_cap: 1,
                faction: 0,
                hp: 100,
                color: Color::GREEN,
            }]
        } else {
            Vec::new()
        };
        p0.submit(cmds, &mut t0);
        p1.submit(Vec::new(), &mut t1);
        p0.pump(&mut t0);
        p1.pump(&mut t1);
        let s0 = p0.try_step(&mut t0);
        let s1 = p1.try_step(&mut t1);
        p0.pump(&mut t0);
        p1.pump(&mut t1);
        // With everyone submitting every frame, the barrier never stalls
        // past the input delay.
        if frame >= 3 {
            assert!(s0 && s1, "the barrier opened on frame {frame}");
        }
    }
    assert_eq!(p0.sim.world.tick, p1.sim.world.tick, "peers advanced together");
    assert_eq!(p0.sim.last_hash, p1.sim.last_hash, "identical state on both machines");
    assert_eq!(p0.sim.world.bots.len(), 1, "the command reached peer 0's sim");
    assert_eq!(p1.sim.world.bots.len(), 1, "…and peer 1's, at the same tick");
    assert!(p0.desync.is_none() && p1.desync.is_none(), "no divergence anywhere");
}

#[test]
fn a_diverged_peer_is_caught_by_the_hash_exchange() {
    let mut hub = LocalHub::new();
    let mut t0 = hub.endpoint(0);
    let mut t1 = hub.endpoint(1);
    let mut p0 = LockstepPeer::new(0, Sim::new(&spec()), 2, vec![0, 1]);
    let mut p1 = LockstepPeer::new(1, Sim::new(&spec()), 2, vec![0, 1]);

    for frame in 0..20u64 {
        if frame == 8 {
            // Peer 1's world silently corrupts (a bug, a cheat, cosmic
            // rays) — exactly what the hash exchange exists to catch.
            p1.sim.world.data.insert(0, 999);
        }
        p0.submit(Vec::new(), &mut t0);
        p1.submit(Vec::new(), &mut t1);
        p0.pump(&mut t0);
        p1.pump(&mut t1);
        p0.try_step(&mut t0);
        p1.try_step(&mut t1);
        p0.pump(&mut t0);
        p1.pump(&mut t1);
    }
    let d0 = p0.desync.as_ref().expect("peer 0 saw the divergence");
    let d1 = p1.desync.as_ref().expect("peer 1 saw it too");
    assert_eq!(d0.tick, d1.tick, "both latch the SAME first divergent tick");
    assert_eq!(d0.hashes.len(), 2);
    assert_ne!(d0.hashes[0].1, d0.hashes[1].1);
}

#[test]
fn a_stalled_barrier_never_drops_commands() {
    let mut hub = LocalHub::new();
    let mut t0 = hub.endpoint(0);
    let mut t1 = hub.endpoint(1);
    let mut p0 = LockstepPeer::new(0, Sim::new(&spec()), 1, vec![0, 1]);
    let mut p1 = LockstepPeer::new(1, Sim::new(&spec()), 1, vec![0, 1]);

    // Peer 0 keeps its frame loop running while peer 1 lags silent: the
    // real command from frame 0 must survive the two empty frames that
    // follow it (review 2026-07-16: same-tick overwrite lost it).
    p0.submit(
        vec![Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: "wait(60)\n".into(),
            cpu: 8,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        }],
        &mut t0,
    );
    p0.submit(Vec::new(), &mut t0);
    p0.submit(Vec::new(), &mut t0);
    p0.try_step(&mut t0); // warm-up tick 1 runs; tick 2 stalls on peer 1

    // Peer 1 catches up.
    for _ in 0..3 {
        p1.submit(Vec::new(), &mut t1);
    }
    for _ in 0..6 {
        p0.pump(&mut t0);
        p1.pump(&mut t1);
        p0.try_step(&mut t0);
        p1.try_step(&mut t1);
    }
    assert_eq!(p0.sim.world.bots.len(), 1, "the stalled frame's command survived on peer 0");
    assert_eq!(p1.sim.world.bots.len(), 1, "…and on peer 1");
    assert_eq!(p0.sim.last_hash, p1.sim.last_hash);
    assert!(p0.desync.is_none() && p1.desync.is_none());
}

#[test]
fn the_hash_sees_in_flight_actions_and_wreck_detail() {
    let mk = || {
        let mut sim = Sim::new(&spec());
        sim.apply(&Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: "wait(600)\n".into(),
            cpu: 8,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        })
        .unwrap()
        .unwrap();
        sim.step();
        sim
    };
    let a = mk();
    let mut b = mk();
    assert_eq!(a.state_hash(), b.state_hash(), "identical worlds hash identically");
    // A diverged in-flight action must move the hash THIS tick (review
    // 2026-07-16: invisible Action state made desyncs surface late).
    let id = *b.world.bots.keys().next().unwrap();
    b.world.bots.get_mut(&id).unwrap().data.action =
        Some(sim::world::Action::Wait { ticks_left: 7 });
    assert_ne!(a.state_hash(), b.state_hash(), "action state is hashed");

    // Wreck detail: same upgrade COUNT, different upgrade — must differ.
    let a = {
        let mut sim = mk();
        let id = *sim.world.bots.keys().next().unwrap();
        sim.world.bots.get_mut(&id).unwrap().data.upgrades.push(1);
        sim.apply(&Command::KillBot { bot: id }).unwrap();
        sim.step();
        sim
    };
    let b = {
        let mut sim = mk();
        let id = *sim.world.bots.keys().next().unwrap();
        sim.world.bots.get_mut(&id).unwrap().data.upgrades.push(2);
        sim.apply(&Command::KillBot { bot: id }).unwrap();
        sim.step();
        sim
    };
    assert_ne!(a.state_hash(), b.state_hash(), "wreck upgrade IDENTITY is hashed, not just count");
}

#[test]
fn catch_up_bursts_leave_no_gaps() {
    // Frames and ticks decouple: each round submits once then steps as
    // far as the barrier allows. The old `.max()` claim skipped tick
    // keys after a burst and froze the match (review 2026-07-16).
    let mut hub = LocalHub::new();
    let mut t0 = hub.endpoint(0);
    let mut t1 = hub.endpoint(1);
    let mut p0 = LockstepPeer::new(0, Sim::new(&spec()), 2, vec![0, 1]);
    let mut p1 = LockstepPeer::new(1, Sim::new(&spec()), 2, vec![0, 1]);
    for _ in 0..6 {
        p0.submit(Vec::new(), &mut t0);
        p1.submit(Vec::new(), &mut t1);
        p0.pump(&mut t0);
        p1.pump(&mut t1);
        while p0.try_step(&mut t0) {}
        while p1.try_step(&mut t1) {}
        p0.pump(&mut t0);
        p1.pump(&mut t1);
    }
    assert!(p0.sim.world.tick >= 6, "the barrier keeps opening across bursts");
    assert_eq!(p0.sim.world.tick, p1.sim.world.tick);
    assert_eq!(p0.sim.last_hash, p1.sim.last_hash);
    assert!(p0.desync.is_none() && p1.desync.is_none());
}

#[test]
fn non_roster_messages_are_ignored() {
    let mut hub = LocalHub::new();
    let mut t0 = hub.endpoint(0);
    let mut t1 = hub.endpoint(1);
    let mut forger = hub.endpoint(7); // not in anyone's roster
    let mut p0 = LockstepPeer::new(0, Sim::new(&spec()), 2, vec![0, 1]);
    let mut p1 = LockstepPeer::new(1, Sim::new(&spec()), 2, vec![0, 1]);
    use sim::lockstep::{Message, Transport};
    forger.send(Message::Commands {
        peer: 7,
        tick: 3,
        commands: vec![Command::SpawnBot {
            pos: TilePos::new(2, 2),
            source: "wait(600)\n".into(),
            cpu: 8,
            cargo_cap: 1,
            faction: 0,
            hp: 100,
            color: Color::GREEN,
        }],
    });
    for _ in 0..12 {
        p0.submit(Vec::new(), &mut t0);
        p1.submit(Vec::new(), &mut t1);
        p0.pump(&mut t0);
        p1.pump(&mut t1);
        p0.try_step(&mut t0);
        p1.try_step(&mut t1);
        p0.pump(&mut t0);
        p1.pump(&mut t1);
    }
    assert_eq!(p0.sim.world.bots.len(), 0, "a forged non-roster spawn is dropped");
    assert_eq!(p1.sim.world.bots.len(), 0);
    assert_eq!(p0.sim.last_hash, p1.sim.last_hash);
    assert!(p0.desync.is_none() && p1.desync.is_none());
}
