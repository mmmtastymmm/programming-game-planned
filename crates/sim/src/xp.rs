//! XP v2 (M6, docs/02): the quadratic curve, income constants, perk
//! magnitudes, and total-XP milestones — all data (`data/xp.ron`).
//!
//! Storage is DECI-XP (round 4): awards and multipliers compute in
//! tenths, so Learning's 10% of a 1-XP drip is a real 1 deci-XP and the
//! gain multipliers bite on every award instead of flooring to zero.

/// XP tuning (loaded from `data/xp.ron`). Docs tables read whole XP —
/// the human unit; deci fields here are explicit.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XpConfig {
    /// Level n costs `curve_base × n` MORE than the last (whole XP):
    /// cumulative 100/300/600/1000/1500 at the default 100.
    pub curve_base: u64,
    /// Level cap per track.
    pub level_cap: u32,

    // --- incomes (Q83 first-pass; deci-XP where the event is per-unit) ---
    /// Combat: +this per kill (whole XP; docs: 25).
    pub combat_kill_xp: u64,
    /// Flinch: per hostile-source flinch (whole XP; docs: 10).
    pub flinch_xp: u64,
    /// Age: deci-XP per tick survived (docs: 1 XP / 10 ticks = 1 deci).
    pub age_deci_per_tick: u64,
    /// Mileage: deci-XP per tile traveled (docs: 1 XP/tile = 10 deci).
    pub mileage_deci_per_tile: u64,
    /// Learning: percent of every OTHER track's post-multiplier award
    /// that feeds Learning (docs: 10%; never re-multiplied).
    pub learning_feed_pct: u64,
    /// Learning perk: +this percent XP gain per Learning level (docs: 5).
    pub learning_gain_pct_per_level: u64,

    // --- task-perk magnitudes (docs/02 table) ---
    /// Mining: +% mine yield per level (10).
    pub mining_yield_pct: u32,
    /// Mining L3: mine() action time −% (25).
    pub mining_l3_time_pct: u32,
    /// Hauling: +% cargo capacity per level (10).
    pub hauling_cargo_pct: u32,
    /// Hauling L3: +% move speed while loaded (10).
    pub hauling_l3_loaded_speed_pct: u32,
    /// Combat: +% damage per level (5).
    pub combat_damage_pct: u32,
    /// Building: +% build speed per level (10).
    pub building_speed_pct: u32,
    /// Building L3: repairs restore this % more (25).
    pub building_l3_repair_pct: u32,
    /// Scouting: +sensor tiles per level (1).
    pub scouting_sensors_per_level: u32,

    // --- body-perk magnitudes (docs/02 names them, gives NO numbers —
    // every figure below is a first-pass invention, NEEDS DISCUSSION) ---
    /// Age: +max HP per level.
    pub age_hp_per_level: i64,
    /// Age: +self-repair amount per level (on the regen tick).
    pub age_repair_per_level: i64,
    /// Mileage: −% move rate per level (worn-in bearings).
    pub mileage_move_pct: u32,
    /// Flinch: −% flinch duration per level.
    pub flinch_time_pct: u32,
    /// Boot: −% boot ritual per level.
    pub boot_time_pct: u32,

    /// Hiding: per detection episode OPENED against this bot (whole XP).
    pub hiding_episode_xp: u64,
    /// Scouting: per node discovered / per survey completed (whole XP).
    pub scouting_node_xp: u64,
    pub scouting_survey_xp: u64,

    // --- total-XP milestones (whole XP) ---
    /// Module slots: +1 at each threshold, in order (docs: 1000, 3000;
    /// cap 3 slots).
    pub slot_milestones: Vec<u64>,
    pub slot_cap: u32,
}

impl Default for XpConfig {
    fn default() -> Self {
        let xp: XpConfig = ron::from_str(include_str!("../data/xp.ron"))
            .expect("data/xp.ron parses (unknown fields are errors)");
        assert!(xp.curve_base > 0, "xp: curve_base must be > 0");
        assert!(xp.level_cap > 0, "xp: level_cap must be > 0");
        assert!(xp.learning_feed_pct <= 100, "xp: learning feed is a percent");
        xp
    }
}

impl XpConfig {
    /// Level for a track's deci-XP under the quadratic curve: level n
    /// costs `curve_base × n` more than the last, cap at `level_cap`.
    pub fn level(&self, deci_xp: u64) -> u32 {
        let mut level = 0u32;
        let mut cumulative = 0u64;
        while level < self.level_cap {
            cumulative += self.curve_base * (level as u64 + 1) * 10; // deci
            if deci_xp < cumulative {
                break;
            }
            level += 1;
        }
        level
    }

    /// A track's deci-XP ceiling (the L-cap boundary): awards clamp here —
    /// XP into a capped track still feeds Learning (docs/02).
    pub fn track_cap_deci(&self) -> u64 {
        let n = self.level_cap as u64;
        // Cumulative cost of levels 1..=cap: base × n(n+1)/2, in deci.
        self.curve_base * n * (n + 1) / 2 * 10
    }
}
