//! Channels (M11, docs/01): rendezvous message passing — no queues, no
//! mailboxes, a message exists only at the instant of delivery. Blocking
//! `send`/`receive`/`broadcast` park the VM (its budget burns — waiting
//! is what its CPU is doing); the settle pass pairs them each tick:
//! longest-blocked receiver first, ties by lowest entity id. Timeouts
//! fault `err_timeout`. Corruption jams bot-to-bot radio both ways
//! (blocked participants inside never wake — cloud telemetry is exempt
//! because it never was a channel). Per-faction namespaces: addressing a
//! FOREIGN faction's channel needs its comm key (`analyze()` steals one;
//! ally grants land with M13).

use crate::sim::Sim;
use crate::world::{Action, BotId, ChannelOp};
use pyrite::{faults, Fault, Value};
use std::collections::BTreeMap;

impl Sim {
    fn jammed(&self, pos: crate::map::TilePos) -> bool {
        self.world.grid.is_corruption(pos)
    }

    /// Phase 4b: the rendezvous settle. Runs after every bot has resolved
    /// its actions, so this tick's fresh blocks participate immediately.
    pub(crate) fn settle_channels(&mut self) {
        // 1. Parked deliveries (try_send handoffs from phase 2) resolve.
        let ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            if let Some(Action::Channel { op, delivered: Some(_), .. }) = &bot.data.action {
                let result = match op {
                    ChannelOp::Receive => {
                        let Some(Action::Channel { delivered, .. }) = &mut bot.data.action
                        else {
                            unreachable!()
                        };
                        Ok(delivered.take().expect("checked above"))
                    }
                    _ => Ok(Value::Unit),
                };
                self.finish_action(id, result);
            }
        }

        // 2. Rendezvous matching per (namespace, channel), jam-filtered.
        // Participants: (waited DESC, entity ASC, bot) per side.
        type Key = (u8, String);
        let mut receivers: BTreeMap<Key, Vec<(u32, u64, BotId)>> = BTreeMap::new();
        let mut senders: BTreeMap<Key, Vec<(u32, u64, BotId)>> = BTreeMap::new();
        let mut broadcasters: BTreeMap<Key, Vec<(u32, u64, BotId)>> = BTreeMap::new();
        for (id, bot) in &self.world.bots {
            if bot.data.dying || self.jammed(bot.data.pos) {
                continue;
            }
            if let Some(Action::Channel { op, ch, namespace, waited, delivered: None, .. }) =
                &bot.data.action
            {
                let key = (*namespace, ch.clone());
                let entry = (*waited, bot.data.entity.0, *id);
                match op {
                    ChannelOp::Receive => receivers.entry(key).or_default().push(entry),
                    ChannelOp::Send(_) => senders.entry(key).or_default().push(entry),
                    ChannelOp::Broadcast(_) => broadcasters.entry(key).or_default().push(entry),
                }
            }
        }
        let order = |v: &mut Vec<(u32, u64, BotId)>| {
            // Longest-blocked first; ties by LOWEST entity id.
            v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        };
        let keys: Vec<Key> = receivers
            .keys()
            .chain(senders.keys())
            .chain(broadcasters.keys())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        for key in keys {
            let mut rx = receivers.remove(&key).unwrap_or_default();
            let mut tx = senders.remove(&key).unwrap_or_default();
            let mut bx = broadcasters.remove(&key).unwrap_or_default();
            order(&mut rx);
            order(&mut tx);
            order(&mut bx);
            let mut rx = rx.into_iter();
            // One-receiver handoffs first (docs/01's table order).
            for (_, _, sender) in tx {
                let Some((_, _, receiver)) = rx.next() else { break };
                let value = match &self.world.bots[&sender].data.action {
                    Some(Action::Channel { op: ChannelOp::Send(v), .. }) => v.clone(),
                    _ => continue, // resolved earlier this pass — skip
                };
                self.finish_action(receiver, Ok(value));
                self.finish_action(sender, Ok(Value::Unit));
            }
            // ONE broadcast then consumes every remaining receiver; later
            // broadcasters keep waiting for the next audience.
            let remaining: Vec<BotId> = rx.map(|(_, _, id)| id).collect();
            if !remaining.is_empty()
                && let Some((_, _, caster)) = bx.first().copied()
            {
                let value = match &self.world.bots[&caster].data.action {
                    Some(Action::Channel { op: ChannelOp::Broadcast(v), .. }) => v.clone(),
                    _ => continue,
                };
                for receiver in remaining {
                    self.finish_action(receiver, Ok(value.clone()));
                }
                self.finish_action(caster, Ok(Value::Unit));
            }
        }

        // 3. Everyone still blocked waits on — and timeouts fault
        // err_timeout (the lease mechanism: a gatekeeper's expired
        // receive is an ordinary fault it can trap).
        let ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in ids {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            let timed_out = match &mut bot.data.action {
                Some(Action::Channel { waited, timeout, delivered: None, .. }) => {
                    *waited += 1;
                    timeout.is_some_and(|t| *waited >= t)
                }
                _ => false,
            };
            if timed_out {
                self.finish_action_fault(
                    id,
                    Fault::new(faults::TIMEOUT, "channel timeout: nobody answered"),
                );
            }
        }
    }
}

