//! The Pyrite VM: a resumable, cycle-metered stepper over the AST.
//!
//! Design (docs/01-language.md, docs/07-architecture.md):
//! - Explicit work stack instead of recursion, so execution can pause
//!   *anywhere*: mid-expression when the cycle budget runs out, or mid-call
//!   when an action blocks.
//! - An operation only executes once its full cost has accumulated
//!   ("cycle debt": ops costing more than the remaining budget wait).
//! - Programs loop forever; wrapping to line 1 clears variables (state must
//!   be re-derived each pass) and costs one statement (no free spinning).
//! - Unified fault path: any runtime failure either enters `on error:`
//!   (trap cost, variables preserved, overtime ×2 past the grace window) or
//!   force-calls `upload_crash_dump()` — then restarts from line 1.
//! - Double-handle rule: any signal or fault while a handler (or an
//!   engine interrupt context: boot/recall) is active destroys the bot.
//! - `on death:` runs under the hard black-box budget, then the engine
//!   force-calls `become_disabled()`.
//!
//! Determinism: no floats, no hash-ordered iteration (BTreeMap only), no
//! host time. All randomness must come from the Host.

use crate::ast::{BinOp, Expr, ExprId, Pattern, Program, SignalKind, Stmt, StmtId, UnOp};
use crate::costs::CostTable;
use crate::value::{EnumValue, Value};
use std::collections::BTreeMap;
use std::rc::Rc;

/// Execution context passed to every host call: enough for the host to
/// write crash dumps (line numbers) and serve `last_error()` without
/// reaching into the VM.
#[derive(Debug, Clone, Copy)]
pub struct CallCtx<'a> {
    pub line: u32,
    pub last_fault: Option<&'a str>,
}

/// The world-side interface. Builtins and entity attributes live here.
pub trait Host {
    /// Invoke a builtin. `Block` means an action was started: the VM parks
    /// until the sim calls [`Vm::resolve_action`].
    fn call(&mut self, name: &str, args: &[Value], ctx: CallCtx<'_>) -> HostCall;

    /// Entity attribute lookup (`bot.distance` style).
    fn attr(&mut self, entity: u64, name: &str) -> Result<Value, String> {
        let _ = entity;
        Err(format!("unknown attribute {name}"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostCall {
    Ready(Value),
    Block,
    Fault(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Cycle budget exhausted; call `grant` next tick and `run` again.
    Paused,
    /// Waiting on a world action; call `resolve_action` when it finishes.
    Blocked,
    /// Death handler finished (or no handler): `become_disabled()` was
    /// force-called. The bot is a wreck; the VM is inert.
    Dead,
    /// Double handle. The bot is destroyed instantly: no wreck, no black box.
    Exploded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiseOutcome {
    /// Handler entered; `run` will execute it.
    Handled,
    /// No handler installed for this signal; nothing happened.
    Ignored,
    /// Double handle: signal arrived while a handler / interrupt was active.
    Exploded,
    /// Death with no handler: `become_disabled()` force-called immediately.
    Died,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Hurt,
    Death,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Main,
    Handler(SignalKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Blocked,
    Dead,
    Exploded,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VmConfig {
    /// Max user-function call depth (base 4; +4 per Stack module).
    pub stack_depth: usize,
    /// Host-defined named constants (e.g. entity kinds: `ore`, `depot`).
    /// Read-only fallback below globals — assignments shadow them, and they
    /// survive the reset after a fault (globals don't).
    pub constants: BTreeMap<String, Value>,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self { stack_depth: 4, constants: BTreeMap::new() }
    }
}

/// One unit of pending execution. The VM pops exactly one per step, after
/// its cost has been paid.
#[derive(Debug, Clone, PartialEq)]
enum Work {
    Stmt(StmtId),
    Eval(ExprId),
    // Continuations (operate on the value stack):
    Binary(BinOp),
    Unary(UnOp),
    AndRhs(ExprId),
    OrRhs(ExprId),
    AssertBool,
    StoreVar(String),
    Discard,
    IfArm { stmt: StmtId, arm: usize },
    IfTest { stmt: StmtId, arm: usize },
    WhileIter { stmt: StmtId },
    WhileTest { stmt: StmtId },
    ForBegin { stmt: StmtId },
    ForIter { stmt: StmtId, items: Vec<Value>, idx: usize },
    MatchBegin { stmt: StmtId },
    MatchArm { stmt: StmtId, value: EnumValue, case: usize },
    CallExec { name: String, argc: usize, line: u32 },
    MethodExec { name: String, argc: usize, line: u32 },
    EnumCtorExec { enum_name: String, variant: String, argc: usize },
    MakeList { count: usize },
    IndexGet,
    AttrGet { name: String },
    ReturnNow,
    FrameEnd,
}

#[derive(Debug, Clone)]
struct Frame {
    locals: BTreeMap<String, Value>,
    val_base: usize,
}

#[derive(Debug)]
pub struct Vm {
    program: Rc<Program>,
    config: VmConfig,
    work: Vec<Work>,
    values: Vec<Value>,
    frames: Vec<Frame>,
    globals: BTreeMap<String, Value>,
    /// Accumulated cycles. Forced charges (trap cost, crash dump) may drive
    /// this negative — that is cycle debt, repaid before the next op runs.
    budget: i64,
    phase: Phase,
    /// Ticks spent in the current `on error:` handler (for the grace window).
    handler_ticks: u32,
    /// Remaining black-box budget while in the death handler.
    death_budget: i64,
    /// Set by the sim during Boot / Recall — an interrupt context for the
    /// double-handle rule.
    engine_interrupt: bool,
    state: State,
    current_line: u32,
    last_fault: Option<String>,
    /// Total faults so far (crash dumps AND handled traps) — lets the
    /// outside world observe fault *events*, not just the latest message.
    fault_count: u64,
    /// UNHANDLED faults only (crash-dump path). The sim charges chassis
    /// damage per crash — handlers are armor.
    crash_count: u64,
    /// A redeployed program, installed at the next loop boundary
    /// (docs/01: "redeploy takes effect at each bot's next loop boundary").
    pending_program: Option<Rc<Program>>,
}

impl Vm {
    pub fn new(program: Rc<Program>, config: VmConfig) -> Self {
        let work = Self::block_work(&program.body);
        Self {
            program,
            config,
            work,
            values: Vec::new(),
            frames: Vec::new(),
            globals: BTreeMap::new(),
            budget: 0,
            phase: Phase::Main,
            handler_ticks: 0,
            death_budget: 0,
            engine_interrupt: false,
            state: State::Running,
            current_line: 0,
            last_fault: None,
            fault_count: 0,
            crash_count: 0,
            pending_program: None,
        }
    }

    fn block_work(block: &[StmtId]) -> Vec<Work> {
        block.iter().rev().map(|&s| Work::Stmt(s)).collect()
    }

    // --- inspector-facing accessors ---

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn current_line(&self) -> u32 {
        self.current_line
    }

    pub fn last_fault(&self) -> Option<&str> {
        self.last_fault.as_deref()
    }

    /// Monotone count of faults (unhandled and handled alike).
    pub fn fault_count(&self) -> u64 {
        self.fault_count
    }

    /// Monotone count of UNHANDLED faults (those that crash-dumped).
    pub fn crash_count(&self) -> u64 {
        self.crash_count
    }

    /// Queue a redeployed program; it takes effect at the next loop
    /// boundary (natural wrap, or the restart after a fault/handler).
    pub fn queue_program(&mut self, program: Rc<Program>) {
        self.pending_program = Some(program);
    }

    pub fn budget(&self) -> i64 {
        self.budget
    }

    pub fn globals(&self) -> &BTreeMap<String, Value> {
        &self.globals
    }

    pub fn is_dead(&self) -> bool {
        self.state == State::Dead
    }

    pub fn is_blocked(&self) -> bool {
        self.state == State::Blocked
    }

    /// The hurt threshold this program wants (`on hurt(n):`), if any.
    /// The sim reads this to decide when to raise `Signal::Hurt`.
    pub fn hurt_threshold(&self) -> Option<i64> {
        self.program.handlers.get(&SignalKind::Hurt).and_then(|h| h.hurt_threshold)
    }

    // --- sim-facing control ---

    /// Grant this tick's cycles. Also advances the error-handler grace clock.
    pub fn grant(&mut self, cycles: u64) {
        self.budget = self.budget.saturating_add(cycles as i64);
        if self.phase == Phase::Handler(SignalKind::Error) {
            self.handler_ticks = self.handler_ticks.saturating_add(1);
        }
    }

    /// Mark the start/end of an engine interrupt context (Boot, Recall).
    /// While set, any signal or fault is a double handle.
    pub fn set_engine_interrupt(&mut self, active: bool) {
        self.engine_interrupt = active;
    }

    /// Full reset: line 1, variables cleared. Used by the sim at boot.
    /// A queued redeploy installs here — every reset is a loop boundary.
    pub fn reset(&mut self) {
        if let Some(program) = self.pending_program.take() {
            self.program = program;
        }
        self.work = Self::block_work(&self.program.body.clone());
        self.values.clear();
        self.frames.clear();
        self.globals.clear();
        self.phase = Phase::Main;
        self.handler_ticks = 0;
        if self.state != State::Dead && self.state != State::Exploded {
            self.state = State::Running;
        }
    }

    /// Resolve a blocking action started by a builtin.
    pub fn resolve_action(
        &mut self,
        result: Result<Value, String>,
        host: &mut dyn Host,
        costs: &CostTable,
    ) {
        if self.state != State::Blocked {
            return;
        }
        self.state = State::Running;
        match result {
            Ok(v) => self.values.push(v),
            Err(msg) => self.fault(msg, host, costs),
        }
    }

    /// Deliver an external signal (hurt / death).
    pub fn raise(
        &mut self,
        signal: Signal,
        host: &mut dyn Host,
        costs: &CostTable,
    ) -> RaiseOutcome {
        match self.state {
            State::Dead | State::Exploded => return RaiseOutcome::Ignored,
            State::Blocked => {
                // Handlers fire while blocked; the pending action is
                // abandoned (the sim should cancel it).
                self.state = State::Running;
            }
            State::Running => {}
        }
        if self.engine_interrupt || self.phase != Phase::Main {
            self.state = State::Exploded;
            return RaiseOutcome::Exploded;
        }
        let kind = match signal {
            Signal::Hurt => SignalKind::Hurt,
            Signal::Death => SignalKind::Death,
        };
        let Some(handler) = self.program.handlers.get(&kind) else {
            return match signal {
                Signal::Hurt => RaiseOutcome::Ignored,
                Signal::Death => {
                    // No death handler: straight to the forced call.
                    let _ = host.call("become_disabled", &[], self.ctx());
                    self.state = State::Dead;
                    RaiseOutcome::Died
                }
            };
        };
        let body = handler.body.clone();
        // Variables are preserved while a handler runs; work/values are not.
        self.work = Self::block_work(&body);
        self.values.clear();
        self.frames.clear();
        self.phase = Phase::Handler(kind);
        self.handler_ticks = 0;
        if kind == SignalKind::Death {
            self.death_budget = costs.blackbox_budget as i64;
        }
        RaiseOutcome::Handled
    }

    /// Step until the budget runs out, an action blocks, or the bot dies.
    pub fn run(&mut self, host: &mut dyn Host, costs: &CostTable) -> Outcome {
        loop {
            match self.state {
                State::Blocked => return Outcome::Blocked,
                State::Dead => return Outcome::Dead,
                State::Exploded => return Outcome::Exploded,
                State::Running => {}
            }

            let Some(top) = self.work.last() else {
                match self.phase {
                    Phase::Handler(SignalKind::Death) => {
                        self.finish_death(host);
                        return Outcome::Dead;
                    }
                    Phase::Handler(_) => {
                        // Handler completed: restart from line 1, full reset.
                        self.reset();
                        continue;
                    }
                    Phase::Main => {
                        // Implicit program loop: wrap to line 1. Costs one
                        // statement; variables do not survive the wrap.
                        let cost = self.adjusted(costs.statement, costs);
                        if self.budget < cost as i64 {
                            return Outcome::Paused;
                        }
                        self.budget -= cost as i64;
                        if let Some(program) = self.pending_program.take() {
                            self.program = program; // redeploy lands here
                        }
                        self.globals.clear();
                        self.values.clear();
                        self.frames.clear();
                        self.work = Self::block_work(&self.program.body.clone());
                        continue;
                    }
                }
            };

            let cost = self.adjusted(self.cost_of(top, costs), costs);
            if self.phase == Phase::Handler(SignalKind::Death) && self.death_budget < cost as i64 {
                // Black-box budget spent: the explosion doesn't wait.
                self.finish_death(host);
                return Outcome::Dead;
            }
            if self.budget < cost as i64 {
                return Outcome::Paused;
            }
            self.budget -= cost as i64;
            if self.phase == Phase::Handler(SignalKind::Death) {
                self.death_budget -= cost as i64;
            }

            let work = self.work.pop().expect("checked non-empty above");
            self.execute(work, host, costs);
        }
    }

    // --- internals ---

    fn adjusted(&self, cost: u64, costs: &CostTable) -> u64 {
        if self.phase == Phase::Handler(SignalKind::Error)
            && self.handler_ticks > costs.grace_window_ticks
        {
            cost * costs.overtime_mult
        } else {
            cost
        }
    }

    fn cost_of(&self, work: &Work, costs: &CostTable) -> u64 {
        match work {
            Work::Stmt(id) => match self.program.stmt(*id) {
                Stmt::Expr { .. } => costs.statement,
                Stmt::Assign { .. } => 0, // StoreVar charges `assign`
                Stmt::If { .. } => 0,     // IfArm charges per arm
                Stmt::While { .. } => 0,  // WhileIter charges per iteration
                Stmt::For { .. } => 0,    // ForIter charges per iteration
                Stmt::Break { .. } | Stmt::Continue { .. } => costs.statement,
                Stmt::Return { .. } => costs.statement,
                Stmt::Match { .. } => costs.match_base,
            },
            Work::Eval(_) => 0,
            Work::Binary(op) => {
                if op.is_arith() {
                    costs.arith
                } else {
                    costs.compare
                }
            }
            Work::Unary(UnOp::Neg) => costs.arith,
            Work::Unary(UnOp::Not) => costs.compare,
            Work::AndRhs(_) | Work::OrRhs(_) => costs.compare,
            Work::AssertBool => 0,
            Work::StoreVar(_) => costs.assign,
            Work::Discard => 0,
            Work::IfArm { .. } => costs.if_eval,
            Work::IfTest { .. } => 0,
            Work::WhileIter { .. } => costs.loop_iter,
            Work::WhileTest { .. } => 0,
            Work::ForBegin { .. } => 0,
            Work::ForIter { .. } => costs.loop_iter,
            Work::MatchBegin { .. } => 0,
            Work::MatchArm { .. } => costs.match_arm,
            Work::CallExec { name, .. } => {
                if self.program.functions.contains_key(name) {
                    costs.user_call
                } else {
                    costs.builtin_cost(name)
                }
            }
            Work::MethodExec { name, .. } => costs.builtin_cost(name),
            Work::EnumCtorExec { .. } => costs.enum_ctor,
            Work::MakeList { .. } => costs.list_op,
            Work::IndexGet => costs.list_op,
            Work::AttrGet { .. } => costs.attr,
            Work::ReturnNow => 0,
            Work::FrameEnd => 0,
        }
    }

    /// The unified fault path (docs/01-language.md "Errors & Signals").
    fn fault(&mut self, msg: String, host: &mut dyn Host, costs: &CostTable) {
        self.last_fault = Some(msg.clone());
        self.fault_count += 1;
        if self.engine_interrupt || self.phase != Phase::Main {
            // Double handle — including a fault inside `on death:`.
            self.state = State::Exploded;
            return;
        }
        if let Some(handler) = self.program.handlers.get(&SignalKind::Error) {
            // Trap: pay the (cheap) trap cost, run the handler as normal
            // code. Variables preserved; work/values cleared.
            let body = handler.body.clone();
            self.budget -= costs.trap_cost as i64;
            self.work = Self::block_work(&body);
            self.values.clear();
            self.frames.clear();
            self.phase = Phase::Handler(SignalKind::Error);
            self.handler_ticks = 0;
        } else {
            // Unhandled: the engine force-calls upload_crash_dump() — an
            // ordinary builtin, charged as cycle debt — then restarts.
            self.crash_count += 1;
            self.budget -= costs.crash_dump as i64;
            let ctx = CallCtx { line: self.current_line, last_fault: Some(msg.as_str()) };
            let _ = host.call("upload_crash_dump", &[Value::Str(msg.clone())], ctx);
            self.reset();
        }
    }

    fn finish_death(&mut self, host: &mut dyn Host) {
        // Every death exits through the forced ordinary function.
        let _ = host.call("become_disabled", &[], self.ctx());
        self.state = State::Dead;
    }

    fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(frame) = self.frames.last()
            && let Some(v) = frame.locals.get(name)
        {
            return Some(v.clone());
        }
        if let Some(v) = self.globals.get(name) {
            return Some(v.clone());
        }
        self.config.constants.get(name).cloned()
    }

    fn store(&mut self, name: String, value: Value) {
        if let Some(frame) = self.frames.last_mut() {
            frame.locals.insert(name, value);
        } else {
            self.globals.insert(name, value);
        }
    }

    fn pop_value(&mut self) -> Value {
        self.values.pop().expect("value stack underflow is a VM bug")
    }

    fn ctx(&self) -> CallCtx<'_> {
        CallCtx { line: self.current_line, last_fault: self.last_fault.as_deref() }
    }

    fn push_block(&mut self, block: &[StmtId]) {
        for &s in block.iter().rev() {
            self.work.push(Work::Stmt(s));
        }
    }

    fn execute(&mut self, work: Work, host: &mut dyn Host, costs: &CostTable) {
        let program = Rc::clone(&self.program);
        match work {
            Work::Stmt(id) => {
                let stmt = program.stmt(id).clone();
                self.current_line = stmt.line();
                match stmt {
                    Stmt::Expr { expr, .. } => {
                        self.work.push(Work::Discard);
                        self.work.push(Work::Eval(expr));
                    }
                    Stmt::Assign { name, value, .. } => {
                        self.work.push(Work::StoreVar(name));
                        self.work.push(Work::Eval(value));
                    }
                    Stmt::If { .. } => {
                        self.work.push(Work::IfArm { stmt: id, arm: 0 });
                    }
                    Stmt::While { .. } => {
                        self.work.push(Work::WhileIter { stmt: id });
                    }
                    Stmt::For { iter, .. } => {
                        self.work.push(Work::ForBegin { stmt: id });
                        self.work.push(Work::Eval(iter));
                    }
                    Stmt::Break { .. } => self.unwind_loop(true, host, costs),
                    Stmt::Continue { .. } => self.unwind_loop(false, host, costs),
                    Stmt::Return { value, .. } => {
                        if self.frames.is_empty() {
                            self.fault("return outside function".into(), host, costs);
                            return;
                        }
                        match value {
                            Some(expr) => {
                                self.work.push(Work::ReturnNow);
                                self.work.push(Work::Eval(expr));
                            }
                            None => {
                                self.values.push(Value::Unit);
                                self.work.push(Work::ReturnNow);
                            }
                        }
                    }
                    Stmt::Match { scrutinee, .. } => {
                        self.work.push(Work::MatchBegin { stmt: id });
                        self.work.push(Work::Eval(scrutinee));
                    }
                }
            }

            Work::Eval(id) => {
                let expr = program.expr(id).clone();
                match expr {
                    Expr::Int(v) => self.values.push(Value::Int(v)),
                    Expr::Str(s) => self.values.push(Value::Str(s)),
                    Expr::Bool(b) => self.values.push(Value::Bool(b)),
                    Expr::Name(name) => match self.lookup(&name) {
                        Some(v) => self.values.push(v),
                        None => {
                            self.fault(format!("read of unset variable '{name}'"), host, costs);
                        }
                    },
                    Expr::List(items) => {
                        self.work.push(Work::MakeList { count: items.len() });
                        for &item in items.iter().rev() {
                            self.work.push(Work::Eval(item));
                        }
                    }
                    Expr::Index { base, index } => {
                        self.work.push(Work::IndexGet);
                        self.work.push(Work::Eval(index));
                        self.work.push(Work::Eval(base));
                    }
                    Expr::Attr { base, name } => {
                        self.work.push(Work::AttrGet { name });
                        self.work.push(Work::Eval(base));
                    }
                    Expr::EnumUnit { enum_name, variant } => {
                        self.work.push(Work::EnumCtorExec { enum_name, variant, argc: 0 });
                    }
                    Expr::EnumCtor { enum_name, variant, args } => {
                        self.work.push(Work::EnumCtorExec { enum_name, variant, argc: args.len() });
                        for &arg in args.iter().rev() {
                            self.work.push(Work::Eval(arg));
                        }
                    }
                    Expr::Call { name, args, line } => {
                        self.current_line = line;
                        self.work.push(Work::CallExec { name, argc: args.len(), line });
                        for &arg in args.iter().rev() {
                            self.work.push(Work::Eval(arg));
                        }
                    }
                    Expr::MethodCall { base, name, args, line } => {
                        self.current_line = line;
                        self.work.push(Work::MethodExec { name, argc: args.len(), line });
                        for &arg in args.iter().rev() {
                            self.work.push(Work::Eval(arg));
                        }
                        self.work.push(Work::Eval(base));
                    }
                    Expr::Unary { op, operand } => {
                        self.work.push(Work::Unary(op));
                        self.work.push(Work::Eval(operand));
                    }
                    Expr::Binary { op: BinOp::And, lhs, rhs } => {
                        self.work.push(Work::AndRhs(rhs));
                        self.work.push(Work::Eval(lhs));
                    }
                    Expr::Binary { op: BinOp::Or, lhs, rhs } => {
                        self.work.push(Work::OrRhs(rhs));
                        self.work.push(Work::Eval(lhs));
                    }
                    Expr::Binary { op, lhs, rhs } => {
                        self.work.push(Work::Binary(op));
                        self.work.push(Work::Eval(rhs));
                        self.work.push(Work::Eval(lhs));
                    }
                }
            }

            Work::Binary(op) => {
                let rhs = self.pop_value();
                let lhs = self.pop_value();
                match binary_op(op, lhs, rhs) {
                    Ok(v) => self.values.push(v),
                    Err(msg) => self.fault(msg, host, costs),
                }
            }
            Work::Unary(op) => {
                let v = self.pop_value();
                let result = match (op, v) {
                    (UnOp::Neg, Value::Int(i)) => i
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| "integer overflow".to_string()),
                    (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnOp::Neg, other) => Err(format!("cannot negate {}", other.type_name())),
                    (UnOp::Not, other) => Err(format!("'not' requires bool, got {}", other.type_name())),
                };
                match result {
                    Ok(v) => self.values.push(v),
                    Err(msg) => self.fault(msg, host, costs),
                }
            }
            Work::AndRhs(rhs) => match self.pop_value() {
                Value::Bool(false) => self.values.push(Value::Bool(false)),
                Value::Bool(true) => {
                    self.work.push(Work::AssertBool);
                    self.work.push(Work::Eval(rhs));
                }
                other => self.fault(format!("'and' requires bool, got {}", other.type_name()), host, costs),
            },
            Work::OrRhs(rhs) => match self.pop_value() {
                Value::Bool(true) => self.values.push(Value::Bool(true)),
                Value::Bool(false) => {
                    self.work.push(Work::AssertBool);
                    self.work.push(Work::Eval(rhs));
                }
                other => self.fault(format!("'or' requires bool, got {}", other.type_name()), host, costs),
            },
            Work::AssertBool => {
                let v = self.pop_value();
                if matches!(v, Value::Bool(_)) {
                    self.values.push(v);
                } else {
                    self.fault(format!("boolean operator requires bool, got {}", v.type_name()), host, costs);
                }
            }
            Work::StoreVar(name) => {
                let v = self.pop_value();
                self.store(name, v);
            }
            Work::Discard => {
                self.pop_value();
            }

            Work::IfArm { stmt, arm } => {
                let Stmt::If { arms, .. } = program.stmt(stmt) else { unreachable!() };
                let (cond, _) = arms[arm];
                self.work.push(Work::IfTest { stmt, arm });
                self.work.push(Work::Eval(cond));
            }
            Work::IfTest { stmt, arm } => {
                let Stmt::If { arms, else_body, .. } = program.stmt(stmt) else { unreachable!() };
                match self.pop_value() {
                    Value::Bool(true) => {
                        let body = arms[arm].1.clone();
                        self.push_block(&body);
                    }
                    Value::Bool(false) => {
                        if arm + 1 < arms.len() {
                            self.work.push(Work::IfArm { stmt, arm: arm + 1 });
                        } else if let Some(body) = else_body.clone() {
                            self.push_block(&body);
                        }
                    }
                    other => {
                        self.fault(format!("condition must be bool, got {}", other.type_name()), host, costs);
                    }
                }
            }

            Work::WhileIter { stmt } => {
                let Stmt::While { cond, .. } = program.stmt(stmt) else { unreachable!() };
                self.work.push(Work::WhileTest { stmt });
                self.work.push(Work::Eval(*cond));
            }
            Work::WhileTest { stmt } => {
                let Stmt::While { body, .. } = program.stmt(stmt) else { unreachable!() };
                match self.pop_value() {
                    Value::Bool(true) => {
                        let body = body.clone();
                        self.work.push(Work::WhileIter { stmt });
                        self.push_block(&body);
                    }
                    Value::Bool(false) => {}
                    other => {
                        self.fault(format!("condition must be bool, got {}", other.type_name()), host, costs);
                    }
                }
            }

            Work::ForBegin { stmt } => match self.pop_value() {
                Value::List(items) => {
                    self.work.push(Work::ForIter { stmt, items, idx: 0 });
                }
                other => {
                    self.fault(format!("for-in requires a list, got {}", other.type_name()), host, costs);
                }
            },
            Work::ForIter { stmt, items, idx } => {
                let Stmt::For { var, body, .. } = program.stmt(stmt) else { unreachable!() };
                if idx < items.len() {
                    let item = items[idx].clone();
                    self.store(var.clone(), item);
                    let body = body.clone();
                    self.work.push(Work::ForIter { stmt, items, idx: idx + 1 });
                    self.push_block(&body);
                }
            }

            Work::MatchBegin { stmt } => match self.pop_value() {
                Value::Enum(e) => {
                    self.work.push(Work::MatchArm { stmt, value: e, case: 0 });
                }
                other => {
                    self.fault(format!("match requires an enum value, got {}", other.type_name()), host, costs);
                }
            },
            Work::MatchArm { stmt, value, case } => {
                let Stmt::Match { cases, .. } = program.stmt(stmt) else { unreachable!() };
                let Pattern::EnumVariant { enum_name, variant, binds } = &cases[case].pattern;
                if *enum_name == value.enum_name && *variant == value.variant {
                    if binds.len() != value.fields.len() {
                        self.fault(
                            format!(
                                "pattern {}.{} binds {} field(s), value has {}",
                                enum_name,
                                variant,
                                binds.len(),
                                value.fields.len()
                            ),
                            host,
                            costs,
                        );
                        return;
                    }
                    let binds = binds.clone();
                    let body = cases[case].body.clone();
                    for (name, field) in binds.into_iter().zip(value.fields) {
                        self.store(name, field);
                    }
                    self.push_block(&body);
                } else if case + 1 < cases.len() {
                    self.work.push(Work::MatchArm { stmt, value, case: case + 1 });
                } else {
                    self.fault(
                        format!("no case matched {}.{}", value.enum_name, value.variant),
                        host,
                        costs,
                    );
                }
            }

            Work::CallExec { name, argc, line } => {
                self.current_line = line;
                let args: Vec<Value> = self.values.split_off(self.values.len() - argc);
                if let Some(func) = program.functions.get(&name) {
                    if self.frames.len() >= self.config.stack_depth {
                        self.fault("stack overflow".into(), host, costs);
                        return;
                    }
                    if func.params.len() != args.len() {
                        self.fault(
                            format!(
                                "{name}() takes {} argument(s), got {}",
                                func.params.len(),
                                args.len()
                            ),
                            host,
                            costs,
                        );
                        return;
                    }
                    let mut locals = BTreeMap::new();
                    for (param, arg) in func.params.iter().zip(args) {
                        locals.insert(param.clone(), arg);
                    }
                    self.frames.push(Frame { locals, val_base: self.values.len() });
                    let body = func.body.clone();
                    self.work.push(Work::FrameEnd);
                    self.push_block(&body);
                } else {
                    let ctx = CallCtx { line: self.current_line, last_fault: self.last_fault.as_deref() };
                    match host.call(&name, &args, ctx) {
                        HostCall::Ready(v) => self.values.push(v),
                        HostCall::Block => self.state = State::Blocked,
                        HostCall::Fault(msg) => self.fault(msg, host, costs),
                    }
                }
            }

            Work::MethodExec { name, argc, line } => {
                self.current_line = line;
                let args: Vec<Value> = self.values.split_off(self.values.len() - argc);
                let base = self.pop_value();
                match (name.as_str(), base) {
                    ("expect", Value::Enum(e)) if e.enum_name == Value::RESULT_ENUM => {
                        if !args.is_empty() {
                            self.fault("expect() takes no arguments".into(), host, costs);
                        } else if e.variant == "Ok" {
                            let v = e.fields.into_iter().next().unwrap_or(Value::Unit);
                            self.values.push(v);
                        } else {
                            // Err: fault with the carried message.
                            let msg = match e.fields.first() {
                                Some(Value::Str(s)) => s.clone(),
                                Some(other) => other.to_string(),
                                None => "expect() on Result.Err".to_string(),
                            };
                            self.fault(msg, host, costs);
                        }
                    }
                    ("expect", other) => {
                        self.fault(
                            format!("expect() requires a Result, got {}", other.type_name()),
                            host,
                            costs,
                        );
                    }
                    (_, _) => {
                        self.fault(format!("unknown method {name}()"), host, costs);
                    }
                }
            }

            Work::EnumCtorExec { enum_name, variant, argc } => {
                let fields: Vec<Value> = self.values.split_off(self.values.len() - argc);
                self.values.push(Value::Enum(EnumValue { enum_name, variant, fields }));
            }

            Work::MakeList { count } => {
                let items: Vec<Value> = self.values.split_off(self.values.len() - count);
                self.values.push(Value::List(items));
            }

            Work::IndexGet => {
                let index = self.pop_value();
                let base = self.pop_value();
                match (base, index) {
                    (Value::List(items), Value::Int(i)) => {
                        let len = items.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            self.fault(format!("index {i} out of range (len {len})"), host, costs);
                        } else {
                            self.values.push(items[effective as usize].clone());
                        }
                    }
                    (base, index) => {
                        self.fault(
                            format!("cannot index {} with {}", base.type_name(), index.type_name()),
                            host,
                            costs,
                        );
                    }
                }
            }

            Work::AttrGet { name } => {
                let base = self.pop_value();
                match base {
                    Value::Entity(id) => match host.attr(id, &name) {
                        Ok(v) => self.values.push(v),
                        Err(msg) => self.fault(msg, host, costs),
                    },
                    Value::Enum(e) => {
                        // Named field access on enum values, resolved via the
                        // program's declaration when available.
                        let field = self
                            .program
                            .enums
                            .get(&e.enum_name)
                            .and_then(|decl| decl.variants.get(&e.variant))
                            .and_then(|fields| fields.iter().position(|f| *f == name))
                            .and_then(|idx| e.fields.get(idx).cloned());
                        match field {
                            Some(v) => self.values.push(v),
                            None => {
                                self.fault(
                                    format!("{}.{} has no field {name}", e.enum_name, e.variant),
                                    host,
                                    costs,
                                );
                            }
                        }
                    }
                    other => {
                        self.fault(
                            format!("cannot read attribute {name} of {}", other.type_name()),
                            host,
                            costs,
                        );
                    }
                }
            }

            Work::ReturnNow => {
                let value = self.pop_value();
                loop {
                    match self.work.pop() {
                        Some(Work::FrameEnd) => break,
                        Some(_) => {}
                        None => {
                            self.fault("return unwound past program root".into(), host, costs);
                            return;
                        }
                    }
                }
                let frame = self.frames.pop().expect("ReturnNow requires a frame");
                self.values.truncate(frame.val_base);
                self.values.push(value);
            }
            Work::FrameEnd => {
                let frame = self.frames.pop().expect("FrameEnd requires a frame");
                self.values.truncate(frame.val_base);
                self.values.push(Value::Unit);
            }
        }
    }

    /// `break` (pop the loop marker) / `continue` (stop at it).
    fn unwind_loop(&mut self, is_break: bool, host: &mut dyn Host, costs: &CostTable) {
        loop {
            match self.work.last() {
                Some(Work::WhileIter { .. }) | Some(Work::ForIter { .. }) => {
                    if is_break {
                        self.work.pop();
                    }
                    return;
                }
                Some(Work::FrameEnd) | None => {
                    let which = if is_break { "break" } else { "continue" };
                    self.fault(format!("{which} outside loop"), host, costs);
                    return;
                }
                Some(_) => {
                    self.work.pop();
                }
            }
        }
    }
}

fn binary_op(op: BinOp, lhs: Value, rhs: Value) -> Result<Value, String> {
    use BinOp::*;
    match op {
        Eq => Ok(Value::Bool(lhs == rhs)),
        NotEq => Ok(Value::Bool(lhs != rhs)),
        Add | Sub | Mul | FloorDiv | Mod | Lt | Gt | Le | Ge => {
            let (Value::Int(a), Value::Int(b)) = (&lhs, &rhs) else {
                return Err(format!(
                    "operator requires ints, got {} and {}",
                    lhs.type_name(),
                    rhs.type_name()
                ));
            };
            let (a, b) = (*a, *b);
            match op {
                Add => a.checked_add(b).map(Value::Int).ok_or_else(overflow),
                Sub => a.checked_sub(b).map(Value::Int).ok_or_else(overflow),
                Mul => a.checked_mul(b).map(Value::Int).ok_or_else(overflow),
                FloorDiv => {
                    if b == 0 {
                        Err("division by zero".to_string())
                    } else {
                        // Python-style floor division.
                        let q = a.checked_div(b).ok_or_else(overflow)?;
                        let r = a % b;
                        Ok(Value::Int(if r != 0 && (a < 0) != (b < 0) { q - 1 } else { q }))
                    }
                }
                Mod => {
                    if b == 0 {
                        Err("modulo by zero".to_string())
                    } else {
                        // Python-style: result has the sign of the divisor.
                        let r = a % b;
                        Ok(Value::Int(if r != 0 && (a < 0) != (b < 0) { r + b } else { r }))
                    }
                }
                Lt => Ok(Value::Bool(a < b)),
                Gt => Ok(Value::Bool(a > b)),
                Le => Ok(Value::Bool(a <= b)),
                Ge => Ok(Value::Bool(a >= b)),
                _ => unreachable!(),
            }
        }
        And | Or => unreachable!("short-circuit ops handled via AndRhs/OrRhs"),
    }
}

fn overflow() -> String {
    "integer overflow".to_string()
}
