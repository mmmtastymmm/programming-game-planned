//! The sim's `pyrite::Host` implementation: one per (world, bot) pair while
//! that bot's VM is stepping. Queries answer instantly; actions record an
//! `ActionRequest` and return `Block` — the resolve phase starts them.

use crate::world::{ActionRequest, ArchiveEntry, ArchiveKind, BotId, EntityId, World, LOG_BUFFER_CAP};
use pyrite::vm::CallCtx;
use pyrite::{HostCall, Value};

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
}

impl pyrite::Host for BotHost<'_> {
    fn call(&mut self, name: &str, args: &[Value], ctx: CallCtx<'_>) -> HostCall {
        let tick = self.world.tick;
        let bot_id = self.bot;
        let bot_pos = self.world.bots.get(&bot_id).expect("bot exists").data.pos;
        match name {
            // --- instant queries ---
            "nearest_ore" => match self.world.nearest_ore(bot_pos) {
                Some(id) => HostCall::Ready(Value::Entity(id.0)),
                None => HostCall::Fault("no ore anywhere".into()),
            },
            "nearest_depot" => match self.world.nearest_depot(bot_pos) {
                Some(id) => HostCall::Ready(Value::Entity(id.0)),
                None => HostCall::Fault("no depot anywhere".into()),
            },
            "cargo_full" => {
                let data = &self.world.bots[&bot_id].data;
                HostCall::Ready(Value::Bool(data.cargo >= data.cargo_cap))
            }
            "nearest_enemy" => {
                let faction = self.world.bots[&bot_id].data.faction;
                match self.world.nearest_enemy(bot_pos, faction) {
                    Some(id) => HostCall::Ready(Value::Entity(id.0)),
                    None => HostCall::Fault("no enemy anywhere".into()),
                }
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
            "nearest_blueprint" => match self.world.nearest_blueprint(bot_pos) {
                Some(id) => HostCall::Ready(Value::Entity(id.0)),
                None => HostCall::Fault("no blueprint anywhere".into()),
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
