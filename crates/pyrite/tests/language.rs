//! Integration tests: parsing, gating, cycle metering, faults, handlers,
//! double-handle, blocking actions. Each maps to a rule in docs/01-language.md.

use pyrite::vm::{CallCtx, Host, HostCall, Outcome, RaiseOutcome, Signal, Vm, VmConfig};
use pyrite::{faults, parse, Construct, CostTable, Fault, PyriteErrorKind, UnlockSet, Value};
use std::rc::Rc;

/// Scripted test host: records calls, returns canned values.
#[derive(Default)]
struct TestHost {
    calls: Vec<(String, Vec<Value>)>,
    /// name -> value returned when called
    returns: std::collections::BTreeMap<String, Value>,
    /// names that block instead of returning
    blocking: std::collections::BTreeSet<String>,
    /// names that fault
    faulting: std::collections::BTreeMap<String, String>,
    /// reported log-buffer length (for upload_log's sized cost)
    log_len: u64,
}

impl Host for TestHost {
    fn call(&mut self, name: &str, args: &[Value], _ctx: CallCtx<'_>) -> HostCall {
        self.calls.push((name.to_string(), args.to_vec()));
        if let Some(msg) = self.faulting.get(name) {
            return HostCall::Fault(Fault::new(faults::ACTION, msg.clone()));
        }
        if self.blocking.contains(name) {
            return HostCall::Block;
        }
        HostCall::Ready(self.returns.get(name).cloned().unwrap_or(Value::Unit))
    }

    fn attr(&mut self, entity: u64, name: &str) -> Result<Value, (&'static str, String)> {
        match name {
            "id" => Ok(Value::Int(entity as i64)),
            _ => Err((faults::NAME, format!("unknown attribute {name}"))),
        }
    }

    fn log_len(&self) -> u64 {
        self.log_len
    }
}

fn vm_for(source: &str) -> (Vm, TestHost, CostTable) {
    let program = parse(source, &UnlockSet::all()).expect("parse failed");
    let mut host = TestHost::default();
    // Programs loop forever by design; tests end with `halt()` to park the
    // VM in Blocked instead of wrapping and re-running.
    host.blocking.insert("halt".into());
    (Vm::new(Rc::new(program), VmConfig::default()), host, CostTable::default())
}

fn call_names(host: &TestHost) -> Vec<&str> {
    host.calls.iter().map(|(n, _)| n.as_str()).collect()
}

// --- parsing & gating ---

#[test]
fn tier0_program_parses_with_no_unlocks() {
    let src = "move_to(closest(ore).expect())\nmine()\ndeposit()\n";
    assert!(parse(src, &UnlockSet::none()).is_ok());
}

#[test]
fn assignment_requires_variables_unlock() {
    let err = parse("x = 5\n", &UnlockSet::none()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::LockedConstruct(Construct::Variables));
    assert!(parse("x = 5\n", &UnlockSet::none().with(Construct::Variables)).is_ok());
}

#[test]
fn if_requires_unlock() {
    let src = "if cargo_full():\n    deposit()\n";
    let err = parse(src, &UnlockSet::none().with(Construct::Variables)).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::LockedConstruct(Construct::If));
}

#[test]
fn while_break_require_unlock() {
    let src = "while True:\n    break\n";
    let err = parse(src, &UnlockSet::all().without_while()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::LockedConstruct(Construct::WhileLoop));
}

// Helper: UnlockSet::all() minus WhileLoop, built from scratch.
trait WithoutWhile {
    fn without_while(&self) -> UnlockSet;
}
impl WithoutWhile for UnlockSet {
    fn without_while(&self) -> UnlockSet {
        let mut set = UnlockSet::none();
        for c in Construct::ALL {
            if c != Construct::WhileLoop {
                set.unlock(c);
            }
        }
        set
    }
}

#[test]
fn signal_windows_require_their_unlocks() {
    let src = "on hurt:\n    drop_cargo()\n";
    let err = parse(src, &UnlockSet::none()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::LockedConstruct(Construct::OnHurt));
    assert!(parse(src, &UnlockSet::none().with(Construct::OnHurt)).is_ok());
    // bump + bumped share one unlock (docs/06's tree).
    let bump = "on bump:\n    drop_cargo()\n";
    let bumped = "on bumped:\n    drop_cargo()\n";
    assert!(parse(bump, &UnlockSet::none().with(Construct::OnBumpBumped)).is_ok());
    assert!(parse(bumped, &UnlockSet::none().with(Construct::OnBumpBumped)).is_ok());
}

#[test]
fn abort_and_recall_have_no_windows() {
    // Fully engine-reserved (docs/01): writing them is a parse error.
    assert!(parse("on abort:\n    log(1)\n", &UnlockSet::all()).is_err());
    assert!(parse("on recall:\n    log(1)\n", &UnlockSet::all()).is_err());
    // The old unified/death handlers are gone with them.
    assert!(parse("on signal(s):\n    log(1)\n", &UnlockSet::all()).is_err());
    assert!(parse("on death:\n    log(1)\n", &UnlockSet::all()).is_err());
}

#[test]
fn locked_construct_error_names_the_unlock() {
    let err = parse("x = 1\n", &UnlockSet::none()).unwrap_err();
    assert!(err.to_string().contains("requires unlock: Variables"), "got: {err}");
}

// --- cycle metering ---

#[test]
fn tier0_line_costs_match_table() {
    // Table entries are FULL charges (Q80): mine() = 2 cycles total — the
    // call statement's overhead is folded into the figure.
    let (mut vm, mut host, costs) = vm_for("mine()\n");
    vm.grant(1, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Paused);
    assert!(host.calls.is_empty(), "should not have run yet at 1 cycle");
    vm.grant(1, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["mine"], "2nd cycle completes the call");
}

#[test]
fn cycle_debt_ops_wait_until_affordable() {
    // Expensive op executes only once enough cycles accumulate across grants.
    let (mut vm, mut host, costs) = vm_for("scan_enemies()\n");
    // scan_enemies = 4 total (full charge); grant 1/tick — it waits 3 ticks.
    for _ in 0..3 {
        vm.grant(1, &costs);
        assert_eq!(vm.run(&mut host, &costs), Outcome::Paused);
    }
    assert!(host.calls.is_empty());
    vm.grant(1, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["scan_enemies"]);
}

#[test]
fn stall_cost_reports_the_op_being_saved_for() {
    // The UI pairs stall_cost() with budget() to fill a "saving up" bar, so
    // the pair must describe the SAME pending op every starved tick and go
    // quiet the moment it is affordable.
    let (mut vm, mut host, costs) = vm_for("scan_enemies()\n");
    assert_eq!(vm.stall_cost(), None, "nothing reported before the first run");
    for tick in 1..=3 {
        vm.grant(1, &costs);
        assert_eq!(vm.run(&mut host, &costs), Outcome::Paused);
        // scan_enemies = 4 cycles = 400 centicycles, unchanging while saving.
        assert_eq!(vm.stall_cost(), Some(400), "tick {tick}");
        assert_eq!(vm.budget(), tick * 100, "tick {tick}");
    }
    vm.grant(1, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["scan_enemies"]);
    // Afforded and spent: the bar must stop advertising a stall.
    assert_ne!(vm.stall_cost(), Some(400), "the paid-for op is no longer pending");
}

#[test]
fn program_loops_forever_with_a_wrap_charge() {
    // One pass: x = 1 (assign 1) + log(x) (full charge 1) = 2. The wrap
    // costs one statement, so two passes = 2 + 1 + 2 = 5; a 6th cycle is
    // needed before a third pass could start.
    let (mut vm, mut host, costs) = vm_for("x = 1\nlog(x)\n");
    vm.grant(5, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["log", "log"]);
}

#[test]
fn variables_survive_the_wrap() {
    // Q80: the loop-around does NOT clear variables — a counter seeded from
    // a constant (constants sit below globals) keeps climbing pass after
    // pass. Only fault/handler restarts clear.
    let program = parse("x = x + 1\nlog(x)\n", &UnlockSet::all()).unwrap();
    let mut config = VmConfig::default();
    config.constants.insert("x".into(), Value::Int(0));
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    let costs = CostTable::default();
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert!(logged.len() >= 3, "several passes expected");
    assert_eq!(logged[0], &Value::Int(1));
    assert_eq!(logged[1], &Value::Int(2), "x survived the wrap");
    assert_eq!(logged[2], &Value::Int(3));
}

// --- expressions ---

#[test]
fn arithmetic_and_comparisons() {
    let (mut vm, mut host, costs) = vm_for("log(2 + 3 * 4)\nlog(7 // 2)\nlog(-7 // 2)\nlog(-7 % 2)\nlog(1 < 2)\nhalt()\n");
    vm.grant(200, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> = host
        .calls
        .iter()
        .filter(|(n, _)| n == "log")
        .map(|(_, args)| &args[0])
        .collect();
    assert_eq!(
        logged,
        [
            &Value::Int(14),
            &Value::Int(3),
            &Value::Int(-4), // Python floor division
            &Value::Int(1),  // Python modulo: sign of divisor
            &Value::Bool(true),
        ]
    );
}

#[test]
fn short_circuit_and() {
    // Rhs must not be evaluated when lhs is False.
    let (mut vm, mut host, costs) = vm_for("log(False and cargo_full())\nhalt()\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["log", "halt"], "cargo_full must not be called");
}

#[test]
fn if_elif_else_flow() {
    let src = "\
x = 2
if x == 1:
    log(1)
elif x == 2:
    log(2)
else:
    log(3)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(2)]);
}

#[test]
fn while_loop_with_counter() {
    let src = "\
n = 0
while n < 3:
    log(n)
    n = n + 1
done()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(200, &costs);
    vm.run(&mut host, &costs);
    let names = call_names(&host);
    assert_eq!(names.iter().filter(|n| **n == "log").count(), 3);
    assert!(names.contains(&"done"));
}

#[test]
fn for_in_list_with_break_and_continue() {
    let src = "\
for x in [1, 2, 3, 4]:
    if x == 2:
        continue
    if x == 4:
        break
    log(x)
after()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(500, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(1), &Value::Int(3)]);
    assert!(call_names(&host).contains(&"after"));
}

#[test]
fn user_functions_and_return() {
    let src = "\
def double(x):
    return x + x

log(double(21))
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(42)]);
}

#[test]
fn recursion_overflows_at_stack_cap() {
    // Base stack depth is 4: recursion deeper than that is a stack-overflow
    // fault → forced crash dump (docs/01: overflow is a standard fault).
    let src = "\
def down(n):
    return down(n - 1)

down(100)
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"upload_crash_dump"));
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump").unwrap();
    assert!(matches!(&dump.1[0], Value::Str(s) if s.contains("stack overflow")));
}

#[test]
fn enums_and_match() {
    let src = "\
enum Order:
    Idle
    Mine(target)

o = Order.Mine(7)
match o:
    case Order.Idle:
        log(0)
    case Order.Mine(t):
        log(t)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(7)]);
}

#[test]
fn match_on_host_enum_without_declaration() {
    // Host-domain enums match by name without a player declaration
    // (the retired Recv enum used to be the example; try_receive now
    // returns Option — docs/01).
    let src = "\
match probe(\"orders\"):
    case Contact.Seen(v):
        log(v)
    case Contact.Lost:
        idle()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    host.returns.insert(
        "probe".into(),
        Value::Enum(pyrite::EnumValue {
            enum_name: "Contact".into(),
            variant: "Seen".into(),
            fields: vec![Value::Int(99)],
        }),
    );
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(99)]);
}

// --- blocking actions ---

#[test]
fn actions_block_until_resolved() {
    let (mut vm, mut host, costs) = vm_for("move_to(5)\narrived()\n");
    host.blocking.insert("move_to".into());
    vm.grant(100, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked);
    assert_eq!(call_names(&host), ["move_to"]);
    // Sim resolves the action; the program continues.
    vm.resolve_action(Ok(Value::Unit), &mut host, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"arrived"));
}

#[test]
fn failed_action_faults() {
    let (mut vm, mut host, costs) = vm_for("mine()\n");
    host.faulting.insert("mine".into(), "no ore in range".into());
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump");
    assert!(matches!(dump, Some((_, args)) if matches!(&args[0], Value::Str(s) if s.contains("no ore"))));
}

// --- faults & handlers ---

#[test]
fn unhandled_fault_forces_crash_dump_and_restarts() {
    let (mut vm, mut host, costs) = vm_for("log(1)\nboom(1 // 0)\n");
    // The bank cap (25) means one giant grant can't fund the dump debt
    // plus a restart pass — drip grants like the sim does.
    for _ in 0..5 {
        vm.grant(50, &costs);
        vm.run(&mut host, &costs);
    }
    let names = call_names(&host);
    // log, crash dump, then the restart reaches log again.
    let first_dump = names.iter().position(|n| *n == "upload_crash_dump").unwrap();
    assert!(names[..first_dump].contains(&"log"));
    assert!(names[first_dump..].contains(&"log"), "program must restart from line 1");
    assert_eq!(vm.last_fault(), Some("division by zero"));
}

#[test]
fn crash_dump_cost_is_charged_as_debt() {
    let (mut vm, mut host, costs) = vm_for("boom(1 // 0)\n");
    vm.grant(4, &costs); // enough to reach the fault, nowhere near dump cost
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"upload_crash_dump"));
    assert!(vm.budget() < 0, "crash dump must leave the bot in cycle debt");
    assert_eq!(vm.fault_count(), 1, "one fault, one count");
    assert_eq!(vm.crash_count(), 1, "unhandled fault counts as a crash");
}

#[test]
fn error_window_runs_instead_of_crash_dump() {
    let src = "\
on error:
    handled()

boom(1 // 0)
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(50, &costs);
    vm.run(&mut host, &costs);
    let names = call_names(&host);
    assert!(names.contains(&"handled"));
    assert!(!names.contains(&"upload_crash_dump"));
    assert_eq!(vm.crash_count(), 0, "handled faults are not crashes");
}

#[test]
fn variables_preserved_during_handler_cleared_after() {
    let src = "\
on error:
    log(x)

x = 42
boom(1 // 0)
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(14, &costs); // x=42 (1), stmt(1)+call(1)+arith(1)=fault, trap(5), window: log(1) + init
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(42)], "the window must see pre-fault variables");
}

#[test]
fn fault_inside_error_window_is_double_handle_abort() {
    let src = "\
on error:
    boom(1 // 0)

boom(2 // 0)
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Dead, "double-handle aborts, never explodes");
    let names = call_names(&host);
    assert!(names.contains(&"upload_log"), "abort's logs always go home");
    assert_eq!(*names.last().unwrap(), "become_disabled", "every death exits through abort");
    assert!(vm.aborted());
}

// --- signals: templates, abort, double-handle ---

#[test]
fn hurt_window_runs_then_program_restarts() {
    let src = "\
on hurt:
    drop_cargo()

work()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1, &costs); // work() = 1 full charge; no headroom for a wrap
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["work"]);
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Handled);
    host.calls.clear();
    vm.grant(20, &costs);
    vm.run(&mut host, &costs);
    let names = call_names(&host);
    assert_eq!(names[0], "handler_init", "every template starts with the forced prologue");
    assert_eq!(names[1], "drop_cargo");
    assert!(names.contains(&"work"), "program restarts from line 1 after the template");
}

#[test]
fn hurt_without_window_still_enters_the_template() {
    // No `on hurt:` and no factory contents: the sandwich still runs —
    // the prologue flinch IS the default reaction (docs/01).
    let (mut vm, mut host, costs) = vm_for("work()\n");
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Handled);
    vm.grant(20, &costs);
    vm.run(&mut host, &costs);
    let names = call_names(&host);
    assert_eq!(names[0], "handler_init", "the flinch runs even with an empty window");
    assert!(names.contains(&"work"), "then the program restarts");
}

#[test]
fn abort_force_calls_upload_log_then_become_disabled() {
    let (mut vm, mut host, costs) = vm_for("work()\n");
    assert_eq!(vm.raise(Signal::Abort, &mut host, &costs), RaiseOutcome::Aborted);
    assert_eq!(call_names(&host), ["upload_log", "become_disabled"]);
    assert!(vm.is_dead());
    assert!(vm.aborted());
}

#[test]
fn abort_sequence_needs_no_budget_and_absorbs_signals() {
    // The forced sequence charges as debt (the budget may go negative) and
    // always completes; anything arriving afterwards is absorbed.
    let (mut vm, mut host, costs) = vm_for("work()\n");
    assert_eq!(vm.budget(), 0, "no cycles granted");
    assert_eq!(vm.raise(Signal::Abort, &mut host, &costs), RaiseOutcome::Aborted);
    assert!(vm.budget() < 0, "the forced upload charged as debt");
    assert_eq!(call_names(&host), ["upload_log", "become_disabled"]);
    host.calls.clear();
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Ignored);
    assert_eq!(vm.raise(Signal::Abort, &mut host, &costs), RaiseOutcome::Ignored);
    assert!(call_names(&host).is_empty(), "aborted bots absorb everything");
}

#[test]
fn abort_verb_is_the_player_scuttle() {
    let (mut vm, mut host, costs) = vm_for("abort()\nnever()\n");
    vm.grant(50, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Dead);
    let names = call_names(&host);
    assert_eq!(names, ["upload_log", "become_disabled"], "abort() runs the reserved sequence");
    assert!(vm.aborted());
}

#[test]
fn become_disabled_is_engine_only() {
    // Q76: the player-facing scuttle is abort(); a direct call must fault
    // err_unknown_function BEFORE the host ever sees it — deleting the
    // registry entry alone can't stop the passthrough dispatch, so the VM
    // blocks the name itself.
    let (mut vm, mut host, costs) = vm_for("become_disabled()\nhalt()\n");
    vm.grant(50, &costs);
    vm.run(&mut host, &costs);
    assert!(
        !call_names(&host).contains(&"become_disabled"),
        "the host must never see a player become_disabled call"
    );
    assert!(vm.fault_count() >= 1, "the call faults");
    assert!(!vm.is_dead(), "and the bot is NOT scuttled");
}

#[test]
fn signal_during_hurt_window_aborts() {
    let src = "\
on hurt:
    limp_home()

work()
";
    let (mut vm, mut host, costs) = vm_for(src);
    host.blocking.insert("limp_home".into());
    vm.raise(Signal::Hurt, &mut host, &costs);
    vm.grant(10, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked);
    // A second event mid-retreat: double handle → abort (wreck, not
    // vaporization) — the retreat is over, the rescue race starts here.
    assert_eq!(vm.raise(Signal::Bump, &mut host, &costs), RaiseOutcome::Aborted);
    let names = call_names(&host);
    assert!(names.contains(&"upload_log"), "the logs still go home");
    assert_eq!(*names.last().unwrap(), "become_disabled");
}

#[test]
fn signal_during_engine_interrupt_aborts() {
    let (mut vm, mut host, costs) = vm_for("work()\n");
    vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot)); // boot / recall context
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Aborted);
    assert!(vm.is_dead(), "a rescue under fire re-downs the bot");
}


// --- determinism ---

#[test]
fn identical_runs_produce_identical_traces() {
    let src = "\
n = 0
while n < 5:
    log(n * 3 % 4)
    n = n + 1
";
    let run = || {
        let (mut vm, mut host, costs) = vm_for(src);
        for _ in 0..30 {
            vm.grant(3, &costs);
            vm.run(&mut host, &costs);
        }
        host.calls
    };
    assert_eq!(run(), run());
}

#[test]
fn queued_program_installs_at_the_loop_boundary() {
    let (mut vm, mut host, costs) = vm_for("log(1)\n");
    vm.grant(1, &costs); // log = 1 full charge; the wrap is unaffordable
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["log"]);
    // Redeploy: takes effect at the wrap, not mid-pass.
    let new_program = parse("log(2)\nhalt()\n", &UnlockSet::all()).unwrap();
    vm.queue_program(Rc::new(new_program));
    vm.grant(20, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged.last(), Some(&&Value::Int(2)), "new program runs after the wrap");
}

// --- Result / .expect() / kind constants (generic queries) ---

/// vm_for, but with `ore` bound as a kind constant and `closest` returning
/// the given Result value.
fn vm_with_closest(source: &str, closest: Value) -> (Vm, TestHost, CostTable) {
    let program = parse(source, &UnlockSet::all()).expect("parse failed");
    let mut host = TestHost::default();
    host.blocking.insert("halt".into());
    host.returns.insert("closest".into(), closest);
    let mut config = VmConfig::default();
    config.constants.insert("ore".into(), Value::Str("ore".into()));
    (Vm::new(Rc::new(program), config), host, CostTable::default())
}

#[test]
fn expect_unwraps_ok() {
    let (mut vm, mut host, costs) =
        vm_with_closest("move_to(closest(ore).expect())\nhalt()\n", Value::result_ok(Value::Entity(7)));
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    // The kind constant arrives at the host as its string value.
    assert_eq!(host.calls[0], ("closest".to_string(), vec![Value::Str("ore".into())]));
    assert_eq!(host.calls[1], ("move_to".to_string(), vec![Value::Entity(7)]));
}

#[test]
fn expect_on_err_faults_with_the_carried_message() {
    let (mut vm, mut host, costs) = vm_with_closest(
        "move_to(closest(ore).expect())\n",
        Value::result_err("no ore anywhere"),
    );
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let dump = host
        .calls
        .iter()
        .find(|(n, _)| n == "upload_crash_dump")
        .expect("Err.expect() must fault into a crash dump");
    assert_eq!(dump.1, vec![Value::Str("no ore anywhere".into())]);
    assert!(!call_names(&host).contains(&"move_to"), "move_to must not run");
}

#[test]
fn match_handles_result_miss_without_fault() {
    let src = "\
match closest(ore):
    case Result.Ok(t):
        move_to(t)
    case Result.Err(msg):
        log(msg)
halt()
";
    let (mut vm, mut host, costs) = vm_with_closest(src, Value::result_err("no ore anywhere"));
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"log"), "Err arm must run, got {:?}", call_names(&host));
    assert!(!call_names(&host).contains(&"upload_crash_dump"), "no fault on the match path");
}

#[test]
fn expect_on_non_result_faults() {
    let (mut vm, mut host, costs) = vm_for("log(5.expect())\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump").expect("must fault");
    assert_eq!(dump.1, vec![Value::Str("expect() requires a Result or Option, got int".into())]);
}

#[test]
fn kind_constants_are_shadowable_and_faults_restore_them() {
    // Shadow `ore` on pass 1. Variables survive the wrap (Q80), so the
    // shadow persists into pass 2; a FAULT restart clears globals, and the
    // constant (living below globals) shows through again.
    let (mut vm, mut host, costs) =
        vm_with_closest("log(ore)\nore = 5\nlog(ore)\n", Value::Unit);
    vm.grant(5, &costs); // pass: log(1) + assign(1) + log(1) = 3, wrap(1), log(1)
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged[0], &Value::Str("ore".into()), "constant read");
    assert_eq!(logged[1], &Value::Int(5), "assignment shadows the constant");
    assert_eq!(logged[2], &Value::Int(5), "the shadow survives the wrap (Q80)");

    // A fault restart clears globals — the constant returns. Drip grants:
    // the bank cap means one grant can't fund dump debt + a fresh pass.
    let (mut vm, mut host, costs) =
        vm_with_closest("log(ore)\nore = 5\nboom(1 // 0)\n", Value::Unit);
    for _ in 0..5 {
        vm.grant(100, &costs);
        vm.run(&mut host, &costs);
    }
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged[0], &Value::Str("ore".into()));
    assert!(
        logged.iter().filter(|v| ***v == Value::Str("ore".into())).count() >= 2,
        "post-fault pass reads the constant again: {logged:?}"
    );
}

#[test]
fn signal_interrupts_a_blocked_action_cleanly() {
    // A template entry un-blocks (the pending action is abandoned — the
    // sim cancels the world side) and the stacks stay consistent: after
    // the template, the program restarts from line 1 without underflow.
    let (mut vm, mut host, costs) = vm_for("move_to(5)\nlog(1)\nhalt()\n");
    host.blocking.insert("move_to".into());
    vm.grant(100, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked);
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Handled);
    assert!(!vm.is_blocked(), "signals interrupt Blocked bots (docs/01)");
    vm.grant(100, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked, "restarted into move_to again");
    assert!(call_names(&host).contains(&"handler_init"), "the template ran");
}

// --- engine default handlers as real code ---

fn config_with_default(kind: pyrite::ast::SignalKind, source: &str) -> VmConfig {
    let mut config = VmConfig::default();
    let program = parse(source, &UnlockSet::all()).unwrap();
    config.default_handlers.insert(
        kind,
        pyrite::vm::DefaultHandler { source: source.to_string(), program: Rc::new(program) },
    );
    config
}

#[test]
fn factory_error_window_runs_as_watchable_code() {
    use pyrite::ast::SignalKind;
    let program = parse("boom(1 // 0)\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Error, "upload_crash_dump()\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    let costs = CostTable::default();
    // Enough to reach the fault, not enough to finish the factory window:
    // the VM sits INSIDE it, visibly (that's the point).
    vm.grant(5, &costs);
    vm.run(&mut host, &costs);
    assert!(vm.handler_is_default(), "should be mid factory window");
    assert_eq!(vm.crash_count(), 1, "factory contents are not armor — still a crash");
    // Finish it: the dump call happens FROM CODE, then restart + refault...
    vm.grant(60, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"upload_crash_dump"));
}

#[test]
fn factory_contents_double_handle_like_player_code() {
    // Q50: the humble-defaults carve-out is GONE — a signal landing on a
    // running factory window aborts exactly like one landing on yours.
    use pyrite::ast::SignalKind;
    let program = parse("boom(1 // 0)\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Error, "upload_crash_dump()\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    let costs = CostTable::default();
    vm.grant(5, &costs);
    vm.run(&mut host, &costs);
    assert!(vm.handler_is_default());
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Aborted);
    assert!(vm.is_dead());
    let names = call_names(&host);
    assert!(names.contains(&"upload_log"));
    assert_eq!(*names.last().unwrap(), "become_disabled");
}

#[test]
fn factory_bump_window_waits_in_code() {
    use pyrite::ast::SignalKind;
    let program = parse("work()\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Bump, "wait(50)\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    host.blocking.insert("wait".into());
    let costs = CostTable::default();
    assert_eq!(vm.raise(Signal::Bump, &mut host, &costs), RaiseOutcome::Handled);
    assert!(vm.handler_is_default());
    vm.grant(20, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked, "waiting inside the factory stun");
    assert!(call_names(&host).contains(&"wait"));
    // Resolve the wait: the template completes, the program restarts.
    vm.resolve_action(Ok(Value::Unit), &mut host, &costs);
    vm.grant(20, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"work"), "program restarts after the template");
}

#[test]
fn handler_init_window_is_observable() {
    use pyrite::ast::SignalKind;
    let program = parse("work()\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Bump, "wait(35)\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    host.blocking.insert("handler_init".into());
    host.blocking.insert("wait".into());
    let costs = CostTable::default();

    assert!(!vm.in_handler_init(), "quiet before the signal");
    vm.raise(Signal::Bump, &mut host, &costs);
    assert!(vm.in_handler_init(), "ritual pending from entry");
    vm.grant(20, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked, "blocked inside handler_init");
    assert!(vm.in_handler_init(), "still flinching while the init wait runs");
    // The init resolves: ritual over, the window proceeds (to wait(35)).
    vm.resolve_action(Ok(Value::Unit), &mut host, &costs);
    vm.grant(20, &costs);
    vm.run(&mut host, &costs);
    assert!(!vm.in_handler_init(), "ritual complete — now in the window");
    assert!(vm.active_signal().is_some(), "still handling the signal");
    assert!(call_names(&host).contains(&"wait"), "the window's wait(35) issued");
}

// --- deploy-time window analysis (M3) ---

#[test]
fn window_over_cap_is_a_deploy_error() {
    // bump's cap is 4 (costs.ron): five statements reject.
    let src = "\
on bump:
    log(1)
    log(2)
    log(3)
    log(4)
    log(5)
";
    let program = parse(src, &UnlockSet::all()).unwrap();
    let err = pyrite::check_windows(&program, &CostTable::default()).unwrap_err();
    assert!(
        matches!(err.kind, PyriteErrorKind::WindowOverCap { signal: "bump", worst: 5, cap: 4 }),
        "got: {err}"
    );
}

#[test]
fn if_branches_count_their_longest_arm() {
    // 1 (if) + max(2, 1) = 3 ≤ hurt's 6 — the worst case, not the sum.
    let src = "\
on hurt:
    if health_low():
        drop_cargo()
        upload_log()
    else:
        log(1)
";
    let program = parse(src, &UnlockSet::all()).unwrap();
    assert!(pyrite::check_windows(&program, &CostTable::default()).is_ok());
}

#[test]
fn non_signal_safe_calls_are_rejected_in_windows() {
    // attack is signal_safe: false in the registry.
    let src = "\
on hurt:
    attack(closest(enemy).expect())
";
    let program = parse(src, &UnlockSet::all()).unwrap();
    let err = pyrite::check_windows(&program, &CostTable::default()).unwrap_err();
    assert!(
        matches!(&err.kind, PyriteErrorKind::WindowUnsafeCall { signal: "hurt", func } if func == "attack"),
        "got: {err}"
    );
}

#[test]
fn loops_are_banned_window_reachable() {
    let direct = "\
on hurt:
    while True:
        log(1)
";
    let program = parse(direct, &UnlockSet::all()).unwrap();
    let err = pyrite::check_windows(&program, &CostTable::default()).unwrap_err();
    assert!(matches!(err.kind, PyriteErrorKind::WindowLoop { signal: "hurt" }), "got: {err}");

    // ... including through a def (docs/01: straight-line + if, all the
    // way down).
    let through_def = "\
def spin():
    while True:
        log(1)

on hurt:
    spin()
";
    let program = parse(through_def, &UnlockSet::all()).unwrap();
    let err = pyrite::check_windows(&program, &CostTable::default()).unwrap_err();
    assert!(matches!(err.kind, PyriteErrorKind::WindowLoop { signal: "hurt" }), "got: {err}");
}

#[test]
fn recursion_is_banned_window_reachable() {
    let src = "\
def a():
    b()

def b():
    a()

on hurt:
    a()
";
    let program = parse(src, &UnlockSet::all()).unwrap();
    let err = pyrite::check_windows(&program, &CostTable::default()).unwrap_err();
    assert!(
        matches!(err.kind, PyriteErrorKind::WindowRecursion { signal: "hurt", .. }),
        "got: {err}"
    );
}

#[test]
fn def_calls_charge_their_worst_case_against_the_cap() {
    // report() worst case = 3; the call statement charges max(1, 3) = 3,
    // + log(9) = 4 total > bumped's cap of 4? No: 3 + 1 = 4 = cap — OK.
    let ok = "\
def report():
    log(1)
    log(2)
    log(3)

on bumped:
    report()
    log(9)
";
    let program = parse(ok, &UnlockSet::all()).unwrap();
    assert!(pyrite::check_windows(&program, &CostTable::default()).is_ok());

    // One more statement in the def pushes the worst case over the cap —
    // you can't smuggle a long function through a short window.
    let over = "\
def report():
    log(1)
    log(2)
    log(3)
    log(4)

on bumped:
    report()
    log(9)
";
    let program = parse(over, &UnlockSet::all()).unwrap();
    let err = pyrite::check_windows(&program, &CostTable::default()).unwrap_err();
    assert!(matches!(err.kind, PyriteErrorKind::WindowOverCap { .. }), "got: {err}");
}

// --- modules & imports (docs/01 "Modules & the Program Library") ---

/// A deploy artifact: the editor inlines the library as `module` blocks;
/// `from m import f` binds the function bare.
#[test]
fn from_import_binds_module_functions_bare() {
    let src = "\
module hauling:
    def haul_home():
        deposit()

from hauling import haul_home

haul_home()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"deposit"));
}

/// `import m` enables qualified `m.f()` calls.
#[test]
fn plain_import_enables_qualified_calls() {
    let src = "\
module hauling:
    def haul_home():
        deposit()

import hauling

hauling.haul_home()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"deposit"));
}

/// Module functions call their siblings bare, including forward references.
#[test]
fn module_functions_resolve_sibling_calls() {
    let src = "\
module hauling:
    def haul_home():
        go_home()
        deposit()
    def go_home():
        beep()

from hauling import haul_home

haul_home()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let names = call_names(&host);
    assert!(names.contains(&"beep") && names.contains(&"deposit"));
}

/// Importing a module the source doesn't carry is a deploy error — the
/// library has no module by that name.
#[test]
fn importing_an_unknown_module_is_an_error() {
    let err = parse("import nosuch\n", &UnlockSet::all()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::UnknownModule("nosuch".into()));
    let err = parse("from nosuch import f\n", &UnlockSet::all()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::UnknownModule("nosuch".into()));
}

/// `from m import f` must name a function the module actually defines.
#[test]
fn from_import_of_a_missing_function_is_an_error() {
    let src = "\
module hauling:
    def haul_home():
        deposit()

from hauling import nosuch
";
    let err = parse(src, &UnlockSet::all()).unwrap_err();
    assert_eq!(
        err.kind,
        PyriteErrorKind::UnknownModuleMember { module: "hauling".into(), name: "nosuch".into() }
    );
}

/// Qualified calls require the plain import — carrying the module block
/// alone is not enough.
#[test]
fn qualified_call_without_import_is_an_error() {
    let src = "\
module hauling:
    def haul_home():
        deposit()

hauling.haul_home()
";
    let err = parse(src, &UnlockSet::all()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::ModuleNotImported("hauling".into()));
}

/// Modules are pure libraries: a module that did things on import would be
/// a program.
#[test]
fn statements_in_a_module_block_are_an_error() {
    let src = "\
module hauling:
    mine()
";
    let err = parse(src, &UnlockSet::all()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::StatementInModule);
}

/// A from-import can't shadow a local def (and vice versa).
#[test]
fn from_import_colliding_with_a_local_def_is_an_error() {
    let src = "\
module hauling:
    def haul_home():
        deposit()

def haul_home():
    deposit()

from hauling import haul_home
";
    let err = parse(src, &UnlockSet::all()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::DuplicateDefinition("haul_home".into()));
}

/// Imports are top-level declarations, like defs and handlers.
#[test]
fn imports_inside_blocks_are_an_error() {
    let src = "\
module hauling:
    def haul_home():
        deposit()

if True:
    import hauling
";
    let err = parse(src, &UnlockSet::all()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::HandlerNotAtTopLevel);
}

/// Import lines carry nothing else — trailing tokens are errors, not
/// silently-parsed statements.
#[test]
fn trailing_tokens_after_an_import_are_an_error() {
    let src = "\
module hauling:
    def haul_home():
        deposit()

import hauling extra
";
    assert!(parse(src, &UnlockSet::all()).is_err());
}

// --- docstrings ---

/// A leading bare string in a def body is its docstring: captured on the
/// Function, stripped from the runtime block (free — it doesn't exist at
/// runtime), and the rest of the body runs normally.
#[test]
fn docstrings_are_captured_and_stripped() {
    let src = "\
def haul():
    \"\"\"Take cargo home.\"\"\"
    deposit()

haul()
halt()
";
    let program = parse(src, &UnlockSet::all()).expect("docstring def parses");
    let f = &program.functions["haul"];
    assert_eq!(f.doc.as_deref(), Some("Take cargo home."));
    assert_eq!(f.body.len(), 1, "docstring must not remain in the runtime body");

    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"deposit"));
}

/// Triple-quoted strings span lines; the content is raw with literal
/// newlines.
#[test]
fn multi_line_docstrings_lex() {
    let src = "\
def haul():
    \"\"\"Take cargo home.
    Walks to the nearest depot.\"\"\"
    deposit()
";
    let program = parse(src, &UnlockSet::all()).expect("multi-line docstring parses");
    let doc = program.functions["haul"].doc.clone().unwrap();
    assert!(doc.contains("Take cargo home.\n"));
    assert!(doc.contains("Walks to the nearest depot."));
}

/// Python allows a docstring-only body; so do we (a documented no-op).
#[test]
fn docstring_only_def_is_legal() {
    let src = "\
def todo():
    \"\"\"Not written yet.\"\"\"

todo()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"halt"), "empty documented def returns cleanly");
}

/// Outside the docstring position a triple-quoted string is an ordinary
/// string value.
#[test]
fn triple_quoted_strings_are_ordinary_values() {
    let src = "\
x = \"\"\"big \"quote\" here\"\"\"
log(x)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let logged = host.calls.iter().find(|(n, _)| n == "log").unwrap();
    assert_eq!(logged.1[0], Value::Str("big \"quote\" here".into()));
}

/// An unclosed triple quote reports at the opener, not the end of file.
#[test]
fn unterminated_triple_quote_is_an_error() {
    let err = parse("x = \"\"\"never closed\nmine()\n", &UnlockSet::all()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::UnterminatedString);
    assert_eq!(err.line, 1);
}

// --- containers (docs/01 Tier 5: lists & dicts, value semantics) ---

/// Lists: append writes back through the variable, len() counts, index
/// assignment mutates in place.
#[test]
fn list_append_len_and_index_assignment() {
    let src = "\
xs = [1, 2]
xs.append(3)
xs[0] = 9
log(len(xs))
log(xs[0])
log(xs[2])
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(3), &Value::Int(9), &Value::Int(3)]);
}

/// Dicts: literals, key writes, reads, membership, len.
#[test]
fn dict_literal_write_read_membership() {
    let src = "\
d = {\"ore\": 3, \"scrap\": 1}
d[\"ore\"] = 4
d[7] = \"seven\"
log(d[\"ore\"])
log(len(d))
log(\"scrap\" in d)
log(\"gold\" in d)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(
        logged,
        [&Value::Int(4), &Value::Int(3), &Value::Bool(true), &Value::Bool(false)]
    );
}

/// Missing keys fault on `d[k]`; `d.get(k)` is the fault-free Option form
/// (and `.expect()` unwraps Options like Results).
#[test]
fn dict_missing_key_faults_and_get_returns_option() {
    let (mut vm, mut host, costs) = vm_for("d = {1: 2}\nlog(d[9])\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump").expect("must fault");
    assert!(matches!(&dump.1[0], Value::Str(s) if s.contains("key 9 not found")));

    let src = "\
d = {1: 2}
log(d.get(1).expect())
match d.get(9):
    case Option.Some(v):
        log(v)
    case Option.None:
        log(\"absent\")
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(2), &Value::Str("absent".into())]);
}

/// Dict iteration is sorted key order — deterministic by construction,
/// never insertion order.
#[test]
fn dict_iteration_is_sorted_key_order() {
    let src = "\
d = {\"zebra\": 1, \"ant\": 2, \"moth\": 3}
for k in d:
    log(k)
for v in d.values():
    log(v)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(
        logged,
        [
            &Value::Str("ant".into()),
            &Value::Str("moth".into()),
            &Value::Str("zebra".into()),
            &Value::Int(2),
            &Value::Int(3),
            &Value::Int(1),
        ]
    );
}

/// Containers are values: passing to a def copies; index assignment inside
/// a def mutates the outer variable (Python-consistent: only `xs = ...`
/// makes a local).
#[test]
fn containers_are_values_with_python_write_back() {
    let src = "\
def eat(copy):
    copy.append(99)
    return len(copy)

def poke():
    xs[0] = 42

xs = [1]
log(eat(xs))
log(len(xs))
poke()
log(xs[0])
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(2), &Value::Int(1), &Value::Int(42)]);
}

/// range() builds int lists (one or two bounds) and enforces its cap.
#[test]
fn range_builds_lists_under_a_cap() {
    let src = "\
total = 0
for i in range(4):
    total = total + i
for i in range(10, 13):
    total = total + i
log(total)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    // Both loops cost more than one bank_cap of cycles: drip grants.
    for _ in 0..10 {
        vm.grant(25, &costs);
        vm.run(&mut host, &costs);
    }
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(6 + 33)]);

    let (mut vm, mut host, costs) = vm_for("range(100000)\n");
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump").expect("must fault");
    assert!(matches!(&dump.1[0], Value::Str(s) if s.contains("range too large")));
}

/// Mutating a temporary is a fault, not a silent no-op — containers are
/// values, so the mutation would vanish.
#[test]
fn appending_to_a_temporary_faults() {
    let (mut vm, mut host, costs) = vm_for("[1, 2].append(3)\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump").expect("must fault");
    assert!(matches!(&dump.1[0], Value::Str(s) if s.contains("needs a list variable")));
}

/// dict.remove(k) deletes through the variable and returns the Option.
#[test]
fn dict_remove_writes_back() {
    let src = "\
d = {1: \"a\", 2: \"b\"}
log(d.remove(1).expect())
log(len(d))
log(1 in d)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Str("a".into()), &Value::Int(1), &Value::Bool(false)]);
}

// --- M1: keyword arguments & defaults ---

/// User defs bind positionals, then keywords, then literal defaults.
#[test]
fn user_def_kwargs_and_defaults() {
    let src = "\
def report(val, level=\"info\", repeat=1):
    log(level)
    log(val * repeat)

report(5)
report(6, level=\"warn\")
report(7, repeat=3)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(
        logged,
        [
            &Value::Str("info".into()),
            &Value::Int(5),
            &Value::Str("warn".into()),
            &Value::Int(6),
            &Value::Str("info".into()),
            &Value::Int(21),
        ]
    );
}

/// Registry builtins canonicalize keywords + defaults before the host call:
/// the host always sees the full positional form.
#[test]
fn builtin_kwargs_canonicalize_against_the_registry() {
    // Levels are the pre-bound int constants (trace=0 … error=4); the
    // sim binds them — tests pass the ints directly.
    let (mut vm, mut host, costs) = vm_for("log(9, level=3)\nlog(8)\nhalt()\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let log_calls: Vec<&Vec<Value>> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| a).collect();
    assert_eq!(log_calls[0], &vec![Value::Int(9), Value::Int(3)]);
    assert_eq!(
        log_calls[1],
        &vec![Value::Int(8), Value::Int(2)],
        "the default level (info = 2) fills in"
    );
}

/// Unknown keywords and missing required args fault with err_arity.
#[test]
fn bad_kwargs_fault_with_err_arity() {
    let (mut vm, mut host, costs) = vm_for("log(1, volume=11)\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(vm.last_fault_id(), Some(pyrite::faults::ARITY));

    let (mut vm, mut host, costs) = vm_for("def f(a, b):\n    return a\n\nf(1)\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(vm.last_fault_id(), Some(pyrite::faults::ARITY));
    assert!(vm.last_fault().unwrap().contains("missing required argument 'b'"));
}

// --- M1: None is reserved sugar for Option.None ---

#[test]
fn none_is_option_none() {
    let src = "\
x = None
match x:
    case None:
        log(\"none\")
    case _:
        log(\"other\")
log(x == None)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Str("none".into()), &Value::Bool(true)]);
}

#[test]
fn assigning_to_none_is_a_parse_error() {
    assert!(parse("None = 5\n", &UnlockSet::all()).is_err());
}

/// Builtin enums construct from source (docs/01 Types): Option.Some(v),
/// Result.Ok/Err — and .expect() unwraps them.
#[test]
fn builtin_enums_construct_from_source() {
    let src = "\
x = Option.Some(4)
log(x.expect())
y = Result.Err(\"nope\")
match y:
    case Result.Err(msg):
        log(msg)
    case _:
        log(0)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(4), &Value::Str("nope".into())]);
}

// --- M1: structural enum identity includes arity (Q80) ---

/// A wrong-arity pattern is a non-match that falls through, not a fault.
#[test]
fn match_arity_mismatch_falls_through() {
    let src = "\
enum Msg:
    Ping(a)

m = Msg.Ping(1)
match m:
    case Msg.Ping(a, b):
        log(\"two\")
    case Msg.Ping(a):
        log(a)
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(1)], "arity mismatch falls to the next arm");
}

// --- M1: fault-id constants ---

/// Fault ids are pre-bound constants, comparable with == in handlers.
#[test]
fn fault_ids_are_prebound_constants() {
    let (mut vm, mut host, costs) = vm_for("boom(1 // 0)\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(vm.last_fault_id(), Some(pyrite::faults::DIV_ZERO));

    // The constant is readable from source without any host setup.
    let (mut vm, mut host, costs) = vm_for("log(err_div_zero)\nhalt()\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    let logged = host.calls.iter().find(|(n, _)| n == "log").unwrap();
    assert_eq!(logged.1[0], Value::Str("err_div_zero".into()));
}

// --- M1: payload-sized costs & payload_cap (Q82) ---

/// An oversized payload faults err_payload; the send never reaches the host.
#[test]
fn oversized_payload_faults() {
    let (mut vm, mut host, costs) =
        vm_for("try_send(\"ch\", \"a very long message indeed\")\n");
    vm.grant(1000, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(vm.last_fault_id(), Some(pyrite::faults::PAYLOAD));
    assert!(!call_names(&host).contains(&"try_send"), "host must not see the send");
}

/// Payload units price the call: try_send = 3 + size (full charge).
#[test]
fn payload_units_price_sized_ops() {
    // "abcd" = 4 units → 3 + 4 = 7 cycles total.
    let (mut vm, mut host, costs) = vm_for("try_send(\"c\", \"abcd\")\n");
    vm.grant(6, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Paused);
    assert!(host.calls.is_empty());
    vm.grant(1, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["try_send"]);
}

/// upload_log = min(5 + buffered, 25): a huge buffer caps at the dump's 25.
#[test]
fn upload_log_cost_caps_at_the_dump(){
    let (mut vm, mut host, costs) = vm_for("upload_log()\n");
    host.log_len = 100;
    vm.grant(24, &costs);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Paused);
    assert!(host.calls.is_empty());
    vm.grant(1, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["upload_log"]);
}

// --- audit regressions: reserved names & kwargs on unknown functions ---

/// The engine verbs are FULLY reserved (M3): a player def would shadow the
/// scuttle / the forced prologue, because user functions resolve before
/// the VM's name intercept.
#[test]
fn reserved_engine_verbs_cannot_be_defined() {
    for name in ["abort", "handler_init", "become_disabled"] {
        let src = format!("def {name}():\n    log(1)\n");
        let err = parse(&src, &UnlockSet::all()).unwrap_err();
        assert!(
            matches!(&err.kind, PyriteErrorKind::ReservedName(n) if n == name),
            "def {name} must be a reserved-name parse error, got: {err}"
        );
    }
}

/// Option/Result are language furniture (`None` = Option.None, `.expect()`):
/// a player redeclaration would shadow them in pattern resolution.
#[test]
fn builtin_enums_cannot_be_redeclared() {
    for name in ["Option", "Result"] {
        let src = format!("enum {name}:\n    Weird\n");
        let err = parse(&src, &UnlockSet::all()).unwrap_err();
        assert!(
            matches!(&err.kind, PyriteErrorKind::ReservedName(n) if n == name),
            "enum {name} must be a reserved-name parse error, got: {err}"
        );
    }
}

/// handler_init is engine-injected (line 0 only); a player call faults
/// err_unknown_function, and a fault inside a template is a double-handle
/// abort — so deploy analysis must reject it in windows, never at runtime.
#[test]
fn handler_init_in_a_window_is_rejected_at_deploy() {
    let src = "\
on hurt:
    handler_init()
";
    let program = parse(src, &UnlockSet::all()).unwrap();
    let err = pyrite::check_windows(&program, &CostTable::default()).unwrap_err();
    assert!(
        matches!(&err.kind, PyriteErrorKind::WindowUnsafeCall { signal: "hurt", func } if func == "handler_init"),
        "got: {err}"
    );
}

/// Keywords on a name the registry doesn't know at all: the name is the
/// error (err_unknown_function), not the keyword binding (err_arity).
#[test]
fn kwargs_on_an_unknown_function_fault_unknown_function() {
    let (mut vm, mut host, costs) = vm_for("bogus(x=1)\nhalt()\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(vm.last_fault_id(), Some(pyrite::faults::UNKNOWN_FUNCTION));
    assert!(!call_names(&host).contains(&"bogus"), "the host must not see the call");
}

// --- M5: the bank cap + blocked-grant rules ---

/// The budget clamps to the table-derived bank_cap after every grant
/// (Q75/Q82) — saving up can't overshoot the priciest effective op. The
/// cap bounds SAVING, not throughput: a single grant bigger than the cap
/// (a fast CPU's tick) is still fully spendable that tick.
#[test]
fn budget_banks_to_the_derived_cap() {
    let (mut vm, _host, costs) = vm_for("wait(1)\nhalt()\n");
    vm.grant(1_000_000, &costs);
    assert_eq!(vm.budget(), 1_000_000 * 100, "one grant is always fully spendable");
    vm.grant(1, &costs);
    assert_eq!(
        vm.budget(),
        costs.bank_cap as i64 * 100,
        "unspent surplus can't be BANKED past the cap"
    );
    vm.grant(1, &costs);
    assert_eq!(vm.budget(), costs.bank_cap as i64 * 100, "re-grants never overshoot");
}

/// No banking while blocked (docs/01): a waiting bot burns its grant —
/// the rule lives in the VM now, not as a sim special case.
#[test]
fn blocked_bots_bank_nothing() {
    let (mut vm, mut host, costs) = vm_for("halt()\n");
    vm.grant(5, &costs);
    vm.run(&mut host, &costs); // parks Blocked on halt()
    assert!(vm.is_blocked());
    let banked = vm.budget();
    vm.grant(10, &costs);
    assert_eq!(vm.budget(), banked, "waiting is what its CPU is doing");
}

#[test]
fn hardware_bar_counts_code_not_comments_or_frame_locals() {
    // docs/01 Q80: def bodies are frame-local — params and in-def locals
    // never occupy a global variable slot. docs/01: docstrings, comments,
    // blanks, and imports are not runtime code (review 2026-07-17).
    let bare = "x = 1\ndef helper(a, b):\n    tmp = a + b\n    return tmp\ny = helper(x, 2)\n";
    let padded = "# comment\n\nx = 1\n\ndef helper(a, b):\n    \"\"\"doc line\"\"\"\n    tmp = a + b\n    return tmp\n\ny = helper(x, 2)\n# trailing comment\n";
    let p1 = parse(bare, &UnlockSet::all()).unwrap();
    let p2 = parse(padded, &UnlockSet::all()).unwrap();
    let r1 = pyrite::analysis::artifact_requirements(bare, &p1);
    let r2 = pyrite::analysis::artifact_requirements(padded, &p2);
    assert_eq!(r1, r2, "comments/blanks/docstrings never change the hardware bar");
    assert_eq!(r1.1, 2, "only top-level globals (x, y) count; a/b/tmp are frame-local");
    assert!(
        r1.0 < padded.lines().count() as u32,
        "non-code physical lines don't count toward program memory"
    );
}

/// The `%` operator is checked like `//`: i64::MIN % -1 overflows the raw
/// operator, and the Mod arm must raise a trappable err_overflow fault, never
/// panic a debug build (release would silently yield 0 — a determinism gap).
/// i64::MIN is built as MIN+1 - 1 (the literal for its magnitude is
/// rejected at lex time).
#[test]
fn mod_by_negative_one_at_int_min_faults_not_panics() {
    let (mut vm, mut host, costs) = vm_for("boom((-9223372036854775807 - 1) % -1)\n");
    vm.grant(100, &costs);
    vm.run(&mut host, &costs);
    assert_eq!(vm.last_fault_id(), Some(pyrite::faults::OVERFLOW));
}
