//! The cycle-cost table and function registry (docs/01-language.md).
//!
//! Costs are data, not code: operation costs live in `data/costs.ron` and
//! the function registry — `name → (signature, cost, signal_safe, params,
//! doc)` (docs/01, Q80/round 4) — in `data/builtins.ron`, both baked in at
//! compile time and parsed once at load. Per-map/biome overlays layer on
//! later (docs/07). Every constant is a tuning value.
//!
//! **Table entries are FULL charges** (Q80): the figure listed for a
//! function is its total price — no call-base is added on top. Sized ops
//! (`send` 3 + payload, `upload_log` min(5+size, 25)) are priced by
//! [`CostSpec`].

use crate::value::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostTable {
    /// Simple statement / no-op line — and the implicit loop's wrap charge
    /// (no free spinning). NOT added to bare-call statements: a builtin
    /// call's table figure is its full price (Q80).
    pub statement: u64,
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
    /// Cost of `upload_crash_dump()` when force-called on a bare VM (the
    /// registry entry prices the player-callable form).
    pub crash_dump: u64,
    /// Paid to enter an `on error:` handler instead of the crash dump.
    pub trap_cost: u64,
    /// Hard cycle cap for `on death:` — the black-box budget.
    pub blackbox_budget: u64,

    /// Longest list `range()` may build in one call — a fault beyond it.
    pub range_cap: u64,
    /// Max payload units a sized op may carry — beyond it faults
    /// `err_payload` (Q82). Bounds every `+ payload` cost.
    pub payload_cap: u64,

    /// The function registry (loaded from `data/builtins.ron`). Unlisted
    /// builtins cost `default_builtin` and pass args through unchecked.
    #[serde(skip)]
    pub builtins: BTreeMap<String, BuiltinSpec>,
    pub default_builtin: u64,
}

/// How a registry function is priced.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub enum CostSpec {
    /// A flat total (the quoted price is the real price — Q80).
    Fixed(u64),
    /// `base + payload_units(args[payload_arg])`, bounded by `payload_cap`.
    PlusPayload { base: u64, payload_arg: usize },
    /// `min(base + <host's log-buffer length>, cap)` — `upload_log` (Q82).
    LogSized { base: u64, cap: u64 },
}

/// A declared parameter: required, or optional with a literal default
/// (docs/01: "optional parameters come last and are Python-style keyword
/// defaults").
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ParamSpec {
    pub name: String,
    #[serde(default)]
    pub default: Option<DefaultVal>,
}

/// The literal subset expressible as a registry default.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub enum DefaultVal {
    NoneVal,
    Int(i64),
    Bool(bool),
    Str(String),
}

impl DefaultVal {
    pub fn to_value(&self) -> Value {
        match self {
            DefaultVal::NoneVal => Value::option_none(),
            DefaultVal::Int(i) => Value::Int(*i),
            DefaultVal::Bool(b) => Value::Bool(*b),
            DefaultVal::Str(s) => Value::Str(s.clone()),
        }
    }
}

/// One registry entry: everything the VM, deploy checks, and the editor
/// need to know about a callable (docs/01: "part of its registry entry").
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinSpec {
    pub cost: CostSpec,
    /// May handler windows call this? (Enforced by M3's static checks;
    /// recorded per entry now — docs/01 signature convention.)
    pub signal_safe: bool,
    /// Declared parameters. `None` = passthrough: positional args go to the
    /// host unchecked and keywords are rejected (host-internal calls).
    #[serde(default)]
    pub params: Option<Vec<ParamSpec>>,
    /// Editor hover: rendered signature, e.g. "closest(kind) -> Result".
    pub signature: String,
    /// Editor hover: one-paragraph summary.
    pub summary: String,
    /// What the cost figure excludes, e.g. " + travel" (empty when flat).
    #[serde(default)]
    pub cost_note: String,
}

impl Default for CostTable {
    fn default() -> Self {
        let mut table: CostTable = ron::from_str(include_str!("../data/costs.ron"))
            .expect("data/costs.ron parses (unknown fields are errors)");
        table.builtins = ron::from_str(include_str!("../data/builtins.ron"))
            .expect("data/builtins.ron parses");
        // Load-time sanity: a zero statement cost would let empty programs
        // spin forever for free (the wrap charge is this value).
        assert!(table.statement > 0, "costs: statement must be > 0");
        assert!(table.range_cap > 0, "costs: range_cap must be > 0");
        assert!(table.payload_cap > 0, "costs: payload_cap must be > 0");
        for (name, spec) in &table.builtins {
            if let CostSpec::PlusPayload { payload_arg, .. } = spec.cost {
                let params = spec.params.as_ref().unwrap_or_else(|| {
                    panic!("builtins: {name} is payload-sized but declares no params")
                });
                assert!(payload_arg < params.len(), "builtins: {name} payload_arg out of range");
            }
        }
        table
    }
}

impl CostTable {
    /// Registry entry for a callable, if it has one.
    pub fn spec(&self, name: &str) -> Option<&BuiltinSpec> {
        self.builtins.get(name)
    }

    /// Full charge for a builtin call (Q80: the figure IS the total).
    /// `args` are the canonical (post-keyword-resolution) arguments;
    /// `log_len` is the host's log-buffer length (for `upload_log`).
    /// Payload contributions are clamped to `payload_cap` so the charge is
    /// bounded even for oversized payloads (which then fault `err_payload`).
    pub fn builtin_charge(&self, name: &str, args: &[Value], log_len: u64) -> u64 {
        match self.builtins.get(name).map(|s| &s.cost) {
            Some(CostSpec::Fixed(c)) => *c,
            Some(CostSpec::PlusPayload { base, payload_arg }) => {
                let units = args.get(*payload_arg).map_or(0, |v| v.payload_units());
                base + units.min(self.payload_cap)
            }
            Some(CostSpec::LogSized { base, cap }) => (base + log_len).min(*cap),
            None => self.default_builtin,
        }
    }

    /// Editor-facing cost text, e.g. "4", "3 + size", "min(5+size, 25)".
    pub fn cost_display(&self, name: &str) -> String {
        match self.builtins.get(name).map(|s| &s.cost) {
            Some(CostSpec::Fixed(c)) => c.to_string(),
            Some(CostSpec::PlusPayload { base, .. }) => format!("{base} + size"),
            Some(CostSpec::LogSized { base, cap }) => format!("min({base}+size, {cap})"),
            None => self.default_builtin.to_string(),
        }
    }
}
