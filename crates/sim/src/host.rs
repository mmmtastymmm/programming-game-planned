//! The sim's `pyrite::Host` implementation: one per (world, bot) pair while
//! that bot's VM is stepping. Queries answer instantly; actions record an
//! `ActionRequest` and return `Block` — the resolve phase starts them.

use crate::world::{ActionRequest, ArchiveEntry, ArchiveKind, BotId, EntityId, World, LOG_BUFFER_CAP};
use pyrite::vm::CallCtx;
use pyrite::{HostCall, Value};

/// Entity kinds understood by the generic queries (`exists(kind)`,
/// `closest(kind)`). Each is bound as a global constant of the same name in
/// every bot VM (see `Sim::new`), so programs write `closest(ore)` bare.
pub const KINDS: &[&str] = &["blueprint", "depot", "enemy", "ore"];

/// Editor-facing documentation for one callable (builtin or the `.expect()`
/// method). Cycle costs are deliberately NOT written here — the editor reads
/// them from the live cost table, so tuning changes can't leave docs stale.
pub struct BuiltinDoc {
    pub name: &'static str,
    pub signature: &'static str,
    pub summary: &'static str,
    /// What the base cost excludes, e.g. " + travel" (empty when flat).
    pub cost_note: &'static str,
}

/// Docs for every builtin the host implements (plus `.expect()`). Shown in
/// the editor on hover.
pub const BUILTIN_DOCS: &[BuiltinDoc] = &[
    BuiltinDoc {
        name: "closest",
        signature: "closest(kind) -> Result",
        summary: "Nearest entity of a kind (blueprint / depot / enemy / ore). \
                  Gives Result.Ok(entity), or Result.Err(msg) when none exist — \
                  unwrap with .expect() or handle with match.",
        cost_note: "",
    },
    BuiltinDoc {
        name: "exists",
        signature: "exists(kind) -> bool",
        summary: "True while at least one entity of the kind exists.",
        cost_note: "",
    },
    BuiltinDoc {
        name: "expect",
        signature: "result.expect() -> entity",
        summary: "Unwrap a Result: Ok gives the value; Err faults with the \
                  carried message (crash dump unless an error handler is installed).",
        cost_note: "",
    },
    BuiltinDoc {
        name: "move_to",
        signature: "move_to(entity)",
        summary: "Pathfind to the target and walk there. Blocks until arrival; \
                  faults if no route exists.",
        cost_note: " + travel",
    },
    BuiltinDoc {
        name: "mine",
        signature: "mine()",
        summary: "Extract ore from a node in range into cargo. Blocks; faults \
                  with no ore in range.",
        cost_note: " + action",
    },
    BuiltinDoc {
        name: "deposit",
        signature: "deposit()",
        summary: "Unload all cargo into a depot in range. Blocks; faults with \
                  no depot in range.",
        cost_note: " + action",
    },
    BuiltinDoc {
        name: "build",
        signature: "build()",
        summary: "Work the nearest blueprint in range, 1 progress per tick. \
                  Blocks; faults when none is in range.",
        cost_note: " + action",
    },
    BuiltinDoc {
        name: "attack",
        signature: "attack(entity)",
        summary: "Strike the target while adjacent. Blocks for the swing.",
        cost_note: " + action",
    },
    BuiltinDoc {
        name: "wait",
        signature: "wait(n)",
        summary: "Idle for n ticks — deliberate pacing, the Tier-0 traffic tool.",
        cost_note: " + n idle ticks",
    },
    BuiltinDoc {
        name: "rng",
        signature: "rng(n) -> int",
        summary: "Uniform random integer in [0, n) from the sim's seeded \
                  stream (identical programs can desync on purpose: wait(rng(20))).",
        cost_note: "",
    },
    BuiltinDoc {
        name: "cargo_full",
        signature: "cargo_full() -> bool",
        summary: "True when cargo is at capacity.",
        cost_note: "",
    },
    BuiltinDoc {
        name: "health_low",
        signature: "health_low() -> bool",
        summary: "True below 50% hp — the Damaged threshold.",
        cost_note: "",
    },
    BuiltinDoc {
        name: "last_error",
        signature: "last_error() -> string",
        summary: "The most recent fault message; mainly for on error: handlers.",
        cost_note: "",
    },
    BuiltinDoc {
        name: "log",
        signature: "log(value, ...)",
        summary: "Append one line to the bot's local ring buffer (drops the \
                  oldest line when full).",
        cost_note: "",
    },
    BuiltinDoc {
        name: "upload_log",
        signature: "upload_log()",
        summary: "Transmit the log buffer to the colony cloud and clear it.",
        cost_note: " + size",
    },
    BuiltinDoc {
        name: "upload_crash_dump",
        signature: "upload_crash_dump()",
        summary: "File a full debug report to the cloud. The engine force-calls \
                  this on any unhandled fault.",
        cost_note: "",
    },
    BuiltinDoc {
        name: "become_disabled",
        signature: "become_disabled()",
        summary: "Wreck this bot and start its self-destruct countdown — a \
                  deliberate scuttle. Also forced at the end of on death:.",
        cost_note: "",
    },
];

/// Doc entry for a builtin or method, by name.
pub fn builtin_doc(name: &str) -> Option<&'static BuiltinDoc> {
    BUILTIN_DOCS.iter().find(|d| d.name == name)
}

pub struct BotHost<'a> {
    pub world: &'a mut World,
    pub bot: BotId,
    /// Duration of the forced handler-entry wait (from Tuning).
    pub tuning_handler_init_ticks: u32,
}

impl BotHost<'_> {
    fn request(&mut self, req: ActionRequest) -> HostCall {
        let bot = self.world.bots.get_mut(&self.bot).expect("bot exists while running");
        bot.data.requested = Some(req);
        HostCall::Block
    }

    /// Nearest entity of `kind` to this bot, or None if none exist.
    fn find_kind(&self, kind: &str) -> Option<EntityId> {
        let bot = &self.world.bots[&self.bot].data;
        match kind {
            "blueprint" => self.world.nearest_blueprint(bot.pos),
            "depot" => self.world.nearest_depot(bot.pos),
            "enemy" => self.world.nearest_enemy(bot.pos, bot.faction),
            "ore" => self.world.nearest_ore(bot.pos),
            _ => unreachable!("kind_arg only admits KINDS"),
        }
    }
}

/// Validate the single kind argument of a generic query.
fn kind_arg<'v>(func: &str, args: &'v [Value]) -> Result<&'v str, HostCall> {
    match args {
        [Value::Str(s)] if KINDS.contains(&s.as_str()) => Ok(s),
        [other] => Err(HostCall::Fault(format!(
            "{func} requires a kind ({}), got {}",
            KINDS.join("/"),
            other
        ))),
        _ => Err(HostCall::Fault(format!("{func} takes 1 kind argument"))),
    }
}

impl pyrite::Host for BotHost<'_> {
    fn call(&mut self, name: &str, args: &[Value], ctx: CallCtx<'_>) -> HostCall {
        let tick = self.world.tick;
        let bot_id = self.bot;
        let bot_pos = self.world.bots.get(&bot_id).expect("bot exists").data.pos;
        match name {
            // --- instant queries ---
            // Generic fallible query: closest(kind) -> Result.Ok(entity) / Result.Err(msg).
            "closest" => match kind_arg("closest", args) {
                Ok(kind) => HostCall::Ready(match self.find_kind(kind) {
                    Some(id) => Value::result_ok(Value::Entity(id.0)),
                    None => Value::result_err(format!("no {kind} anywhere")),
                }),
                Err(fault) => fault,
            },
            "exists" => match kind_arg("exists", args) {
                Ok(kind) => HostCall::Ready(Value::Bool(self.find_kind(kind).is_some())),
                Err(fault) => fault,
            },
            "cargo_full" => {
                let data = &self.world.bots[&bot_id].data;
                HostCall::Ready(Value::Bool(data.cargo >= data.cargo_cap))
            }
            "health_low" => {
                let data = &self.world.bots[&bot_id].data;
                // "Low" = below the Damaged threshold (50%).
                HostCall::Ready(Value::Bool(data.hp * 2 < data.max_hp))
            }
            "last_error" => {
                HostCall::Ready(Value::Str(ctx.last_fault.unwrap_or("").to_string()))
            }

            // --- blocking actions ---
            "move_to" => match args {
                [Value::Entity(target)] => self.request(ActionRequest::MoveTo(EntityId(*target))),
                [other] => HostCall::Fault(format!("move_to requires an entity, got {}", other.type_name())),
                _ => HostCall::Fault("move_to takes 1 argument".into()),
            },
            "mine" => self.request(ActionRequest::Mine),
            // The forced handler-entry ritual: an engine wait the VM
            // injects at every unified-handler entry (the flinch).
            "handler_init" => {
                let ticks = self.tuning_handler_init_ticks;
                if ticks == 0 {
                    HostCall::Ready(Value::Unit)
                } else {
                    self.request(ActionRequest::Wait(ticks))
                }
            }
            "wait" => match args {
                [Value::Int(0)] => HostCall::Ready(Value::Unit), // waiting 0 is free
                [Value::Int(n)] if *n > 0 => self.request(ActionRequest::Wait(*n as u32)),
                [Value::Int(_)] => HostCall::Fault("wait requires a non-negative tick count".into()),
                _ => HostCall::Fault("wait takes 1 integer argument".into()),
            },
            // Uniform integer in [0, n) from the sim's seeded stream —
            // the sanctioned randomness (wait(rng(20)) desyncs identical
            // programs).
            "rng" => match args {
                [Value::Int(n)] if *n > 0 => {
                    let v = (self.world.next_rand() % *n as u64) as i64;
                    HostCall::Ready(Value::Int(v))
                }
                [Value::Int(_)] => HostCall::Fault("rng requires a positive bound".into()),
                _ => HostCall::Fault("rng takes 1 integer argument".into()),
            },
            "build" => {
                // Work on the nearest blueprint in range.
                let target = self
                    .world
                    .blueprints
                    .iter()
                    .filter(|(_, b)| bot_pos.chebyshev(b.pos) <= 1)
                    .map(|(id, _)| *id)
                    .next();
                match target {
                    Some(id) => self.request(ActionRequest::Build(id)),
                    None => HostCall::Fault("build: no blueprint in range".into()),
                }
            }
            "deposit" => self.request(ActionRequest::Deposit),
            "attack" => match args {
                [Value::Entity(target)] => self.request(ActionRequest::Attack(EntityId(*target))),
                [other] => HostCall::Fault(format!("attack requires an entity, got {}", other.type_name())),
                _ => HostCall::Fault("attack takes 1 argument".into()),
            },

            // --- logging (docs/01: ordinary costed functions) ---
            "log" => {
                let text = args.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
                let buf = &mut self.world.bots.get_mut(&bot_id).expect("bot exists").data.log_buf;
                if buf.len() >= LOG_BUFFER_CAP {
                    buf.remove(0);
                }
                buf.push(text);
                HostCall::Ready(Value::Unit)
            }
            "upload_log" => {
                let logs = std::mem::take(
                    &mut self.world.bots.get_mut(&bot_id).expect("bot exists").data.log_buf,
                );
                for text in logs {
                    self.world.archive.push(ArchiveEntry {
                        tick,
                        bot: bot_id,
                        kind: ArchiveKind::Log,
                        line: ctx.line,
                        text,
                    });
                }
                HostCall::Ready(Value::Unit)
            }
            "upload_crash_dump" => {
                // Force-called on unhandled faults; also player-callable.
                let msg = match args {
                    [Value::Str(s)] => s.clone(),
                    _ => ctx.last_fault.unwrap_or("").to_string(),
                };
                self.world.archive.push(ArchiveEntry {
                    tick,
                    bot: bot_id,
                    kind: ArchiveKind::CrashDump,
                    line: ctx.line,
                    text: msg,
                });
                HostCall::Ready(Value::Unit)
            }

            // --- lifecycle (forced calls are ordinary functions) ---
            "become_disabled" => {
                self.world.bots.get_mut(&bot_id).expect("bot exists").data.dying = true;
                HostCall::Ready(Value::Unit)
            }

            other => HostCall::Fault(format!("unknown function {other}()")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn builtin_docs_are_unique_and_costed() {
        let costs = pyrite::CostTable::default();
        let mut seen = BTreeSet::new();
        for doc in BUILTIN_DOCS {
            assert!(seen.insert(doc.name), "duplicate doc for {}", doc.name);
            // The editor shows a live cost next to each doc — every entry
            // must exist in the cost table or the display lies.
            // (upload_crash_dump is costed by the dedicated crash_dump field.)
            assert!(
                doc.name == "upload_crash_dump" || costs.builtins.contains_key(doc.name),
                "{} documented but missing from the cost table",
                doc.name
            );
        }
        for name in ["closest", "exists", "expect"] {
            assert!(builtin_doc(name).is_some(), "{name} needs docs");
        }
    }
}
