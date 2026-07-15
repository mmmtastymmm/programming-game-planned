//! Damage, signals, and destruction.

use crate::host::BotHost;
use crate::sim::Sim;
use crate::world::{
    BlackBox, BotId,
};
use pyrite::{RaiseOutcome, Signal, Vm};

impl Sim {
    /// Test hook: drive the damage pipeline directly (signals included).
    pub fn apply_damage_for_test(&mut self, id: BotId, amount: i64) {
        self.apply_damage(id, amount);
    }

    /// Damage pipeline: hp change, then signals per docs/01-language.md.
    /// Hurt is edge-triggered at the program's threshold (default 50%);
    /// death raises `Signal::Death`; a signal landing while a handler (or
    /// boot/recall) is active is a double handle — the bot explodes.
    pub(crate) fn apply_damage(&mut self, id: BotId, amount: i64) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        if bot.data.dying {
            return; // effectively a wreck already
        }
        bot.data.hp = (bot.data.hp - amount).max(0);
        let hp = bot.data.hp;
        let max_hp = bot.data.max_hp;
        if hp == 0 {
            self.raise_signal(id, Signal::Death);
            return;
        }
        // Fixed Damaged threshold (custom thresholds went with the old
        // per-signal handler syntax; can return as an on signal(s, n) param).
        let threshold_pct = 50;
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        if !bot.data.hurt_fired && hp * 100 < max_hp * threshold_pct {
            bot.data.hurt_fired = true;
            self.raise_signal(id, Signal::Hurt);
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
