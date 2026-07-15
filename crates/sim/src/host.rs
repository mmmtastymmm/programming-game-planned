//! The sim's `pyrite::Host` implementation: one per (world, bot) pair while
//! that bot's VM is stepping. Queries answer instantly; actions record an
//! `ActionRequest` and return `Block` — the resolve phase starts them.

use crate::world::{ActionRequest, ArchiveEntry, ArchiveKind, BotId, EntityId, World, LOG_BUFFER_CAP};
use pyrite::vm::CallCtx;
use pyrite::{faults, Fault, HostCall, Value};

/// Entity kinds understood by the generic queries (`exists(kind)`,
/// `closest(kind)`). Each is bound as a global constant of the same name in
/// every bot VM (see `Sim::new`), so programs write `closest(ore)` bare.
pub const KINDS: &[&str] = &["blueprint", "depot", "enemy", "ore"];

/// Editor-facing doc lookup, backed by the function registry
/// (`pyrite/data/builtins.ron`): signature, summary, and cost note all come
/// from the same data the VM prices calls with, so hover docs can't go
/// stale against the cost table.
pub fn builtin_doc<'c>(costs: &'c pyrite::CostTable, name: &str) -> Option<&'c pyrite::BuiltinSpec> {
    costs.spec(name)
}

pub struct BotHost<'a> {
    pub world: &'a mut World,
    pub bot: BotId,
    pub tuning: &'a crate::sim::Tuning,
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
        [other] => Err(HostCall::Fault(Fault::new(
            faults::TYPE,
            format!("{func} requires a kind ({}), got {}", KINDS.join("/"), other),
        ))),
        _ => Err(HostCall::Fault(Fault::new(
            faults::ARITY,
            format!("{func} takes 1 kind argument"),
        ))),
    }
}

impl pyrite::Host for BotHost<'_> {
    fn log_len(&self) -> u64 {
        self.world.bots[&self.bot].data.log_buf.len() as u64
    }

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
                // "Low" = below the bot's own hurt_line env (read live at
                // each evaluation — moving it mid-flight is legal).
                let data = &self.world.bots[&bot_id].data;
                let line =
                    crate::world::env_read(&data.env, "hurt_line", self.tuning);
                HostCall::Ready(Value::Bool(data.hp * 100 < data.max_hp * line))
            }
            "last_error" => {
                // The fault's identity constant (err_type, err_action, ...)
                // — ==-comparable against the pre-bound err_* names (Q80).
                HostCall::Ready(Value::Str(ctx.last_fault_id.unwrap_or("").to_string()))
            }

            // --- blocking actions ---
            "move_to" => match args {
                [Value::Entity(target)] => self.request(ActionRequest::MoveTo(EntityId(*target))),
                [other] => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    format!("move_to requires an entity, got {}", other.type_name()),
                )),
                _ => HostCall::Fault(Fault::new(faults::ARITY, "move_to takes 1 argument")),
            },
            "mine" => self.request(ActionRequest::Mine),
            // The forced handler-entry ritual: an engine wait the VM
            // injects at every unified-handler entry (the flinch).
            "handler_init" => {
                let ticks = self.tuning.handler_init_ticks;
                if ticks == 0 {
                    HostCall::Ready(Value::Unit)
                } else {
                    self.request(ActionRequest::Wait(ticks))
                }
            }
            "wait" => match args {
                [Value::Int(0)] => HostCall::Ready(Value::Unit), // waiting 0 is free
                [Value::Int(n)] if *n > 0 => self.request(ActionRequest::Wait(*n as u32)),
                [Value::Int(_)] => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    "wait requires a non-negative tick count",
                )),
                _ => HostCall::Fault(Fault::new(faults::TYPE, "wait takes 1 integer argument")),
            },
            // Uniform integer in [0, n) from this bot's own rng.program
            // stream, seeded by (match seed, entity ID) — the sanctioned
            // randomness (wait(rng(20)) desyncs identical programs), and
            // isolated so player draws can never perturb engine streams.
            "rng" => match args {
                [Value::Int(n)] if *n > 0 => {
                    let bot = self.world.bots.get_mut(&self.bot).expect("host bot exists");
                    let v = (crate::world::next_rand(&mut bot.data.rng_program)
                        % *n as u64) as i64;
                    HostCall::Ready(Value::Int(v))
                }
                [Value::Int(_)] => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    "rng requires a positive bound",
                )),
                _ => HostCall::Fault(Fault::new(faults::TYPE, "rng takes 1 integer argument")),
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
                    None => HostCall::Fault(Fault::new(faults::ACTION, "build: no blueprint in range")),
                }
            }
            "deposit" => self.request(ActionRequest::Deposit),
            "attack" => match args {
                [Value::Entity(target)] => self.request(ActionRequest::Attack(EntityId(*target))),
                [other] => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    format!("attack requires an entity, got {}", other.type_name()),
                )),
                _ => HostCall::Fault(Fault::new(faults::ARITY, "attack takes 1 argument")),
            },

            // --- logging (docs/01: ordinary costed functions) ---
            "log" => {
                // Registry signature: log(val, level=info) — the VM sends
                // the canonical positional form. Entries below the bot's
                // `log_min_level` env are discarded before they consume a
                // slot (the call still cost its cycle).
                let text = args.first().map(|v| v.to_string()).unwrap_or_default();
                let level = match args.get(1) {
                    Some(Value::Int(l)) if (0..=4).contains(l) => *l as u8,
                    None => 2, // info
                    Some(other) => {
                        return HostCall::Fault(Fault::new(
                            faults::RANGE,
                            format!("log: level must be trace..error, got {other}"),
                        ));
                    }
                };
                let min_level = crate::world::env_read(
                    &self.world.bots[&bot_id].data.env,
                    "log_min_level",
                    self.tuning,
                );
                if (level as i64) >= min_level {
                    let buf =
                        &mut self.world.bots.get_mut(&bot_id).expect("bot exists").data.log_buf;
                    if buf.len() >= LOG_BUFFER_CAP {
                        buf.remove(0);
                    }
                    buf.push((level, text));
                }
                HostCall::Ready(Value::Unit)
            }
            "upload_log" => {
                let logs = std::mem::take(
                    &mut self.world.bots.get_mut(&bot_id).expect("bot exists").data.log_buf,
                );
                for (level, text) in logs {
                    self.world.archive.push(ArchiveEntry {
                        tick,
                        bot: bot_id,
                        kind: ArchiveKind::Log,
                        level,
                        line: ctx.line,
                        text,
                    });
                }
                HostCall::Ready(Value::Unit)
            }

            // --- the environment (docs/01: policy, never stats) ---
            "setenv" => match args {
                [Value::Str(key), Value::Int(value)] => {
                    let Some(spec) =
                        crate::world::ENV_KEYS.iter().find(|k| k.name == key.as_str())
                    else {
                        return HostCall::Fault(Fault::new(
                            faults::KEY,
                            format!("setenv: unknown env key {key:?} (keys are engine-defined)"),
                        ));
                    };
                    if *value < spec.min || *value > spec.max {
                        return HostCall::Fault(Fault::new(
                            faults::RANGE,
                            format!(
                                "setenv: {key} must be in {}..={}, got {value}",
                                spec.min, spec.max
                            ),
                        ));
                    }
                    let bot = self.world.bots.get_mut(&bot_id).expect("bot exists");
                    bot.data.env.insert(key.clone(), *value);
                    HostCall::Ready(Value::Unit)
                }
                _ => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    "setenv takes (key, int) — e.g. setenv(hurt_line, 30)",
                )),
            },
            "getenv" => match args {
                // Never faults on an unset key — unset means default. An
                // unknown key is still err_key (a typo, not a policy).
                [Value::Str(key)] => {
                    if !crate::world::ENV_KEYS.iter().any(|k| k.name == key.as_str()) {
                        return HostCall::Fault(Fault::new(
                            faults::KEY,
                            format!("getenv: unknown env key {key:?}"),
                        ));
                    }
                    let value = crate::world::env_read(
                        &self.world.bots[&bot_id].data.env,
                        key,
                        self.tuning,
                    );
                    HostCall::Ready(Value::Int(value))
                }
                _ => HostCall::Fault(Fault::new(faults::TYPE, "getenv takes an env key")),
            },
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
                    level: 4, // crash dumps archive at error severity
                    line: ctx.line,
                    text: msg,
                });
                HostCall::Ready(Value::Unit)
            }

            // --- lifecycle (forced calls are ordinary functions) ---
            "become_disabled" => {
                let bot = self.world.bots.get_mut(&bot_id).expect("bot exists");
                bot.data.dying = true;
                // Dying bots stop blocking: out of the occupancy index the
                // moment the flag is set (wrecks don't block).
                let pos = bot.data.pos;
                self.world.unindex_bot(bot_id, pos);
                HostCall::Ready(Value::Unit)
            }

            other => HostCall::Fault(Fault::new(
                faults::UNKNOWN_FUNCTION,
                format!("unknown function {other}()"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_documents_every_host_builtin() {
        let costs = pyrite::CostTable::default();
        // Every PLAYER-CALLABLE builtin this host implements must have a
        // registry entry — the editor shows its signature, summary, and
        // live cost from there. become_disabled is deliberately absent:
        // engine-only (Q76), reachable solely through abort's forced
        // sequence; an unregistered player call faults err_unknown_function.
        for name in [
            "closest", "exists", "cargo_full", "health_low", "last_error", "move_to",
            "mine", "wait", "rng", "build", "deposit", "attack", "log", "upload_log",
            "upload_crash_dump", "handler_init", "setenv", "getenv", "abort",
        ] {
            let spec = builtin_doc(&costs, name)
                .unwrap_or_else(|| panic!("{name} implemented but missing from builtins.ron"));
            assert!(!spec.signature.is_empty(), "{name} needs a signature");
            assert!(!spec.summary.is_empty(), "{name} needs a summary");
        }
        assert!(
            builtin_doc(&costs, "become_disabled").is_none(),
            "become_disabled must stay off the player registry (engine-only, Q76)"
        );
    }
}
