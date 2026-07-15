//! Damage, signals, and destruction.

use crate::host::BotHost;
use crate::sim::Sim;
use crate::world::BotId;
use pyrite::{RaiseOutcome, Signal};

impl Sim {
    /// Test hook: drive the full phase-6 pipeline directly — queue, settle,
    /// dispatch — so a single hit lands with its signals, as in a real tick.
    pub fn apply_damage_for_test(&mut self, id: BotId, amount: i64) {
        self.queue_damage(id, amount);
        self.settle_damage();
        self.dispatch_signals();
    }

    /// Queue a hit for the phase-6 settle (docs/07: damage is a phase, not
    /// an inline side effect of whichever system landed the blow).
    pub(crate) fn queue_damage(&mut self, id: BotId, amount: i64) {
        self.world.pending_damage.push((id, amount));
    }

    /// Phase 6a: drain the damage queue in arrival order (phases queue in
    /// stable id order, so the order is deterministic). Hp changes apply
    /// here; the signals they trigger are queued, not raised — the phase-6
    /// dispatch resolves co-arrivals by severity (Q81).
    pub(crate) fn settle_damage(&mut self) {
        let events = std::mem::take(&mut self.world.pending_damage);
        for (id, amount) in events {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            if bot.data.dying {
                continue; // effectively a wreck already
            }
            bot.data.hp = (bot.data.hp - amount).max(0);
            let hp = bot.data.hp;
            let max_hp = bot.data.max_hp;
            if hp == 0 {
                // HP 0 = abort (docs/01): the highest severity — it wins
                // any co-arrival, and raising it on a mid-template bot is
                // exactly the double-handle outcome anyway.
                self.world.pending_signals.push((id, Signal::Abort));
                continue;
            }
            // Edge-triggered at the bot's own hurt_line env (default: the
            // tuning value); the latch re-arms when regen climbs back over
            // the same line (phase 8).
            let line = crate::world::env_read(
                &bot.data.env,
                "hurt_line",
                self.tuning.hurt_line_pct,
            );
            if !bot.data.hurt_fired && hp * 100 < max_hp * line {
                bot.data.hurt_fired = true;
                self.world.pending_signals.push((id, Signal::Hurt));
            }
        }
    }

    /// Phase 6b: dispatch queued signals at the op boundary — one template
    /// entry per bot per tick. Co-arrivals resolve by severity (docs/01:
    /// abort > error > recall > hurt > bumped > bump); the highest enters
    /// its template and the extras are DROPPED — the winner's forced
    /// prologue flinch is the physical stagger, and bump's long at-fault
    /// stun is its factory window's `wait`, so a dropped extra carries no
    /// residue. Co-arrival is NOT a double-handle (Q81 — that rule needs a
    /// template already running, which `raise` checks against the VM).
    pub(crate) fn dispatch_signals(&mut self) {
        use std::collections::BTreeMap;
        let pending = std::mem::take(&mut self.world.pending_signals);
        let mut per_bot: BTreeMap<BotId, Vec<Signal>> = BTreeMap::new();
        for (id, signal) in pending {
            per_bot.entry(id).or_default().push(signal);
        }
        for (id, signals) in per_bot {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if bot.data.dying {
                continue;
            }
            // Ties can only be duplicates (severity is injective per kind),
            // so max_by_key's last-wins tie-break picks an equal value.
            let winner =
                *signals.iter().max_by_key(|s| s.severity()).expect("group is non-empty");
            self.raise_signal(id, winner);
        }
    }

    pub(crate) fn raise_signal(&mut self, id: BotId, signal: Signal) -> RaiseOutcome {
        let Some(bot) = self.world.bots.get_mut(&id) else { return RaiseOutcome::Ignored };
        let mut vm = bot.vm.take().expect("vm present between phases");
        let outcome = {
            let mut host = BotHost { world: &mut self.world, bot: id, tuning: &self.tuning };
            vm.raise(signal, &mut host, &self.costs)
        };
        match outcome {
            RaiseOutcome::Handled => {
                // Entering a template abandons any in-flight action (the
                // pending world action is cancelled).
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.data.action = None;
                    bot.data.requested = None;
                    bot.vm = Some(vm);
                }
            }
            RaiseOutcome::Ignored => {
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.vm = Some(vm);
                }
            }
            RaiseOutcome::Aborted => {
                // The forced sequence already ran (upload_log went to the
                // archive; become_disabled set `dying` and unindexed). The
                // wreck lands in this tick's death sweep. No instant
                // destruction exists any more (docs/01).
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.data.action = None;
                    bot.data.requested = None;
                    bot.vm = Some(vm);
                }
            }
        }
        outcome
    }
}
