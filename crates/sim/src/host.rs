//! The sim's `pyrite::Host` implementation: one per (world, bot) pair while
//! that bot's VM is stepping. Queries answer instantly; actions record an
//! `ActionRequest` and return `Block` — the resolve phase starts them.

use crate::world::{ActionRequest, ArchiveEntry, ArchiveKind, BotId, EntityId, World};
use pyrite::vm::CallCtx;
use pyrite::{faults, Fault, HostCall, Value};

/// Entity kinds understood by the generic queries (`exists(kind)`,
/// `closest(kind)`). Each is bound as a global constant of the same name in
/// every bot VM (see `Sim::new`), so programs write `closest(ore)` bare.
pub const KINDS: &[&str] = &[
    "blueprint", "depot", "enemy", "ore", "printer", "wreck", "smelter", "foundry", "archive",
    // the creep's heart (M8-C) — attackable, so it must be findable
    "blight",
    // every resource kind is also a queryable kind: raw names find nodes,
    // refined names exist for cargo_count/withdraw (closest() on a refined
    // kind finds nothing until stock queries land)
    "water", "stone", "sand", "wood", "coal", "iron", "copper", "tin", "silver", "gold",
    "crystal", "steel", "bronze", "wire", "chips", "glass", "lens", "gold_chip",
];

/// The host-domain fault id for reads through a contact the colony can't
/// actually see (M7, docs/05): heard-only handles are position-only, and
/// stale handles fault rather than answer from thin air. Bound as a VM
/// constant so `last_error() == err_unknown_contact` reads naturally.
pub const UNKNOWN_CONTACT: &str = "err_unknown_contact";

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
    /// The stat pipeline's read context (M6): per-bot effective stats —
    /// flinch/cargo/sensor reads and quirk introspection go through it.
    pub ctx: crate::stats::StatCtx<'a>,
}

impl BotHost<'_> {
    fn request(&mut self, req: ActionRequest) -> HostCall {
        let bot = self.world.bots.get_mut(&self.bot).expect("bot exists while running");
        bot.data.requested = Some(req);
        HostCall::Block
    }

    /// This bot's faction perception (LAST tick's compute — docs/05:
    /// everyone's queries read last tick's perception).
    fn perception(&self) -> Option<&crate::world::Perception> {
        self.world.perception.get(&self.world.bots[&self.bot].data.faction)
    }

    /// Is this entity available to the colony's queries: our own, fully
    /// seen, or a heard-only blip?
    fn perceived(&self, entity: EntityId) -> bool {
        let faction = self.world.bots[&self.bot].data.faction;
        if let Some(id) = self.world.bot_entities.get(&entity)
            && self.world.bots.get(id).is_some_and(|b| b.data.faction == faction)
        {
            return true;
        }
        if self.world.printers.get(&entity).is_some_and(|p| p.faction == faction) {
            return true;
        }
        if self.world.structures.get(&entity).is_some_and(|s| s.faction == faction) {
            return true;
        }
        self.perception()
            .is_some_and(|p| p.seen.contains(&entity) || p.heard.contains_key(&entity))
    }

    /// Nearest entity of `kind` to this bot, or None — PERCEPTION-SCOPED
    /// (M7): own things always; enemy things only if seen (structures) or
    /// seen-or-heard (bots); resource nodes from permanent map knowledge
    /// (a known vein is a fact, not a perception — docs/05), skipping
    /// nodes the colony has OBSERVED exhausted.
    fn find_kind(&self, kind: &str) -> Option<EntityId> {
        use crate::resources::Resource;
        let bot = &self.world.bots[&self.bot].data;
        let faction = bot.faction;
        let known = self.world.known_nodes.get(&faction);
        let node_query = |filter: &dyn Fn(Resource) -> bool| -> Option<EntityId> {
            known?
                .iter()
                .filter(|(_, n)| !n.exhausted && filter(n.kind))
                .map(|(id, n)| (bot.pos.manhattan(n.pos), *id))
                .min()
                .map(|(_, id)| id)
        };
        if let Some(res) = Resource::from_name(kind) {
            return node_query(&|k| k == res);
        }
        match kind {
            "ore" => node_query(&|k| k.is_ore_family()),
            // Own-colony infrastructure is cloud knowledge, always.
            "blueprint" => self.world.nearest_blueprint(bot.pos),
            // The creep front is visible terrain, so its source is
            // queryable un-gated (perception gating: see TASKS.md).
            "blight" => self
                .world
                .blight_cores
                .iter()
                .map(|(id, c)| (bot.pos.manhattan(c.pos), *id))
                .min()
                .map(|(_, id)| id),
            "depot" => self.world.nearest_depot(bot.pos),
            "printer" => self
                .world
                .printers
                .iter()
                .filter(|(id, p)| {
                    p.faction == faction
                        || self.perception().is_some_and(|per| per.seen.contains(id))
                })
                .map(|(id, p)| (bot.pos.manhattan(p.pos), *id))
                .min()
                .map(|(_, id)| id),
            "enemy" => {
                let per = self.perception()?;
                let mut best: Option<(u32, EntityId)> = None;
                for (id, b) in &self.world.bots {
                    if b.data.faction == faction || b.data.dying {
                        continue;
                    }
                    let entity = b.data.entity;
                    let pos = if per.seen.contains(&entity) {
                        b.data.pos
                    } else if let Some(blip) = per.heard.get(&entity) {
                        *blip
                    } else {
                        continue;
                    };
                    let _ = id;
                    let candidate = (bot.pos.manhattan(pos), entity);
                    if best.is_none_or(|b| candidate < b) {
                        best = Some(candidate);
                    }
                }
                best.map(|(_, id)| id)
            }
            "wreck" => None, // wrecks are BotId-keyed; targeting lands M10
            "smelter" | "foundry" | "archive" => self
                .world
                .structures
                .iter()
                .filter(|(id, st)| {
                    st.kind.name() == kind
                        && (st.faction == faction
                            || self.perception().is_some_and(|per| per.seen.contains(id)))
                })
                .map(|(id, st)| (bot.pos.manhattan(st.pos), *id))
                .min()
                .map(|(_, id)| id),
            _ => unreachable!("kind_arg only admits KINDS"),
        }
    }

    /// The resource kind named by a builtin's argument.
    fn resource_arg(func: &str, args: &[Value]) -> Result<crate::resources::Resource, HostCall> {
        match args {
            [Value::Str(s)] => crate::resources::Resource::from_name(s).ok_or_else(|| {
                HostCall::Fault(Fault::new(
                    faults::TYPE,
                    format!("{func}: unknown resource kind {s:?}"),
                ))
            }),
            _ => Err(HostCall::Fault(Fault::new(
                faults::TYPE,
                format!("{func} takes one resource-kind constant"),
            ))),
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
                HostCall::Ready(Value::Bool(
                    data.cargo_total() >= self.ctx.cargo_cap_for(data),
                ))
            }
            "health_low" => {
                // "Low" = below the bot's own hurt_line env (read live at
                // each evaluation — moving it mid-flight is legal).
                let data = &self.world.bots[&bot_id].data;
                let line = crate::world::env_read(data, "hurt_line", self.tuning, self.ctx.quirks);
                HostCall::Ready(Value::Bool(data.hp * 100 < data.max_hp * line))
            }
            // Quirk introspection (docs/09: free whenever quirks are on —
            // per-bot adaptation is the system's payoff). Latent quirks
            // are invisible, introspection included.
            "my_quirks" => {
                let data = &self.world.bots[&bot_id].data;
                HostCall::Ready(Value::List(
                    data.quirks
                        .iter()
                        .filter_map(|&q| self.ctx.quirks.quirks.get(q as usize))
                        .map(|spec| Value::Str(spec.name.clone()))
                        .collect(),
                ))
            }
            "has_quirk" => match args {
                [Value::Str(name)] => {
                    let data = &self.world.bots[&bot_id].data;
                    let has = self
                        .ctx
                        .quirks
                        .by_name(name)
                        .is_some_and(|idx| data.quirks.contains(&idx));
                    HostCall::Ready(Value::Bool(has))
                }
                [other] => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    format!("has_quirk requires a quirk name, got {}", other.type_name()),
                )),
                _ => HostCall::Fault(Fault::new(faults::ARITY, "has_quirk takes 1 argument")),
            },
            "last_error" => {
                // The fault's identity constant (err_type, err_action, ...)
                // — ==-comparable against the pre-bound err_* names (Q80).
                HostCall::Ready(Value::Str(ctx.last_fault_id.unwrap_or("").to_string()))
            }

            // --- blocking actions ---
            "move_to" => match args {
                // Stale handles fault (M7): a target neither ours, nor
                // perceived, nor in map knowledge doesn't exist to us.
                [Value::Entity(target)] => {
                    let entity = EntityId(*target);
                    let faction = self.world.bots[&bot_id].data.faction;
                    let known_node = self
                        .world
                        .known_nodes
                        .get(&faction)
                        .is_some_and(|k| k.contains_key(&entity));
                    if !self.perceived(entity) && !known_node
                        && !self.world.depots.contains_key(&entity)
                        && !self.world.blueprints.contains_key(&entity)
                    {
                        return HostCall::Fault(Fault::new(
                            UNKNOWN_CONTACT,
                            "move_to: stale or unknown contact",
                        ));
                    }
                    self.request(ActionRequest::MoveTo(entity))
                }
                [other] => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    format!("move_to requires an entity, got {}", other.type_name()),
                )),
                _ => HostCall::Fault(Fault::new(faults::ARITY, "move_to takes 1 argument")),
            },
            "mine" => self.request(ActionRequest::Mine),
            // --- the exploration stances (M7) ---
            "search" => self.request(ActionRequest::Search),
            "wander" => self.request(ActionRequest::Wander),
            "explore" => self.request(ActionRequest::Explore),
            // The forced handler-entry ritual: an engine wait the VM
            // injects at every unified-handler entry (the flinch). The
            // duration runs the pipeline: quirks (Rubber Ducky / Race
            // Condition) and the Flinch body track shorten or stretch it.
            "handler_init" => {
                let data = &self.world.bots[&bot_id].data;
                let ticks = self.ctx.flinch_ticks_for(data, self.tuning.handler_init_ticks);
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
            "deposit" | "try_deposit" => self.request(ActionRequest::Deposit {
                fault_on_fail: name == "deposit",
            }),
            // Pull refined goods out of an adjacent refinery's output
            // buffer (or colony stock at a depot). Instant (docs/03 prices
            // it "+ action"; the tick cost is flagged in TASKS.md).
            "withdraw" | "try_withdraw" => {
                let kind = match Self::resource_arg(name, args) {
                    Ok(k) => k,
                    Err(fault) => return fault,
                };
                let (faction, space) = {
                    let data = &self.world.bots[&bot_id].data;
                    (
                        data.faction,
                        self.ctx.cargo_cap_for(data).saturating_sub(data.cargo_total()),
                    )
                };
                let mut got = 0u32;
                if space > 0 {
                    // Adjacent structure output first (lowest id wins),
                    // then colony stock at an adjacent depot.
                    let source = self
                        .world
                        .structures
                        .iter()
                        .filter(|(_, st)| {
                            st.faction == faction
                                && bot_pos.chebyshev(st.pos) <= 1
                                && st.output.get(&kind).copied().unwrap_or(0) > 0
                        })
                        .map(|(id, _)| *id)
                        .next();
                    if let Some(sid) = source {
                        let st = self.world.structures.get_mut(&sid).expect("just found");
                        let have = st.output.get_mut(&kind).expect("just found");
                        got = (*have).min(space);
                        *have -= got;
                        if *have == 0 {
                            st.output.remove(&kind);
                        }
                    } else if self
                        .world
                        .depots
                        .values()
                        .any(|d| bot_pos.chebyshev(d.pos) <= 1)
                    {
                        let stocked = self.world.stock_get(faction, kind).min(space as u64);
                        if stocked > 0 && self.world.stock_take(faction, kind, stocked) {
                            got = stocked as u32;
                            // Provenance: stock-withdrawn cargo earns no
                            // delivery-milestone credit when re-deposited
                            // (cycling mints nothing) — tracked on the BOT
                            // so seeded-stock withdrawals can never
                            // suppress genuinely earned milestones.
                            // (Refinery-output withdrawals above don't
                            // count — that produce was never delivered.)
                            self.world
                                .bots
                                .get_mut(&bot_id)
                                .expect("bot exists")
                                .data
                                .withdrawn_aboard += got;
                        }
                    }
                }
                if got > 0 {
                    let bot = self.world.bots.get_mut(&bot_id).expect("bot exists");
                    let loaded = bot.data.cargo_add(kind, got, u32::MAX);
                    debug_assert_eq!(loaded, got, "space was pre-checked");
                }
                if name == "try_withdraw" {
                    HostCall::Ready(Value::Bool(got > 0))
                } else if got > 0 {
                    HostCall::Ready(Value::Unit)
                } else {
                    HostCall::Fault(Fault::new(
                        faults::ACTION,
                        format!("withdraw: no {} available here", kind.name()),
                    ))
                }
            }
            "cargo_count" => {
                let kind = match Self::resource_arg(name, args) {
                    Ok(k) => k,
                    Err(fault) => return fault,
                };
                let deci = self.world.bots[&bot_id]
                    .data
                    .cargo
                    .get(&kind)
                    .copied()
                    .unwrap_or(0);
                HostCall::Ready(Value::Int((deci / crate::resources::DECI) as i64))
            }
            "scan_resources" => {
                // Perception-scoped (M7): the colony's PERMANENT node
                // knowledge, observed-exhausted skipped, (distance, id)
                // sorted — a known vein is a fact, not a perception.
                let faction = self.world.bots[&bot_id].data.faction;
                let mut nodes: Vec<(u32, u64)> = self
                    .world
                    .known_nodes
                    .get(&faction)
                    .map(|known| {
                        known
                            .iter()
                            .filter(|(_, n)| !n.exhausted)
                            .map(|(id, n)| (bot_pos.manhattan(n.pos), id.0))
                            .collect()
                    })
                    .unwrap_or_default();
                nodes.sort();
                HostCall::Ready(Value::List(
                    nodes.into_iter().map(|(_, id)| Value::Entity(id)).collect(),
                ))
            }
            "scan_enemies" => {
                // Seen ∪ heard enemy bots (docs/05: full returns within
                // seeing, movers within hearing), (distance, id) sorted.
                let faction = self.world.bots[&bot_id].data.faction;
                let mut found: Vec<(u32, u64)> = Vec::new();
                if let Some(per) = self.world.perception.get(&faction) {
                    for b in self.world.bots.values() {
                        if b.data.faction == faction || b.data.dying {
                            continue;
                        }
                        let entity = b.data.entity;
                        let pos = if per.seen.contains(&entity) {
                            b.data.pos
                        } else if let Some(blip) = per.heard.get(&entity) {
                            *blip
                        } else {
                            continue;
                        };
                        found.push((bot_pos.manhattan(pos), entity.0));
                    }
                }
                found.sort();
                HostCall::Ready(Value::List(
                    found.into_iter().map(|(_, id)| Value::Entity(id)).collect(),
                ))
            }
            "is_seen" => match args {
                // Seen = full dossier; heard-only = position, nothing
                // else; NEITHER = a stale handle, and stale handles fault
                // (docs/05 M7).
                [Value::Entity(e)] => {
                    let entity = EntityId(*e);
                    let Some(per) = self.perception() else {
                        return HostCall::Fault(Fault::new(
                            UNKNOWN_CONTACT,
                            "is_seen: no perception".to_string(),
                        ));
                    };
                    if per.seen.contains(&entity) {
                        HostCall::Ready(Value::Bool(true))
                    } else if per.heard.contains_key(&entity) {
                        HostCall::Ready(Value::Bool(false))
                    } else if self.perceived(entity) {
                        // Our own unit/structure: trivially seen.
                        HostCall::Ready(Value::Bool(true))
                    } else {
                        HostCall::Fault(Fault::new(
                            UNKNOWN_CONTACT,
                            "is_seen: stale or unknown contact",
                        ))
                    }
                }
                [other] => HostCall::Fault(Fault::new(
                    faults::TYPE,
                    format!("is_seen requires an entity, got {}", other.type_name()),
                )),
                _ => HostCall::Fault(Fault::new(faults::ARITY, "is_seen takes 1 argument")),
            },
            "path_blocked" => {
                // The Tier-2 corridor sensor: is the current move path's
                // next tile occupied by a bot?
                let data = &self.world.bots[&bot_id].data;
                let blocked = match &data.action {
                    Some(crate::world::Action::Move { path, .. }) => path
                        .first()
                        .is_some_and(|next| self.world.tile_occupied(*next, bot_id)),
                    _ => false,
                };
                HostCall::Ready(Value::Bool(blocked))
            }
            "drop_cargo" => {
                // Spill the manifest onto the bot's own tile as nodes —
                // mine() recovers it (deterministic: no scatter for the
                // deliberate drop; death spills keep their RNG scatter).
                let data = &mut self.world.bots.get_mut(&bot_id).expect("bot exists").data;
                // The dropped cargo takes its stock provenance AND its
                // undelivered hauling distance with it (docs/02: income is
                // cargo-distance DELIVERED; mining it back re-earns).
                data.withdrawn_aboard = 0;
                data.haul_accum = 0;
                let manifest = std::mem::take(&mut data.cargo);
                for (kind, amount) in manifest {
                    if amount == 0 {
                        continue;
                    }
                    let existing = self
                        .world
                        .nodes
                        .iter()
                        .find(|(_, n)| n.pos == bot_pos && n.kind == kind)
                        .map(|(id, _)| *id);
                    match existing {
                        Some(id) => {
                            self.world.nodes.get_mut(&id).expect("just found").amount += amount;
                        }
                        None => {
                            let id = self.world.alloc_entity();
                            self.world.nodes.insert(
                                id,
                                crate::world::ResourceNode {
                                    kind,
                                    pos: bot_pos,
                                    amount,
                                    regen: false,
                                },
                            );
                        }
                    }
                }
                HostCall::Ready(Value::Unit)
            }
            "attack" => match args {
                [Value::Entity(target)] => {
                    let entity = EntityId(*target);
                    if !self.perceived(entity) {
                        return HostCall::Fault(Fault::new(
                            UNKNOWN_CONTACT,
                            "attack: stale or unknown contact",
                        ));
                    }
                    self.request(ActionRequest::Attack(entity))
                }
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
                    &self.world.bots[&bot_id].data,
                    "log_min_level",
                    self.tuning,
                    self.ctx.quirks,
                );
                if (level as i64) >= min_level {
                    let data = &mut self.world.bots.get_mut(&bot_id).expect("bot exists").data;
                    // The ring cap is a hardware stat (stats floor +
                    // Memory banks), not a global const (M5).
                    let cap = data.log_cap as usize;
                    let buf = &mut data.log_buf;
                    while buf.len() >= cap.max(1) {
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
                    // Reads land inside any quirk compulsion clamp — this
                    // is how getenv "reports where the value actually
                    // landed" (docs/09 Q60).
                    let value = crate::world::env_read(
                        &self.world.bots[&bot_id].data,
                        key,
                        self.tuning,
                        self.ctx.quirks,
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
