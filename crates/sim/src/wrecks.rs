//! The wreck race (M10, docs/02): countdown + the game's only explosion,
//! and the four-way race — field repair (rescue) vs salvage (materials +
//! decryption) vs analyze (Data + intel + comm key) vs hijack (the bot).
//!
//! NEEDS DISCUSSION (TASKS.md): tool gating (repair/hijack should need a
//! build tool — tool modules don't exist yet, so both are ungated);
//! analyze's Non-PvP ban waits on M13's harm mode; the archive is still
//! faction-less, so analyzed logs land in the shared cloud.

use crate::sim::Sim;
use crate::world::{BlackBox, BotId, EntityId};
use pyrite::Vm;
use std::collections::BTreeSet;
use std::rc::Rc;

impl Sim {
    /// Phase 6 (before the damage settle): tick every countdown; expiries
    /// detonate in entity-id order — max-HP-scaled area damage, friend
    /// and foe, NEVER chaining (a wreck destroyed BY a blast just dies:
    /// black box, no detonation).
    pub(crate) fn tick_wrecks(&mut self) {
        let ids: Vec<BotId> = self.world.wrecks.keys().copied().collect();
        let mut exploding: BTreeSet<BotId> = BTreeSet::new();
        for id in ids {
            let w = self.world.wrecks.get_mut(&id).expect("collected");
            if w.countdown > 1 {
                w.countdown -= 1;
            } else {
                exploding.insert(id);
            }
        }
        let mut destroyed_by_blast: BTreeSet<BotId> = BTreeSet::new();
        for id in &exploding {
            let (pos, max_hp) = {
                let w = &self.world.wrecks[id];
                (w.data.pos, w.data.max_hp)
            };
            let damage = (max_hp * self.tuning.blast_damage_pct as i64 / 100).max(1);
            let radius = self.tuning.blast_radius;
            // Bots ride the ordinary damage phase (this tick's settle).
            let victims: Vec<BotId> = self
                .world
                .bots
                .iter()
                .filter(|(_, b)| !b.data.dying && b.data.pos.chebyshev(pos) <= radius)
                .map(|(v, _)| *v)
                .collect();
            for v in victims {
                self.queue_damage(v, damage, None);
            }
            // Structures fall inline (their damage stays inline pre-M10 —
            // the TASKS.md phase-4 divergence note covers this too).
            let st_ids: Vec<EntityId> = self
                .world
                .structures
                .iter()
                .filter(|(_, s)| s.pos.chebyshev(pos) <= radius)
                .map(|(sid, _)| *sid)
                .collect();
            for sid in st_ids {
                let st = self.world.structures.get_mut(&sid).expect("collected");
                st.hp = (st.hp - damage).max(0);
                if st.hp == 0 {
                    self.world.structures.remove(&sid);
                }
            }
            // Other wrecks: hull damage only — never a chain detonation.
            let hit: Vec<BotId> = self
                .world
                .wrecks
                .iter()
                .filter(|(wid, w)| {
                    *wid != id && !exploding.contains(wid) && w.data.pos.chebyshev(pos) <= radius
                })
                .map(|(wid, _)| *wid)
                .collect();
            for h in hit {
                let w = self.world.wrecks.get_mut(&h).expect("collected");
                w.hp = (w.hp - damage).max(0);
                if w.hp == 0 {
                    destroyed_by_blast.insert(h);
                }
            }
        }
        for id in exploding {
            self.destroy_wreck(id, "self-destruct countdown expired");
        }
        for id in destroyed_by_blast {
            self.destroy_wreck(id, "caught in a blast (no chain — destroyed, not detonated)");
        }
    }

    /// Remove a wreck for good, dropping its Black Box (docs/02: EVERY
    /// destruction drops one — information always survives).
    pub(crate) fn destroy_wreck(&mut self, id: BotId, cause: &str) {
        let Some(w) = self.world.wrecks.remove(&id) else { return };
        self.world.bot_entities.remove(&w.data.entity);
        let entity = self.world.alloc_entity();
        self.world.black_boxes.push(BlackBox {
            entity,
            tick: self.world.tick,
            bot: id,
            pos: w.data.pos,
            cause: cause.to_string(),
            logs: w.data.log_buf,
            env: w.data.env,
        });
    }

    /// Field repair complete: the wreck boots at the DAMAGED LINE with the
    /// hurt latch re-armed (the first shot mid-boot fires hurt → double-
    /// handle → re-wreck, whose countdown RESUMES — rescuing under fire
    /// genuinely burns the rescue). XP, quirks, hardware, env: preserved.
    /// Returns false while the wreck's tile is occupied (bots are solid).
    pub(crate) fn rescue_wreck(&mut self, id: BotId) -> bool {
        let Some(w) = self.world.wrecks.get(&id) else { return true };
        let pos = w.data.pos;
        if self.world.tile_occupied(pos, BotId(u32::MAX)) {
            return false; // hold until the tile frees
        }
        // The fleet cap holds through death and rescue (docs/02: the
        // printer-derived ceiling is the only hard cap): a rescue that
        // would boot a COUNTING member (ghosts are already outside the
        // allocation) into a full fleet waits — the countdown keeps
        // burning, so a stuffed roster can genuinely lose the race.
        let faction = w.data.faction;
        if !self.world.is_ghost(&w.data)
            && self.world.fleet_size(faction) >= self.fleet_cap(faction)
        {
            return false;
        }
        let w = self.world.wrecks.remove(&id).expect("checked");
        let mut data = w.data;
        data.hp = (data.max_hp * self.tuning.hurt_line_pct / 100).max(1);
        data.hurt_fired = false; // re-armed at the line it sits on
        data.countdown_carry = Some(w.countdown);
        data.action = None;
        data.requested = None;
        data.recall = None;
        data.pad_sit = false;
        data.bump_frozen = 0;
        // The program: its color's CURRENT artifact (a dev bot whose
        // custom source died with its VM re-boots on the color program —
        // flagged in TASKS.md). The Q52 hardware bar holds here like
        // every other boot path (review 2026-07-16): an artifact this
        // chassis can't hold boots the inert fallback instead — a lame
        // duck for the next allocation, never a bar bypass.
        let ctx = self.ctx();
        let program = self
            .world
            .color_programs
            .get(&(data.faction, data.color.0))
            .filter(|cp| {
                cp.req_lines <= ctx.program_lines_for(&data)
                    && cp.req_names <= ctx.variable_slots_for(&data)
            })
            .map(|cp| Rc::clone(&cp.program));
        let boot = self.ctx().boot_ticks_for(&data, self.tuning.boot_ticks);
        data.booting = Some(boot);
        let config = self.vm_config_for(&data);
        let mut vm = match program {
            Some(p) => Vm::new(p, config),
            None => Vm::new(
                Rc::new(pyrite::parse("wait(1)\n", &pyrite::UnlockSet::all()).expect("parses")),
                config,
            ),
        };
        vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot));
        let entity = data.entity;
        let bot_id = id;
        self.world.bots.insert(bot_id, crate::world::Bot { data, vm: Some(vm) });
        self.world.bot_entities.insert(entity, bot_id);
        self.world.index_bot(bot_id, pos);
        true
    }

    /// Salvage complete: a cut of the invested build receipt (chassis line
    /// + bought hardware — currently-installed only, Q72) to the
    /// salvager's stock, plus permanent decryption of the wreck's color.
    /// Destroys the wreck; the black box still drops.
    pub(crate) fn salvage_wreck(&mut self, id: BotId, salvager_faction: u8) {
        let Some(w) = self.world.wrecks.get(&id) else { return };
        let pct = self.tuning.salvage_receipt_pct as u64;
        let mut receipt: Vec<(crate::resources::Resource, u64)> = Vec::new();
        // Chassis line: what the print actually cost (0 when free — Q72).
        if self.tuning.print_cost_steel > 0 {
            receipt.push((
                crate::resources::Resource::Steel,
                self.tuning.print_cost_steel * pct / 100,
            ));
        }
        for &u in &w.data.upgrades {
            if let Some(spec) = self.stats.upgrades.get(u as usize) {
                for (kind, units) in &spec.cost {
                    receipt
                        .push((*kind, (*units as u64 * crate::resources::DECI as u64) * pct / 100));
                }
            }
        }
        for &m in &w.data.modules {
            if let Some(spec) = self.stats.modules.get(m as usize) {
                for (kind, units) in &spec.cost {
                    receipt
                        .push((*kind, (*units as u64 * crate::resources::DECI as u64) * pct / 100));
                }
            }
        }
        let (owner, color) = (w.data.faction, w.data.color.0);
        for (kind, deci) in receipt {
            if deci > 0 {
                self.world.stock_add(salvager_faction, kind, deci);
            }
        }
        // Programs are read on murder (docs/08): +N%, never back down.
        // Declared ALLIES advance together from this salvage on (M13,
        // docs/08: one teammate's salvage advances the team's level;
        // earlier progress isn't retroactively merged).
        let beneficiaries: Vec<u8> = std::iter::once(salvager_faction)
            .chain(self.world.live_factions().into_iter().filter(|f| {
                *f != salvager_faction && self.world.allied(salvager_faction, *f)
            }))
            .collect();
        for viewer in beneficiaries {
            let entry = self.world.decryption.entry((viewer, owner, color)).or_insert(0);
            *entry = (*entry + self.tuning.salvage_decrypt_pct).min(100);
        }
        self.destroy_wreck(id, "salvaged");
    }

    /// Analyze complete (other factions only — your own wrecks yield you
    /// nothing): Data, the wreck's logs + env into the cloud, and the
    /// victim's comm key. Destroys the wreck.
    pub(crate) fn analyze_wreck(&mut self, id: BotId, analyzer_faction: u8) {
        let Some(w) = self.world.wrecks.get(&id) else { return };
        let victim_faction = w.data.faction;
        if victim_faction == analyzer_faction {
            return;
        }
        *self.world.data.entry(analyzer_faction).or_insert(0) += self.tuning.analyze_data;
        let tick = self.world.tick;
        let logs = w.data.log_buf.clone();
        for (level, text) in logs {
            self.world.archive.push(crate::world::ArchiveEntry {
                tick,
                bot: id,
                kind: crate::world::ArchiveKind::Log,
                level,
                line: 0,
                text: format!("[analyzed] {text}"),
            });
        }
        self.world.comm_keys.entry(analyzer_faction).or_default().insert(victim_faction);
        self.destroy_wreck(id, "analyzed");
    }

    /// Hijack complete: the wreck boots under the CLAIMER's remainder
    /// color, XP intact — a stolen veteran, a full fleet member. Fails
    /// (returns false) while the claimer has no working remainder printer
    /// or the tile is blocked.
    pub(crate) fn hijack_wreck(&mut self, id: BotId, claimer_faction: u8) -> bool {
        let Some(remainder) = self.world.remainder_printer(claimer_faction) else {
            return false;
        };
        let printer = &self.world.printers[&remainder];
        if printer.state != crate::world::PrinterState::Working {
            return false;
        }
        // A hijacked veteran counts against the claimer's cap (docs/04)
        // — it boots the remainder color of a WORKING printer, so it
        // always counts. At the ceiling the theft holds until room frees.
        if self.world.fleet_size(claimer_faction) >= self.fleet_cap(claimer_faction) {
            return false;
        }
        let color = printer.color;
        let Some(w) = self.world.wrecks.get(&id) else { return true };
        let pos = w.data.pos;
        if self.world.tile_occupied(pos, BotId(u32::MAX)) {
            return false;
        }
        let w = self.world.wrecks.remove(&id).expect("checked");
        let mut data = w.data;
        data.faction = claimer_faction;
        data.color = color;
        data.hp = (data.max_hp * self.tuning.hurt_line_pct / 100).max(1);
        data.hurt_fired = false;
        data.countdown_carry = Some(w.countdown);
        data.action = None;
        data.requested = None;
        data.recall = None;
        data.pad_sit = false;
        data.bump_frozen = 0;
        data.episodes.clear(); // new allegiance, fresh detection ledger
        // Q52 holds on stolen chassis too (review 2026-07-16): a
        // remainder artifact the prize can't hold boots the fallback —
        // hijacking cheap rookies is never a hardware-bar bypass.
        let ctx = self.ctx();
        let program = self
            .world
            .color_programs
            .get(&(claimer_faction, color.0))
            .filter(|cp| {
                cp.req_lines <= ctx.program_lines_for(&data)
                    && cp.req_names <= ctx.variable_slots_for(&data)
            })
            .map(|cp| Rc::clone(&cp.program));
        let boot = self.ctx().boot_ticks_for(&data, self.tuning.boot_ticks);
        data.booting = Some(boot);
        let config = self.vm_config_for(&data);
        let mut vm = match program {
            Some(p) => Vm::new(p, config),
            None => Vm::new(
                Rc::new(pyrite::parse("wait(1)\n", &pyrite::UnlockSet::all()).expect("parses")),
                config,
            ),
        };
        vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot));
        let entity = data.entity;
        self.world.bots.insert(id, crate::world::Bot { data, vm: Some(vm) });
        self.world.bot_entities.insert(entity, id);
        self.world.index_bot(id, pos);
        true
    }

    /// Bank a black box to the colony cloud and remove it from the field.
    pub(crate) fn recover_black_box(&mut self, entity: EntityId) {
        let Some(idx) = self.world.black_boxes.iter().position(|bb| bb.entity == entity) else {
            return;
        };
        let bb = self.world.black_boxes.remove(idx);
        let tick = self.world.tick;
        self.world.archive.push(crate::world::ArchiveEntry {
            tick,
            bot: bb.bot,
            kind: crate::world::ArchiveKind::Log,
            level: 3, // warn: a recovered box is loud forensics
            line: 0,
            text: format!("[black box] {} — {} entries banked", bb.cause, bb.logs.len()),
        });
        for (level, text) in bb.logs {
            self.world.archive.push(crate::world::ArchiveEntry {
                tick,
                bot: bb.bot,
                kind: crate::world::ArchiveKind::Log,
                level,
                line: 0,
                text,
            });
        }
    }
}
