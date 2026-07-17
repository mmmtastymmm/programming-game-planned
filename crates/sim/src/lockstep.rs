//! Deterministic-lockstep relay scaffold (M13, docs/08).
//!
//! The sim API was shaped for this from M0: ordered [`Command`]s in, a
//! phase-9 snapshot hash out. This module adds the peer protocol around
//! it — command scheduling at `T + delay`, the all-peers barrier, and
//! per-tick hash comparison — over a [`Transport`] trait so the actual
//! wire (in-process channels today; QUIC/whatever later, in the game
//! crate) stays out of the deterministic core. REAL networking is
//! deliberately absent (flagged in TASKS.md); [`LocalHub`] is the
//! reference transport for tests and single-machine play.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::sim::{Command, Sim};

/// One lockstep wire message. Every peer must send a Commands message for
/// EVERY scheduled tick — an empty list is an explicit "nothing from me",
/// and the barrier waits for it (docs/08's agreed command set).
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Commands { peer: u8, tick: u64, commands: Vec<Command> },
    Hash { peer: u8, tick: u64, hash: u64 },
}

/// The wire. Implementations move messages between peers; they may delay
/// or batch but must never drop, reorder within a peer, or duplicate.
pub trait Transport {
    fn send(&mut self, msg: Message);
    fn recv(&mut self) -> Vec<Message>;
}

/// A detected divergence (docs/08: pause, dump, resync — the CALLER's
/// policy; the scaffold only detects and reports).
#[derive(Debug, Clone, PartialEq)]
pub struct Desync {
    pub tick: u64,
    /// (peer, hash) for every participant, self included.
    pub hashes: Vec<(u8, u64)>,
}

/// One lockstep participant wrapping a [`Sim`]. Drive it with
/// [`LockstepPeer::submit`] (local player input) and
/// [`LockstepPeer::try_step`] (the barrier + step); both peers advance
/// identically or `desync` reports where they split.
pub struct LockstepPeer {
    pub id: u8,
    pub sim: Sim,
    /// Input delay D in ticks (docs/08: ~3 at 10 tps — invisible for
    /// "deploy code" inputs).
    pub delay: u64,
    /// Every participant id, self included (fixed at match start).
    pub roster: Vec<u8>,
    /// tick → peer → that peer's agreed command list.
    inbox: BTreeMap<u64, BTreeMap<u8, Vec<Command>>>,
    /// tick → peer → phase-9 hash, compared once complete.
    hashes: BTreeMap<u64, BTreeMap<u8, u64>>,
    /// The tick the NEXT submit will schedule for. Advances with every
    /// submit — NOT with the sim — so a stalled barrier (a lagging remote
    /// peer) never makes two frames' submissions collide on one tick key
    /// and silently drop the earlier frame's commands.
    next_submit: u64,
    /// First divergence seen, if any. Latched — the caller decides policy.
    pub desync: Option<Desync>,
}

impl LockstepPeer {
    pub fn new(id: u8, sim: Sim, delay: u64, roster: Vec<u8>) -> Self {
        // The first `delay` ticks predate any submittable input — every
        // peer agrees they're empty (the standard lockstep warm-up), so
        // pre-seed them rather than deadlock the barrier at tick 1.
        let mut inbox: BTreeMap<u64, BTreeMap<u8, Vec<Command>>> = BTreeMap::new();
        let first = sim.world.tick + 1;
        for tick in first..first + delay {
            let entry = inbox.entry(tick).or_default();
            for &peer in &roster {
                entry.insert(peer, Vec::new());
            }
        }
        let next_submit = first + delay;
        Self { id, sim, delay, roster, inbox, hashes: BTreeMap::new(), next_submit, desync: None }
    }

    /// The tick the NEXT `try_step` will simulate.
    pub fn next_tick(&self) -> u64 {
        self.sim.world.tick + 1
    }

    /// Publish this frame's local commands. Call EVERY frame, empty or
    /// not. Coverage is gap-free and bounded (review 2026-07-16):
    ///
    /// - Every tick from `next_submit` through the input horizon
    ///   (`next_tick() + delay`) gets a Commands message — a catch-up
    ///   burst (more steps than frames) back-fills each owed tick, so
    ///   the barrier never waits on a tick key nobody will ever send.
    /// - When frames outpace ticks the horizon doesn't move, so empty
    ///   frames send NOTHING (no unbounded scheduling drift); only a
    ///   command-bearing frame extends coverage by one extra tick.
    pub fn submit(&mut self, commands: Vec<Command>, transport: &mut dyn Transport) {
        let horizon = self.next_tick() + self.delay;
        let mut commands = Some(commands);
        while self.next_submit <= horizon {
            let tick = self.next_submit;
            self.next_submit += 1;
            let batch = commands.take().unwrap_or_default();
            self.inbox.entry(tick).or_default().insert(self.id, batch.clone());
            transport.send(Message::Commands { peer: self.id, tick, commands: batch });
        }
        // Inputs are rare and small (docs/08): a command-bearing frame
        // past the horizon claims exactly one extra tick rather than
        // being dropped or overwriting a sent batch.
        if let Some(batch) = commands {
            if !batch.is_empty() {
                let tick = self.next_submit;
                self.next_submit += 1;
                self.inbox.entry(tick).or_default().insert(self.id, batch.clone());
                transport.send(Message::Commands { peer: self.id, tick, commands: batch });
            }
        }
    }

    /// Drain the transport into the inbox and hash ledger. Messages from
    /// ids outside the roster are DROPPED (review 2026-07-16): applying
    /// them would depend on arrival timing the transport contract leaves
    /// open, so a stray or forged sender could desync honest peers.
    pub fn pump(&mut self, transport: &mut dyn Transport) {
        for msg in transport.recv() {
            match msg {
                Message::Commands { peer, tick, commands } => {
                    if self.roster.contains(&peer) {
                        self.inbox.entry(tick).or_default().insert(peer, commands);
                    }
                }
                Message::Hash { peer, tick, hash } => {
                    if self.roster.contains(&peer) {
                        self.hashes.entry(tick).or_default().insert(peer, hash);
                        self.compare_hashes(tick);
                    }
                }
            }
        }
    }

    /// The barrier: if every roster peer's commands for the next tick are
    /// in, apply them in PEER-ID order (the deterministic total order),
    /// step the sim, and broadcast the snapshot hash. Returns whether a
    /// tick ran.
    pub fn try_step(&mut self, transport: &mut dyn Transport) -> bool {
        let tick = self.next_tick();
        let ready = self
            .inbox
            .get(&tick)
            .is_some_and(|m| self.roster.iter().all(|p| m.contains_key(p)));
        if !ready {
            return false;
        }
        let batch = self.inbox.remove(&tick).expect("checked ready");
        for (_peer, commands) in batch {
            for command in &commands {
                // A rejected command is a DESYNC RISK only if peers
                // disagree; apply() is deterministic, so all peers
                // reject identically. Parse errors just drop the command.
                let _ = self.sim.apply(command);
            }
        }
        self.sim.step();
        let hash = self.sim.last_hash;
        self.hashes.entry(tick).or_default().insert(self.id, hash);
        transport.send(Message::Hash { peer: self.id, tick, hash });
        self.compare_hashes(tick);
        true
    }

    /// Once every roster hash for `tick` is present, compare; mismatch
    /// latches the first desync (docs/08: pause / dump / resync is policy
    /// above this layer). Agreed ticks are pruned.
    fn compare_hashes(&mut self, tick: u64) {
        let Some(m) = self.hashes.get(&tick) else { return };
        if !self.roster.iter().all(|p| m.contains_key(p)) {
            return;
        }
        let m = self.hashes.remove(&tick).expect("checked present");
        let mut values = m.values();
        let first = *values.next().expect("roster is never empty");
        if values.any(|h| *h != first) && self.desync.is_none() {
            self.desync = Some(Desync { tick, hashes: m.into_iter().collect() });
        }
    }
}

/// In-process reference transport: a shared mailbox hub. Every send is
/// visible to every OTHER peer on the next `recv` (loopback excluded —
/// local messages are booked directly by `submit`/`try_step`).
#[derive(Default)]
pub struct LocalHub {
    boxes: Rc<RefCell<Vec<(u8, Vec<Message>)>>>,
}

impl LocalHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// A transport endpoint for peer `id`. Register every peer before play.
    pub fn endpoint(&mut self, id: u8) -> LocalEndpoint {
        self.boxes.borrow_mut().push((id, Vec::new()));
        LocalEndpoint { id, boxes: Rc::clone(&self.boxes) }
    }
}

pub struct LocalEndpoint {
    id: u8,
    boxes: Rc<RefCell<Vec<(u8, Vec<Message>)>>>,
}

impl Transport for LocalEndpoint {
    fn send(&mut self, msg: Message) {
        for (peer, mailbox) in self.boxes.borrow_mut().iter_mut() {
            if *peer != self.id {
                mailbox.push(msg.clone());
            }
        }
    }

    fn recv(&mut self) -> Vec<Message> {
        for (peer, mailbox) in self.boxes.borrow_mut().iter_mut() {
            if *peer == self.id {
                return std::mem::take(mailbox);
            }
        }
        Vec::new()
    }
}
