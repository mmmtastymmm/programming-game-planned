//! The cycle-cost table (docs/01-language.md "Cycle Costs").
//!
//! Costs are data, not code: the sim loads a base table plus per-map/biome
//! overlays. Every constant here is a tuning value.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
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
        let mut builtins = BTreeMap::new();
        // Starter set from docs/01-language.md (values are "cost" minus the
        // call_base of 1, matching the "1 + function cost" convention).
        for (name, cost) in [
            ("move_to", 2),
            ("mine", 2),
            ("deposit", 1),
            ("wait", 1),
            ("rng", 1),
            ("closest", 3),
            ("exists", 1),
            // `.expect()` method: total = call_base alone.
            ("expect", 0),
            // Container builtins & methods (VM-level, not host calls).
            ("len", 0),
            ("range", 2),
            ("append", 0),
            ("get", 0),
            ("remove", 0),
            ("keys", 1),
            ("values", 1),
            // Priced here too now that the default error handler calls it
            // as ordinary code (the crash_dump field covers the bare-VM
            // forced-call fallback).
            ("upload_crash_dump", 25),
            ("build", 2),
            ("cargo_full", 1),
            ("health_low", 1),
            ("attack", 2),
            ("scan_enemies", 4),
            ("send", 3),
            ("try_send", 3),
            ("broadcast", 5),
            ("try_broadcast", 5),
            ("receive", 2),
            ("try_receive", 2),
            ("log", 1),
            ("upload_log", 5),
            ("last_error", 1),
            ("drop_cargo", 1),
            ("salvage", 2),
            ("recover_black_box", 2),
            ("become_disabled", 1),
        ] {
            builtins.insert(name.to_string(), cost);
        }
        Self {
            statement: 1,
            call_base: 1,
            assign: 1,
            arith: 1,
            compare: 1,
            if_eval: 1,
            loop_iter: 1,
            user_call: 2,
            list_op: 1,
            attr: 1,
            enum_ctor: 1,
            match_base: 1,
            match_arm: 1,
            crash_dump: 25,
            trap_cost: 5,
            grace_window_ticks: 10,
            overtime_mult: 2,
            blackbox_budget: 10,
            range_cap: 256,
            builtins,
            default_builtin: 1,
        }
    }
}

impl CostTable {
    pub fn builtin_cost(&self, name: &str) -> u64 {
        self.call_base + self.builtins.get(name).copied().unwrap_or(self.default_builtin)
    }
}
