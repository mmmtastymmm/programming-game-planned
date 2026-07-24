//! The Pyrite VM: a resumable, cycle-metered stepper over the AST.
//!
//! Design (docs/01-language.md, docs/07-architecture.md):
//! - Explicit work stack instead of recursion, so execution can pause
//!   *anywhere*: mid-expression when the cycle budget runs out, or mid-call
//!   when an action blocks.
//! - An operation only executes once its full cost has accumulated
//!   ("cycle debt": ops costing more than the remaining budget wait).
//!   Budgets are STORED in centicycles (×100, Q56/Q75) — table entries stay
//!   whole cycles, converted at charge time — and builtin table entries are
//!   FULL charges (Q80): the figure is the total price of the call.
//! - Programs loop forever; the wrap to line 1 costs one statement (no free
//!   spinning) and variables SURVIVE it (Q80) — only fault/handler restarts
//!   clear them, which is exactly when state might be corrupted.
//! - Unified fault path: any runtime failure carries a fault-id constant
//!   (`err_type`, `err_action`, … — see [`crate::error::faults`]) and
//!   either enters `on error:` (trap cost, variables preserved) or
//!   force-calls `upload_crash_dump()` — then restarts from line 1.
//! - Double-handle rule: any signal or fault while a template (or an
//!   engine interrupt context: boot/recall) is active forces abort — the
//!   reserved scuttle: forced `upload_log()` charged as debt, then the
//!   engine force-calls `become_disabled()`.
//!
//! Determinism: no floats, no hash-ordered iteration (BTreeMap only), no
//! host time. All randomness must come from the Host.

use crate::ast::{BinOp, DefaultLit, Expr, ExprId, Pattern, Program, SignalKind, Stmt, StmtId, UnOp};
use crate::costs::{CostSpec, CostTable};
use crate::error::faults;
use crate::value::{DictKey, EnumValue, Value};
use std::collections::BTreeMap;
use std::rc::Rc;

/// Centicycles per cycle: budgets/debt are stored fine-grained (Q56/Q75)
/// so percent effects (brownout −50%) bite even a 1-cycle CPU.
const CENT: i64 = 100;

/// A runtime fault: a pre-bound, `==`-comparable identity constant
/// (returned by `last_error()`) plus the human-facing message (carried by
/// `Signal.Error(msg)` and crash dumps). See [`crate::error::faults`].
#[derive(Debug, Clone, PartialEq)]
pub struct Fault {
    pub id: &'static str,
    pub msg: String,
}

impl Fault {
    pub fn new(id: &'static str, msg: impl Into<String>) -> Self {
        Self { id, msg: msg.into() }
    }
}

/// Execution context passed to every host call: enough for the host to
/// write crash dumps (line numbers) and serve `last_error()` without
/// reaching into the VM.
#[derive(Debug, Clone, Copy)]
pub struct CallCtx<'a> {
    pub line: u32,
    /// Most recent fault's message (for crash-dump prose).
    pub last_fault: Option<&'a str>,
    /// Most recent fault's identity constant (what `last_error()` returns).
    pub last_fault_id: Option<&'static str>,
}

/// The world-side interface. Builtins and entity attributes live here.
pub trait Host {
    /// Invoke a builtin. `Block` means an action was started: the VM parks
    /// until the sim calls [`Vm::resolve_action`].
    fn call(&mut self, name: &str, args: &[Value], ctx: CallCtx<'_>) -> HostCall;

    /// Entity attribute lookup (`bot.distance` style). Errors carry the
    /// fault id so the host can distinguish an unknown property
    /// (`err_name`) from a read through a contact it can't see
    /// (`err_unknown_contact`).
    fn attr(&mut self, entity: u64, name: &str) -> Result<Value, (&'static str, String)> {
        let _ = entity;
        Err((faults::NAME, format!("unknown attribute {name}")))
    }

    /// Current log-buffer length, for `upload_log`'s sized cost
    /// (min(base + size, cap) — Q82).
    fn log_len(&self) -> u64 {
        0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HostCall {
    Ready(Value),
    Block,
    Fault(Fault),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Cycle budget exhausted; call `grant` next tick and `run` again.
    Paused,
    /// Waiting on a world action; call `resolve_action` when it finishes.
    Blocked,
    /// Abort completed: `upload_log()` + `become_disabled()` were
    /// force-called. The bot is a wreck; the VM is inert. There is no
    /// instant-destroy outcome — every downed bot exits through abort
    /// (docs/01: explosion is only the wreck countdown expiring).
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiseOutcome {
    /// Template entered; `run` will execute it.
    Handled,
    /// The VM is already inert (dead) — nothing happened.
    Ignored,
    /// The signal was (or forced) an abort: the reserved `upload_log()` +
    /// `become_disabled()` sequence ran to completion. The bot is a wreck.
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// HP crossed the `hurt_line`.
    Hurt,
    /// This bot rammed something.
    Bump,
    /// Something rammed this bot.
    Bumped,
    /// Printer rebalancing / over-capacity scrap. Fully engine-reserved:
    /// the sim drives the walk home; the VM only records the interrupt
    /// context (double-handle applies all the way home).
    Recall,
    /// HP hit 0, a deliberate `abort()` call, or a double-handle. Fully
    /// engine-reserved: the forced sequence always completes and cannot
    /// itself be interrupted.
    Abort,
}

impl Signal {
    /// Co-arrival severity (docs/01, Q81): signals landing at one op
    /// boundary resolve by **abort > error > recall > hurt > bumped >
    /// bump** — the highest enters its template, the rest are dropped,
    /// and co-arrival is NOT a double-handle (that needs a template
    /// already *running*). `error` is synchronous (raised inside the op,
    /// never queued) — its rank, 5, sits between abort and recall.
    pub fn severity(self) -> u8 {
        match self {
            Signal::Abort => 6,
            Signal::Recall => 4,
            Signal::Hurt => 3,
            Signal::Bumped => 2,
            Signal::Bump => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Main,
    /// Inside a signal's reserved template (prologue or window — the five
    /// player-window signals only; abort runs synchronously and recall is
    /// an engine context, so neither appears here).
    Template(SignalKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Running,
    Blocked,
    Dead,
}

/// The engine-driven interrupt contexts (docs/07's run states that live
/// outside the VM's own execution): set by the sim, participating in the
/// double-handle rule exactly like a running template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineCtx {
    /// Boot countdown / boot template pending.
    Boot,
    /// The recall walk home.
    Recall,
    /// Sitting on an Upgrade-Station pad (lands with M5).
    PadSit,
}

/// The public run-state view — docs/07's shape. A projection over the
/// VM's internals for the sim, tests, and the renderer's thought clouds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    /// Inside the error template (docs/07 lists `Faulted { error }`
    /// separately from the other templates; the payload is `last_error`).
    Faulted,
    /// Parked on a world action (the channel variant lands with M11).
    Blocked,
    /// Inside a non-error signal template. `flinching` = still in the
    /// forced `handler_init()` prologue.
    Template { signal: SignalKind, flinching: bool },
    Boot,
    /// The recall walk home (engine-owned).
    Recall,
    PadSit,
    /// Aborted: the VM is inert, the bot is a wreck.
    Disabled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VmConfig {
    /// Max user-function call depth (base 4; +4 per Stack module).
    pub stack_depth: usize,
    /// Host-defined named constants (e.g. entity kinds: `ore`, `depot`).
    /// Read-only fallback below globals — assignments shadow them, and they
    /// survive the reset after a fault (globals don't).
    pub constants: BTreeMap<String, Value>,
    /// FACTORY WINDOW contents per signal (docs/01): the engine default
    /// that fills a window the player hasn't written, as REAL Pyrite —
    /// visible, line-highlighted, costed, replaceable. A missing entry
    /// means an empty factory window: the template still runs (prologue
    /// flinch included); there is just nothing in the middle.
    pub default_handlers: BTreeMap<SignalKind, DefaultHandler>,
}

/// A factory window: its source (for inspectors) and parsed program.
#[derive(Debug, Clone)]
pub struct DefaultHandler {
    pub source: String,
    pub program: Rc<Program>,
}

impl PartialEq for DefaultHandler {
    fn eq(&self, other: &Self) -> bool {
        // Source is canonical (the program is derived from it).
        self.source == other.source
    }
}

impl Default for VmConfig {
    fn default() -> Self {
        // Fault-id constants are pre-bound in every VM (docs/01, Q80):
        // `if last_error() == err_payload:` works without host setup.
        let mut constants = BTreeMap::new();
        for id in faults::ALL {
            constants.insert(id.to_string(), Value::Str(id.to_string()));
        }
        Self { stack_depth: 4, constants, default_handlers: BTreeMap::new() }
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
    /// `kwargs` are the keyword names, in source order; their values sit on
    /// the value stack after the `argc` positionals.
    CallExec { name: String, argc: usize, kwargs: Vec<String>, line: u32 },
    /// Marker popped when the forced handler_init() completes — flips the
    /// VM out of "entry ritual" state (inspector visibility).
    InitDone,
    /// `base_name` is the variable the base expression read, when it was a
    /// bare name — mutating methods (`xs.append(v)`) write back through it
    /// (containers are values; mutation is name-rooted).
    MethodExec { name: String, argc: usize, line: u32, base_name: Option<String> },
    EnumCtorExec { enum_name: String, variant: String, argc: usize },
    MakeList { count: usize },
    MakeDict { count: usize },
    IndexGet,
    /// `name[index] = value` — pops value then index, mutates the variable.
    IndexSet { name: String },
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
    /// The program the work stack currently indexes into: the main program,
    /// or a default handler's program while one runs.
    active: Rc<Program>,
    config: VmConfig,
    work: Vec<Work>,
    values: Vec<Value>,
    frames: Vec<Frame>,
    globals: BTreeMap<String, Value>,
    /// Accumulated cycles. Forced charges (trap cost, crash dump) may drive
    /// this negative — that is cycle debt, repaid before the next op runs.
    budget: i64,
    phase: Phase,
    /// Is the running window FACTORY contents? Factory code is not armor:
    /// faults there still count as crashes, and (like any template code)
    /// a signal landing on it is a double-handle → abort (Q50 — the old
    /// humble-defaults carve-out is gone).
    handler_is_default: bool,
    /// True from template entry until the forced handler_init() resolves —
    /// the visible flinch window.
    handler_init_active: bool,
    /// Set by the sim during Boot / the recall walk / a pad-sit — an
    /// engine-owned interrupt context for the double-handle rule.
    engine_ctx: Option<EngineCtx>,
    /// The abort sequence ran (the distinct skull-cloud tell, and how the
    /// sim distinguishes an aborted wreck from a dev-killed one).
    aborted: bool,
    state: State,
    current_line: u32,
    last_fault: Option<String>,
    /// The most recent fault's identity constant (see error::faults).
    last_fault_id: Option<&'static str>,
    /// Total faults so far (crash dumps AND handled traps) — lets the
    /// outside world observe fault *events*, not just the latest message.
    fault_count: u64,
    /// UNHANDLED faults only (crash-dump path). The sim charges chassis
    /// damage per crash — handlers are armor.
    crash_count: u64,
    /// A redeployed program, installed at the next loop boundary
    /// (docs/01: "redeploy takes effect at each bot's next loop boundary").
    pending_program: Option<Rc<Program>>,
    /// Flat per-op surcharge in CENTICYCLES, set by the sim every tick
    /// from world state (M8: Corruption's compute tax). Applies to every
    /// op the table charges for — zero-cost bookkeeping stays free — and
    /// the charged figure never drops below one full cycle (the floor
    /// guards a future negative overlay, not the tax). Derived state:
    /// re-set before every grant, so replays never need it persisted.
    cost_overlay_centi: i64,
    /// The charged price (centicycles) of the op `run` last stopped short
    /// of affording — `Some` exactly when the last `run` returned
    /// `Outcome::Paused`. Purely observational: the UI pairs it with
    /// `budget` to show a bot saving up. Cleared on entry to `run`, so a
    /// bot that goes Blocked or Dead reports nothing. Never hashed — it is
    /// derived from state the replay already covers.
    stall_centi: Option<i64>,
}

impl Vm {
    pub fn new(program: Rc<Program>, config: VmConfig) -> Self {
        let work = Self::block_work(&program.body);
        Self {
            active: Rc::clone(&program),
            program,
            config,
            work,
            values: Vec::new(),
            frames: Vec::new(),
            globals: BTreeMap::new(),
            budget: 0,
            phase: Phase::Main,
            handler_is_default: false,
            handler_init_active: false,
            engine_ctx: None,
            aborted: false,
            state: State::Running,
            current_line: 0,
            last_fault: None,
            last_fault_id: None,
            fault_count: 0,
            crash_count: 0,
            pending_program: None,
            cost_overlay_centi: 0,
            stall_centi: None,
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

    /// The most recent fault's identity constant (`err_type`, ...).
    pub fn last_fault_id(&self) -> Option<&'static str> {
        self.last_fault_id
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

    /// Banked budget in CENTICYCLES (Q56: storage is ×100; divide by 100
    /// for whole cycles in displays).
    pub fn budget(&self) -> i64 {
        self.budget
    }

    /// The charged price (CENTICYCLES) of the op this VM is saving up for,
    /// or `None` when it is not cycle-starved (running, blocked, or dead).
    /// Pair with `budget` for a "cycles until the next op" meter: a bot
    /// with 43 of 200 is three ticks from moving on a stock CPU.
    ///
    /// Blocked is deliberately NOT a stall — a bot waiting on an action
    /// burns its grant rather than banking it (see `grant_centi`), so a
    /// fill bar there would creep toward an execution that isn't coming.
    pub fn stall_cost(&self) -> Option<i64> {
        self.stall_centi
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

    /// Is the currently running window FACTORY contents?
    pub fn handler_is_default(&self) -> bool {
        self.phase != Phase::Main && self.handler_is_default
    }

    /// Is the VM inside the forced handler-entry ritual (handler_init)?
    pub fn in_handler_init(&self) -> bool {
        self.phase != Phase::Main && self.handler_init_active
    }

    /// Name of the signal whose template is running, if any.
    pub fn active_signal(&self) -> Option<&'static str> {
        match self.phase {
            Phase::Main => None,
            Phase::Template(kind) => Some(kind.name()),
        }
    }

    /// Did this VM exit through the abort sequence?
    pub fn aborted(&self) -> bool {
        self.aborted
    }

    /// The public run state — docs/07's shape (a projection; the sim's
    /// thought clouds and tests switch on this, not on internals).
    pub fn run_state(&self) -> RunState {
        if self.state == State::Dead {
            return RunState::Disabled;
        }
        match self.engine_ctx {
            Some(EngineCtx::Boot) => return RunState::Boot,
            Some(EngineCtx::Recall) => return RunState::Recall,
            Some(EngineCtx::PadSit) => return RunState::PadSit,
            None => {}
        }
        match self.phase {
            Phase::Template(SignalKind::Error) => RunState::Faulted,
            Phase::Template(signal) => {
                RunState::Template { signal, flinching: self.handler_init_active }
            }
            Phase::Main if self.state == State::Blocked => RunState::Blocked,
            Phase::Main => RunState::Running,
        }
    }

    /// The factory window for `kind` (source + program), if the host
    /// installed one. Absent = an empty window.
    pub fn default_handler(&self, kind: SignalKind) -> Option<&DefaultHandler> {
        self.config.default_handlers.get(&kind)
    }

    /// Source line of the program's window for `kind`, if written.
    pub fn handler_line(&self, kind: SignalKind) -> Option<u32> {
        self.program.handlers.get(&kind).map(|h| h.line)
    }

    // --- sim-facing control ---

    /// Grant this tick's cycles (stored as centicycles, Q56).
    pub fn grant(&mut self, cycles: u64, costs: &CostTable) {
        self.grant_centi(cycles.saturating_mul(CENT as u64), costs);
    }

    /// Grant this tick's budget directly in centicycles (the stat pipeline
    /// hands out modified amounts — brownout's −50% on a stock CPU is 50).
    ///
    /// Two rules live here, not in the sim (docs/01 M5):
    /// - **No banking while blocked**: a bot waiting on an action or
    ///   channel burns its grant — waiting is what its CPU is doing.
    /// - **Bank cap**: the budget clamps to the table-derived `bank_cap`
    ///   after every grant — but never below THIS grant, so a CPU faster
    ///   than the cap still spends its full rate each tick (the cap bounds
    ///   SAVING, not throughput). Debt (negative budget) is untouched —
    ///   forced charges are owed, not banked.
    pub fn grant_centi(&mut self, centi: u64, costs: &CostTable) {
        if matches!(self.state, State::Blocked) {
            return;
        }
        self.budget = self.budget.saturating_add(centi.min(i64::MAX as u64) as i64);
        // The cap is "the priciest effective op" (Q75) — a flat overlay
        // raises every op by the same margin, so the cap rises with it:
        // saving up for the big op must stay possible ON the tax tile.
        let cap = (costs.bank_cap as i64)
            .saturating_mul(CENT)
            .saturating_add(self.cost_overlay_centi.max(0))
            .max(centi.min(i64::MAX as u64) as i64);
        if self.budget > cap {
            self.budget = cap;
        }
    }

    /// Set this tick's flat per-op surcharge (centicycles). The sim owns
    /// the figure — tile kind under the chassis, M8 — and re-sets it
    /// before every grant; the VM only applies it.
    pub fn set_cost_overlay_centi(&mut self, centi: i64) {
        self.cost_overlay_centi = centi;
    }

    /// A charged op's effective price: base + overlay, floored at one
    /// full cycle. Free work (base 0) stays free — the overlay taxes
    /// OPS, not the VM's internal bookkeeping.
    fn charged(&self, base_centi: i64) -> i64 {
        if base_centi == 0 {
            return 0;
        }
        (base_centi.saturating_add(self.cost_overlay_centi)).max(CENT)
    }

    /// Mark the start/end of an engine interrupt context (Boot, the recall
    /// walk, a pad-sit). While set, any signal or fault is a double handle.
    pub fn set_engine_ctx(&mut self, ctx: Option<EngineCtx>) {
        self.engine_ctx = ctx;
    }

    /// Re-derive the call-depth cap after a hardware change (the Upgrade
    /// Station's Stack extension applies to the LIVE VM — hardware is the
    /// body, not the program).
    pub fn set_stack_depth(&mut self, depth: usize) {
        self.config.stack_depth = depth;
    }

    /// Enter a signal's reserved template: forced prologue (the
    /// `handler_init()` flinch no window can skip), then the window body —
    /// the player's `on <signal>:` contents or the factory's. Boot's
    /// template has a different prologue and enters via [`Vm::begin_boot`].
    fn enter_template(&mut self, kind: SignalKind, body: Vec<StmtId>, factory: bool) {
        self.work = body.into_iter().rev().map(Work::Stmt).collect();
        self.work.push(Work::InitDone);
        self.work.push(Work::Discard);
        self.work.push(Work::CallExec {
            name: "handler_init".to_string(),
            argc: 0,
            kwargs: Vec::new(),
            line: 0,
        });
        self.handler_init_active = true;
        self.values.clear();
        self.frames.clear();
        self.phase = Phase::Template(kind);
        self.handler_is_default = factory;
    }

    /// The window body for `kind`: the player's block if written, else the
    /// factory contents, else empty. The bool is `factory`.
    fn window_body(&mut self, kind: SignalKind) -> (Vec<StmtId>, bool) {
        if let Some(handler) = self.program.handlers.get(&kind) {
            let body = handler.body.clone();
            self.active = Rc::clone(&self.program);
            (body, false)
        } else if let Some(default) = self.config.default_handlers.get(&kind) {
            let program = Rc::clone(&default.program);
            let body = program.body.clone();
            self.active = program;
            (body, true)
        } else {
            // No factory contents installed: an empty window. The template
            // (and its prologue flinch) still runs.
            self.active = Rc::clone(&self.program);
            (Vec::new(), true)
        }
    }

    /// Enter the Boot template: prologue — the forced `upload_log()` when
    /// the local buffer is non-empty (the rescued veteran's automatic
    /// incident report) — then the `on boot:` window (the dotfile), then
    /// the main program from line 1 (the reset when the template ends).
    /// The sim calls this as the boot countdown completes.
    pub fn begin_boot(&mut self, upload_pending_log: bool) {
        let (body, factory) = self.window_body(SignalKind::Boot);
        self.work = body.into_iter().rev().map(Work::Stmt).collect();
        if upload_pending_log {
            self.work.push(Work::Discard);
            self.work.push(Work::CallExec {
                name: "upload_log".to_string(),
                argc: 0,
                kwargs: Vec::new(),
                line: 0,
            });
        }
        self.values.clear();
        self.frames.clear();
        self.phase = Phase::Template(SignalKind::Boot);
        self.handler_is_default = factory;
        self.handler_init_active = false;
    }

    /// Full reset: line 1, variables cleared. Used by the sim at boot.
    /// A queued redeploy installs here — every reset is a loop boundary.
    pub fn reset(&mut self) {
        if let Some(program) = self.pending_program.take() {
            self.program = program;
        }
        self.active = Rc::clone(&self.program);
        self.handler_is_default = false;
        self.handler_init_active = false;
        self.work = Self::block_work(&self.program.body.clone());
        self.values.clear();
        self.frames.clear();
        self.globals.clear();
        self.phase = Phase::Main;
        if self.state != State::Dead {
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
            // Failed world actions carry the generic action id; typed
            // host-domain ids go through resolve_action_fault.
            Err(msg) => self.fault(faults::ACTION, msg, host, costs),
        }
    }

    /// Resolve a blocking action with a TYPED host-domain fault id (M11:
    /// a channel timeout is `err_timeout`, not the generic action fault).
    pub fn resolve_action_fault(&mut self, fault: Fault, host: &mut dyn Host, costs: &CostTable) {
        if self.state != State::Blocked {
            return;
        }
        self.state = State::Running;
        self.fault(fault.id, fault.msg, host, costs);
    }

    /// Deliver an external signal at an op boundary. Every signal enters
    /// its reserved template — there is no "unhandled": an unwritten
    /// window runs its factory contents (or nothing), but the sandwich
    /// (prologue flinch included) always runs. A signal landing while ANY
    /// template phase or engine context is active is a double-handle →
    /// abort, factory contents included (Q50 — no humble carve-out).
    pub fn raise(
        &mut self,
        signal: Signal,
        host: &mut dyn Host,
        costs: &CostTable,
    ) -> RaiseOutcome {
        if self.state == State::Dead {
            // Inert (aborted bots absorb everything — the forced sequence
            // already completed; nothing can interrupt what's finished).
            return RaiseOutcome::Ignored;
        }
        if signal == Signal::Abort {
            self.abort(host, costs);
            return RaiseOutcome::Aborted;
        }
        if self.engine_ctx.is_some() || self.phase != Phase::Main {
            self.abort(host, costs);
            return RaiseOutcome::Aborted;
        }
        if signal == Signal::Recall {
            // Fully engine-reserved: the sim drives the walk home; the VM
            // records the interrupt context so the double-handle rule
            // holds all the way to the printer. Recall interrupts Blocked
            // bots too (docs/01) — the pending action's owed result value
            // will never arrive, so the stacks are FULLY SUSPENDED here
            // (cleared, exactly like a template entry): every recall exit
            // either replaces the VM (re-color, scrap) or resets it
            // (vanished destination → boot), and a cleared VM that somehow
            // steps just wraps cleanly to line 1.
            self.engine_ctx = Some(EngineCtx::Recall);
            self.state = State::Running;
            self.work.clear();
            self.values.clear();
            self.frames.clear();
            return RaiseOutcome::Handled;
        }
        let kind = match signal {
            Signal::Hurt => SignalKind::Hurt,
            Signal::Bump => SignalKind::Bump,
            Signal::Bumped => SignalKind::Bumped,
            Signal::Abort | Signal::Recall => unreachable!("handled above"),
        };
        // Entering a template abandons any pending action (the sim cancels
        // the world side); only now is un-blocking sound.
        self.state = State::Running;
        let (body, factory) = self.window_body(kind);
        self.enter_template(kind, body, factory);
        RaiseOutcome::Handled
    }

    /// The fully reserved abort sequence (docs/01): forced `upload_log()`
    /// then `become_disabled()`, as ordinary registry functions — charged
    /// as debt (the budget may go negative; the sequence never waits) and
    /// un-interruptible (it runs to completion synchronously; the VM is
    /// inert afterwards, absorbing anything that arrives later).
    fn abort(&mut self, host: &mut dyn Host, costs: &CostTable) {
        if self.aborted || self.state == State::Dead {
            return;
        }
        self.aborted = true;
        let upload_cost = costs.builtin_charge("upload_log", &[], host.log_len());
        self.budget -= upload_cost as i64 * CENT;
        let _ = host.call("upload_log", &[], self.ctx());
        let _ = host.call("become_disabled", &[], self.ctx());
        self.work.clear();
        self.phase = Phase::Main;
        self.handler_is_default = false;
        self.handler_init_active = false;
        self.state = State::Dead;
    }

    /// Step until the budget runs out, an action blocks, or the bot dies.
    pub fn run(&mut self, host: &mut dyn Host, costs: &CostTable) -> Outcome {
        // Only a Paused exit below re-arms this, so Blocked/Dead report no
        // stall and a resumed bot stops advertising the op it just paid for.
        self.stall_centi = None;
        loop {
            match self.state {
                State::Blocked => return Outcome::Blocked,
                State::Dead => return Outcome::Dead,
                State::Running => {}
            }

            let Some(top) = self.work.last() else {
                match self.phase {
                    Phase::Template(_) => {
                        // Template completed: restart from line 1, full
                        // reset (every window "exits via restart").
                        self.reset();
                        continue;
                    }
                    Phase::Main => {
                        // Implicit program loop: wrap to line 1. Costs one
                        // statement (no free spinning). Variables SURVIVE
                        // the wrap (Q80) — only fault/handler restarts
                        // clear them.
                        let cost = self.charged(costs.statement as i64 * CENT);
                        if self.budget < cost {
                            self.stall_centi = Some(cost);
                            return Outcome::Paused;
                        }
                        self.budget -= cost;
                        if let Some(program) = self.pending_program.take() {
                            self.program = program; // redeploy lands here
                            // A redeploy replaces the code out from under
                            // the old state; stale names re-derive.
                            self.globals.clear();
                        }
                        self.active = Rc::clone(&self.program);
                        self.values.clear();
                        self.frames.clear();
                        self.work = Self::block_work(&self.program.body.clone());
                        continue;
                    }
                }
            };

            let cost = self.charged(self.cost_of(top, costs, &*host) as i64 * CENT);
            if self.budget < cost {
                self.stall_centi = Some(cost);
                return Outcome::Paused;
            }
            self.budget -= cost;

            let work = self.work.pop().expect("checked non-empty above");
            self.execute(work, host, costs);
        }
    }

    // --- internals ---

    fn cost_of(&self, work: &Work, costs: &CostTable, host: &dyn Host) -> u64 {
        match work {
            Work::Stmt(id) => match self.active.stmt(*id) {
                // A bare-call statement's overhead is folded into the
                // function's full charge (Q80); other expression statements
                // pay the plain statement figure.
                Stmt::Expr { expr, .. } => match self.active.expr(*expr) {
                    Expr::Call { .. } | Expr::MethodCall { .. } => 0,
                    _ => costs.statement,
                },
                Stmt::Assign { .. } => 0, // StoreVar charges `assign`
                Stmt::IndexAssign { .. } => 0, // IndexSet charges `list_op`
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
            Work::CallExec { name, argc, kwargs, .. } => {
                if self.active.functions.contains_key(name) {
                    costs.user_call
                } else {
                    self.call_charge(name, *argc, kwargs, costs, host)
                }
            }
            Work::MethodExec { name, .. } => costs.builtin_charge(name, &[], 0),
            Work::EnumCtorExec { .. } => costs.enum_ctor,
            Work::MakeList { .. } => costs.list_op,
            Work::MakeDict { .. } => costs.list_op,
            Work::IndexGet => costs.list_op,
            Work::IndexSet { .. } => costs.list_op,
            Work::AttrGet { .. } => costs.attr,
            Work::ReturnNow => 0,
            Work::FrameEnd => 0,
            Work::InitDone => 0,
        }
    }

    /// Full charge for a builtin about to execute (its args are already on
    /// the value stack: positionals, then keyword values). Sized costs read
    /// the payload argument by canonical position — positionally when
    /// supplied that way, else by keyword name. Payload contributions clamp
    /// to `payload_cap` (oversize faults at execution, but the charge stays
    /// bounded either way).
    fn call_charge(
        &self,
        name: &str,
        argc: usize,
        kwargs: &[String],
        costs: &CostTable,
        host: &dyn Host,
    ) -> u64 {
        let spec = costs.spec(name);
        match spec.map(|s| &s.cost) {
            Some(CostSpec::Fixed(c)) => *c,
            Some(CostSpec::LogSized { base, cap }) => (base + host.log_len()).min(*cap),
            Some(CostSpec::PlusPayload { base, payload_arg }) => {
                let total = argc + kwargs.len();
                let stack = &self.values[self.values.len() - total..];
                let payload = if *payload_arg < argc {
                    stack.get(*payload_arg)
                } else {
                    spec.and_then(|s| s.params.as_ref())
                        .and_then(|ps| ps.get(*payload_arg))
                        .and_then(|p| kwargs.iter().position(|k| *k == p.name))
                        .map(|i| &stack[argc + i])
                };
                let units = payload.map_or(0, |v| v.payload_units());
                base + units.min(costs.payload_cap)
            }
            None => costs.default_builtin,
        }
    }

    /// The fault path: synchronous entry into the error template
    /// (docs/01 "Errors & Signals"; error ranks right after abort in the
    /// severity order because it happened *inside* the op).
    fn fault(&mut self, id: &'static str, msg: String, host: &mut dyn Host, costs: &CostTable) {
        self.last_fault = Some(msg.clone());
        self.last_fault_id = Some(id);
        self.fault_count += 1;
        if self.engine_ctx.is_some() || self.phase != Phase::Main {
            // A fault inside ANY template — your window, factory contents,
            // a forced line — or an engine context is a double-handle →
            // abort (Q50: no humble carve-out; factory contents double-
            // handle like player code).
            self.abort(host, costs);
            return;
        }
        if self.program.handlers.contains_key(&SignalKind::Error)
            || self.config.default_handlers.contains_key(&SignalKind::Error)
        {
            // Trap: pay the (cheap) trap cost, enter the error template.
            // Variables preserved while it runs; cleared on the exit
            // restart. Factory contents are not armor: a crash landing in
            // them still counts (the sim chips the chassis per crash).
            self.budget -= costs.trap_cost as i64 * CENT;
            let (body, factory) = self.window_body(SignalKind::Error);
            if factory {
                self.crash_count += 1;
            }
            self.enter_template(SignalKind::Error, body, factory);
        } else {
            // No factory installed (bare VM, tests): the hard fallback —
            // dump and restart without the template machinery.
            self.crash_count += 1;
            self.budget -= costs.crash_dump as i64 * CENT;
            let ctx = CallCtx {
                line: self.current_line,
                last_fault: Some(msg.as_str()),
                last_fault_id: Some(id),
            };
            let _ = host.call("upload_crash_dump", &[Value::Str(msg.clone())], ctx);
            self.reset();
        }
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

    /// Write back a mutated container to where the variable LIVES —
    /// current frame if it's a local, else the globals (matching Python:
    /// `xs[0] = 1` in a def mutates the outer list; only `xs = ...` makes
    /// a local). Builtin constants aren't assignable.
    fn store_existing(&mut self, name: String, value: Value, host: &mut dyn Host, costs: &CostTable) {
        if let Some(frame) = self.frames.last_mut()
            && frame.locals.contains_key(&name)
        {
            frame.locals.insert(name, value);
            return;
        }
        if let Some(slot) = self.globals.get_mut(&name) {
            *slot = value;
            return;
        }
        self.fault(faults::NAME, format!("cannot mutate '{name}' — not a variable"), host, costs);
    }

    fn pop_value(&mut self) -> Value {
        self.values.pop().expect("value stack underflow is a VM bug")
    }

    fn ctx(&self) -> CallCtx<'_> {
        CallCtx {
            line: self.current_line,
            last_fault: self.last_fault.as_deref(),
            last_fault_id: self.last_fault_id,
        }
    }

    fn push_block(&mut self, block: &[StmtId]) {
        for &s in block.iter().rev() {
            self.work.push(Work::Stmt(s));
        }
    }

    fn execute(&mut self, work: Work, host: &mut dyn Host, costs: &CostTable) {
        let program = Rc::clone(&self.active);
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
                    Stmt::IndexAssign { name, index, value, .. } => {
                        // Index evaluates before value (left-to-right).
                        self.work.push(Work::IndexSet { name });
                        self.work.push(Work::Eval(value));
                        self.work.push(Work::Eval(index));
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
                            self.fault(faults::CONTROL, "return outside function".into(), host, costs);
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
                            self.fault(faults::NAME, format!("read of unset variable '{name}'"), host, costs);
                        }
                    },
                    Expr::List(items) => {
                        self.work.push(Work::MakeList { count: items.len() });
                        for &item in items.iter().rev() {
                            self.work.push(Work::Eval(item));
                        }
                    }
                    Expr::Dict(entries) => {
                        self.work.push(Work::MakeDict { count: entries.len() });
                        for &(key, value) in entries.iter().rev() {
                            self.work.push(Work::Eval(value));
                            self.work.push(Work::Eval(key));
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
                    Expr::Call { name, args, kwargs, line } => {
                        self.current_line = line;
                        self.work.push(Work::CallExec {
                            name,
                            argc: args.len(),
                            kwargs: kwargs.iter().map(|(k, _)| k.clone()).collect(),
                            line,
                        });
                        // Evaluation order: positionals, then keyword
                        // values, left to right (stack pushes reversed).
                        for &(_, v) in kwargs.iter().rev() {
                            self.work.push(Work::Eval(v));
                        }
                        for &arg in args.iter().rev() {
                            self.work.push(Work::Eval(arg));
                        }
                    }
                    Expr::MethodCall { base, name, args, line } => {
                        self.current_line = line;
                        // A bare-name base lets mutating methods (append)
                        // write the container back to its variable.
                        let base_name = match program.expr(base) {
                            Expr::Name(n) => Some(n.clone()),
                            _ => None,
                        };
                        self.work.push(Work::MethodExec {
                            name,
                            argc: args.len(),
                            line,
                            base_name,
                        });
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
                    Err(f) => self.fault(f.id, f.msg, host, costs),
                }
            }
            Work::Unary(op) => {
                let v = self.pop_value();
                let result = match (op, v) {
                    (UnOp::Neg, Value::Int(i)) => i
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| Fault::new(faults::OVERFLOW, "integer overflow")),
                    (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnOp::Neg, other) => {
                        Err(Fault::new(faults::TYPE, format!("cannot negate {}", other.type_name())))
                    }
                    (UnOp::Not, other) => Err(Fault::new(
                        faults::TYPE,
                        format!("'not' requires bool, got {}", other.type_name()),
                    )),
                };
                match result {
                    Ok(v) => self.values.push(v),
                    Err(f) => self.fault(f.id, f.msg, host, costs),
                }
            }
            Work::AndRhs(rhs) => match self.pop_value() {
                Value::Bool(false) => self.values.push(Value::Bool(false)),
                Value::Bool(true) => {
                    self.work.push(Work::AssertBool);
                    self.work.push(Work::Eval(rhs));
                }
                other => self.fault(faults::TYPE, format!("'and' requires bool, got {}", other.type_name()), host, costs),
            },
            Work::OrRhs(rhs) => match self.pop_value() {
                Value::Bool(true) => self.values.push(Value::Bool(true)),
                Value::Bool(false) => {
                    self.work.push(Work::AssertBool);
                    self.work.push(Work::Eval(rhs));
                }
                other => self.fault(faults::TYPE, format!("'or' requires bool, got {}", other.type_name()), host, costs),
            },
            Work::AssertBool => {
                let v = self.pop_value();
                if matches!(v, Value::Bool(_)) {
                    self.values.push(v);
                } else {
                    self.fault(faults::TYPE, format!("boolean operator requires bool, got {}", v.type_name()), host, costs);
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
                        self.fault(faults::TYPE, format!("condition must be bool, got {}", other.type_name()), host, costs);
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
                        self.fault(faults::TYPE, format!("condition must be bool, got {}", other.type_name()), host, costs);
                    }
                }
            }

            Work::ForBegin { stmt } => match self.pop_value() {
                Value::List(items) => {
                    self.work.push(Work::ForIter { stmt, items, idx: 0 });
                }
                Value::Dict(entries) => {
                    // Keys, in sorted order (deterministic by construction).
                    let keys: Vec<Value> = entries.keys().map(|k| k.to_value()).collect();
                    self.work.push(Work::ForIter { stmt, items: keys, idx: 0 });
                }
                other => {
                    self.fault(
                        faults::TYPE,
                        format!("for-in requires a list or dict, got {}", other.type_name()),
                        host,
                        costs,
                    );
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
                    self.fault(faults::TYPE, format!("match requires an enum value, got {}", other.type_name()), host, costs);
                }
            },
            Work::MatchArm { stmt, value, case } => {
                let Stmt::Match { cases, .. } = program.stmt(stmt) else { unreachable!() };
                let Pattern::EnumVariant { enum_name, variant, binds } = &cases[case].pattern
                else {
                    // Wildcard: matches anything, binds nothing.
                    let body = cases[case].body.clone();
                    self.push_block(&body);
                    return;
                };
                // Structural identity is name + variant + ARITY (Q80): a
                // wrong-arity pattern is a non-match and falls through,
                // like a wrong variant — not a fault.
                if *enum_name == value.enum_name
                    && *variant == value.variant
                    && binds.len() == value.fields.len()
                {
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
                        faults::NO_MATCH,
                        format!("no case matched {}.{}", value.enum_name, value.variant),
                        host,
                        costs,
                    );
                }
            }

            Work::CallExec { name, argc, kwargs, line } => {
                self.current_line = line;
                let kwvals: Vec<Value> = self.values.split_off(self.values.len() - kwargs.len());
                let args: Vec<Value> = self.values.split_off(self.values.len() - argc);
                let kwpairs: Vec<(String, Value)> =
                    kwargs.into_iter().zip(kwvals).collect();
                if let Some(func) = program.functions.get(&name) {
                    if self.frames.len() >= self.config.stack_depth {
                        self.fault(faults::STACK, "stack overflow".into(), host, costs);
                        return;
                    }
                    let params: Vec<(String, Option<Value>)> = func
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.default.as_ref().map(default_lit_value)))
                        .collect();
                    let bound = match bind_args(&name, &params, args, kwpairs) {
                        Ok(b) => b,
                        Err(f) => {
                            self.fault(f.id, f.msg, host, costs);
                            return;
                        }
                    };
                    let mut locals = BTreeMap::new();
                    for ((pname, _), arg) in params.into_iter().zip(bound) {
                        locals.insert(pname, arg);
                    }
                    self.frames.push(Frame { locals, val_base: self.values.len() });
                    let body = func.body.clone();
                    self.work.push(Work::FrameEnd);
                    self.push_block(&body);
                } else if name == "abort" {
                    // The player scuttle (Q76) — the ONLY deliberate way
                    // down. Runs the fully reserved sequence right here;
                    // nothing after this call ever executes.
                    self.abort(host, costs);
                } else if name == "become_disabled" || name == "handler_init" {
                    // Engine-only forced calls (Q76): the host implements
                    // them, but a PLAYER call must not reach it — deleting
                    // the registry entry alone doesn't stop the passthrough
                    // dispatch below, so the VM blocks them by name. The
                    // engine invokes them via host.call directly (abort's
                    // sequence) or engine-injected work items (the flinch,
                    // which carries line 0 — never a player line).
                    if name == "handler_init" && line == 0 {
                        // The template prologue's own injected call.
                        match host.call(&name, &[], self.ctx()) {
                            HostCall::Ready(v) => self.values.push(v),
                            HostCall::Block => self.state = State::Blocked,
                            HostCall::Fault(f) => self.fault(f.id, f.msg, host, costs),
                        }
                    } else {
                        self.fault(
                            faults::UNKNOWN_FUNCTION,
                            format!("unknown function {name}()"),
                            host,
                            costs,
                        );
                    }
                } else if !kwpairs.is_empty() && costs.spec(&name).is_none() {
                    // No registry entry at all: there is nothing to bind
                    // keywords against, and the registry documents every
                    // host builtin — the name itself is the error.
                    self.fault(
                        faults::UNKNOWN_FUNCTION,
                        format!("unknown function {name}()"),
                        host,
                        costs,
                    );
                } else if !kwpairs.is_empty() && costs.spec(&name).is_some_and(|s| s.params.is_none()) {
                    // Keywords need a declared parameter list to bind against.
                    self.fault(
                        faults::ARITY,
                        format!("{name}() takes no keyword arguments"),
                        host,
                        costs,
                    );
                } else if name == "len" {
                    // Pure container builtins live in the VM, not the host.
                    match args.as_slice() {
                        [Value::List(items)] => self.values.push(Value::Int(items.len() as i64)),
                        [Value::Dict(entries)] => {
                            self.values.push(Value::Int(entries.len() as i64));
                        }
                        [Value::Str(s)] => {
                            self.values.push(Value::Int(s.chars().count() as i64));
                        }
                        [other] => {
                            self.fault(
                                faults::TYPE,
                                format!("len() requires a container, got {}", other.type_name()),
                                host,
                                costs,
                            );
                        }
                        _ => self.fault(faults::ARITY, "len() takes one argument".into(), host, costs),
                    }
                } else if name == "range" {
                    let bounds = match args.as_slice() {
                        [Value::Int(n)] => Some((0, *n)),
                        [Value::Int(a), Value::Int(b)] => Some((*a, *b)),
                        _ => None,
                    };
                    let Some((lo, hi)) = bounds else {
                        self.fault(faults::ARITY, "range() takes one or two int arguments".into(), host, costs);
                        return;
                    };
                    // `checked_sub` so a span that overflows i64 (e.g.
                    // range(i64::MIN, i64::MAX)) is treated as over-cap rather
                    // than wrapping negative and slipping past the guard into a
                    // ~1e19-element allocation that would OOM every peer.
                    let over_cap = match hi.checked_sub(lo) {
                        Some(span) => span > costs.range_cap as i64,
                        None => true,
                    };
                    if over_cap {
                        self.fault(
                            faults::RANGE,
                            format!("range too large (cap {})", costs.range_cap),
                            host,
                            costs,
                        );
                        return;
                    }
                    self.values.push(Value::List((lo..hi).map(Value::Int).collect()));
                } else {
                    // Registry-declared params: resolve keywords + defaults
                    // into canonical positional order; undeclared builtins
                    // pass positionals through (host validates).
                    let final_args = match costs.spec(&name).and_then(|s| s.params.as_ref()) {
                        Some(param_specs) => {
                            let params: Vec<(String, Option<Value>)> = param_specs
                                .iter()
                                .map(|p| (p.name.clone(), p.default.as_ref().map(|d| d.to_value())))
                                .collect();
                            match bind_args(&name, &params, args, kwpairs) {
                                Ok(b) => b,
                                Err(f) => {
                                    self.fault(f.id, f.msg, host, costs);
                                    return;
                                }
                            }
                        }
                        None => args,
                    };
                    // Oversized payloads fault err_payload (Q82) — the
                    // charge above was clamped, the send never happens.
                    if let Some(CostSpec::PlusPayload { payload_arg, .. }) =
                        costs.spec(&name).map(|s| &s.cost)
                        && final_args.get(*payload_arg).map_or(0, |v| v.payload_units())
                            > costs.payload_cap
                    {
                        self.fault(
                            faults::PAYLOAD,
                            format!("payload exceeds payload_cap ({})", costs.payload_cap),
                            host,
                            costs,
                        );
                        return;
                    }
                    let ctx = CallCtx {
                        line: self.current_line,
                        last_fault: self.last_fault.as_deref(),
                        last_fault_id: self.last_fault_id,
                    };
                    match host.call(&name, &final_args, ctx) {
                        HostCall::Ready(v) => self.values.push(v),
                        HostCall::Block => self.state = State::Blocked,
                        HostCall::Fault(f) => self.fault(f.id, f.msg, host, costs),
                    }
                }
            }

            Work::MethodExec { name, argc, line, base_name } => {
                self.current_line = line;
                let args: Vec<Value> = self.values.split_off(self.values.len() - argc);
                let base = self.pop_value();
                match (name.as_str(), base) {
                    // --- container methods ---
                    ("append", Value::List(mut items)) => {
                        let [item] = args.as_slice() else {
                            self.fault(faults::ARITY, "append() takes one argument".into(), host, costs);
                            return;
                        };
                        let Some(base_name) = base_name else {
                            // Containers are values: mutating a temporary
                            // would silently vanish, so it's a fault.
                            self.fault(
                                faults::NAME,
                                "append() needs a list variable (containers are values)".into(),
                                host,
                                costs,
                            );
                            return;
                        };
                        items.push(item.clone());
                        self.store_existing(base_name, Value::List(items), host, costs);
                        self.values.push(Value::Unit);
                    }
                    ("append", other) => {
                        self.fault(
                            faults::TYPE,
                            format!("append() requires a list, got {}", other.type_name()),
                            host,
                            costs,
                        );
                    }
                    ("get", Value::Dict(entries)) => {
                        let [key] = args.as_slice() else {
                            self.fault(faults::ARITY, "get() takes one argument (the key)".into(), host, costs);
                            return;
                        };
                        match DictKey::from_value(key.clone()) {
                            Ok(key) => self.values.push(match entries.get(&key) {
                                Some(v) => Value::option_some(v.clone()),
                                None => Value::option_none(),
                            }),
                            Err(msg) => self.fault(faults::KEY, msg, host, costs),
                        }
                    }
                    ("remove", Value::Dict(mut entries)) => {
                        let [key] = args.as_slice() else {
                            self.fault(faults::ARITY, "remove() takes one argument (the key)".into(), host, costs);
                            return;
                        };
                        let Some(base_name) = base_name else {
                            self.fault(
                                faults::NAME,
                                "remove() needs a dict variable (containers are values)".into(),
                                host,
                                costs,
                            );
                            return;
                        };
                        match DictKey::from_value(key.clone()) {
                            Ok(key) => {
                                let removed = entries.remove(&key);
                                self.store_existing(base_name, Value::Dict(entries), host, costs);
                                self.values.push(match removed {
                                    Some(v) => Value::option_some(v),
                                    None => Value::option_none(),
                                });
                            }
                            Err(msg) => self.fault(faults::KEY, msg, host, costs),
                        }
                    }
                    ("keys", Value::Dict(entries)) => {
                        let keys: Vec<Value> = entries.keys().map(|k| k.to_value()).collect();
                        self.values.push(Value::List(keys));
                    }
                    ("values", Value::Dict(entries)) => {
                        let values: Vec<Value> = entries.values().cloned().collect();
                        self.values.push(Value::List(values));
                    }
                    (m @ ("get" | "remove" | "keys" | "values"), other) => {
                        self.fault(
                            faults::TYPE,
                            format!("{m}() requires a dict, got {}", other.type_name()),
                            host,
                            costs,
                        );
                    }
                    ("expect", Value::Enum(e))
                        if e.enum_name == Value::RESULT_ENUM
                            || e.enum_name == Value::OPTION_ENUM =>
                    {
                        if !args.is_empty() {
                            self.fault(faults::ARITY, "expect() takes no arguments".into(), host, costs);
                        } else if e.variant == "Ok" || e.variant == "Some" {
                            let v = e.fields.into_iter().next().unwrap_or(Value::Unit);
                            self.values.push(v);
                        } else {
                            // Err / None: fault with the carried message.
                            let msg = match e.fields.first() {
                                Some(Value::Str(s)) => s.clone(),
                                Some(other) => other.to_string(),
                                None => format!("expect() on {}.{}", e.enum_name, e.variant),
                            };
                            self.fault(faults::EXPECT, msg, host, costs);
                        }
                    }
                    ("expect", other) => {
                        self.fault(
                            faults::TYPE,
                            format!(
                                "expect() requires a Result or Option, got {}",
                                other.type_name()
                            ),
                            host,
                            costs,
                        );
                    }
                    (_, _) => {
                        self.fault(faults::UNKNOWN_FUNCTION, format!("unknown method {name}()"), host, costs);
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

            Work::MakeDict { count } => {
                let flat: Vec<Value> = self.values.split_off(self.values.len() - count * 2);
                let mut entries = std::collections::BTreeMap::new();
                let mut it = flat.into_iter();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    match DictKey::from_value(k) {
                        // Duplicate keys: last one wins, Python-style.
                        Ok(key) => {
                            entries.insert(key, v);
                        }
                        Err(msg) => {
                            self.fault(faults::KEY, msg, host, costs);
                            return;
                        }
                    }
                }
                self.values.push(Value::Dict(entries));
            }

            Work::IndexGet => {
                let index = self.pop_value();
                let base = self.pop_value();
                match (base, index) {
                    (Value::List(items), Value::Int(i)) => {
                        let len = items.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            self.fault(faults::INDEX, format!("index {i} out of range (len {len})"), host, costs);
                        } else {
                            self.values.push(items[effective as usize].clone());
                        }
                    }
                    (Value::Dict(entries), key) => match DictKey::from_value(key) {
                        Ok(key) => match entries.get(&key) {
                            Some(v) => self.values.push(v.clone()),
                            // Missing key faults, like Python's KeyError —
                            // `d.get(k)` is the fault-free Option form.
                            None => {
                                self.fault(faults::KEY, format!("key {key} not found"), host, costs);
                            }
                        },
                        Err(msg) => self.fault(faults::KEY, msg, host, costs),
                    },
                    (base, index) => {
                        self.fault(
                            faults::TYPE,
                            format!("cannot index {} with {}", base.type_name(), index.type_name()),
                            host,
                            costs,
                        );
                    }
                }
            }

            Work::IndexSet { name } => {
                let value = self.pop_value();
                let index = self.pop_value();
                let Some(container) = self.lookup(&name) else {
                    self.fault(faults::NAME, format!("read of unset variable '{name}'"), host, costs);
                    return;
                };
                match (container, index) {
                    (Value::List(mut items), Value::Int(i)) => {
                        let len = items.len() as i64;
                        let effective = if i < 0 { i + len } else { i };
                        if effective < 0 || effective >= len {
                            self.fault(faults::INDEX, format!("index {i} out of range (len {len})"), host, costs);
                        } else {
                            items[effective as usize] = value;
                            self.store_existing(name, Value::List(items), host, costs);
                        }
                    }
                    (Value::Dict(mut entries), key) => match DictKey::from_value(key) {
                        Ok(key) => {
                            entries.insert(key, value);
                            self.store_existing(name, Value::Dict(entries), host, costs);
                        }
                        Err(msg) => self.fault(faults::KEY, msg, host, costs),
                    },
                    (container, index) => {
                        self.fault(
                            faults::TYPE,
                            format!(
                                "cannot index {} with {}",
                                container.type_name(),
                                index.type_name()
                            ),
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
                        Err((fid, msg)) => self.fault(fid, msg, host, costs),
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
                                    faults::NAME,
                                    format!("{}.{} has no field {name}", e.enum_name, e.variant),
                                    host,
                                    costs,
                                );
                            }
                        }
                    }
                    other => {
                        self.fault(
                            faults::TYPE,
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
                            self.fault(faults::CONTROL, "return unwound past program root".into(), host, costs);
                            return;
                        }
                    }
                }
                let frame = self.frames.pop().expect("ReturnNow requires a frame");
                self.values.truncate(frame.val_base);
                self.values.push(value);
            }
            Work::InitDone => {
                self.handler_init_active = false;
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
                    self.fault(faults::CONTROL, format!("{which} outside loop"), host, costs);
                    return;
                }
                Some(_) => {
                    self.work.pop();
                }
            }
        }
    }
}

/// A `def` default literal as a runtime value.
fn default_lit_value(lit: &DefaultLit) -> Value {
    match lit {
        DefaultLit::Int(i) => Value::Int(*i),
        DefaultLit::Str(s) => Value::Str(s.clone()),
        DefaultLit::Bool(b) => Value::Bool(*b),
        DefaultLit::NoneVal => Value::option_none(),
    }
}

/// Bind positional + keyword arguments against a declared parameter list,
/// filling defaults — Python's rules (docs/01 signature convention).
/// Returns the canonical positional argument vector.
fn bind_args(
    fname: &str,
    params: &[(String, Option<Value>)],
    args: Vec<Value>,
    kwargs: Vec<(String, Value)>,
) -> Result<Vec<Value>, Fault> {
    if args.len() > params.len() {
        return Err(Fault::new(
            faults::ARITY,
            format!("{fname}() takes {} argument(s), got {}", params.len(), args.len()),
        ));
    }
    let positional = args.len();
    let mut slots: Vec<Option<Value>> = args.into_iter().map(Some).collect();
    slots.resize(params.len(), None);
    for (key, value) in kwargs {
        let Some(idx) = params.iter().position(|(name, _)| *name == key) else {
            return Err(Fault::new(
                faults::ARITY,
                format!("{fname}() has no parameter '{key}'"),
            ));
        };
        if idx < positional || slots[idx].is_some() {
            return Err(Fault::new(
                faults::ARITY,
                format!("{fname}() got multiple values for '{key}'"),
            ));
        }
        slots[idx] = Some(value);
    }
    let mut bound = Vec::with_capacity(params.len());
    for ((name, default), slot) in params.iter().zip(slots) {
        match slot.or_else(|| default.clone()) {
            Some(v) => bound.push(v),
            None => {
                return Err(Fault::new(
                    faults::ARITY,
                    format!("{fname}() missing required argument '{name}'"),
                ));
            }
        }
    }
    Ok(bound)
}

fn binary_op(op: BinOp, lhs: Value, rhs: Value) -> Result<Value, Fault> {
    use BinOp::*;
    match op {
        Eq => Ok(Value::Bool(lhs == rhs)),
        NotEq => Ok(Value::Bool(lhs != rhs)),
        // Membership: list item, dict key, or substring.
        In => match (&lhs, &rhs) {
            (item, Value::List(items)) => Ok(Value::Bool(items.contains(item))),
            (key, Value::Dict(entries)) => {
                let key = DictKey::from_value(key.clone())
                    .map_err(|msg| Fault::new(faults::KEY, msg))?;
                Ok(Value::Bool(entries.contains_key(&key)))
            }
            (Value::Str(needle), Value::Str(haystack)) => {
                Ok(Value::Bool(haystack.contains(needle.as_str())))
            }
            (l, r) => Err(Fault::new(
                faults::TYPE,
                format!(
                    "'in' requires a container on the right, got {} in {}",
                    l.type_name(),
                    r.type_name()
                ),
            )),
        },
        Add | Sub | Mul | FloorDiv | Mod | Lt | Gt | Le | Ge => {
            let (Value::Int(a), Value::Int(b)) = (&lhs, &rhs) else {
                return Err(Fault::new(
                    faults::TYPE,
                    format!(
                        "operator requires ints, got {} and {}",
                        lhs.type_name(),
                        rhs.type_name()
                    ),
                ));
            };
            let (a, b) = (*a, *b);
            match op {
                Add => a.checked_add(b).map(Value::Int).ok_or_else(overflow),
                Sub => a.checked_sub(b).map(Value::Int).ok_or_else(overflow),
                Mul => a.checked_mul(b).map(Value::Int).ok_or_else(overflow),
                FloorDiv => {
                    if b == 0 {
                        Err(Fault::new(faults::DIV_ZERO, "division by zero"))
                    } else {
                        // Python-style floor division.
                        let q = a.checked_div(b).ok_or_else(overflow)?;
                        let r = a % b;
                        Ok(Value::Int(if r != 0 && (a < 0) != (b < 0) { q - 1 } else { q }))
                    }
                }
                Mod => {
                    if b == 0 {
                        Err(Fault::new(faults::DIV_ZERO, "modulo by zero"))
                    } else {
                        // Python-style: result has the sign of the divisor.
                        // Checked like FloorDiv so i64::MIN % -1 faults
                        // err_overflow instead of panicking a debug build
                        // (release would silently yield 0 — a determinism gap).
                        let r = a.checked_rem(b).ok_or_else(overflow)?;
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

fn overflow() -> Fault {
    Fault::new(faults::OVERFLOW, "integer overflow")
}
