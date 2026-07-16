//! Printer lifecycle: printing, boot sequences, recalls, re-coloring,
//! and scrapping.

use crate::map::{astar_avoiding, TilePos};
use crate::sim::Sim;
use crate::world::{
    ArchiveEntry, ArchiveKind, BotId, Color, EntityId, PrinterState, Recall, RecallPurpose,
};
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
        let Some(printer) = self.world.printers.get(&dest) else {
            // Destination vanished: end the recall, resume the old program.
            self.finish_boot(id);
            return true;
        };
        let (printer_pos, color, faction) = (printer.pos, printer.color, printer.faction);
        let Some(cp) = self.world.color_programs.get(&(faction, color.0)) else {
            self.finish_boot(id);
            return true;
        };
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
        let boot = crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }
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

        // Rebalance BEFORE starting print jobs: moving an existing bot is
        // cheaper than printing and preserves XP, so recalls claim headroom
        // first (incoming re-colors count toward the destination's
        // population, which the job loop below then sees as filled).
        // Recall only fires when a destination exists — docs/01.
        for pid in printer_ids.iter().copied() {
            let printer = &self.world.printers[&pid];
            if printer.state != PrinterState::Working {
                continue;
            }
            let (faction, color, desired) = (printer.faction, printer.color, printer.desired_max);
            let population = self.world.color_population(faction, color);
            if population <= desired {
                continue;
            }
            // Destination: lowest-id working printer of this faction with
            // headroom (population + pending job below its dial).
            let dest = printer_ids.iter().copied().find(|did| {
                let p = &self.world.printers[did];
                if p.faction != faction || p.color == color || p.state != PrinterState::Working {
                    return false;
                }
                if !self.world.color_programs.contains_key(&(p.faction, p.color.0)) {
                    return false;
                }
                let pop = self.world.color_population(p.faction, p.color)
                    + p.job.is_some() as u32;
                pop < p.desired_max
            });
            let Some(dest) = dest else { continue };
            self.start_recall(faction, color, pid, RecallPurpose::Recolor { dest });
        }

        // Start new jobs where population is below the dial.
        for pid in printer_ids.iter() {
            let printer = &self.world.printers[pid];
            if printer.state != PrinterState::Working || printer.job.is_some() {
                continue;
            }
            let (faction, color, desired) = (printer.faction, printer.color, printer.desired_max);
            if !self.world.color_programs.contains_key(&(faction, color.0)) {
                continue;
            }
            let population = self.world.color_population(faction, color);
            let print_faction = self.world.printers[&pid].faction;
            if population < desired
                && self.world.stock_take(
                    print_faction,
                    crate::resources::Resource::Steel,
                    self.tuning.print_cost_steel,
                )
            {
                self.world.printers.get_mut(pid).expect("printer exists").job =
                    Some(self.tuning.print_ticks);
            }
        }

        // Capacity: scrap recalls when the colony over-extends.
        let mut factions: BTreeSet<u8> = BTreeSet::new();
        for bot in self.world.bots.values() {
            factions.insert(bot.data.faction);
        }
        for faction in factions {
            let live = self
                .world
                .bots
                .values()
                .filter(|b| b.data.faction == faction && !b.data.dying && b.data.recall.is_none())
                .count() as u32;
            if live > self.tuning.capacity {
                self.scrap_recall_lowest(faction);
            }
        }
    }

    /// Fire one scrap recall at the faction's lowest-XP eligible bot —
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
                    && b.data.recall.is_none()
                    && b.data.booting.is_none()
                    && !b.data.pad_sit
                    && b.vm.as_ref().is_none_or(|vm| vm.phase() == pyrite::Phase::Main)
            })
            .map(|b| (b.data.xp(crate::world::XpTrack::Mining) + b.data.xp(crate::world::XpTrack::Hauling) + b.data.xp(crate::world::XpTrack::Combat), b.data.id))
            .min();
        if let Some((_, victim)) = victim {
            let home = self.nearest_faction_printer(victim);
            if let Some(home) = home {
                self.begin_recall_walk(victim, home, RecallPurpose::Scrap);
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

    /// Pick the lowest-total-XP bot of (faction, color) and start its
    /// recall toward its own printer.
    pub(crate) fn start_recall(&mut self, faction: u8, color: Color, home: EntityId, purpose: RecallPurpose) {
        let victim = self
            .world
            .bots
            .values()
            .filter(|b| {
                // Polite engine-fired selection: mid-template bots are
                // skipped, not re-pulled into a double-handle (Q85/M3).
                b.data.faction == faction
                    && b.data.color == color
                    && !b.data.dying
                    && b.data.recall.is_none()
                    && b.data.booting.is_none()
                    && !b.data.pad_sit
                    && b.vm.as_ref().is_none_or(|vm| vm.phase() == pyrite::Phase::Main)
            })
            .map(|b| (b.data.xp(crate::world::XpTrack::Mining) + b.data.xp(crate::world::XpTrack::Hauling) + b.data.xp(crate::world::XpTrack::Combat), b.data.id))
            .min();
        if let Some((_, victim)) = victim {
            self.begin_recall_walk(victim, home, purpose);
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
    pub(crate) fn begin_recall_walk(&mut self, id: BotId, home: EntityId, purpose: RecallPurpose) {
        let Some(home_pos) = self.world.printers.get(&home).map(|p| p.pos) else { return };
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        let start = bot.data.pos;
        // Goals: the passable, non-structure tiles ORTHOGONALLY beside home
        // (the printer tile itself is solid; diagonal corner-touch reads as
        // arriving a square away, so it doesn't count as arrived).
        let structures = self.world.structure_tiles();
        let mut goals = BTreeSet::new();
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let g = TilePos::new(home_pos.x + dx, home_pos.y + dy);
            if self.world.grid.get(g).is_some_and(|t| t.move_ticks().is_some())
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
            return;
        };
        let ticks_left = path
            .first()
            .map(|p| {
                crate::stats::step_ticks(crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }, &self.world.grid, &self.world.bots[&id].data, *p)
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
            return;
        }
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.recall = Some(Recall { path, ticks_left, home, purpose });
    }
}
