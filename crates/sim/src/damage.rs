//! Damage, signals, and destruction.

use crate::host::BotHost;
use crate::sim::Sim;
use crate::world::{
    BlackBox, BotId,
};
use pyrite::{RaiseOutcome, Signal, Vm};

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
                // Queued even when hp was already 0: a hit landing during
                // the death template re-raises, and the double-handle rule
                // decides (the per-bot dispatch dedups within a tick).
                self.world.pending_signals.push((id, Signal::Death));
                continue;
            }
            // Edge-triggered at the Damaged line; the latch re-arms when
            // regen climbs back over the SAME tuning value (phase 8).
            if !bot.data.hurt_fired && hp * 100 < max_hp * self.tuning.hurt_line_pct {
                bot.data.hurt_fired = true;
                self.world.pending_signals.push((id, Signal::Hurt));
            }
        }
    }

    /// Phase 6b: dispatch queued signals at the op boundary — one template
    /// entry per bot per tick. Co-arrivals resolve by severity (docs/01:
    /// abort > error > recall > hurt > bumped > bump); the highest enters
    /// its template, the extras are dropped. Co-arrival is NOT a
    /// double-handle (Q81) — that rule needs a template already running,
    /// which `raise` checks against the VM's own phase.
    ///
    /// Dropping is a TEMPLATE rule, not a physics rule: a dropped
    /// bump/bumped still leaves its collision stun (docs/02 — the crunch
    /// happened whether or not the program got to react), and the at-fault
    /// freeze never downgrades, so a bot that rams and is rammed in one
    /// tick keeps the longer stun even though only one template runs.
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
            let outcome = self.raise_signal(id, winner);
            let mut freeze = 0;
            // Winner unhandled with no engine default installed: the raw
            // collision fallback is the asymmetric bump freeze (docs/02).
            if outcome == RaiseOutcome::Ignored {
                freeze = freeze.max(self.default_freeze(winner));
            }
            // Dropped extras of a different kind keep their physical stun.
            for signal in signals {
                if signal != winner {
                    freeze = freeze.max(self.default_freeze(signal));
                }
            }
            if freeze > 0
                && let Some(bot) = self.world.bots.get_mut(&id)
            {
                bot.data.bump_frozen = bot.data.bump_frozen.max(freeze);
            }
        }
    }

    /// The engine's unhandled-default stun for a signal (docs/02: bumps
    /// freeze asymmetrically — long for the rammer, short for the victim;
    /// other signals carry no stun).
    fn default_freeze(&self, signal: Signal) -> u32 {
        match signal {
            Signal::Bump => self.tuning.bump_freeze_ticks,
            Signal::Bumped => self.tuning.bump_victim_freeze_ticks,
            _ => 0,
        }
    }

    pub(crate) fn raise_signal(&mut self, id: BotId, signal: Signal) -> RaiseOutcome {
        let Some(bot) = self.world.bots.get_mut(&id) else { return RaiseOutcome::Ignored };
        let mut vm = bot.vm.take().expect("vm present between phases");
        let outcome = {
            let mut host = BotHost { world: &mut self.world, bot: id, tuning_handler_init_ticks: self.tuning.handler_init_ticks };
            vm.raise(signal, &mut host, &self.costs)
        };
        match outcome {
            RaiseOutcome::Handled => {
                // Entering a handler abandons any in-flight action (the
                // pending world action is cancelled).
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.data.action = None;
                    bot.data.requested = None;
                    bot.vm = Some(vm);
                }
            }
            RaiseOutcome::Ignored | RaiseOutcome::Died => {
                if let Some(bot) = self.world.bots.get_mut(&id) {
                    bot.vm = Some(vm);
                }
            }
            RaiseOutcome::Exploded => self.explode(id, &vm),
        }
        outcome
    }

    /// Double handle: instant destruction — no wreck, but every destruction
    /// drops a Black Box (docs/02-agents.md).
    pub(crate) fn explode(&mut self, id: BotId, vm: &Vm) {
        if let Some(bot) = self.world.bots.remove(&id) {
            self.world.unindex_bot(id, bot.data.pos);
            self.world.bot_entities.remove(&bot.data.entity);
            self.drop_cargo_to_ground(bot.data.pos, bot.data.cargo);
            self.world.black_boxes.push(BlackBox {
                tick: self.world.tick,
                bot: id,
                pos: bot.data.pos,
                cause: vm.last_fault().unwrap_or("double handle").to_string(),
                logs: bot.data.log_buf,
            });
        }
    }
}
