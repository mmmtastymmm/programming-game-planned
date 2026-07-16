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

    /// Per-bot stack depth: base + extensions (hardware layer; nothing
    /// else touches it).
    pub fn stack_depth_for(&self, data: &BotData) -> usize {
        let exts = data
            .upgrades.iter()
            .filter(|&&u| matches!(self.upgrades.get(u as usize).map(|s| s.effect),
                Some(UpgradeEffect::StackExt)))
            .count() as u32;
        (self.stack_depth + exts * self.stack_ext_depth) as usize
    }

    /// Per-bot log-buffer cap: base + memory banks.
    pub fn log_cap_for(&self, data: &BotData) -> usize {
        let banks = data
            .upgrades.iter()
            .filter(|&&u| matches!(self.upgrades.get(u as usize).map(|s| s.effect),
                Some(UpgradeEffect::MemoryBank)))
            .count() as u32;
        (self.log_buffer + banks * self.memory_bank_log) as usize
    }

    /// Per-bot sensor range: base + Optics (consumed by M7 perception).
    pub fn sensors_for(&self, data: &BotData) -> u32 {
        let optics = data
            .modules.iter()
            .filter(|&&m| matches!(self.modules.get(m as usize).map(|s| s.effect),
                Some(ModuleEffect::Optics)))
            .count() as u32;
        data.sensors + optics * self.optics_sensors
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
    stats: &Stats,
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
            stats.upgrades.get(u as usize).map(|s| s.effect)
        {
            v = c as i64;
        }
    }
    // XP perks, quirks: identity until M6.
    // state: Damaged then brownout, each a percent of the running subtotal.
    if is_damaged(data) {
        v -= ceil_pct(v, stats.damaged_penalty_pct);
    }
    if brownout && !brownout_exempt {
        v -= ceil_pct(v, stats.brownout_penalty_pct);
    }
    // clamp: never below 1 stored unit (1 centicycle).
    v.max(1) as u64
}

/// Ticks for this bot to ENTER `tile`: the move-rate stat through the
/// pipeline, multiplied by terrain, pessimistically rounded up. `None` =
/// impassable. (A* keeps terrain-relative costs — a bot-constant factor
/// never changes the argmin path.)
pub fn step_ticks(
    stats: &Stats,
    grid: &Grid,
    data: &BotData,
    tile: TilePos,
) -> Option<u32> {
    let mult = grid.get(tile)?.move_ticks()? as i64;
    let mut rate = data.move_rate_deci as i64;
    // hardware/XP/quirks: nothing modifies move rate until M6 (Mileage).
    // state: Damaged slows by the penalty percent (a move-rate increase —
    // rate is ticks-per-tile, so worse = bigger; pessimistic ceil).
    if is_damaged(data) {
        rate += ceil_pct(rate, stats.damaged_penalty_pct);
    }
    let rate = rate.max(1); // never below 1 stored unit
    Some((((rate * mult) as u64).div_ceil(10)).max(1) as u32)
}
