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

pub struct BotHost<'a> {
    pub world: &'a mut World,
    pub bot: BotId,
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
