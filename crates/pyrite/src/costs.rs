//! The cycle-cost table (docs/01-language.md "Cycle Costs").
//!
//! Costs are data, not code: the base table lives in `data/costs.ron`
//! (baked in at compile time, parsed once at load); per-map/biome overlays
//! layer on later (docs/07). Every constant is a tuning value.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostTable {
    /// Simple statement / no-op line (also charged when the implicit program
    /// loop wraps back to line 1, so an empty program can't spin for free).
    pub statement: u64,
    /// Base cost of issuing any call (builtin function cost is added on top).
    pub call_base: u64,
    /// Variable assignment (`x = ...` total, reads are free).
    pub assign: u64,
    /// Per arithmetic operator (`+ - * // %`).
    pub arith: u64,
    /// Per comparison / boolean operator.
    pub compare: u64,
    /// `if` / `elif` arm evaluation (plus the condition's own costs).
    pub if_eval: u64,
    /// Loop iteration overhead — the "loop tax".
    pub loop_iter: u64,
    /// User function call overhead (`def`), on top of the body.
    pub user_call: u64,
    /// List literal / index / append.
    pub list_op: u64,
    /// Attribute access (`e.field`).
    pub attr: u64,
    /// Enum value construction.
    pub enum_ctor: u64,
    /// `match` dispatch base cost.
    pub match_base: u64,
    /// Per match arm checked.
    pub match_arm: u64,

    // --- Error & signal machinery ---
    /// Cost of `upload_crash_dump()` — force-called on unhandled faults.
    pub crash_dump: u64,
    /// Paid to enter an `on error:` handler instead of the crash dump.
    pub trap_cost: u64,
    /// Ticks an `on error:` handler runs at normal cost before overtime.
    pub grace_window_ticks: u32,
    /// Cost multiplier applied past the grace window.
    pub overtime_mult: u64,
    /// Hard cycle cap for `on death:` — the black-box budget.
    pub blackbox_budget: u64,

    /// Longest list `range()` may build in one call — a fault beyond it.
    /// Bounds the single-op allocation (one op can't conjure a megalist).
    pub range_cap: u64,

    /// Extra per-builtin costs (on top of `call_base`). Unlisted builtins
    /// cost `default_builtin`.
    pub builtins: BTreeMap<String, u64>,
    pub default_builtin: u64,
}

impl Default for CostTable {
    fn default() -> Self {
        let table: CostTable = ron::from_str(include_str!("../data/costs.ron"))
            .expect("data/costs.ron parses (unknown fields are errors)");
        // Load-time sanity: a zero statement cost would let empty programs
        // spin forever for free (the wrap charge is this value).
        assert!(table.statement > 0, "costs: statement must be > 0");
        assert!(table.range_cap > 0, "costs: range_cap must be > 0");
        table
    }
}

impl CostTable {
    pub fn builtin_cost(&self, name: &str) -> u64 {
        self.call_base + self.builtins.get(name).copied().unwrap_or(self.default_builtin)
    }
}
