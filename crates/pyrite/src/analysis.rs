//! Deploy-time static analysis of handler windows (docs/01 "Signal
//! handlers", M3). Three checks, all deploy-time rejections — nothing
//! here runs per-tick:
//!
//! 1. **Instruction caps**: a window's WORST-CASE statement count must fit
//!    its signal's cap (cost-table data). An instruction = one statement;
//!    nested builtin calls don't multiply the count; a user-`def` call
//!    charges its deploy-computed worst case (longest branch, calls
//!    expanded) — you can't smuggle a long function through a short window.
//! 2. **Signal safety**: windows may only call `signal_safe`-flagged
//!    registry functions; a `def` is safe iff everything it calls is safe.
//! 3. **Boundedness**: no loops, no recursion anywhere window-reachable —
//!    straight-line + `if`, all the way down (Q51). Boundedness is what
//!    makes "would this take too long?" a compile-time question.

use crate::ast::{Expr, ExprId, Program, SignalKind, Stmt, StmtId};
use crate::costs::CostTable;
use crate::error::{PyriteError, PyriteErrorKind};
use std::collections::BTreeMap;

/// Check every `on <signal>:` window in `program` against the caps and
/// the safety/boundedness rules. Called at deploy (and by the editor for
/// live squiggles); a clean parse that fails here is still a rejected
/// deploy.
pub fn check_windows(program: &Program, costs: &CostTable) -> Result<(), PyriteError> {
    for (kind, handler) in &program.handlers {
        let signal = kind.name();
        let mut walker = Walker {
            program,
            costs,
            signal,
            line: handler.line,
            worst_memo: BTreeMap::new(),
            visiting: Vec::new(),
        };
        let worst = walker.count_block(&handler.body)?;
        let cap = window_cap(costs, *kind);
        if worst > cap {
            return Err(PyriteError {
                line: handler.line,
                col: 1,
                kind: PyriteErrorKind::WindowOverCap { signal, worst, cap },
            });
        }
    }
    Ok(())
}

/// The per-signal window cap (docs/01's template table; cost-table data).
pub fn window_cap(costs: &CostTable, kind: SignalKind) -> u64 {
    match kind {
        SignalKind::Error => costs.window_cap_error,
        SignalKind::Hurt => costs.window_cap_hurt,
        SignalKind::Bump => costs.window_cap_bump,
        SignalKind::Bumped => costs.window_cap_bumped,
        SignalKind::Boot => costs.window_cap_boot,
    }
}

/// The editor's cap meter: (worst-case instruction count, cap) for a
/// written window. `None` when the program has no `on <signal>:` block for
/// `kind` or when analysis rejects it (the rejection itself surfaces as
/// the deploy error with details).
pub fn window_usage(
    program: &Program,
    costs: &CostTable,
    kind: SignalKind,
) -> Option<(u64, u64)> {
    let handler = program.handlers.get(&kind)?;
    let mut walker = Walker {
        program,
        costs,
        signal: kind.name(),
        line: handler.line,
        worst_memo: BTreeMap::new(),
        visiting: Vec::new(),
    };
    let worst = walker.count_block(&handler.body).ok()?;
    Some((worst, window_cap(costs, kind)))
}

/// Is `name` (a builtin or user def) callable from a window? Used by the
/// editor to grey out unsafe functions; the same derivation the checker
/// enforces. Errs on the side of "unsafe" for unknown names.
pub fn signal_safe(program: &Program, costs: &CostTable, name: &str) -> bool {
    let mut walker = Walker {
        program,
        costs,
        signal: "error",
        line: 0,
        worst_memo: BTreeMap::new(),
        visiting: Vec::new(),
    };
    if program.functions.contains_key(name) {
        return walker.def_worst_case(name).is_ok();
    }
    costs.spec(name).is_some_and(|s| s.signal_safe)
}

struct Walker<'a> {
    program: &'a Program,
    costs: &'a CostTable,
    signal: &'static str,
    line: u32,
    /// def name → worst-case instruction count (memoized across windows).
    worst_memo: BTreeMap<String, u64>,
    /// Call chain for recursion detection.
    visiting: Vec<String>,
}

impl Walker<'_> {
    /// Worst-case instruction count of a block: each statement is 1
    /// instruction, `if`/`match` take their LONGEST branch, and any
    /// user-def call adds that def's worst case on top of its statement.
    fn count_block(&mut self, block: &[StmtId]) -> Result<u64, PyriteError> {
        let mut total = 0u64;
        for &sid in block {
            total = total.saturating_add(self.count_stmt(sid)?);
        }
        Ok(total)
    }

    fn count_stmt(&mut self, sid: StmtId) -> Result<u64, PyriteError> {
        let stmt = self.program.stmt(sid).clone();
        match stmt {
            Stmt::While { line, .. } | Stmt::For { line, .. } => Err(PyriteError {
                line,
                col: 1,
                kind: PyriteErrorKind::WindowLoop { signal: self.signal },
            }),
            // break/continue can only appear inside loops, which are
            // already banned — but count them defensively.
            Stmt::Break { .. } | Stmt::Continue { .. } => Ok(1),
            Stmt::Expr { expr, .. } => {
                // A bare user-def call charges its worst case INSTEAD of
                // the plain statement figure (docs/01: "the one exception
                // is a user-def call, which charges its deploy-computed
                // worst case") — never less than one instruction.
                if let Expr::Call { name, .. } = self.program.expr(expr)
                    && self.program.functions.contains_key(name)
                {
                    return Ok(self.expr_calls(expr)?.max(1));
                }
                Ok(1u64.saturating_add(self.expr_calls(expr)?))
            }
            Stmt::Assign { value, .. } => Ok(1u64.saturating_add(self.expr_calls(value)?)),
            Stmt::IndexAssign { index, value, .. } => Ok(1u64
                .saturating_add(self.expr_calls(index)?)
                .saturating_add(self.expr_calls(value)?)),
            Stmt::Return { value, .. } => Ok(1u64.saturating_add(match value {
                Some(v) => self.expr_calls(v)?,
                None => 0,
            })),
            Stmt::If { arms, else_body, .. } => {
                // Worst case: every condition evaluated (their def calls
                // charge), plus the LONGEST branch body.
                let mut conds = 0u64;
                let mut longest = 0u64;
                for (cond, body) in &arms {
                    conds = conds.saturating_add(self.expr_calls(*cond)?);
                    longest = longest.max(self.count_block(body)?);
                }
                if let Some(body) = &else_body {
                    longest = longest.max(self.count_block(body)?);
                }
                Ok(1u64.saturating_add(conds).saturating_add(longest))
            }
            Stmt::Match { scrutinee, cases, .. } => {
                let mut longest = 0u64;
                for case in &cases {
                    longest = longest.max(self.count_block(&case.body)?);
                }
                Ok(1u64.saturating_add(self.expr_calls(scrutinee)?).saturating_add(longest))
            }
        }
    }

    /// The user-def worst cases (and safety checks) carried by an
    /// expression: builtin calls are checked for the signal_safe flag but
    /// add nothing to the count; user-def calls add their worst case.
    fn expr_calls(&mut self, eid: ExprId) -> Result<u64, PyriteError> {
        let expr = self.program.expr(eid).clone();
        match expr {
            Expr::Int(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Name(_) | Expr::EnumUnit { .. } => {
                Ok(0)
            }
            Expr::List(items) => {
                let mut sum = 0u64;
                for item in items {
                    sum = sum.saturating_add(self.expr_calls(item)?);
                }
                Ok(sum)
            }
            Expr::Dict(pairs) => {
                let mut sum = 0u64;
                for (k, v) in pairs {
                    sum = sum
                        .saturating_add(self.expr_calls(k)?)
                        .saturating_add(self.expr_calls(v)?);
                }
                Ok(sum)
            }
            Expr::Index { base, index } => Ok(self
                .expr_calls(base)?
                .saturating_add(self.expr_calls(index)?)),
            Expr::Attr { base, .. } => self.expr_calls(base),
            Expr::EnumCtor { args, .. } => {
                let mut sum = 0u64;
                for arg in args {
                    sum = sum.saturating_add(self.expr_calls(arg)?);
                }
                Ok(sum)
            }
            Expr::Unary { operand, .. } => self.expr_calls(operand),
            Expr::Binary { lhs, rhs, .. } => Ok(self
                .expr_calls(lhs)?
                .saturating_add(self.expr_calls(rhs)?)),
            Expr::MethodCall { base, args, .. } => {
                // Methods (.expect/.append/.get) are core language — exempt
                // from the acquisition rule and always signal-safe.
                let mut sum = self.expr_calls(base)?;
                for arg in args {
                    sum = sum.saturating_add(self.expr_calls(arg)?);
                }
                Ok(sum)
            }
            Expr::Call { name, args, kwargs, line } => {
                let mut sum = 0u64;
                for arg in args {
                    sum = sum.saturating_add(self.expr_calls(arg)?);
                }
                for (_, arg) in kwargs {
                    sum = sum.saturating_add(self.expr_calls(arg)?);
                }
                if self.program.functions.contains_key(&name) {
                    self.line = line;
                    sum = sum.saturating_add(self.def_worst_case(&name)?);
                } else {
                    // A builtin: nested calls don't multiply the count, but
                    // the safety flag gates them. Unknown names can't be
                    // proven safe — rejected.
                    let safe = self.costs.spec(&name).is_some_and(|s| s.signal_safe);
                    if !safe {
                        return Err(PyriteError {
                            line,
                            col: 1,
                            kind: PyriteErrorKind::WindowUnsafeCall {
                                signal: self.signal,
                                func: name,
                            },
                        });
                    }
                }
                Ok(sum)
            }
        }
    }

    /// Deploy-computed worst case of a user def (memoized), with the
    /// recursion ban: a cycle anywhere window-reachable is unbounded.
    fn def_worst_case(&mut self, name: &str) -> Result<u64, PyriteError> {
        if let Some(&wc) = self.worst_memo.get(name) {
            return Ok(wc);
        }
        if self.visiting.iter().any(|n| n == name) {
            return Err(PyriteError {
                line: self.line,
                col: 1,
                kind: PyriteErrorKind::WindowRecursion {
                    signal: self.signal,
                    func: name.to_string(),
                },
            });
        }
        let func = self.program.functions.get(name).expect("checked by caller");
        let body = func.body.clone();
        self.visiting.push(name.to_string());
        let wc = self.count_block(&body)?;
        self.visiting.pop();
        self.worst_memo.insert(name.to_string(), wc);
        Ok(wc)
    }
}
