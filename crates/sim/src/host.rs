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
            "deposit" => self.request(ActionRequest::Deposit),

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
