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

    /// The Upgrade Station's compute catalog (docs/06): flat prices, no
    /// per-bot cost curve (Q68) — the tier ladder is the whole curve.
    pub upgrades: Vec<UpgradeSpec>,
    /// Capability tier catalog (Q105): the price of reaching each tier,
    /// per capability. Index 0 is the price of tier 2 (tier 1 is free with
    /// the chassis), so a capability's ceiling is `1 + rows.len()`.
    pub tiers: Vec<TierSpec>,
    /// Per-tier flat grants, added on top of the capability's level perks.
    /// **Q105-R1**: a tier's grant must dominate the levels it resets, or
    /// buying an upgrade could leave a maxed bot WORSE at the very stat it
    /// paid for (validated at load).
    pub tier_sensors: u32,
    pub tier_damage_pct: u32,
    pub tier_build_pct: u32,
    pub tier_cpu_centi: u64,
    /// Q105 tier scaling: each tier multiplies its capability's level
    /// thresholds AND its XP gain by this percent of the tier below
    /// (10_000 = 100×). Any value above ~1500 makes even a maxed lower
    /// tier land below the new L1, so the reset is arithmetic, not code.
    pub tier_xp_scale_pct: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierSpec {
    pub capability: crate::world::Capability,
    /// The tier this row buys (2 and up).
    pub tier: u8,
    pub cost: Vec<(Resource, u32)>,
    pub time_ticks: u32,
}

impl Stats {
    /// The catalog row for `capability` at `tier`, or None past the cap.
    pub fn tier_cost(&self, capability: crate::world::Capability, tier: u8) -> Option<Vec<(Resource, u32)>> {
        self.tiers
            .iter()
            .find(|t| t.capability == capability && t.tier == tier)
            .map(|t| t.cost.clone())
    }

    /// Pad-sit duration for a tier purchase.
    pub fn tier_time(&self, capability: crate::world::Capability, tier: u8) -> u32 {
        self.tiers
            .iter()
            .find(|t| t.capability == capability && t.tier == tier)
            .map(|t| t.time_ticks)
            .unwrap_or(0)
    }

    /// The highest tier the catalog sells for a capability.
    pub fn tier_cap(&self, capability: crate::world::Capability) -> u8 {
        self.tiers.iter().filter(|t| t.capability == capability).map(|t| t.tier).max().unwrap_or(1)
    }
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
    /// Preserve every capability TIER into the reprint and wipe all XP
    /// (Q100): it protects what you *bought*, never what you *earned*.
    /// The carrier from a destroyed bot to its replacement is the one
    /// detail still open (auto-banked vs the Black Box — TASKS.md), so
    /// the purchase records on the bot and its receipt for now.
    ///
    /// (Q100 RETIRED the Coprocessor: think-while-acting is a language
    /// feature, not hardware, so actions block permanently and cycles
    /// became the Processor capability.)
    BackupCore,
}

impl Default for Stats {
    fn default() -> Self {
        let stats: Stats = ron::from_str(include_str!("../data/stats.ron"))
            .expect("data/stats.ron parses (unknown fields are errors)");
        assert!(stats.hp > 0, "stats: hp must be > 0");
        assert!(stats.cpu_centi > 0, "stats: cpu_centi must be > 0");

        assert!(stats.move_rate_deci > 0, "stats: move_rate_deci must be > 0");
        assert!(stats.damaged_penalty_pct < 100 && stats.brownout_penalty_pct < 100,
            "stats: penalties must leave something");
        let mut names: Vec<&str> = stats
            .upgrades.iter().map(|u| u.name.as_str())

            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            stats.upgrades.len(),
            "stats: catalog names must be unique"
        );
        stats
    }
}

impl Stats {
    /// Q105-R1: a tier's own grant must DOMINATE the level bonus its
    /// purchase resets, or buying an upgrade would leave a maxed bot
    /// WORSE at the very stat it paid for — a Scouting-L5 scout that buys
    /// Optics tier 2 must not end up seeing less than before. The
    /// tier/level split gives Mining a distinct *reach* (resource tiers),
    /// but Optics, Combat and Building all grant the same stat their
    /// level does, so this is a real trap rather than a theoretical one.
    /// Checked at load because both sides are data.
    pub fn validate_against_xp(&self, xp: &crate::xp::XpConfig) {
        let cap = xp.level_cap as u64;
        assert!(
            self.tier_sensors as u64 >= cap * xp.scouting_sensors_per_level as u64,
            "stats: Optics tier grant ({}) must clear the Scouting L{cap} bonus ({}) — Q105-R1",
            self.tier_sensors,
            cap * xp.scouting_sensors_per_level as u64,
        );
        assert!(
            self.tier_damage_pct as u64 >= cap * xp.combat_damage_pct as u64,
            "stats: Combat tier grant must clear the Combat L{cap} bonus — Q105-R1",
        );
        assert!(
            self.tier_build_pct as u64 >= cap * xp.building_speed_pct as u64,
            "stats: Building tier grant must clear the Building L{cap} bonus — Q105-R1",
        );
    }

    pub fn upgrade(&self, name: &str) -> Option<(u8, &UpgradeSpec)> {
        self.upgrades.iter().enumerate().find(|(_, u)| u.name == name).map(|(i, u)| (i as u8, u))
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
    /// Terrain costs live in tuning (M8) — step_ticks reads the table.
    pub tuning: &'a crate::sim::Tuning,
}

impl StatCtx<'_> {
    /// A track's LEVEL for this bot. Capability tracks divide by their
    /// tier scale first (Q105): each tier multiplies both the level
    /// thresholds and the XP gain, so dividing recovers progress *within
    /// the current tier* — which is what makes a tier purchase reset the
    /// level by arithmetic, with no reset branch anywhere. XP carried
    /// from the tier below survives as a small head start (at scale 100,
    /// a maxed tier-1 bot lands 15% of the way to the new L1).
    fn level(&self, data: &BotData, track: crate::world::XpTrack) -> u32 {
        self.xp.level(data.xp(track) / self.track_scale(data, track))
    }

    /// A capability's earned LEVEL — its proficiency with the tool
    /// currently in hand (Q105). Public because the inspector, the tier
    /// gates, and the tests all ask the same question.
    pub fn capability_level(&self, data: &BotData, cap: crate::world::Capability) -> u32 {
        self.level(data, cap.track())
    }

    /// The XP scale for a track: `M^(tier-1)` for a capability's paired
    /// track, 1 for the body tracks (Age, Mileage, Hiding, Flinch, Boot,
    /// Learning) and Hauling, which no capability tiers.
    pub fn track_scale(&self, data: &BotData, track: crate::world::XpTrack) -> u64 {
        let Some(cap) = crate::world::Capability::ALL
            .iter()
            .copied()
            .find(|c| c.track() == track)
        else {
            return 1;
        };
        let step = self.stats.tier_xp_scale_pct / 100;
        (0..data.tier(cap).saturating_sub(1)).fold(1u64, |acc, _| acc.saturating_mul(step.max(1)))
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

    /// Per-bot program memory in LINES (M9, Q52): base → hardware
    /// (Memory banks). Quirks never enter deploy-time stats.
    pub fn program_lines_for(&self, data: &BotData) -> u32 {
        self.stats.program_lines + self.memory_banks(data) * self.stats.memory_bank_lines
    }

    /// Per-bot variable slots (M9, Q52): base → hardware (Memory banks).
    pub fn variable_slots_for(&self, data: &BotData) -> u32 {
        self.stats.variable_slots + self.memory_banks(data) * self.stats.memory_bank_vars
    }

    fn memory_banks(&self, data: &BotData) -> u32 {
        data.upgrades
            .iter()
            .filter(|&&u| {
                matches!(
                    self.stats.upgrades.get(u as usize).map(|s| s.effect),
                    Some(UpgradeEffect::MemoryBank)
                )
            })
            .count() as u32
    }

    /// Per-bot sensor range: base → hardware (Optics) → XP (Scouting
    /// +1/level) → quirks (Retina Display / Deprecated Drivers), floor 1.
    pub fn sensors_for(&self, data: &BotData) -> u32 {
        // Q105: the Optics MODULE became the Optics CAPABILITY — every
        // bot has the slot, and the bought tier above the free base 1 is
        // what adds range. Q105-R1 guarantees a tier's grant dominates
        // the level bonus its purchase resets, so buying Optics can never
        // leave a scout seeing less than before.
        let optics = (data.tier(crate::world::Capability::Optics) as u32).saturating_sub(1);
        let mut v = (data.sensors + optics * self.stats.tier_sensors) as i64
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
        // Q105: the bought Combat TIER and the earned Combat LEVEL both
        // raise damage. Q105-R1 keeps the tier's grant above the L5 bonus
        // it resets, so arming up is never a net downgrade. (Base weapon
        // damage now comes from Combat tier 1 — weapon modules died with
        // the generic slots.)
        let tiers = (data.tier(crate::world::Capability::Combat) as u32).saturating_sub(1);
        let mut pct = 100i64
            + (self.level(data, crate::world::XpTrack::Combat) * self.xp.combat_damage_pct) as i64
            + (tiers * self.stats.tier_damage_pct) as i64;
        for effect in self.quirks.effects_of(data) {
            if let crate::quirks::QuirkEffect::DamagePct(p) = effect {
                pct += p as i64;
            }
        }
        (base * pct.max(1)) / 100
    }

    /// Build rate in deci-progress per tick: Building +10%/level.
    pub fn build_rate_for(&self, data: &BotData) -> u32 {
        let tiers = (data.tier(crate::world::Capability::Building) as u32).saturating_sub(1);
        let pct = 100
            + self.level(data, crate::world::XpTrack::Building) * self.xp.building_speed_pct
            + tiers * self.stats.tier_build_pct;
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
    /// The PASSIVE half of how loud a bot is: earned Hiding levels. The
    /// per-tick effects — Q103's creep mode and Ford wading — are applied
    /// by the perception pass, which knows the tick they happened on.
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
    // Q100: the Processor capability. The bought TIER adds cycles and
    // the earned Processing LEVEL adds more — compute is the one stat
    // that used to be purely bought, and now sharpens with use too.
    let ptier = (data.tier(crate::world::Capability::Processor) as u64).saturating_sub(1);
    v += (ptier * ctx.stats.tier_cpu_centi) as i64;
    v += (ctx.capability_level(data, crate::world::Capability::Processor) as u64
        * ctx.stats.tier_cpu_centi
        / 2) as i64;
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

/// Ticks for this bot to ENTER `tile` from where it stands: the
/// move-rate stat through the pipeline, times the ×2-scale edge cost
/// (M8 — the from-tile matters: Mountain climbs, Dune sink), rounded
/// pessimistically up. `None` = impassable. (A* keeps terrain-relative
/// costs — a bot-constant factor never changes the argmin path; the
/// per-bot Mud/Dune surcharges below are deliberate plan drift.)
pub fn step_ticks(
    ctx: StatCtx<'_>,
    grid: &Grid,
    data: &BotData,
    tile: TilePos,
    creep: bool,
) -> Option<u32> {
    let costs = &ctx.tuning.tile_costs;
    let mut cost_x2 = costs.edge_cost_x2(grid, data.pos, tile)? as i64;
    // Mud is heavier under load (docs/05: 3×, 4× loaded). Per-bot state,
    // so it rides here rather than in the bot-independent A* table.
    if grid.get(tile) == Some(crate::map::TileKind::Mud) && data.cargo_total() > 0 {
        cost_x2 = cost_x2.max(costs.mud_loaded_x2 as i64);
    }
    // Dunes swallow idlers (Q35): each full sink interval spent standing
    // on sand surcharges the NEXT step, up to the cap — buried, never
    // trapped.
    if grid.get(data.pos) == Some(crate::map::TileKind::Dunes) {
        let steps = (data.dune_idle / ctx.tuning.dune_sink_ticks) as i64;
        cost_x2 += (steps * ctx.tuning.dune_sink_step_x2 as i64)
            .min(ctx.tuning.dune_sink_cap_x2 as i64);
    }
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
    // Creeping (Q103): picking your feet up costs time — the whole
    // trade is slow travel for a small audible footprint (the signature
    // cut rides `signature_for`).
    if creep {
        rate = rate * ctx.tuning.creep_step_pct as i64 / 100;
    }
    let rate = rate.max(1); // never below 1 stored unit
    // deci-rate × (×2 cost) → ticks: the divisor folds both scales.
    Some((((rate * cost_x2) as u64).div_ceil(20)).max(1) as u32)
}
