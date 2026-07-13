//! Integration tests: parsing, gating, cycle metering, faults, handlers,
//! double-handle, blocking actions. Each maps to a rule in docs/01-language.md.

use pyrite::vm::{CallCtx, Host, HostCall, Outcome, RaiseOutcome, Signal, Vm, VmConfig};
use pyrite::{parse, Construct, CostTable, PyriteErrorKind, UnlockSet, Value};
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
}

impl Host for TestHost {
    fn call(&mut self, name: &str, args: &[Value], _ctx: CallCtx<'_>) -> HostCall {
        self.calls.push((name.to_string(), args.to_vec()));
        if let Some(msg) = self.faulting.get(name) {
            return HostCall::Fault(msg.clone());
        }
        if self.blocking.contains(name) {
            return HostCall::Block;
        }
        HostCall::Ready(self.returns.get(name).cloned().unwrap_or(Value::Unit))
    }

    fn attr(&mut self, entity: u64, name: &str) -> Result<Value, String> {
        match name {
            "id" => Ok(Value::Int(entity as i64)),
            _ => Err(format!("unknown attribute {name}")),
        }
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
fn signal_handler_requires_unlock() {
    let src = "on signal(s):\n    drop_cargo()\n";
    let err = parse(src, &UnlockSet::none()).unwrap_err();
    assert_eq!(err.kind, PyriteErrorKind::LockedConstruct(Construct::OnSignal));
    assert!(parse(src, &UnlockSet::none().with(Construct::OnSignal)).is_ok());
}

#[test]
fn locked_construct_error_names_the_unlock() {
    let err = parse("x = 1\n", &UnlockSet::none()).unwrap_err();
    assert!(err.to_string().contains("requires unlock: Variables"), "got: {err}");
}

// --- cycle metering ---

#[test]
fn tier0_line_costs_match_table() {
    // mine() = statement(1) + call_base(1) + builtin mine(2) = 4 cycles.
    let (mut vm, mut host, costs) = vm_for("mine()\n");
    vm.grant(3);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Paused);
    assert!(host.calls.is_empty(), "should not have run yet at 3 cycles");
    vm.grant(1);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["mine"], "4th cycle completes the call");
}

#[test]
fn cycle_debt_ops_wait_until_affordable() {
    // Expensive op executes only once enough cycles accumulate across grants.
    let (mut vm, mut host, costs) = vm_for("scan_enemies()\n");
    // statement(1) + call_base(1) + scan(4) = 6 total; grant 2/tick.
    for _ in 0..2 {
        vm.grant(2);
        assert_eq!(vm.run(&mut host, &costs), Outcome::Paused);
    }
    assert!(host.calls.is_empty());
    vm.grant(2);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["scan_enemies"]);
}

#[test]
fn program_loops_forever_and_wrap_clears_variables() {
    // One pass: x = 1 (assign 1), log(x) (stmt 1 + call 1 + log 1 = 3).
    // Wrap costs statement(1). Second pass reads x fresh after re-assign —
    // but if we *only* read x on pass 2, it must fault. Here we re-assign,
    // so two full passes should produce two log calls.
    let (mut vm, mut host, costs) = vm_for("x = 1\nlog(x)\n");
    vm.grant(9); // pass(4) + wrap(1) + pass(4)
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["log", "log"]);
}

#[test]
fn variables_do_not_survive_the_wrap() {
    // First line reads x, which is only set later in the program: pass 1
    // faults immediately (read of unset variable), crash-dumps, restarts.
    let (mut vm, mut host, costs) = vm_for("log(x)\nx = 1\n");
    vm.grant(100);
    vm.run(&mut host, &costs);
    assert!(
        call_names(&host).contains(&"upload_crash_dump"),
        "unset read must force a crash dump, got {:?}",
        call_names(&host)
    );
}

// --- expressions ---

#[test]
fn arithmetic_and_comparisons() {
    let (mut vm, mut host, costs) = vm_for("log(2 + 3 * 4)\nlog(7 // 2)\nlog(-7 // 2)\nlog(-7 % 2)\nlog(1 < 2)\nhalt()\n");
    vm.grant(200);
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
    vm.grant(100);
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
    vm.grant(100);
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
    vm.grant(200);
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
    vm.grant(500);
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
    vm.grant(100);
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
    vm.grant(1000);
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
    vm.grant(100);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(7)]);
}

#[test]
fn match_on_builtin_enum_from_host() {
    // try_receive returns Recv.Got(v) / Recv.Empty without a declaration.
    let src = "\
match try_receive(\"orders\"):
    case Recv.Got(v):
        log(v)
    case Recv.Empty:
        idle()
halt()
";
    let (mut vm, mut host, costs) = vm_for(src);
    host.returns.insert(
        "try_receive".into(),
        Value::Enum(pyrite::EnumValue {
            enum_name: "Recv".into(),
            variant: "Got".into(),
            fields: vec![Value::Int(99)],
        }),
    );
    vm.grant(100);
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
    vm.grant(100);
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
    vm.grant(100);
    vm.run(&mut host, &costs);
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump");
    assert!(matches!(dump, Some((_, args)) if matches!(&args[0], Value::Str(s) if s.contains("no ore"))));
}

// --- faults & handlers ---

#[test]
fn unhandled_fault_forces_crash_dump_and_restarts() {
    let (mut vm, mut host, costs) = vm_for("log(1)\nboom(1 // 0)\n");
    vm.grant(50);
    vm.run(&mut host, &costs);
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
    vm.grant(4); // enough to reach the fault, nowhere near dump cost
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"upload_crash_dump"));
    assert!(vm.budget() < 0, "crash dump must leave the bot in cycle debt");
    assert_eq!(vm.fault_count(), 1, "one fault, one count");
    assert_eq!(vm.crash_count(), 1, "unhandled fault counts as a crash");
}

#[test]
fn error_handler_runs_instead_of_crash_dump() {
    let src = "\
on signal(s):
    handled()

boom(1 // 0)
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(50);
    vm.run(&mut host, &costs);
    let names = call_names(&host);
    assert!(names.contains(&"handled"));
    assert!(!names.contains(&"upload_crash_dump"));
    assert_eq!(vm.crash_count(), 0, "handled faults are not crashes");
}

#[test]
fn variables_preserved_during_handler_cleared_after() {
    let src = "\
on signal(s):
    log(x)

x = 42
boom(1 // 0)
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(14); // x=42 (1), stmt(1)+call(1)+arith(1)=fault, trap(5), handler: stmt+call+log = ...
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged, [&Value::Int(42)], "handler must see pre-fault variables");
}

#[test]
fn fault_inside_error_handler_is_double_handle() {
    let src = "\
on signal(s):
    boom(1 // 0)

boom(2 // 0)
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(100);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Exploded);
}

#[test]
fn overtime_doubles_costs_after_grace_window() {
    // Handler body: infinite busy loop. Grace window is 10 ticks; after
    // that every op costs double, so per-grant progress halves.
    let src = "\
on signal(s):
    while True:
        spin()

boom(1 // 0)
";
    let (mut vm, mut host, costs) = vm_for(src);
    // Reach the fault + enter handler.
    vm.grant(20);
    vm.run(&mut host, &costs);
    let spins_before = |h: &TestHost| h.calls.iter().filter(|(n, _)| n == "spin").count();
    // Within grace: 12 cycles per grant buys N spins.
    host.calls.clear();
    vm.grant(12);
    vm.run(&mut host, &costs);
    let in_grace = spins_before(&host);
    // Push the handler clock well past the grace window.
    for _ in 0..15 {
        vm.grant(0);
    }
    host.calls.clear();
    vm.grant(24); // double the budget should buy the same number of spins
    vm.run(&mut host, &costs);
    let in_overtime = spins_before(&host);
    assert_eq!(
        in_grace, in_overtime,
        "24 overtime cycles should buy what 12 normal cycles bought"
    );
}

// --- signals: hurt / death / double-handle ---

#[test]
fn hurt_handler_runs_then_program_restarts() {
    let src = "\
on signal(s):
    drop_cargo()

work()
";
    let (mut vm, mut host, costs) = vm_for(src);
    vm.grant(3);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["work"]);
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Handled);
    host.calls.clear();
    vm.grant(20);
    vm.run(&mut host, &costs);
    let names = call_names(&host);
    assert_eq!(names[0], "handler_init", "every unified handler starts with the entry ritual");
    assert_eq!(names[1], "drop_cargo");
    assert!(names.contains(&"work"), "program restarts from line 1 after handler");
}

#[test]
fn hurt_without_handler_is_ignored() {
    let (mut vm, mut host, costs) = vm_for("work()\n");
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Ignored);
}

#[test]
fn death_without_handler_calls_become_disabled() {
    let (mut vm, mut host, costs) = vm_for("work()\n");
    assert_eq!(vm.raise(Signal::Death, &mut host, &costs), RaiseOutcome::Died);
    assert_eq!(call_names(&host), ["become_disabled"]);
    assert!(vm.is_dead());
}

#[test]
fn death_handler_runs_within_blackbox_budget() {
    let src = "\
on death:
    log(1)
    upload_log()

work()
";
    let (mut vm, mut host, costs) = vm_for(src);
    assert_eq!(vm.raise(Signal::Death, &mut host, &costs), RaiseOutcome::Handled);
    vm.grant(100);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Dead);
    let names = call_names(&host);
    // Black box: log(3) + upload_log (would be 7 total > 10? log: stmt1+call1+log1=3;
    // upload: stmt1+call1+upload5=7; 3+7=10 → exactly the budget.
    assert!(names.contains(&"log"));
    assert!(names.contains(&"upload_log"));
    assert_eq!(*names.last().unwrap(), "become_disabled", "death always exits through become_disabled");
}

#[test]
fn death_handler_budget_cuts_off_greedy_code() {
    let src = "\
on death:
    expensive_scan()
    log(1)

work()
";
    let (mut vm, mut host, costs) = vm_for(src);
    // expensive_scan is unknown → call_base(1)+default(1)+stmt(1) = 3;
    // then log = 3 more; fine. Make it pricey instead:
    let mut costs = costs;
    costs.builtins.insert("expensive_scan".into(), 20);
    vm.raise(Signal::Death, &mut host, &costs);
    vm.grant(100);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Dead);
    let names = call_names(&host);
    assert!(!names.contains(&"expensive_scan"), "op exceeding black-box budget must not run");
    assert_eq!(*names.last().unwrap(), "become_disabled");
}

#[test]
fn signal_during_hurt_handler_explodes() {
    let src = "\
on signal(s):
    limp_home()

work()
";
    let (mut vm, mut host, costs) = vm_for(src);
    host.blocking.insert("limp_home".into());
    vm.raise(Signal::Hurt, &mut host, &costs);
    vm.grant(10);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked);
    // Lethal damage mid-retreat: double handle, no death handler runs.
    assert_eq!(vm.raise(Signal::Death, &mut host, &costs), RaiseOutcome::Exploded);
    assert!(!call_names(&host).contains(&"become_disabled"));
}

#[test]
fn signal_during_engine_interrupt_explodes() {
    let (mut vm, mut host, costs) = vm_for("work()\n");
    vm.set_engine_interrupt(true); // boot / recall context
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Exploded);
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
            vm.grant(3);
            vm.run(&mut host, &costs);
        }
        host.calls
    };
    assert_eq!(run(), run());
}

#[test]
fn queued_program_installs_at_the_loop_boundary() {
    let (mut vm, mut host, costs) = vm_for("log(1)\n");
    vm.grant(3);
    vm.run(&mut host, &costs);
    assert_eq!(call_names(&host), ["log"]);
    // Redeploy: takes effect at the wrap, not mid-pass.
    let new_program = parse("log(2)\nhalt()\n", &UnlockSet::all()).unwrap();
    vm.queue_program(Rc::new(new_program));
    vm.grant(20);
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
    vm.grant(100);
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
    vm.grant(100);
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
    vm.grant(100);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"log"), "Err arm must run, got {:?}", call_names(&host));
    assert!(!call_names(&host).contains(&"upload_crash_dump"), "no fault on the match path");
}

#[test]
fn expect_on_non_result_faults() {
    let (mut vm, mut host, costs) = vm_for("log(5.expect())\n");
    vm.grant(100);
    vm.run(&mut host, &costs);
    let dump = host.calls.iter().find(|(n, _)| n == "upload_crash_dump").expect("must fault");
    assert_eq!(dump.1, vec![Value::Str("expect() requires a Result, got int".into())]);
}

#[test]
fn kind_constants_are_shadowable_and_survive_the_wrap() {
    // Shadow `ore` on pass 1; the wrap clears globals, so pass 2 reads the
    // constant again — constants live below globals and survive resets.
    let (mut vm, mut host, costs) =
        vm_with_closest("log(ore)\nore = 5\nlog(ore)\n", Value::Unit);
    vm.grant(11); // pass: log(3) + assign(2) + log(3) = 8, wrap(1), log(3) → 12 lands mid
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged[0], &Value::Str("ore".into()), "constant read");
    assert_eq!(logged[1], &Value::Int(5), "assignment shadows the constant");
    vm.grant(100);
    vm.run(&mut host, &costs);
    let logged: Vec<&Value> =
        host.calls.iter().filter(|(n, _)| n == "log").map(|(_, a)| &a[0]).collect();
    assert_eq!(logged[2], &Value::Str("ore".into()), "wrap restores the constant");
}

#[test]
fn ignored_signal_does_not_unblock_a_pending_action() {
    // Regression: hurt with NO handler while blocked on an action must
    // leave the VM blocked — un-blocking desynced the work/value stacks
    // (stack underflow on the next run).
    let (mut vm, mut host, costs) = vm_for("move_to(5)\nlog(1)\nhalt()\n");
    host.blocking.insert("move_to".into());
    vm.grant(100);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked);
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Ignored);
    assert!(vm.is_blocked(), "ignored signal must not unblock");
    vm.resolve_action(Ok(Value::Unit), &mut host, &costs);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"log"), "program continues cleanly");
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
fn default_error_handler_runs_as_watchable_code() {
    use pyrite::ast::SignalKind;
    let program = parse("boom(1 // 0)\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Signal, "upload_crash_dump()\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    let costs = CostTable::default();
    // Enough to reach the fault, not enough to finish the default handler:
    // the VM sits INSIDE it, visibly (that's the point).
    vm.grant(5);
    vm.run(&mut host, &costs);
    assert!(vm.handler_is_default(), "should be mid default handler");
    assert_eq!(vm.crash_count(), 1, "default error handling still counts as a crash");
    // Finish it: the dump call happens FROM CODE, then restart + refault...
    vm.grant(60);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"upload_crash_dump"));
}

#[test]
fn default_handlers_are_humble_not_double_handles() {
    use pyrite::ast::SignalKind;
    let program = parse("boom(1 // 0)\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Signal, "upload_crash_dump()\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    let costs = CostTable::default();
    vm.grant(5);
    vm.run(&mut host, &costs);
    assert!(vm.handler_is_default());
    // A signal arriving mid-DEFAULT does not explode: the humble default
    // yields, and the UNIFIED default handles the hurt fresh.
    assert_eq!(vm.raise(Signal::Hurt, &mut host, &costs), RaiseOutcome::Handled);
    assert!(vm.handler_is_default());
    assert!(!vm.is_dead());
    // Death mid-default is processed normally (wreck, not explosion).
    assert_eq!(vm.raise(Signal::Death, &mut host, &costs), RaiseOutcome::Died);
    assert!(call_names(&host).contains(&"become_disabled"));
}

#[test]
fn default_bump_handler_waits_in_code() {
    use pyrite::ast::SignalKind;
    let program = parse("work()\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Signal, "wait(50)\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    host.blocking.insert("wait".into());
    let costs = CostTable::default();
    assert_eq!(vm.raise(Signal::Bump, &mut host, &costs), RaiseOutcome::Handled);
    assert!(vm.handler_is_default());
    vm.grant(20);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked, "waiting inside the default");
    assert!(call_names(&host).contains(&"wait"));
    // Resolve the wait: default completes, program restarts.
    vm.resolve_action(Ok(Value::Unit), &mut host, &costs);
    vm.grant(20);
    vm.run(&mut host, &costs);
    assert!(call_names(&host).contains(&"work"), "program restarts after the default");
}


#[test]
fn handler_init_window_is_observable() {
    use pyrite::ast::SignalKind;
    let program = parse("work()\n", &UnlockSet::all()).unwrap();
    let config = config_with_default(SignalKind::Signal, "wait(35)\n");
    let mut vm = Vm::new(Rc::new(program), config);
    let mut host = TestHost::default();
    host.blocking.insert("handler_init".into());
    host.blocking.insert("wait".into());
    let costs = CostTable::default();

    assert!(!vm.in_handler_init(), "quiet before the signal");
    vm.raise(Signal::Bump, &mut host, &costs);
    assert!(vm.in_handler_init(), "ritual pending from entry");
    vm.grant(20);
    assert_eq!(vm.run(&mut host, &costs), Outcome::Blocked, "blocked inside handler_init");
    assert!(vm.in_handler_init(), "still flinching while the init wait runs");
    // The init resolves: ritual over, handler body proceeds (to wait(35)).
    vm.resolve_action(Ok(Value::Unit), &mut host, &costs);
    vm.grant(20);
    vm.run(&mut host, &costs);
    assert!(!vm.in_handler_init(), "ritual complete — now in the handler body");
    assert!(vm.active_signal().is_some(), "still handling the signal");
    assert!(call_names(&host).contains(&"wait"), "the body's wait(35) issued");
}
