//! The universal chassis (M5, docs/02): the floor statline every bot
//! prints with, the Upgrade Station catalog, and the deterministic
//! modifier pipeline
//!
//! > base → hardware → XP perks (M6) → quirks (M6) → state → clamp
//!
//! Percent modifiers are integer percents of the running subtotal with
//! PESSIMISTIC rounding (fractions round toward worse-for-the-bot: gains
//! floor, penalties ceil). Percent-modified stats store fine-grained
//! units (Q56): cycles in centicycles, move rate in deci-ticks per tile.
//! All numbers live in `data/stats.ron`.

use crate::map::{Grid, TilePos};
use crate::resources::Resource;
use crate::world::BotData;

/// The floor statline + catalog (loaded from `data/stats.ron`).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    /// Max HP (whole units — flat-only stats stay whole).
    pub hp: i64,
    /// Move rate in DECI-TICKS per tile (140 = 14 ticks/tile). Lower is
    /// better; terrain multiplies it at step time.
    pub move_rate_deci: u32,
    /// Cargo capacity in deci-units (40 = 4 units).
    pub cargo_cap_deci: u32,
    /// Sensor range in tiles (consumed by M7 perception; Optics adds).
    pub sensors: u32,
    /// Module slots (grown by total-XP milestones in M6, cap 3).
    pub module_slots: u32,
    /// Cycle budget granted per tick, in CENTICYCLES (100 = 1 cycle).
    pub cpu_centi: u64,
    /// Program memory in lines (deploy-bar enforcement lands M9).
    pub program_lines: u32,
    /// Distinct variable names (deploy-bar enforcement lands M9).
    pub variable_slots: u32,
    /// User-def call depth (the VM faults err_stack past it).
    pub stack_depth: u32,
    /// Log ring-buffer entries.
    pub log_buffer: u32,

    // --- state-layer penalties (integer percents, pessimistic) ---
    /// Damaged (< 50% HP, the fixed engine line): speed and cycle budget
    /// lose this percent.
    pub damaged_penalty_pct: u32,
    /// Brownout (colony draw > generation): cycle budgets lose this
    /// percent (the Fabricator trickle exempts one bot).
    pub brownout_penalty_pct: u32,

    /// Station coolant per COMPUTE upgrade, deci-Water from the station's
    /// physical input buffer (docs/03: 1 Water/upgrade; module work draws
    /// no coolant — mechanical, not thermal).
    pub coolant_water_deci: u32,

    // --- per-purchase effect magnitudes (docs/06 catalog fine print) ---
    pub memory_bank_lines: u32,
    pub memory_bank_vars: u32,
    pub memory_bank_log: u32,
    pub stack_ext_depth: u32,
    pub optics_sensors: u32,

    /// The Upgrade Station's compute catalog (docs/06): flat prices, no
    /// per-bot cost curve (Q68) — the tier ladder is the whole curve.
    pub upgrades: Vec<UpgradeSpec>,
    /// Slotted modules — made at the printer or swapped at the Station
    /// (swap destroys the removed part, no refund).
    pub modules: Vec<ModuleSpec>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeSpec {
    pub name: String,
    /// Typed price, paid from colony stock at pad mount (units).
    pub cost: Vec<(Resource, u32)>,
    /// Pad-sit duration.
    pub time_ticks: u32,
    pub effect: UpgradeEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub enum UpgradeEffect {
    /// Sets the cycle grant (Mk2 = 200, Mk3 = 400 centicycles).
    CpuCenti(u64),
    /// +lines, +variables, +log entries (magnitudes in [`Stats`]).
    MemoryBank,
    /// +call depth.
    StackExt,
    /// Think while an action resolves. PURCHASABLE BUT INERT in M5 — the
    /// VM's blocked-execution semantics are a design discussion
    /// (TASKS.md); the purchase records on the bot and its receipt.
    Coprocessor,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleSpec {
    pub name: String,
    pub cost: Vec<(Resource, u32)>,
    /// Pad-sit duration for a Station swap.
    pub time_ticks: u32,
    pub effect: ModuleEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub enum ModuleEffect {
    /// Preserve 50% XP on destruction. INERT until the death/XP rework
    /// (M6/M10) — recorded on the bot and its receipt.
    BackupCore,
    /// +sensor range (magnitude in [`Stats`]).
    Optics,
}

impl Default for Stats {
    fn default() -> Self {
        let stats: Stats = ron::from_str(include_str!("../data/stats.ron"))
            .expect("data/stats.ron parses (unknown fields are errors)");
        assert!(stats.hp > 0, "stats: hp must be > 0");
        assert!(stats.cpu_centi > 0, "stats: cpu_centi must be > 0");
        assert!(stats.move_rate_deci > 0, "stats: move_rate_deci must be > 0");
        assert!(stats.module_slots > 0, "stats: module_slots must be > 0");
        assert!(stats.damaged_penalty_pct < 100 && stats.brownout_penalty_pct < 100,
            "stats: penalties must leave something");
        let mut names: Vec<&str> = stats
            .upgrades.iter().map(|u| u.name.as_str())
            .chain(stats.modules.iter().map(|m| m.name.as_str()))
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            stats.upgrades.len() + stats.modules.len(),
            "stats: catalog names must be unique"
        );
        stats
    }
}

impl Stats {
    pub fn upgrade(&self, name: &str) -> Option<(u8, &UpgradeSpec)> {
        self.upgrades.iter().enumerate().find(|(_, u)| u.name == name).map(|(i, u)| (i as u8, u))
    }

    pub fn module(&self, name: &str) -> Option<(u8, &ModuleSpec)> {
        self.modules.iter().enumerate().find(|(_, m)| m.name == name).map(|(i, m)| (i as u8, m))
    }

    /// Per-bot log-buffer cap from hardware alone: base + memory banks
    /// (quirk LogCapPct applies one-time at manifestation, on `log_cap`).
    pub fn log_cap_for(&self, data: &BotData) -> usize {
        let banks = data
            .upgrades.iter()
            .filter(|&&u| matches!(self.upgrades.get(u as usize).map(|s| s.effect),
                Some(UpgradeEffect::MemoryBank)))
            .count() as u32;
        (self.log_buffer + banks * self.memory_bank_log) as usize
    }
}

/// The pipeline's read-side context: the floor statline, XP magnitudes,
/// and the quirk catalog — everything a stat lookup needs beside the bot.
#[derive(Clone, Copy)]
pub struct StatCtx<'a> {
    pub stats: &'a Stats,
    pub xp: &'a crate::xp::XpConfig,
    pub quirks: &'a crate::quirks::QuirkCatalog,
}

impl StatCtx<'_> {
    fn level(&self, data: &BotData, track: crate::world::XpTrack) -> u32 {
        self.xp.level(data.xp(track))
    }

    /// Per-bot stack depth: base → hardware (Stack extensions) → quirks
    /// (Memory Leak / Borrow Checker Approved), never below 1.
    pub fn stack_depth_for(&self, data: &BotData) -> usize {
        let exts = data
            .upgrades
            .iter()
            .filter(|&&u| {
                matches!(
                    self.stats.upgrades.get(u as usize).map(|s| s.effect),
                    Some(UpgradeEffect::StackExt)
                )
            })
            .count() as u32;
        let mut depth = (self.stats.stack_depth + exts * self.stats.stack_ext_depth) as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::StackDepth(d) = effect {
                depth += d as i64;
            }
        }
        depth.max(1) as usize
    }

    /// Per-bot sensor range: base → hardware (Optics) → XP (Scouting
    /// +1/level) → quirks (Retina Display / Deprecated Drivers), floor 1.
    pub fn sensors_for(&self, data: &BotData) -> u32 {
        let optics = data
            .modules
            .iter()
            .filter(|&&m| {
                matches!(
                    self.stats.modules.get(m as usize).map(|s| s.effect),
                    Some(ModuleEffect::Optics)
                )
            })
            .count() as u32;
        let mut v = (data.sensors + optics * self.stats.optics_sensors) as i64
            + (self.level(data, crate::world::XpTrack::Scouting)
                * self.xp.scouting_sensors_per_level) as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::Sensors(d) = effect {
                v += d as i64;
            }
        }
        v.max(1) as u32
    }

    /// Effective cargo capacity in deci-units: base → XP (Hauling
    /// +10%/level) → quirks (CargoPct), pessimistic floor on gains.
    pub fn cargo_cap_for(&self, data: &BotData) -> u32 {
        let mut pct = 100i64
            + (self.level(data, crate::world::XpTrack::Hauling) * self.xp.hauling_cargo_pct)
                as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::CargoPct(p) = effect {
                pct += p as i64;
            }
        }
        ((data.cargo_cap as i64 * pct.max(1)) / 100).max(1) as u32
    }

    /// Effective mine yield for one swing, deci-units before node/hold
    /// clamps: Mining +10%/level (gains floor).
    pub fn mine_yield_for(&self, data: &BotData, base_deci: u32) -> u32 {
        let pct =
            100 + self.level(data, crate::world::XpTrack::Mining) * self.xp.mining_yield_pct;
        ((base_deci as u64 * pct as u64) / 100) as u32
    }

    /// Mine swing duration: Mining L3+ takes −25% (the reduction floors —
    /// pessimistic), never below 1 tick.
    pub fn mine_swing_for(&self, data: &BotData, base_ticks: u32) -> u32 {
        let mut ticks = base_ticks as i64;
        if self.level(data, crate::world::XpTrack::Mining) >= 3 {
            ticks -= (base_ticks as i64 * self.xp.mining_l3_time_pct as i64) / 100;
        }
        ticks.max(1) as u32
    }

    /// Attack damage: base (tuning) → XP (Combat +5%/level) → quirks
    /// (DamagePct), gains floor.
    pub fn attack_damage_for(&self, data: &BotData, base: i64) -> i64 {
        let mut pct = 100i64
            + (self.level(data, crate::world::XpTrack::Combat) * self.xp.combat_damage_pct)
                as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::DamagePct(p) = effect {
                pct += p as i64;
            }
        }
        (base * pct.max(1)) / 100
    }

    /// Build rate in deci-progress per tick: Building +10%/level.
    pub fn build_rate_for(&self, data: &BotData) -> u32 {
        let pct = 100
            + self.level(data, crate::world::XpTrack::Building) * self.xp.building_speed_pct;
        ((crate::resources::DECI as u64 * pct as u64) / 100).max(1) as u32
    }

    /// Flinch (handler_init) duration: base → quirks (Rubber Ducky / Race
    /// Condition, flat ticks) → XP (Flinch −10%/level, reduction floors).
    pub fn flinch_ticks_for(&self, data: &BotData, base: u32) -> u32 {
        let mut ticks = base as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::FlinchTicks(d) = effect {
                ticks += d as i64;
            }
        }
        let level = self.level(data, crate::world::XpTrack::Flinch) as i64;
        ticks -= (ticks * level * self.xp.flinch_time_pct as i64) / 100;
        ticks.max(0) as u32
    }

    /// Boot ritual duration: base → quirks (Hot Reload / Windows Update,
    /// percent) → XP (Boot −10%/level), never below 1.
    pub fn boot_ticks_for(&self, data: &BotData, base: u32) -> u32 {
        let mut ticks = base as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::BootPct(p) = effect {
                ticks += (base as i64 * p as i64) / 100;
            }
        }
        let level = self.level(data, crate::world::XpTrack::Boot) as i64;
        ticks -= (ticks * level * self.xp.boot_time_pct as i64) / 100;
        ticks.max(1) as u32
    }

    /// Unhandled-fault chip damage: base → quirks (Statically Typed /
    /// `unsafe` Block).
    pub fn fault_damage_for(&self, data: &BotData, base: i64) -> i64 {
        let mut pct = 100i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::FaultChipPct(p) = effect {
                pct += p as i64;
            }
        }
        ((base * pct.max(0)) / 100).max(0)
    }

    /// Self-repair per regen tick: base → XP (Age mends).
    pub fn regen_for(&self, data: &BotData, base: i64) -> i64 {
        base + self.level(data, crate::world::XpTrack::Age) as i64
            * self.xp.age_repair_per_level
    }

    /// Movement-noise signature (M7, docs/05 Q54): 0 base; Hiding levels
    /// quiet it (−1/level); loud/quiet quirks join when their catalog
    /// entries land. Negative = must be approached to be heard.
    pub fn signature_for(&self, data: &BotData) -> i64 {
        -(self.level(data, crate::world::XpTrack::Hiding) as i64)
    }

    /// The combined XP-gain percent for this bot (Learning +5%/level +
    /// quirk XpPct — 10x Developer / Tech Debt), floor 0.
    pub fn xp_gain_pct(&self, data: &BotData) -> u64 {
        let mut pct = 100i64
            + (self.level(data, crate::world::XpTrack::Learning)
                * self.xp.learning_gain_pct_per_level as u32) as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::XpPct(p) = effect {
                pct += p as i64;
            }
        }
        pct.max(0) as u64
    }
}

/// Penalty percent of `v`, rounded AGAINST the bot (ceil — docs/02).
pub(crate) fn ceil_pct(v: i64, pct: u32) -> i64 {
    (v.saturating_mul(pct as i64) + 99) / 100
}

/// The Damaged line is a FIXED engine constant at 50% (docs/02) — the
/// movable `hurt_line` env is the SIGNAL's policy knob, not this.
pub fn is_damaged(data: &BotData) -> bool {
    data.hp * 2 < data.max_hp
}

/// Effective centicycles granted to this bot this tick, through the full
/// pipeline. `brownout_exempt` is the Fabricator-trickle pick.
pub fn cpu_centi(
    ctx: StatCtx<'_>,
    data: &BotData,
    brownout: bool,
    brownout_exempt: bool,
) -> u64 {
    // base (per-bot: dev spawns may override the floor)
    let mut v = data.cpu_centi as i64;
    // hardware, purchase order: CPU tiers SET the grant (docs/06 "2 / 4
    // cycles per tick" are absolutes, not additions).
    for &u in &data.upgrades {
        if let Some(UpgradeEffect::CpuCenti(c)) =
            ctx.stats.upgrades.get(u as usize).map(|s| s.effect)
        {
            v = c as i64;
        }
    }
    // XP: no track grows raw cycles (compute is bought, docs/02).
    // quirks: flat centicycle deltas (Overclocked, `unsafe` Block…), and
    // Energy Star softens the brownout percent below.
    let mut brownout_pct = ctx.stats.brownout_penalty_pct;
    for effect in ctx.quirks.effects_of(data) {
        match effect {
            crate::quirks::QuirkEffect::CpuCenti(d) => v += d,
            crate::quirks::QuirkEffect::BrownoutPenaltyPct(p) => brownout_pct = p,
            _ => {}
        }
    }
    // state: Damaged then brownout, each a percent of the running subtotal.
    if is_damaged(data) {
        v -= ceil_pct(v, ctx.stats.damaged_penalty_pct);
    }
    if brownout && !brownout_exempt {
        v -= ceil_pct(v, brownout_pct);
    }
    // clamp: never below 1 stored unit (1 centicycle).
    v.max(1) as u64
}

/// Ticks for this bot to ENTER `tile`: the move-rate stat through the
/// pipeline, multiplied by terrain, pessimistically rounded up. `None` =
/// impassable. (A* keeps terrain-relative costs — a bot-constant factor
/// never changes the argmin path.)
pub fn step_ticks(
    ctx: StatCtx<'_>,
    grid: &Grid,
    data: &BotData,
    tile: TilePos,
) -> Option<u32> {
    let mult = grid.get(tile)?.move_ticks()? as i64;
    let mut rate = data.move_rate_deci as i64;
    // XP: Mileage wears the bearings in (−% per level, reduction floors);
    // Hauling L3 moves +10% faster WHILE LOADED.
    let mileage = ctx.xp.level(data.xp(crate::world::XpTrack::Mileage)) as i64;
    rate -= (rate * mileage * ctx.xp.mileage_move_pct as i64) / 100;
    if data.cargo_total() > 0
        && ctx.xp.level(data.xp(crate::world::XpTrack::Hauling)) >= 3
    {
        rate -= (rate * ctx.xp.hauling_l3_loaded_speed_pct as i64) / 100;
    }
    // quirks: MovePct (Minified, Monorepo-while-loaded is modeled flat).
    for effect in ctx.quirks.effects_of(data) {
        if let crate::quirks::QuirkEffect::MovePct(p) = effect {
            rate += (rate * p as i64) / 100;
        }
    }
    // state: Damaged slows by the penalty percent (a move-rate increase —
    // rate is ticks-per-tile, so worse = bigger; pessimistic ceil).
    if is_damaged(data) {
        rate += ceil_pct(rate, ctx.stats.damaged_penalty_pct);
    }
    let rate = rate.max(1); // never below 1 stored unit
    Some((((rate * mult) as u64).div_ceil(10)).max(1) as u32)
}
