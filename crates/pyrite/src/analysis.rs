//! Deploy-time static analysis of handler windows (docs/01 "Signal
//! handlers", M3). Three checks, all deploy-time rejections — nothing
//! here runs per-tick:
//!
//! 1. **Instruction caps**: a window's WORST-CASE instruction count must
//!    fit its signal's cap (cost-table data). An instruction = one
//!    statement or one builtin CALL (nested calls each count — a single
//!    statement stuffed with a hundred costed calls must not sail under a
//!    cap of 4; this deliberately tightens docs/01's "nested builtin calls
//!    don't multiply" line, flagged for doc reconciliation); a user-`def`
//!    call charges its deploy-computed worst case (longest branch, calls
//!    expanded) — you can't smuggle a long function through a short window.
//! 2. **Signal safety**: windows may only call `signal_safe`-flagged
//!    registry functions; a `def` is safe iff everything it calls is safe.
//! 3. **Boundedness**: no loops, no recursion anywhere window-reachable —
//!    straight-line + `if`, all the way down (Q51). Boundedness is what
//!    makes "would this take too long?" a compile-time question.

use crate::ast::{Block, Expr, ExprId, Pattern, Program, SignalKind, Stmt, StmtId};
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

/// One line's price in the editor's cost gutter.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LineCost {
    /// Cycles the line pays when it executes once (a bare builtin call is
    /// its full table figure — Q80; other statements sum their ops).
    pub cycles: u64,
    /// True when a payload-/log-sized op makes the real charge bigger —
    /// the gutter renders a trailing `+`.
    pub variable: bool,
}

/// The editor's per-line cycle-cost gutter (M5 — docs/01 asks for a
/// gutter, not hover-only). Deliberately approximate where the truth is
/// dynamic: branch and loop LINES charge their own dispatch (bodies
/// charge their own lines), sized ops report their base with the
/// `variable` flag, and a `match` line charges dispatch + scrutinee.
pub fn line_costs(program: &Program, costs: &CostTable) -> BTreeMap<u32, LineCost> {
    let mut out: BTreeMap<u32, LineCost> = BTreeMap::new();
    let mut blocks: Vec<&[StmtId]> = vec![&program.body];
    for f in program.functions.values() {
        blocks.push(&f.body);
    }
    for h in program.handlers.values() {
        blocks.push(&h.body);
    }
    for block in blocks {
        gutter_block(program, costs, block, &mut out);
    }
    out
}

fn gutter_block(
    program: &Program,
    costs: &CostTable,
    block: &[StmtId],
    out: &mut BTreeMap<u32, LineCost>,
) {
    for &sid in block {
        let stmt = program.stmt(sid);
        let mut cost = LineCost::default();
        let add_expr = |cost: &mut LineCost, eid: ExprId| {
            let (c, var) = gutter_expr(program, costs, eid);
            cost.cycles += c;
            cost.variable |= var;
        };
        match stmt {
            Stmt::Expr { expr, .. } => {
                if matches!(program.expr(*expr), Expr::Call { .. }) {
                    // A bare call IS the statement (Q80: full charge).
                    add_expr(&mut cost, *expr);
                } else {
                    cost.cycles += costs.statement;
                    add_expr(&mut cost, *expr);
                }
            }
            Stmt::Assign { value, .. } => {
                cost.cycles += costs.assign;
                add_expr(&mut cost, *value);
            }
            Stmt::IndexAssign { index, value, .. } => {
                cost.cycles += costs.assign + costs.list_op;
                add_expr(&mut cost, *index);
                add_expr(&mut cost, *value);
            }
            Stmt::If { arms, else_body, .. } => {
                cost.cycles += costs.if_eval;
                if let Some((cond, _)) = arms.first() {
                    add_expr(&mut cost, *cond);
                }
                for (_, body) in arms {
                    gutter_block(program, costs, body, out);
                }
                if let Some(body) = else_body {
                    gutter_block(program, costs, body, out);
                }
            }
            Stmt::While { cond, body, .. } => {
                cost.cycles += costs.loop_iter;
                add_expr(&mut cost, *cond);
                gutter_block(program, costs, body, out);
            }
            Stmt::For { iter, body, .. } => {
                cost.cycles += costs.loop_iter;
                add_expr(&mut cost, *iter);
                gutter_block(program, costs, body, out);
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => cost.cycles += costs.statement,
            Stmt::Return { value, .. } => {
                cost.cycles += costs.statement;
                if let Some(v) = value {
                    add_expr(&mut cost, *v);
                }
            }
            Stmt::Match { scrutinee, cases, .. } => {
                cost.cycles += costs.match_base;
                add_expr(&mut cost, *scrutinee);
                for case in cases {
                    gutter_block(program, costs, &case.body, out);
                }
            }
        }
        let entry = out.entry(stmt.line()).or_default();
        entry.cycles += cost.cycles;
        entry.variable |= cost.variable;
    }
}

/// (cycles, variable?) an expression pays when evaluated once.
fn gutter_expr(program: &Program, costs: &CostTable, eid: ExprId) -> (u64, bool) {
    let mut cycles = 0u64;
    let mut variable = false;
    let mut add = |c: u64| cycles = cycles.saturating_add(c);
    match program.expr(eid) {
        Expr::Int(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Name(_) | Expr::EnumUnit { .. } => {}
        Expr::List(items) => {
            add(costs.list_op);
            for &i in items {
                let (c, v) = gutter_expr(program, costs, i);
                add(c);
                variable |= v;
            }
        }
        Expr::Dict(pairs) => {
            add(costs.list_op);
            for &(k, v) in pairs {
                let (ck, vk) = gutter_expr(program, costs, k);
                let (cv, vv) = gutter_expr(program, costs, v);
                add(ck + cv);
                variable |= vk | vv;
            }
        }
        Expr::Index { base, index } => {
            add(costs.list_op);
            for &e in [base, index].iter() {
                let (c, v) = gutter_expr(program, costs, *e);
                add(c);
                variable |= v;
            }
        }
        Expr::Attr { base, .. } => {
            add(costs.attr);
            let (c, v) = gutter_expr(program, costs, *base);
            add(c);
            variable |= v;
        }
        Expr::EnumCtor { args, .. } => {
            add(costs.enum_ctor);
            for &a in args {
                let (c, v) = gutter_expr(program, costs, a);
                add(c);
                variable |= v;
            }
        }
        Expr::Unary { operand, .. } => {
            add(costs.arith);
            let (c, v) = gutter_expr(program, costs, *operand);
            add(c);
            variable |= v;
        }
        Expr::Binary { op, lhs, rhs } => {
            add(if op.is_arith() { costs.arith } else { costs.compare });
            for &e in [lhs, rhs].iter() {
                let (c, v) = gutter_expr(program, costs, *e);
                add(c);
                variable |= v;
            }
        }
        Expr::MethodCall { name, base, args, .. } => {
            add(costs.spec(name).map_or(costs.default_builtin, |s| match &s.cost {
                crate::costs::CostSpec::Fixed(c) => *c,
                _ => costs.default_builtin,
            }));
            let (c, v) = gutter_expr(program, costs, *base);
            add(c);
            variable |= v;
            for &a in args {
                let (c, v) = gutter_expr(program, costs, a);
                add(c);
                variable |= v;
            }
        }
        Expr::Call { name, args, kwargs, .. } => {
            if program.functions.contains_key(name) {
                add(costs.user_call);
            } else {
                match costs.spec(name).map(|s| &s.cost) {
                    Some(crate::costs::CostSpec::Fixed(c)) => add(*c),
                    Some(crate::costs::CostSpec::PlusPayload { base, .. }) => {
                        add(*base);
                        variable = true;
                    }
                    Some(crate::costs::CostSpec::LogSized { base, .. }) => {
                        add(*base);
                        variable = true;
                    }
                    None => add(costs.default_builtin),
                }
            }
            for &a in args {
                let (c, v) = gutter_expr(program, costs, a);
                add(c);
                variable |= v;
            }
            for (_, a) in kwargs {
                let (c, v) = gutter_expr(program, costs, *a);
                add(c);
                variable |= v;
            }
        }
    }
    (cycles, variable)
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
                // A bare call IS the statement: the call's own weight (a
                // builtin = 1; a user def = its worst case) covers the
                // statement figure — never less than one instruction.
                if let Expr::Call { .. } = self.program.expr(expr) {
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

    /// The instruction weight carried by an expression's CALLS: every
    /// builtin call counts as one instruction (nesting a hundred costed
    /// calls into one statement must not sail under a cap of 4 — with the
    /// overtime tax gone, this counter is the only bound on window work);
    /// user-def calls add their deploy-computed worst case. Both are
    /// checked for signal safety. Call-free expressions weigh nothing.
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
                    // A builtin: the safety flag gates it, and the call
                    // itself weighs one instruction wherever it nests.
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
                    sum = sum.saturating_add(1);
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

/// The deployed artifact's hardware requirements (M9, Q52), derived from
/// the PARSED program, not the raw source:
///
/// - **Program memory in LINES** = the count of distinct source lines
///   that carry a statement. Blank lines, comments, docstrings, and
///   import lines never carry one, so they don't count — docs/01: a
///   docstring is "stripped from the runtime body (free — doesn't exist
///   at runtime)", and program memory is "code is code" (Q61). Body,
///   handlers, and functions all count (code is code).
/// - **Variable slots** = distinct TOP-LEVEL names the program binds
///   (assignment targets, top-level loop vars, match binds). Per docs/01
///   Q80 `def` bodies are frame-local — parameters and names first
///   assigned inside a `def` live on the call stack (bounded by the
///   stack-depth stat) and never occupy a global slot — so functions
///   contribute lines but not names.
///
/// A printer claims only bots whose bought hardware meets both figures.
pub fn artifact_requirements(_source: &str, program: &Program) -> (u32, u32) {
    use std::collections::BTreeSet;
    let mut lines: BTreeSet<u32> = BTreeSet::new();
    let mut names: BTreeSet<String> = BTreeSet::new();

    // Walk a block, collecting every statement's line; collect binding
    // names only when `top_level` (globals — not inside a `def`). Nested
    // control flow inside the body/handlers is still top-level scope.
    fn walk(
        program: &Program,
        block: &Block,
        lines: &mut BTreeSet<u32>,
        names: &mut BTreeSet<String>,
        top_level: bool,
    ) {
        let mut work: Vec<StmtId> = block.iter().rev().copied().collect();
        while let Some(id) = work.pop() {
            let stmt = program.stmt(id);
            lines.insert(stmt.line());
            match stmt {
                Stmt::Assign { name, .. } | Stmt::IndexAssign { name, .. } => {
                    if top_level {
                        names.insert(name.clone());
                    }
                }
                Stmt::For { var, body, .. } => {
                    if top_level {
                        names.insert(var.clone());
                    }
                    work.extend(body.iter().rev().copied());
                }
                Stmt::If { arms, else_body, .. } => {
                    for (_, body) in arms {
                        work.extend(body.iter().rev().copied());
                    }
                    if let Some(body) = else_body {
                        work.extend(body.iter().rev().copied());
                    }
                }
                Stmt::While { body, .. } => work.extend(body.iter().rev().copied()),
                Stmt::Match { cases, .. } => {
                    for case in cases {
                        if top_level
                            && let Pattern::EnumVariant { binds, .. } = &case.pattern
                        {
                            for b in binds {
                                names.insert(b.clone());
                            }
                        }
                        work.extend(case.body.iter().rev().copied());
                    }
                }
                Stmt::Expr { .. } | Stmt::Break { .. } | Stmt::Continue { .. }
                | Stmt::Return { .. } => {}
            }
        }
    }

    walk(program, &program.body, &mut lines, &mut names, true);
    for handler in program.handlers.values() {
        walk(program, &handler.body, &mut lines, &mut names, true);
    }
    for function in program.functions.values() {
        // Frame-local: lines count (code is code), names do not.
        walk(program, &function.body, &mut lines, &mut names, false);
    }
    (lines.len() as u32, names.len() as u32)
}
