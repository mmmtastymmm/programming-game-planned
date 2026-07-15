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
    /// Boot step 1: forced `upload_log()` if the buffer is non-empty
    /// (docs/02); step 2: program from line 1, interrupt context ends.
    pub(crate) fn finish_boot(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        bot.data.booting = None;
        let logs = std::mem::take(&mut bot.data.log_buf);
        let tick = self.world.tick;
        for text in logs {
            self.world.archive.push(ArchiveEntry {
                tick,
                bot: id,
                kind: ArchiveKind::Log,
                line: 0,
                text,
            });
        }
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        if let Some(vm) = bot.vm.as_mut() {
            vm.set_engine_interrupt(false);
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
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.color = color;
        bot.data.recall = None;
        bot.data.booting = Some(self.tuning.boot_ticks);
        let mut vm = Vm::new(program, self.vm_config.clone());
        vm.set_engine_interrupt(true);
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
        self.world.stockpile_ore += bot.data.cargo as u64;
        let tick = self.world.tick;
        for text in bot.data.log_buf {
            self.world.archive.push(ArchiveEntry {
                tick,
                bot: id,
                kind: ArchiveKind::Log,
                line: 0,
                text,
            });
        }
        self.world.stockpile_ore += self.tuning.scrap_refund_ore;
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
                            vm.set_engine_interrupt(true);
                            let t = &self.tuning;
                            let (hp, cpu, cap) = (t.printed_hp, t.printed_cpu, t.printed_cargo_cap);
                            self.insert_bot(spawn_pos, faction, color, hp, cpu, cap, vm, true);
                        }
                        None => {
                            // Program was undeployed mid-print: refund.
                            self.world.stockpile_ore += self.tuning.print_cost_ore;
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
            if population < desired && self.world.stockpile_ore >= self.tuning.print_cost_ore {
                self.world.stockpile_ore -= self.tuning.print_cost_ore;
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
                // Lowest-XP bot colony-wide walks home for scrap.
                let victim = self
                    .world
                    .bots
                    .values()
                    .filter(|b| {
                        b.data.faction == faction
                            && !b.data.dying
                            && b.data.recall.is_none()
                            && b.data.booting.is_none()
                    })
                    .map(|b| (b.data.xp_mining + b.data.xp_hauling + b.data.xp_combat, b.data.id))
                    .min();
                if let Some((_, victim)) = victim {
                    let home = self.nearest_faction_printer(victim);
                    if let Some(home) = home {
                        self.begin_recall_walk(victim, home, RecallPurpose::Scrap);
                    }
                }
            }
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
                b.data.faction == faction
                    && b.data.color == color
                    && !b.data.dying
                    && b.data.recall.is_none()
                    && b.data.booting.is_none()
            })
            .map(|b| (b.data.xp_mining + b.data.xp_hauling + b.data.xp_combat, b.data.id))
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
        let path = astar_avoiding(&self.world.grid, &self.world.overlays, start, &goals, &structures)
            .unwrap_or_default();
        let ticks_left = path
            .first()
            .map(|p| self.world.grid.get(*p).and_then(|t| t.move_ticks()).unwrap_or(1))
            .unwrap_or(0);
        let bot = self.world.bots.get_mut(&id).expect("bot exists");
        bot.data.requested = None;
        bot.data.action = None;
        bot.data.recall = Some(Recall { path, ticks_left, home, purpose });
        if let Some(vm) = bot.vm.as_mut() {
            vm.set_engine_interrupt(true);
        }
    }
}
