//! Printer lifecycle: printing, boot sequences, recalls, re-coloring,
//! and scrapping.

use crate::map::{astar_avoiding, TilePos};
use crate::sim::Sim;
use crate::world::{
    ArchiveEntry, ArchiveKind, BotId, Color, EntityId, PrinterState, PrintTarget, Recall,
    RecallPurpose,
};

/// How an allocation change reaches its bot (M9, docs/01 Q85/Q73):
/// player-fired triggers (rule edits, the check interval) dispatch like
/// signals — mid-template landings double-handle, your clock your risk;
/// engine-fired triggers (deploy drops/claims) queue politely and enter
/// only when the bot is out of every template phase.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum RecallMode {
    Signal,
    Polite,
}
use pyrite::Vm;
use std::collections::BTreeSet;
use std::rc::Rc;

impl Sim {
    /// Boot countdown complete: leave the engine context and enter the
    /// Boot TEMPLATE (docs/01) — forced `upload_log()` when the local
    /// buffer is non-empty (the rescued veteran's automatic incident
    /// report, run as real costed code through the ordinary host arm),
    /// then the `on boot:` window (the dotfile), then line 1.
    pub(crate) fn finish_boot(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.booting = None;
        let upload_pending = !bot.data.log_buf.is_empty();
        if let Some(vm) = bot.vm.as_mut() {
            vm.set_engine_ctx(None);
            vm.begin_boot(upload_pending);
        }
    }

    /// Recall arrival: "transported to the new printer for a new color,
    /// keeping XP" (docs/01). Fresh VM on the destination color's program,
    /// then the Boot Sequence. Returns false when no tile near the
    /// destination is free yet — the caller keeps the recall alive and
    /// retries next tick (bots are solid; finished prints wait the same way).
    pub(crate) fn recolor_bot(&mut self, id: BotId, dest: EntityId) -> bool {
        let Some(printer) = self
            .world
            .printers
            .get(&dest)
            .filter(|p| p.state == PrinterState::Working)
        else {
            // Destination vanished (or was ruined mid-walk): end the
            // recall, resume the old program — never re-color at a ruin.
            self.finish_boot(id);
            return true;
        };
        let (printer_pos, color, faction) = (printer.pos, printer.color, printer.faction);
        let Some(cp) = self.world.color_programs.get(&(faction, color.0)) else {
            self.finish_boot(id);
            return true;
        };
        // The hardware bar holds at ARRIVAL too (Q52: printers claim only
        // fitting bots): an over-bar artifact deployed mid-walk turns the
        // arrival into a plain resume — the lame duck keeps its old code
        // and the next allocation re-homes it.
        let ctx = self.ctx();
        let data = &self.world.bots[&id].data;
        if cp.req_lines > ctx.program_lines_for(data)
            || cp.req_names > ctx.variable_slots_for(data)
        {
            self.finish_boot(id);
            return true;
        }
        let program = Rc::clone(&cp.program);
        let Some(landing) = self.world.free_spawn_tile(printer_pos) else {
            return false;
        };
        self.world.move_bot(id, landing);
        // Hardware travels with the chassis: the fresh VM keeps the bot's
        // bought stack depth (re-coloring swaps the program, not the body).
        let config = self.vm_config_for(&self.world.bots[&id].data);
        // Boot ritual through the pipeline (Hot Reload / Windows Update /
        // the Boot track).
        let boot = self.ctx()
            .boot_ticks_for(&self.world.bots[&id].data, self.tuning.boot_ticks);
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.color = color;
        bot.data.recall = None;
        bot.data.booting = Some(boot);
        let mut vm = Vm::new(program, config);
        vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot));
        bot.vm = Some(vm);
        true
    }

    /// Over-capacity decommission: logs upload to the cloud (the bot is at
    /// the printer), partial refund, no wreck, no black box — an orderly
    /// recycling, not a destruction.
    pub(crate) fn scrap_bot(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.remove(&id) else { return };
        self.world.unindex_bot(id, bot.data.pos);
        self.world.bot_entities.remove(&bot.data.entity);
        // Orderly recycling at the printer: carried cargo goes to stores.
        let faction = bot.data.faction;
        for (kind, deci) in &bot.data.cargo {
            self.world.stock_add(faction, *kind, *deci as u64);
        }
        let tick = self.world.tick;
        for (level, text) in bot.data.log_buf {
            self.world.archive.push(ArchiveEntry {
                tick,
                bot: id,
                kind: ArchiveKind::Log,
                level,
                line: 0,
                text,
            });
        }
        self.world.stock_add(faction, crate::resources::Resource::Steel, self.tuning.scrap_refund_steel);
    }

    /// Phase 6: printers — advance/start print jobs, rebalance recalls,
    /// capacity scrap. All iteration in stable id order.
    pub(crate) fn run_printers(&mut self) {
        let printer_ids: Vec<EntityId> = self.world.printers.keys().copied().collect();

        // Advance and complete print jobs.
        for pid in printer_ids.iter() {
            let printer = self.world.printers.get_mut(pid).expect("printer exists");
            if printer.state != PrinterState::Working {
                continue;
            }
            if let Some(ticks) = printer.job {
                if ticks > 1 {
                    printer.job = Some(ticks - 1);
                } else {
                    let (pos, faction, color) = (printer.pos, printer.faction, printer.color);
                    // Bots are solid: hold the finished print until a tile
                    // near the printer frees up.
                    let Some(spawn_pos) = self.world.free_spawn_tile(pos) else {
                        self.world.printers.get_mut(pid).expect("printer exists").job = Some(1);
                        continue;
                    };
                    self.world.printers.get_mut(pid).expect("printer exists").job = None;
                    match self.world.color_programs.get(&(faction, color.0)) {
                        Some(cp) => {
                            let program = Rc::clone(&cp.program);
                            let mut vm = Vm::new(program, self.vm_config.clone());
                            vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot));
                            // The universal floor (docs/02): every print is
                            // the same machine — the statline is stats.ron.
                            let s = &self.stats;
                            let (hp, cpu, cap) = (s.hp, s.cpu_centi, s.cargo_cap_deci);
                            self.insert_bot(spawn_pos, faction, color, hp, cpu, cap, vm, true);
                        }
                        None => {
                            // Program was undeployed mid-print: refund.
                            let f = self.world.printers[pid].faction;
                            self.world.stock_add(
                                f,
                                crate::resources::Resource::Steel,
                                self.tuning.print_cost_steel,
                            );
                        }
                    }
                }
            }
        }

        // --- M9 target shares (docs/01) ---
        let factions: std::collections::BTreeSet<u8> =
            self.world.printers.values().map(|p| p.faction).collect();

        // The per-faction re-allocation clock: player-fired, so its
        // recalls dispatch like signals (mid-template = double-handle;
        // "turn the dials when your bots are somewhere safe" is literal).
        for faction in factions.iter().copied() {
            let interval = self
                .world
                .check_interval
                .get(&faction)
                .copied()
                .unwrap_or(self.printer_cfg.check_interval_ticks);
            if interval > 0 && self.world.tick.is_multiple_of(interval) {
                self.allocate_fleet(faction, RecallMode::Signal, None);
            }
        }

        // Polite queue: engine-fired re-colorings enter when their bot is
        // out of every template phase.
        self.dispatch_polite_recalls();

        // Prints: while the fleet is under cap, a dialed printer short of
        // its target prints its own color (priority order); once every
        // target is met, the remainder printer prints (docs/01). A color
        // whose artifact exceeds STOCK hardware never prints — fresh
        // prints are stock machines that couldn't receive it (Q52; its
        // growth lands at the remainder instead).
        for faction in factions.iter().copied() {
            let cap = self.fleet_cap(faction);
            let jobs: u32 = self
                .world
                .printers
                .values()
                .filter(|p| p.faction == faction && p.job.is_some())
                .count() as u32;
            let mut projected = self.world.fleet_size(faction) + jobs;
            let Some(remainder) = self.world.remainder_printer(faction) else { continue };
            let mut order: Vec<(u32, EntityId)> = self
                .world
                .printers
                .iter()
                .filter(|(id, p)| {
                    p.faction == faction && **id != remainder && p.state == PrinterState::Working
                })
                .filter_map(|(id, p)| p.rules.map(|r| (r.priority, *id)))
                .collect();
            order.sort_unstable();
            let mut queue: Vec<EntityId> = order.into_iter().map(|(_, id)| id).collect();
            queue.push(remainder);
            for pid in queue {
                if projected >= cap {
                    break;
                }
                let printer = &self.world.printers[&pid];
                if printer.state != PrinterState::Working || printer.job.is_some() {
                    continue;
                }
                let (color, rules) = (printer.color, printer.rules);
                let Some(cp) = self.world.color_programs.get(&(faction, color.0)) else {
                    continue;
                };
                // Stock-bar suppression (Q52): prints are stock machines.
                if cp.req_lines > self.stats.program_lines
                    || cp.req_names > self.stats.variable_slots
                {
                    continue;
                }
                // A dialed printer prints only while short of its target;
                // the remainder (rules None) prints whenever the fleet is
                // short of the cap. Incoming re-colors — walking OR
                // politely queued — count toward the destination (the
                // pre-M9 invariant): never print a replacement for a bot
                // already assigned here.
                if let Some(rules) = rules {
                    let target = self.resolve_target(rules.target, cap);
                    let pop = self.world.color_population(faction, color)
                        + self.incoming_recolors(faction, color);
                    if pop >= target {
                        continue;
                    }
                }
                if !self.world.stock_take(
                    faction,
                    crate::resources::Resource::Steel,
                    self.tuning.print_cost_steel,
                ) {
                    continue;
                }
                self.world.printers.get_mut(&pid).expect("printer exists").job =
                    Some(self.tuning.print_ticks);
                projected += 1;
                // The reprint queue is a convenience counter (docs/01: a
                // reprint IS a fresh print) — consume one per started job.
                if let Some(n) = self.world.reprint_queue.get_mut(&faction) {
                    *n -= 1;
                    if *n == 0 {
                        self.world.reprint_queue.remove(&faction);
                    }
                }
            }
        }

        // NO over-capacity scrap here (docs/01: when a ruin shrinks the
        // cap, "printing simply stops until attrition brings the fleet
        // back under — over-capacity scrap remains an economy event
        // only"). Prints already stop at the cap above; the economy's
        // sustained-rust valve (upkeep.ron `rust_scraps`) is the scrap
        // trigger that remains.
    }

    /// The colony's fleet cap: a fixed contribution per WORKING printer
    /// (printers.ron; docs/02 — a dormant/ruined printer's contribution
    /// is withdrawn). Saturating: replay-supplied config must never
    /// panic the sim.
    pub fn fleet_cap(&self, faction: u8) -> u32 {
        let working = self
            .world
            .printers
            .values()
            .filter(|p| p.faction == faction && p.state == PrinterState::Working)
            .count() as u32;
        working.saturating_mul(self.printer_cfg.fleet_cap_per_printer)
    }

    /// A target dial's bot count: absolute, or a floored percentage OF
    /// THE CAP (Q64 — never the live fleet). Widened arithmetic — a
    /// hostile cap × pct must wrap nothing.
    fn resolve_target(&self, target: PrintTarget, cap: u32) -> u32 {
        match target {
            PrintTarget::Count(n) => n,
            PrintTarget::CapPct(pct) => {
                (cap as u64 * pct.min(100) as u64 / 100) as u32
            }
        }
    }

    /// Re-colorings currently headed INTO `color` — walking recalls plus
    /// polite queue entries — from bots not already wearing it.
    fn incoming_recolors(&self, faction: u8, color: Color) -> u32 {
        let walking = self
            .world
            .bots
            .values()
            .filter(|b| {
                b.data.faction == faction
                    && b.data.color != color
                    && matches!(
                        &b.data.recall,
                        Some(Recall { purpose: RecallPurpose::Recolor { dest }, .. })
                            if self.world.printers.get(dest).map(|p| p.color) == Some(color)
                    )
            })
            .count();
        let queued = self
            .world
            .pending_recalls
            .iter()
            .filter(|(b, p)| {
                self.world
                    .bots
                    .get(b)
                    .is_some_and(|bot| bot.data.faction == faction && bot.data.color != color)
                    && self.world.printers.get(p).map(|pr| pr.color) == Some(color)
            })
            .count();
        (walking + queued) as u32
    }

    /// The target-share allocation (M9, docs/01): down the player's
    /// priority list, each dialed printer sorts the fleet by its key
    /// (hardware-bar filter FIRST — Q52; ties break by entity id) and
    /// claims up to its target; the remainder takes the rest. Every bot
    /// whose assigned color differs from its current one is recalled —
    /// by `mode` — and a bot already walking a re-color is RE-TARGETED
    /// engine-side (never re-signaled): destination updated, or the
    /// recall cancelled in place when the new assignment matches its
    /// current color (restart at line 1, no boot — docs/01).
    /// `only_color` scopes the pass to one color's claims and drops
    /// (deploys re-allocate their own color only).
    pub(crate) fn allocate_fleet(
        &mut self,
        faction: u8,
        mode: RecallMode,
        only_color: Option<Color>,
    ) {
        let cap = self.fleet_cap(faction);
        let Some(remainder) = self.world.remainder_printer(faction) else { return };
        let remainder_color = self.world.printers[&remainder].color;
        // A ruined remainder can't receive anyone: recalling the unclaimed
        // fleet to a ruin would re-color them into permanent ghosts. With
        // no WORKING remainder, unclaimed bots simply keep their colors
        // until it is repaired.
        let remainder_working =
            self.world.printers[&remainder].state == PrinterState::Working;

        // The fleet: live, non-ghost members. Scrap-recalled bots are
        // already leaving and stay out; re-color walks stay IN (the
        // allocation may re-target them).
        let mut remaining: Vec<BotId> = self
            .world
            .bots
            .values()
            .filter(|b| {
                b.data.faction == faction
                    && !b.data.dying
                    && !self.world.is_ghost(&b.data)
                    && !matches!(
                        b.data.recall,
                        Some(Recall { purpose: RecallPurpose::Scrap, .. })
                    )
            })
            .map(|b| b.data.id)
            .collect();

        let mut dialed: Vec<(u32, EntityId)> = self
            .world
            .printers
            .iter()
            .filter(|(id, p)| {
                p.faction == faction && **id != remainder && p.state == PrinterState::Working
            })
            .filter_map(|(id, p)| p.rules.map(|r| (r.priority, *id)))
            .collect();
        dialed.sort_unstable();

        let mut assigned: Vec<(BotId, Color, EntityId)> = Vec::new();
        for (_, pid) in dialed {
            let printer = &self.world.printers[&pid];
            let color = printer.color;
            let rules = printer.rules.expect("dialed printers filtered on rules");
            let Some(cp) = self.world.color_programs.get(&(faction, color.0)) else {
                continue; // no artifact, no bar, no claims
            };
            let (req_lines, req_names) = (cp.req_lines, cp.req_names);
            let target = self.resolve_target(rules.target, cap) as usize;
            // Hardware bar before the key (Q52): only fitting bots.
            let ctx = self.ctx();
            let mut candidates: Vec<(i64, u64, BotId)> = remaining
                .iter()
                .filter(|id| {
                    let data = &self.world.bots[id].data;
                    ctx.program_lines_for(data) >= req_lines
                        && ctx.variable_slots_for(data) >= req_names
                })
                .map(|id| {
                    let data = &self.world.bots[id].data;
                    let value = rules.key.value(data);
                    // Best-first follows the key's improvement direction
                    // (Q64); the stored sort key normalizes to ascending.
                    let sort = if rules.best_first == rules.key.higher_is_better() {
                        -value
                    } else {
                        value
                    };
                    (sort, data.entity.0, *id)
                })
                .collect();
            candidates.sort_unstable();
            let claimed: Vec<BotId> =
                candidates.into_iter().take(target).map(|(_, _, id)| id).collect();
            for id in &claimed {
                remaining.retain(|r| r != id);
                assigned.push((*id, color, pid));
            }
        }
        if remainder_working {
            for id in remaining {
                assigned.push((id, remainder_color, remainder));
            }
        }

        for (bot_id, color, printer) in assigned {
            let Some(bot) = self.world.bots.get(&bot_id) else { continue };
            let current = bot.data.color;
            if let Some(scope) = only_color
                && current != scope
                && color != scope
            {
                continue; // a deploy re-allocates its own color only
            }
            // Already walking a re-color: re-target engine-side.
            if let Some(Recall { purpose: RecallPurpose::Recolor { dest }, .. }) =
                &bot.data.recall
            {
                if color == current {
                    // New assignment matches the CURRENT color: cancel in
                    // place, restart at line 1, no boot (docs/01).
                    self.cancel_recall_in_place(bot_id);
                } else if self.world.printers.get(dest).map(|p| p.color) != Some(color) {
                    let bot = self.world.bots.get_mut(&bot_id).expect("checked");
                    if let Some(recall) = bot.data.recall.as_mut() {
                        recall.home = printer;
                        recall.purpose = RecallPurpose::Recolor { dest: printer };
                    }
                    // Re-plan the walk toward the new home.
                    self.replan_after_bump(bot_id);
                }
                continue;
            }
            if color == current {
                self.world.pending_recalls.remove(&bot_id);
                continue;
            }
            match mode {
                // Player-fired: dispatches like a signal — a mid-template
                // landing is a double-handle (your clock, your risk). But
                // boot and pad-sit are ENGINE interrupt states, not the
                // player's clock: raising Recall there would abort the bot
                // (begin_recall_walk's own "bug net"), so those defer to
                // the polite queue instead of wrecking fresh prints and
                // mid-upgrade bots on a dial nudge.
                RecallMode::Signal => {
                    let engine_busy = {
                        let data = &self.world.bots[&bot_id].data;
                        data.booting.is_some() || data.pad_sit
                    };
                    if engine_busy {
                        self.world.pending_recalls.insert(bot_id, printer);
                    } else {
                        self.world.pending_recalls.remove(&bot_id);
                        let _ = self.begin_recall_walk(
                            bot_id,
                            printer,
                            RecallPurpose::Recolor { dest: printer },
                        );
                    }
                }
                // Engine-fired: the lame duck visibly runs the old color
                // until its entry lands politely.
                RecallMode::Polite => {
                    self.world.pending_recalls.insert(bot_id, printer);
                }
            }
        }
    }

    /// Same-color re-target: the recall cancels and the program restarts
    /// at line 1 in place — no boot, since no re-coloring happened.
    fn cancel_recall_in_place(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.recall = None;
        if let Some(vm) = bot.vm.as_mut() {
            vm.reset();
            vm.set_engine_ctx(None);
        }
    }

    /// Drain the polite queue (M9, Q85): an engine-fired re-coloring
    /// enters only when its bot is out of every template phase — never a
    /// double-handle. Stale entries (dead bots, gone printers, already
    /// matching colors) drop silently.
    pub(crate) fn dispatch_polite_recalls(&mut self) {
        let queue: Vec<(BotId, EntityId)> =
            self.world.pending_recalls.iter().map(|(b, p)| (*b, *p)).collect();
        for (bot_id, printer) in queue {
            let Some(bot) = self.world.bots.get(&bot_id) else {
                self.world.pending_recalls.remove(&bot_id);
                continue;
            };
            let Some(p) = self.world.printers.get(&printer) else {
                self.world.pending_recalls.remove(&bot_id);
                continue;
            };
            if bot.data.dying || bot.data.color == p.color {
                self.world.pending_recalls.remove(&bot_id);
                continue;
            }
            let free = bot.data.recall.is_none()
                && bot.data.booting.is_none()
                && !bot.data.pad_sit
                && bot.vm.as_ref().is_none_or(|vm| vm.phase() == pyrite::Phase::Main);
            if !free {
                continue; // stays queued; politeness is patience
            }
            // Consume the entry only when the walk ACTUALLY starts: a
            // momentarily-unreachable printer (or an unhandled raise)
            // keeps the promise queued and retried next tick — the old
            // per-tick rebalance re-selected; the queue must too.
            if self.begin_recall_walk(bot_id, printer, RecallPurpose::Recolor { dest: printer }) {
                self.world.pending_recalls.remove(&bot_id);
            }
        }
    }

    /// Fire one scrap recall at the faction's lowest-TOTAL-XP eligible
    /// bot (M9: every track counts — the pre-M9 pick summed only the
    /// three task tracks and protected the wrong veterans) —
    /// the over-capacity valve, also reused by sustained-rust scrapping
    /// (M5, `upkeep.ron` `rust_scraps`).
    pub(crate) fn scrap_recall_lowest(&mut self, faction: u8) {
        // Lowest-XP bot colony-wide walks home for scrap.
        let victim = self
            .world
            .bots
            .values()
            .filter(|b| {
                // Engine-fired recalls stay POLITE (Q85): never at
                // dying, recalled, booting, or MID-TEMPLATE bots —
                // scrap re-selects the next-lowest instead of
                // wrecking its target against the decided intent.
                b.data.faction == faction
                    && !b.data.dying
                    && !self.world.is_ghost(&b.data) // ghosts exempt (Q65)
                    && b.data.recall.is_none()
                    && b.data.booting.is_none()
                    && !b.data.pad_sit
                    && b.vm.as_ref().is_none_or(|vm| vm.phase() == pyrite::Phase::Main)
            })
            .map(|b| (b.data.xp_total(), b.data.id))
            .min();
        if let Some((_, victim)) = victim {
            let home = self.nearest_faction_printer(victim);
            if let Some(home) = home {
                let _ = self.begin_recall_walk(victim, home, RecallPurpose::Scrap);
            }
        }
    }

    /// Phase 8 (M5): Upgrade Station pads. Works like a printer (Q68):
    /// bots never path onto the pad — they stand ADJACENT with a queued
    /// order and the pad PULLS the next eligible one (lowest entity id,
    /// skipping mid-template/boot/recall bots so the pull never creates a
    /// double-handle). Payment charges AT MOUNT (Chips etc. from stock +
    /// coolant Water from the station's physical buffer); an unaffordable
    /// order is SKIPPED — the pad moves on and the order re-arms when
    /// stock covers it. The sit is an engine interrupt context; stepping
    /// off restarts the program at line 1 (no boot).
    pub(crate) fn run_pads(&mut self) {
        use crate::resources::{Resource, DECI};
        use crate::world::{PadJob, StructureKind, UpgradeOrder};
        // Release any pad-sitter whose station vanished mid-sit (attacked
        // down): the sweep keys off the stations, so orphans free here.
        let sitting: std::collections::BTreeSet<BotId> = self
            .world
            .structures
            .values()
            .filter_map(|s| s.pad.as_ref().map(|p| p.bot))
            .collect();
        let orphans: Vec<BotId> = self
            .world
            .bots
            .iter()
            .filter(|(id, b)| b.data.pad_sit && !sitting.contains(id))
            .map(|(id, _)| *id)
            .collect();
        for id in orphans {
            self.release_from_pad(id);
        }

        let station_ids: Vec<EntityId> = self
            .world
            .structures
            .iter()
            .filter(|(_, s)| s.kind == StructureKind::UpgradeStation)
            .map(|(id, _)| *id)
            .collect();
        for sid in station_ids {
            let Some(st) = self.world.structures.get(&sid) else { continue };
            let (pos, faction) = (st.pos, st.faction);

            // An occupied pad advances (or finishes) before pulling anew.
            if let Some(job) = st.pad {
                let live = self.world.bots.get(&job.bot).is_some_and(|b| !b.data.dying);
                if !live {
                    // Aborted mid-sit: the wreck fell where it sat.
                    self.world.structures.get_mut(&sid).expect("exists").pad = None;
                } else if job.ticks_left > 1 {
                    self.world.structures.get_mut(&sid).expect("exists").pad =
                        Some(PadJob { ticks_left: job.ticks_left - 1, ..job });
                } else {
                    // Done — but bots are solid: hold the graduate on the
                    // pad until a tile beside the station frees up.
                    let Some(step_off) = self.world.free_spawn_tile(pos) else { continue };
                    self.apply_order(job.bot, job.order);
                    self.world.move_bot(job.bot, step_off);
                    self.release_from_pad(job.bot);
                    self.world.structures.get_mut(&sid).expect("exists").pad = None;
                }
                continue;
            }

            // Empty pad: pull the lowest-entity-id adjacent queued bot
            // whose front order is valid and affordable.
            let mut candidates: Vec<(EntityId, BotId)> = self
                .world
                .bots
                .iter()
                .filter(|(_, b)| {
                    b.data.faction == faction
                        && !b.data.dying
                        && !b.data.pad_sit
                        && b.data.booting.is_none()
                        && b.data.recall.is_none()
                        && !b.data.upgrade_queue.is_empty()
                        && b.data.pos.chebyshev(pos) <= 1
                        // The pull itself must never double-handle (07).
                        && b.vm.as_ref().is_none_or(|vm| vm.phase() == pyrite::Phase::Main)
                })
                .map(|(id, b)| (b.data.entity, *id))
                .collect();
            candidates.sort();
            for (_, bot_id) in candidates {
                let order = self.world.bots[&bot_id].data.upgrade_queue[0];
                // Validity first: an order that can never mount is DROPPED
                // (duplicate CPU tier/Coprocessor; module swap without a
                // legal slot). Skipping would wedge the queue forever.
                let (valid, cost, coolant): (bool, Vec<(Resource, u32)>, u32) = match order {
                    UpgradeOrder::Compute(idx) => {
                        let spec = &self.stats.upgrades[idx as usize];
                        let data = &self.world.bots[&bot_id].data;
                        // CPU tiers SET the grant in purchase order, so a
                        // lower-or-equal tier after a higher one would be
                        // a PAID DOWNGRADE — invalid, like a duplicate.
                        let dup = match spec.effect {
                            crate::stats::UpgradeEffect::CpuCenti(c) => {
                                data.upgrades.iter().any(|&u| {
                                    matches!(
                                        self.stats.upgrades.get(u as usize).map(|s| s.effect),
                                        Some(crate::stats::UpgradeEffect::CpuCenti(owned))
                                            if owned >= c
                                    )
                                })
                            }
                            crate::stats::UpgradeEffect::Coprocessor => {
                                data.upgrades.contains(&idx)
                            }
                            _ => false,
                        };
                        (!dup, spec.cost.clone(), self.stats.coolant_water_deci)
                    }
                    UpgradeOrder::Module { idx, replace } => {
                        let data = &self.world.bots[&bot_id].data;
                        let ok = match replace {
                            Some(slot) => (slot as usize) < data.modules.len(),
                            None => data.modules.len() < data.module_slots as usize,
                        };
                        (ok, self.stats.modules[idx as usize].cost.clone(), 0)
                    }
                };
                if !valid {
                    self.world.bots.get_mut(&bot_id).expect("exists").data.upgrade_queue.remove(0);
                    continue; // try this station's next candidate
                }
                // Affordability: typed price from stock (abstract payment)
                // + coolant from the station's own buffer (physical feed).
                let affordable = cost
                    .iter()
                    .all(|(k, units)| self.world.stock_get(faction, *k) >= (*units * DECI) as u64)
                    && self.world.structures[&sid]
                        .input
                        .get(&Resource::Water)
                        .copied()
                        .unwrap_or(0)
                        >= coolant;
                if !affordable {
                    continue; // skip — the order re-arms when stock covers it
                }
                for (k, units) in &cost {
                    let taken = self.world.stock_take(faction, *k, (*units * DECI) as u64);
                    debug_assert!(taken, "checked affordable above");
                }
                if coolant > 0 {
                    let st = self.world.structures.get_mut(&sid).expect("exists");
                    match st.input.get_mut(&Resource::Water) {
                        Some(have) => {
                            *have -= coolant;
                            if *have == 0 {
                                st.input.remove(&Resource::Water);
                            }
                        }
                        None => unreachable!("checked coolant above"),
                    }
                }
                // Mount: onto the pad tile, program suspended (the pull
                // silently cancels the pending action — no signal, Q84),
                // engine interrupt context set.
                let time = match order {
                    UpgradeOrder::Compute(idx) => self.stats.upgrades[idx as usize].time_ticks,
                    UpgradeOrder::Module { idx, .. } => {
                        self.stats.modules[idx as usize].time_ticks
                    }
                };
                self.world.move_bot(bot_id, pos);
                let bot = self.world.bots.get_mut(&bot_id).expect("exists");
                bot.data.upgrade_queue.remove(0);
                bot.data.pad_sit = true;
                bot.data.requested = None;
                bot.data.action = None;
                if let Some(vm) = bot.vm.as_mut() {
                    vm.set_engine_ctx(Some(pyrite::EngineCtx::PadSit));
                }
                self.world.structures.get_mut(&sid).expect("exists").pad =
                    Some(PadJob { bot: bot_id, order, ticks_left: time.max(1) });
                break;
            }
        }
    }

    /// Apply a completed Station order to the chassis (the hardware layer
    /// of the stat pipeline reads these lists).
    fn apply_order(&mut self, bot_id: BotId, order: crate::world::UpgradeOrder) {
        use crate::world::UpgradeOrder;
        let Some(bot) = self.world.bots.get_mut(&bot_id) else { return };
        match order {
            UpgradeOrder::Compute(idx) => {
                bot.data.upgrades.push(idx);
                if let Some(crate::stats::UpgradeEffect::MemoryBank) =
                    self.stats.upgrades.get(idx as usize).map(|s| s.effect)
                {
                    bot.data.log_cap += self.stats.memory_bank_log;
                }
                // A bought Stack extension reaches the LIVE VM.
                let depth = crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }.stack_depth_for(&bot.data);
                if let Some(vm) = bot.vm.as_mut() {
                    vm.set_stack_depth(depth);
                }
            }
            UpgradeOrder::Module { idx, replace } => match replace {
                // The swap destroys the removed part — no refund (Q72);
                // it also drops off the build receipt (M10 reads
                // currently-installed hardware).
                Some(slot) => bot.data.modules[slot as usize] = idx,
                None => bot.data.modules.push(idx),
            },
        }
    }

    /// End a pad-sit: engine context cleared, program restarted at line 1
    /// (docs/03: no boot — no re-coloring happened).
    fn release_from_pad(&mut self, bot_id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&bot_id) else { return };
        bot.data.pad_sit = false;
        if let Some(vm) = bot.vm.as_mut() {
            vm.reset();
            vm.set_engine_ctx(None);
        }
    }

    pub(crate) fn nearest_faction_printer(&self, bot: BotId) -> Option<EntityId> {
        let data = &self.world.bots.get(&bot)?.data;
        self.world
            .printers
            .iter()
            .filter(|(_, p)| p.faction == data.faction && p.state == PrinterState::Working)
            .map(|(id, p)| (data.pos.manhattan(p.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    /// Suspend the program (engine interrupt), cancel any action, and start
    /// walking to the home printer. Double-handle applies the whole way.
    /// Returns TRUE only when the walk actually started — callers keeping
    /// a polite queue must not consume their entry on a false (no route,
    /// raise not handled): politeness is patience, retried next tick.
    pub(crate) fn begin_recall_walk(&mut self, id: BotId, home: EntityId, purpose: RecallPurpose) -> bool {
        let Some(home_pos) = self.world.printers.get(&home).map(|p| p.pos) else { return false };
        let Some(bot) = self.world.bots.get_mut(&id) else { return false };
        let start = bot.data.pos;
        // Goals: the passable, non-structure tiles ORTHOGONALLY beside home
        // (the printer tile itself is solid; diagonal corner-touch reads as
        // arriving a square away, so it doesn't count as arrived).
        let structures = self.world.structure_tiles();
        let mut goals = BTreeSet::new();
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let g = TilePos::new(home_pos.x + dx, home_pos.y + dy);
            if self.world.grid.get(g).is_some_and(|t| t.passable())
                && !structures.contains(&g)
            {
                goals.insert(g);
            }
        }
        // `None` (no route) is NOT the same as `Some([])` (already beside
        // home): an unreachable printer must not start a recall at all —
        // an empty path reads as "arrived", which would scrap the bot in
        // place across the map (or teleport a recolor). The caller's next
        // check re-selects; the bot stays put until a route exists.
        let Some(path) =
            astar_avoiding(&self.world.grid, &self.world.overlays, &self.tuning.tile_costs, start, &goals, &structures)
        else {
            return false;
        };
        let ticks_left = path
            .first()
            .map(|p| {
                crate::stats::step_ticks(self.ctx(), &self.world.grid, &self.world.bots[&id].data, *p)
                    .unwrap_or(1)
            })
            .unwrap_or(0);
        // Recall dispatches like any other signal (docs/01): through raise,
        // which suspends the VM correctly whether Running OR Blocked (the
        // stacks clear — the pending action's owed result never arrives)
        // and applies the double-handle if a template is somehow running.
        // Engine-fired callers select politely, so Aborted here is a bug
        // net, not a normal path.
        if self.raise_signal(id, pyrite::Signal::Recall) != pyrite::RaiseOutcome::Handled {
            return false;
        }
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.recall = Some(Recall { path, ticks_left, home, purpose });
        true
    }
}
