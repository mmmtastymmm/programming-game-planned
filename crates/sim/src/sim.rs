//! The deterministic nine-phase tick loop (docs/07-architecture.md):
//!
//! 1. apply agreed Commands (caller does this via [`Sim::apply`])
//! 2. grant cycles, step every VM (stable BotId order)
//! 3. collect issued actions (recorded as `ActionRequest`s during step)
//! 4. resolve actions per bot in id order, then engine-driven walks
//! 5. perception (stub until M7)
//! 6. damage settlement, signal dispatch by severity, deaths → wrecks
//! 7. XP settlement (awards for bots that died in 6 drop with them)
//! 8. economy: regen, refineries, printers
//! 9. state hash for desync detection ([`Sim::state_hash`])

use crate::hash::Fnv1a;
use crate::host::BotHost;
use crate::map::{MapSpec, OverlayKind, TileKind, TilePos};
use crate::world::{
    Blueprint, BlueprintKind, Bot,
    BotData, BotId, Color, ColorProgram, EntityId, PrinterState, Wreck,
    World, XpTrack,
};
use pyrite::{CostTable, Outcome, PyriteError, UnlockSet, Value, Vm, VmConfig};
use std::rc::Rc;

// (Melee damage moved to tuning.ron `attack_damage` with M6 — every
// number is data; per-weapon hardware still lands later.)

/// Sim tuning constants (all numbers are data — CLAUDE.md convention; the
/// values live in `data/tuning.ron`, baked in at compile time and parsed
/// once at load).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    pub print_ticks: u32,
    /// Steel (deci-units) per print. DEFAULT FREE: a colony must never be
    /// soft-locked out of bots (no steel + no bots = dead end). Maps/
    /// servers can set a cost; population is bounded by dials and capacity
    /// either way.
    pub print_cost_steel: u64,
    /// Repairing the ruined Red printer prices in DATA (docs/03: ~60 —
    /// the flagship early Data sink).
    pub repair_cost_data: u64,
    pub scrap_refund_steel: u64,
    /// mine() yield per swing, deci-units (docs/03 first-pass: 2 units).
    pub mine_yield_deci: u32,
    /// Ticks one mine() swing takes (every number is data — CLAUDE.md).
    pub mine_swing_ticks: u32,
    /// Regenerating nodes (Groves) gain this many deci-units per
    /// regen_interval_ticks, up to the global node_regen_cap_deci ceiling
    /// (per-node caps at the seeded amount are map-gen territory, Q71).
    pub node_regen_deci: u32,
    pub node_regen_cap_deci: u32,
    /// Data income: 20 per this many deci-units delivered (docs/03).
    pub delivery_milestone_deci: u32,
    pub milestone_data: u64,
    pub first_kill_data: u64,
    /// Generic structure durability (per-kind sheets land with M5 data).
    pub structure_hp: i64,
    /// Refinery batch duration (docs/03 first-pass: ~30 ticks).
    pub recipe_batch_ticks: u32,
    /// Boot Sequence duration — an engine interrupt context.
    pub boot_ticks: u32,
    /// Typed build prices per structure kind (docs/03), in UNITS —
    /// every number is data, not code (CLAUDE.md).
    pub structure_costs: Vec<(crate::world::StructureKind, Vec<(crate::resources::Resource, u32)>)>,
    /// Melee damage per hit (per-weapon hardware lands later).
    pub attack_damage: i64,
    // (printed_* chassis defaults moved to data/stats.ron with M5 — the
    // universal floor statline is the print.)
    /// Rammer's total at-fault stun (50 = 5s @10Hz): expressed as the bump
    /// FACTORY WINDOW's `wait(bump_freeze_ticks - handler_init_ticks)` on
    /// top of the forced flinch, and applied directly on engine walks. The
    /// at-fault party sits longest — by the time it re-plans, the victim
    /// has cleared the scene. (The victim's shorter stagger IS the flinch:
    /// the old bump_victim_freeze_ticks died with the template model.)
    pub bump_freeze_ticks: u32,
    /// The forced template prologue: every entry waits this long first
    /// (the visible flinch). Boot's prologue is the upload instead.
    pub handler_init_ticks: u32,
    /// Collisions are accidents: BOTH bots take this chassis damage.
    pub bump_damage: i64,
    /// Bridges price in STONE (docs/03: Stone owns civil works). Deci.
    pub bridge_cost_stone: u64,
    /// Builder-ticks of labor a bridge takes.
    pub bridge_build_ticks: u32,
    /// Placing a traffic overlay (arrow) — instant signage.
    /// Overlays (arrows) price in Stone too — signage is civil kit. Deci.
    pub overlay_cost_stone: u64,
    /// Chassis damage per UNHANDLED fault: crash loops are lethal, and
    /// `on error:` handlers are literal armor (handled faults are free).
    pub fault_damage: i64,
    /// Passive self-repair: +regen_amount hp every regen_interval_ticks.
    pub regen_interval_ticks: u64,
    pub regen_amount: i64,
    /// The Damaged line, in percent of max hp: the hurt signal fires when
    /// hp drops below it and the latch re-arms when regen climbs back over
    /// it — ONE value so the edge trigger can't drift apart. M3's env
    /// registry makes it per-bot (`hurt_line`); this is the match-wide
    /// default until then.
    pub hurt_line_pct: i64,
    // --- perception (M7, docs/05) ---
    pub sense_factor_pct: u32,
    pub structure_sensors: u32,
    pub episode_rearm_ticks: u32,
    pub search_ring_ticks: u32,
    /// How long a bot roots to study a Template Cache (docs/06 Q79: ~10 s).
    pub study_ticks: u32,
    pub explore_radius: u32,
    pub wander_leg: u32,
    pub ford_quiet: i64,
    /// Bonus sensor range while on High Ground / a Mountain summit
    /// (docs/05: +2). Feeds seeing/hearing → the state hash.
    pub high_ground_sensor_bonus: u32,
    /// Combat L3 widens a bot's hearing vs enemies by this (docs/02+05:
    /// "+1 vs enemies").
    pub combat_hearing_bonus: u32,
    // --- terrain v2 (M8, docs/05 Q35–Q40) ---
    /// The ×2-scale move-cost table + Mountain/Mud edge parameters.
    pub tile_costs: crate::map::TileCostTable,
    /// Dunes idle-sink interval / per-interval surcharge / total ceiling.
    pub dune_sink_ticks: u32,
    pub dune_sink_step_x2: u32,
    pub dune_sink_cap_x2: u32,
    /// Scree collapses to Rubble after this many bot entries (Q40).
    pub scree_crossings: u32,
    /// Corruption's per-op cycle tax, in CENTICYCLES (docs/05; M8-B).
    pub corruption_op_tax: u64,
    /// Blight Cores corrupt one nearby tile per this many ticks (M8-C).
    pub corruption_spread_ticks: u64,
    // --- terraform blueprints (M8-D, docs/05): Stone prices in deci ---
    pub clear_ticks: u32,
    /// Clearing rubble YIELDS Stone (deci) to the builder's faction.
    pub clear_yield_stone: u64,
    pub barricade_cost_stone: u64,
    pub barricade_build_ticks: u32,
    pub demolish_ticks: u32,
    pub cleanse_ticks: u32,
    pub road_cost_stone: u64,
    pub road_build_ticks: u32,
    // --- the wreck race (M10, docs/02) ---
    pub wreck_hp_pct: u32,
    pub wreck_countdown_base_ticks: u32,
    pub wreck_countdown_per_100xp_ticks: u32,
    pub blast_radius: u32,
    pub blast_damage_pct: u32,
    pub salvage_ticks: u32,
    pub analyze_ticks: u32,
    pub hijack_ticks: u32,
    pub field_repair_deci: u32,
    pub salvage_receipt_pct: u32,
    pub salvage_decrypt_pct: u32,
    pub analyze_data: u64,
    pub guard_swing_ticks: u32,
    /// How far a guard / escort strays from its anchor before re-closing
    /// (Chebyshev tiles): escort hugs tight, guard holds a short leash.
    pub guard_leash: u32,
    pub escort_leash: u32,
    // --- Ferals (M12, docs/04) ---
    pub nest_hp: i64,
    pub nest_hp_per_arcanum: i64,
    pub nest_seed_stock_deci: u64,
    pub nest_income_deci: u64,
    pub nest_print_cost_deci: u64,
    pub nest_print_ticks: u64,
    pub nest_data_bounty: u64,
    pub nest_guard_radius: u32,
    pub feral_hp_per_threat_pct: i64,
    pub escalation_probing: u64,
    pub escalation_contested: u64,
    pub escalation_overrun: u64,
    pub escalation_kill_weight: u64,
    pub printer_cost_steel: u64,
}

impl Default for Tuning {
    fn default() -> Self {
        let tuning: Tuning = ron::from_str(include_str!("../data/tuning.ron"))
            .expect("data/tuning.ron parses (unknown fields are errors)");
        tuning.validate();
        tuning
    }
}

impl Tuning {
    /// Load-time sanity: durations that gate progress must be non-zero
    /// (a zero here means division-by-zero ticks or instant loops, not a
    /// legitimate tuning choice).
    fn validate(&self) {
        assert!(self.print_ticks > 0, "tuning: print_ticks must be > 0");
        assert!(self.bridge_build_ticks > 0, "tuning: bridge_build_ticks must be > 0");
        assert!(self.regen_interval_ticks > 0, "tuning: regen_interval_ticks must be > 0");
        // The old hardcoded 2 was the implicit >=1 guard; the Mine advance
        // decrements unchecked, so a zero here would underflow-freeze
        // every miner.
        assert!(self.mine_swing_ticks > 0, "tuning: mine_swing_ticks must be > 0");
        for kind in crate::world::StructureKind::ALL {
            assert!(
                self.structure_costs.iter().any(|(k, _)| *k == kind),
                "tuning: structure_costs must price every kind ({} missing)",
                kind.name()
            );
        }
        assert!(
            (1..=100).contains(&self.hurt_line_pct),
            "tuning: hurt_line_pct must be a percentage in 1..=100"
        );
        // Terrain v2: the cost table and the passability predicate are
        // two sources for one fact — validate the BICONDITIONAL, or a
        // tuning edit that drops one entry loads cleanly and panics at
        // the first step onto the unpriced tile (edge_allowed approves
        // via passable(), step_ticks returns None, callers .expect()).
        // Also: A*'s heuristic is manhattan × 1, admissible only while
        // every edge costs at least 1 (review 2026-07-16).
        for kind in crate::map::TileKind::ALL {
            let priced = self.tile_costs.cost_x2(kind);
            assert_eq!(
                priced.is_some(),
                kind.passable(),
                "tuning: tile_costs.x2 must price exactly the passable kinds ({kind:?})"
            );
            if let Some(cost) = priced {
                assert!(cost >= 1, "tuning: tile_costs.x2 must be >= 1 ({kind:?})");
            }
        }
        for cost in [
            self.tile_costs.mountain_climb_x2,
            self.tile_costs.mountain_descend_x2,
            self.tile_costs.mud_loaded_x2,
        ] {
            assert!(cost >= 1, "tuning: mountain/mud edge costs must be >= 1");
        }
        assert!(self.dune_sink_ticks > 0, "tuning: dune_sink_ticks must be > 0");
        for (ticks, name) in [
            (self.clear_ticks, "clear_ticks"),
            (self.barricade_build_ticks, "barricade_build_ticks"),
            (self.demolish_ticks, "demolish_ticks"),
            (self.cleanse_ticks, "cleanse_ticks"),
            (self.road_build_ticks, "road_build_ticks"),
        ] {
            assert!(ticks > 0, "tuning: {name} must be > 0");
        }
        assert!(self.scree_crossings > 0, "tuning: scree_crossings must be > 0");
        assert!(
            self.corruption_spread_ticks > 0,
            "tuning: corruption_spread_ticks must be > 0"
        );
    }
}

/// Printer fleet tuning (M9, `data/printers.ron` — docs/01: the
/// per-printer cap contribution and the default check interval are data).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrinterConfig {
    pub fleet_cap_per_printer: u32,
    pub check_interval_ticks: u64,
}

impl Default for PrinterConfig {
    fn default() -> Self {
        let cfg: PrinterConfig = ron::from_str(include_str!("../data/printers.ron"))
            .expect("data/printers.ron parses (unknown fields are errors)");
        assert!(cfg.check_interval_ticks > 0, "printers: check_interval_ticks must be > 0");
        cfg
    }
}

impl crate::world::BlueprintKind {
    /// Stone price (deci) — beside [`Tuning`], the price sheet's home.
    /// Clear/Demolish/Cleanse are labor-only. Shared with the build bar
    /// so the ghost can't drift from the command (review 2026-07-16).
    pub fn cost_stone(self, tuning: &Tuning) -> u64 {
        use crate::world::BlueprintKind as K;
        match self {
            K::Bridge => tuning.bridge_cost_stone,
            K::Barricade => tuning.barricade_cost_stone,
            K::Road => tuning.road_cost_stone,
            K::Clear | K::Demolish | K::Cleanse => 0,
        }
    }

    /// Builder-ticks at the unleveled rate.
    pub fn build_ticks(self, tuning: &Tuning) -> u32 {
        use crate::world::BlueprintKind as K;
        match self {
            K::Bridge => tuning.bridge_build_ticks,
            K::Clear => tuning.clear_ticks,
            K::Barricade => tuning.barricade_build_ticks,
            K::Demolish => tuning.demolish_ticks,
            K::Cleanse => tuning.cleanse_ticks,
            K::Road => tuning.road_build_ticks,
        }
    }
}

/// Stable hash tag for a selection key (phase-9 snapshot).
fn select_key_tag(key: crate::world::SelectKey) -> u8 {
    use crate::world::SelectKey as K;
    match key {
        K::TotalXp => 0,
        K::Xp(track) => 16 + track.as_u8(),
        K::Hp => 1,
        K::MaxHp => 2,
        K::CpuCenti => 3,
        K::Sensors => 4,
        K::CargoCap => 5,
        K::MoveRate => 6,
        K::ModuleSlots => 7,
    }
}

/// Hash a Station order (queued or mounted) into the phase-9 snapshot.
/// Hash a Pyrite value (channel payloads park inside actions): type tag +
/// Display text — Display is deterministic (dicts iterate in key order).
fn hash_value(h: &mut Fnv1a, v: &pyrite::Value) {
    h.write_str(v.type_name());
    h.write_str(&v.to_string());
}

fn hash_channel_op(h: &mut Fnv1a, op: &crate::world::ChannelOp) {
    use crate::world::ChannelOp;
    match op {
        ChannelOp::Send(v) => {
            h.write_u8(1);
            hash_value(h, v);
        }
        ChannelOp::Receive => h.write_u8(2),
        ChannelOp::Broadcast(v) => {
            h.write_u8(3);
            hash_value(h, v);
        }
    }
}

/// Hash an in-flight action — cross-tick lockstep state (review
/// 2026-07-16: a diverged `waited` counter or race timer must move the
/// phase-9 hash the tick it diverges, not when it finally touches hp).
fn hash_action(h: &mut Fnv1a, action: &crate::world::Action) {
    use crate::world::Action;
    match action {
        Action::Move { path, ticks_left, goals } => {
            h.write_u8(1);
            h.write_u32(path.len() as u32);
            for p in path {
                h.write_i32(p.x);
                h.write_i32(p.y);
            }
            h.write_u32(*ticks_left);
            h.write_u32(goals.len() as u32);
            for g in goals {
                h.write_i32(g.x);
                h.write_i32(g.y);
            }
        }
        Action::Mine { node, ticks_left } => {
            h.write_u8(2);
            h.write_u64(node.0);
            h.write_u32(*ticks_left);
        }
        Action::Deposit { depot, ticks_left, fault_on_fail } => {
            h.write_u8(3);
            h.write_u64(depot.0);
            h.write_u32(*ticks_left);
            h.write_u8(*fault_on_fail as u8);
        }
        Action::Attack { target, ticks_left } => {
            h.write_u8(4);
            h.write_u64(target.0);
            h.write_u32(*ticks_left);
        }
        Action::Wait { ticks_left } => {
            h.write_u8(5);
            h.write_u32(*ticks_left);
        }
        Action::Build { blueprint } => {
            h.write_u8(6);
            h.write_u64(blueprint.0);
        }
        Action::Search { reach, current, ticks_left } => {
            h.write_u8(7);
            h.write_u32(*reach);
            h.write_u32(*current);
            h.write_u32(*ticks_left);
        }
        Action::Repair { target, done_deci } => {
            h.write_u8(8);
            h.write_u64(target.0);
            h.write_u32(*done_deci);
        }
        Action::Race { wreck, kind, ticks_left } => {
            h.write_u8(9);
            h.write_u32(wreck.0);
            h.write_u8(match kind {
                crate::world::RaceKind::Salvage => 1,
                crate::world::RaceKind::Analyze => 2,
                crate::world::RaceKind::Hijack => 3,
            });
            h.write_u32(*ticks_left);
        }
        Action::Recover { target, ticks_left } => {
            h.write_u8(10);
            h.write_u64(target.0);
            h.write_u32(*ticks_left);
        }
        Action::Guard { target, escort, step_wait, cooldown } => {
            h.write_u8(11);
            h.write_u64(target.0);
            h.write_u8(*escort as u8);
            h.write_u32(*step_wait);
            h.write_u32(*cooldown);
        }
        Action::Channel { op, ch, namespace, waited, timeout, delivered } => {
            h.write_u8(12);
            hash_channel_op(h, op);
            h.write_str(ch);
            h.write_u8(*namespace);
            h.write_u32(*waited);
            h.write_u8(timeout.is_some() as u8);
            h.write_u32(timeout.unwrap_or(0));
            h.write_u8(delivered.is_some() as u8);
            if let Some(v) = delivered {
                hash_value(h, v);
            }
        }
        Action::Study { cache, ticks_left } => {
            h.write_u8(13);
            h.write_u64(cache.0);
            h.write_u32(*ticks_left);
        }
    }
}

fn hash_request(h: &mut Fnv1a, req: &crate::world::ActionRequest) {
    use crate::world::ActionRequest;
    match req {
        ActionRequest::MoveTo(e) => {
            h.write_u8(1);
            h.write_u64(e.0);
        }
        ActionRequest::Mine => h.write_u8(2),
        ActionRequest::Deposit { fault_on_fail } => {
            h.write_u8(3);
            h.write_u8(*fault_on_fail as u8);
        }
        ActionRequest::Attack(e) => {
            h.write_u8(4);
            h.write_u64(e.0);
        }
        ActionRequest::Wait(n) => {
            h.write_u8(5);
            h.write_u32(*n);
        }
        ActionRequest::Build(e) => {
            h.write_u8(6);
            h.write_u64(e.0);
        }
        ActionRequest::Search => h.write_u8(7),
        ActionRequest::Wander => h.write_u8(8),
        ActionRequest::Explore => h.write_u8(9),
        ActionRequest::Repair(e) => {
            h.write_u8(10);
            h.write_u64(e.0);
        }
        ActionRequest::Salvage(e) => {
            h.write_u8(11);
            h.write_u64(e.0);
        }
        ActionRequest::Analyze(e) => {
            h.write_u8(12);
            h.write_u64(e.0);
        }
        ActionRequest::Hijack(e) => {
            h.write_u8(13);
            h.write_u64(e.0);
        }
        ActionRequest::Recover(e) => {
            h.write_u8(14);
            h.write_u64(e.0);
        }
        ActionRequest::Guard { target, escort } => {
            h.write_u8(15);
            h.write_u64(target.0);
            h.write_u8(*escort as u8);
        }
        ActionRequest::Channel { op, ch, namespace, timeout } => {
            h.write_u8(16);
            hash_channel_op(h, op);
            h.write_str(ch);
            h.write_u8(*namespace);
            h.write_u8(timeout.is_some() as u8);
            h.write_u32(timeout.unwrap_or(0));
        }
        ActionRequest::Study => h.write_u8(17),
    }
}

/// Hash EVERY BotData field — shared by live bots and wrecks so the two
/// can never drift apart in coverage (review 2026-07-16: the wreck block
/// hashed upgrade COUNTS while its data reboots into live play whole).
fn hash_bot_data(h: &mut Fnv1a, data: &crate::world::BotData) {
    h.write_u32(data.id.0);
    h.write_u64(data.entity.0);
    h.write_u8(data.faction);
    h.write_i32(data.pos.x);
    h.write_i32(data.pos.y);
    h.write_i64(data.hp);
    h.write_i64(data.max_hp);
    h.write_u8(data.hurt_fired as u8);
    h.write_u32(data.cargo.len() as u32);
    for (kind, deci) in &data.cargo {
        h.write_u8(kind.as_u8());
        h.write_u32(*deci);
    }
    h.write_u32(data.cargo_cap);
    h.write_u64(data.cpu_centi);
    h.write_u32(data.move_rate_deci);
    h.write_u32(data.sensors);
    h.write_u32(data.module_slots);
    h.write_u32(data.log_cap);
    h.write_u32(data.upgrades.len() as u32);
    for u in &data.upgrades {
        h.write_u8(*u);
    }
    h.write_u32(data.modules.len() as u32);
    for m in &data.modules {
        h.write_u8(*m);
    }
    h.write_u32(data.upgrade_queue.len() as u32);
    for order in &data.upgrade_queue {
        hash_order(h, order);
    }
    h.write_u32(data.withdrawn_aboard);
    h.write_u8(data.pad_sit as u8);
    h.write_u8(data.survey_after_move as u8);
    h.write_u8(data.color.0);
    h.write_u8(data.requested.is_some() as u8);
    if let Some(req) = &data.requested {
        hash_request(h, req);
    }
    h.write_u8(data.action.is_some() as u8);
    if let Some(action) = &data.action {
        hash_action(h, action);
    }
    h.write_u32(data.booting.unwrap_or(0));
    h.write_u8(data.recall.is_some() as u8);
    if let Some(recall) = &data.recall {
        h.write_u32(recall.path.len() as u32);
        for p in &recall.path {
            h.write_i32(p.x);
            h.write_i32(p.y);
        }
        h.write_u32(recall.ticks_left);
        h.write_u64(recall.home.0);
        match recall.purpose {
            crate::world::RecallPurpose::Recolor { dest } => {
                h.write_u8(1);
                h.write_u64(dest.0);
            }
            crate::world::RecallPurpose::Scrap => h.write_u8(2),
        }
    }
    h.write_u32(data.bump_frozen);
    h.write_u8(data.dying as u8);
    h.write_u32(data.log_buf.len() as u32);
    for (level, entry) in &data.log_buf {
        h.write_u8(*level);
        h.write_str(entry);
    }
    h.write_u32(data.env.len() as u32);
    for (key, value) in &data.env {
        h.write_str(key);
        h.write_i64(*value);
    }
    h.write_u32(data.xp.len() as u32);
    for (track, deci) in &data.xp {
        h.write_u8(track.as_u8());
        h.write_u64(*deci);
    }
    h.write_u64(data.haul_accum);
    h.write_u64(data.learning_carry);
    h.write_u32(data.age_hp_levels);
    h.write_u32(data.gain_carry.len() as u32);
    for (track, rem) in &data.gain_carry {
        h.write_u8(track.as_u8());
        h.write_u64(*rem);
    }
    h.write_u64(data.moved_tick);
    h.write_u32(data.episodes.len() as u32);
    for (faction, counter) in &data.episodes {
        h.write_u8(*faction);
        h.write_u32(*counter);
    }
    h.write_u8(data.countdown_carry.is_some() as u8);
    h.write_u32(data.countdown_carry.unwrap_or(0));
    h.write_u32(data.latent_quirks.len() as u32);
    for q in &data.latent_quirks {
        h.write_u8(*q);
    }
    h.write_u32(data.quirks.len() as u32);
    for q in &data.quirks {
        h.write_u8(*q);
    }
    h.write_u64(data.crash_seen);
    h.write_u64(data.rng_program);
    h.write_u32(data.dune_idle);
}

fn hash_order(h: &mut Fnv1a, order: &crate::world::UpgradeOrder) {
    match order {
        crate::world::UpgradeOrder::Compute(idx) => {
            h.write_u8(1);
            h.write_u8(*idx);
        }
        crate::world::UpgradeOrder::Module { idx, replace } => {
            h.write_u8(2);
            h.write_u8(*idx);
            // Presence + raw value, NO arithmetic: `slot + 1` overflowed
            // on a hostile replace=Some(255) command (hashing must never
            // panic on queueable state).
            h.write_u8(replace.is_some() as u8);
            h.write_u8(replace.unwrap_or(0));
        }
    }
}

/// Upkeep config (M5, docs/02-03 Q84): the data-driven resource mix —
/// v1 = Energy (primary drain) + Steel (chassis maintenance). Values in
/// `data/upkeep.ron`; maps with `dev_free_power` (the default) skip the
/// whole settlement.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upkeep {
    pub interval_ticks: u64,
    pub base_draw: u64,
    pub draw_per_upgrade: u64,
    pub draw_per_module: u64,
    /// Per XP track LEVEL (M6, docs/02: veterans cost more to run).
    pub draw_per_track_level: u64,
    pub draw_per_refinery: u64,
    pub generator_output_wood: u64,
    pub generator_output_coal: u64,
    pub generator_fuel_deci: u32,
    pub generator_stoke_deci: u32,
    pub geothermal_output: u64,
    pub steel_per_bot_deci: u64,
    pub rust_decay_hp: i64,
    pub rust_scraps: bool,
}

impl Default for Upkeep {
    fn default() -> Self {
        let upkeep: Upkeep = ron::from_str(include_str!("../data/upkeep.ron"))
            .expect("data/upkeep.ron parses (unknown fields are errors)");
        assert!(upkeep.interval_ticks > 0, "upkeep: interval_ticks must be > 0");
        assert!(upkeep.generator_fuel_deci > 0, "upkeep: generator_fuel_deci must be > 0");
        upkeep
    }
}

/// External inputs: the ONLY way anything outside the sim mutates it
/// (single-player is lockstep with one peer).
/// A sim-speed proposal (M13, docs/08): unanimous consent or nothing.
/// Pause is SetSpeed(0); Resume is SetSpeed(1000).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Proposal {
    /// Per-mille of base speed (0 = pause, 1000 = normal, 2000 = double).
    SetSpeed(u32),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    /// Test/debug spawn that bypasses printers (bots normally print).
    SpawnBot {
        pos: TilePos,
        source: String,
        cpu: u64,
        cargo_cap: u32,
        faction: u8,
        hp: i64,
        color: Color,
    },
    /// Deploy source to a (faction, color) slot: new prints and re-colors
    /// use it immediately; live bots of that color hot-swap at their next
    /// loop boundary (docs/01).
    DeployProgram { faction: u8, color: Color, source: String },
    /// The population dial on a printer.
    /// Edit one dialed printer's target-share rules (M9, replaces the
    /// superseded desired-max dial). The faction's FIRST printer is the
    /// remainder bucket — not editable, the edit is ignored. Every rule
    /// edit fires an immediate signal-like re-allocation (docs/01);
    /// `check_interval` (when set) retunes the faction's clock.
    EditPrinterRules {
        printer: EntityId,
        target: crate::world::PrintTarget,
        key: crate::world::SelectKey,
        best_first: bool,
        priority: u32,
        check_interval: Option<u64>,
    },
    /// LEGACY (pre-M9 replay files): the superseded desired-max dial.
    /// Kept ONLY so old recordings still DESERIALIZE — it maps to the
    /// closest v2 rule (target = Count(value), default key/priority).
    /// Old replays still diverge in hash (documented with M9's fixture
    /// regeneration); they just never fail to load.
    SetDesiredMax { printer: EntityId, value: u32 },
    /// Queue one replacement stock print (M9: a reprint IS a fresh print;
    /// its color comes from the allocation like anyone else's). The
    /// docs' `loadout` parameter is undefined — flagged in TASKS.md.
    QueuePrint { faction: u8 },
    /// Fix a ruined printer (Data cost; ore stands in until Data exists).
    RepairPrinter { printer: EntityId },
    /// Designate a terraform site (the build UI's output). Bots do the
    /// labor via closest(blueprint).expect()/build(). The placing faction
    /// pays from its stock (serde-defaulted for stored replays).
    PlaceBlueprint {
        pos: TilePos,
        kind: BlueprintKind,
        #[serde(default)]
        faction: u8,
    },
    /// Set or clear a traffic overlay on any tile — instant signage, not
    /// construction (small ore cost to place; clearing is free).
    PlaceOverlay {
        pos: TilePos,
        overlay: Option<OverlayKind>,
        #[serde(default)]
        faction: u8,
    },
    /// Set or clear cosmetic tile paint (free).
    PlacePaint { pos: TilePos, color: Option<u8> },
    /// DEV-ONLY emergency stop (M3: abort() is the only PLAYER scuttle):
    /// straight to wreck, no template — the owner pulled the plug. Logs
    /// ride in the wreck; carried cargo spills onto the ground. Kept for
    /// tests, the editor Kill tool, and the golden replay fixture.
    KillBot { bot: BotId },
    /// Gift Data to another faction (M13, docs/08's trade scaffolding).
    /// Clamped to what the giver has; lockstep trusts the relay to only
    /// forward a faction's own commands (flagged in TASKS.md).
    ExchangeData { from: u8, to: u8, amount: u64 },
    /// Post to the cross-faction Request Box (M13, docs/08). Text is
    /// clamped (hostile peers); the board keeps the newest entries.
    PostRequest { faction: u8, text: String },
    /// Grant (or revoke) Vision / Channels to another faction (M13,
    /// docs/08: allies share WORK PRODUCTS, not capability).
    Grant { from: u8, to: u8, what: crate::world::GrantKind, revoke: bool },
    /// Declare or dissolve an alliance (M13). Allies pool decryption
    /// progress from here on (docs/08: one teammate's salvage advances
    /// the team's level — earlier levels aren't retroactively merged).
    SetAlliance { a: u8, b: u8, allied: bool },
    /// Sim-speed voting (M13, docs/08): approve opens (or joins) the
    /// proposal; unanimity across live factions applies it; a refusal
    /// fails it. Every attempt, pass or fail, starts the cooldown.
    Vote { faction: u8, proposal: Proposal, approve: bool },
    /// Convert a DEFEATED nest into a claimed site (M12, docs/04):
    /// counts toward the claimant's quadratic printer gate. Instead of
    /// razing — pick one. (Docs want a build-tool bot doing the labor;
    /// instant command for now, flagged in TASKS.md.)
    ClaimNest { nest: EntityId, faction: u8 },
    /// Raze a DEFEATED nest: the site is gone, the razer banks the Data
    /// bounty (docs/04: "Data now" vs. the claim's "slots forever").
    RazeNest { nest: EntityId, faction: u8 },
    /// Build a printer (M12, docs/01): slots beyond the map's starting
    /// two are gated by CONTROLLED NESTS on the quadratic curve (3rd
    /// needs 1, 4th needs 3, then 6, 10, …). Steel cost; the new slot
    /// takes the faction's lowest unused color index. Ungated or
    /// unaffordable placements are ignored — lockstep commands never
    /// error.
    PlacePrinter { pos: TilePos, faction: u8 },
    /// Place a structure, paying its typed cost from colony stock
    /// (docs/03: payments are abstract). Instant placement for now; build
    /// labor for structures is flagged for discussion in TASKS.md.
    PlaceStructure { pos: TilePos, kind: crate::world::StructureKind, faction: u8 },
    /// Set (or clear) a refinery's recipe by RECIPES index (docs/03,
    /// round 4: recipe set per structure).
    SetRecipe { structure: EntityId, recipe: Option<u8> },
    /// Queue a Station order for a bot by catalog name (docs/03 M5:
    /// designation is the player's; the PROGRAM must bring the bot to a
    /// pad). `replace` names the module slot a swap destroys (modules
    /// only; required when every slot is full). Unknown names and invalid
    /// slots are ignored — lockstep commands never error.
    QueueUpgrade {
        bot: BotId,
        order: String,
        #[serde(default)]
        replace: Option<u8>,
    },
    /// Spend colony Data on a permanent construct unlock (docs/03/06:
    /// research is structure-free — the Archive is the bank, not the
    /// school).
    Research { faction: u8, construct: pyrite::Construct },
}

impl Command {
    /// The faction this command acts *as*, when it's carried in the operand
    /// (Q86, docs/08). The lockstep relay authorizes each command by
    /// comparing this against the sender's owned faction(s) — a forged
    /// cross-faction command (debit a rival's Data, dissolve another pair's
    /// alliance, vote as someone else, bank another player's nest bounty) is
    /// dropped before `apply`. Entity-scoped commands (whose actor is the
    /// *owner* of a printer/bot/structure, not an operand) return `None`
    /// here; [`Sim::command_actor_faction`] resolves those against the world.
    /// `None` from both means "no faction to check" — cosmetic/ownerless
    /// commands anyone may issue.
    pub fn actor_faction(&self) -> Option<u8> {
        match self {
            Command::SpawnBot { faction, .. }
            | Command::DeployProgram { faction, .. }
            | Command::QueuePrint { faction }
            | Command::PlaceBlueprint { faction, .. }
            | Command::PlaceOverlay { faction, .. }
            | Command::PostRequest { faction, .. }
            | Command::Vote { faction, .. }
            | Command::ClaimNest { faction, .. }
            | Command::RazeNest { faction, .. }
            | Command::PlacePrinter { faction, .. }
            | Command::PlaceStructure { faction, .. }
            | Command::Research { faction, .. } => Some(*faction),
            Command::ExchangeData { from, .. } | Command::Grant { from, .. } => Some(*from),
            Command::SetAlliance { a, .. } => Some(*a),
            // Entity-scoped: actor is the entity's owner (resolved by Sim).
            Command::EditPrinterRules { .. }
            | Command::SetDesiredMax { .. }
            | Command::RepairPrinter { .. }
            | Command::SetRecipe { .. }
            | Command::QueueUpgrade { .. }
            | Command::KillBot { .. } => None,
            // Cosmetic / ownerless — no faction to authorize against.
            Command::PlacePaint { .. } => None,
        }
    }
}

pub struct Sim {
    pub world: World,
    pub costs: CostTable,
    pub vm_config: VmConfig,
    pub tuning: Tuning,
    pub printer_cfg: PrinterConfig,
    /// The universal chassis: floor statline, modifier-pipeline penalties,
    /// and the Upgrade Station catalog (`data/stats.ron`, M5).
    pub stats: crate::stats::Stats,
    /// Energy + Steel upkeep config (`data/upkeep.ron`, M5).
    pub upkeep: Upkeep,
    /// XP curve, incomes, and perk magnitudes (`data/xp.ron`, M6).
    pub xp: crate::xp::XpConfig,
    /// The quirk catalog + manifestation thresholds (`data/quirks.ron`, M6).
    pub quirks: crate::quirks::QuirkCatalog,
    /// Phase-9 snapshot: the state hash of the last completed tick. The
    /// lockstep relay compares this across peers for desync detection.
    pub last_hash: u64,
}

impl Sim {
    pub fn new(spec: &MapSpec) -> Self {
        let mut tuning = Tuning::default();
        // Match-settings overrides (M13, docs/08 Q77): the inventory's
        // dials that shadow tuning.ron figures for this match.
        if let Some(cost) = spec.settings.print_cost_steel {
            tuning.print_cost_steel = cost;
        }
        if let Some(pct) = spec.settings.salvage_decrypt_pct {
            tuning.salvage_decrypt_pct = pct;
        }
        let mut vm_config = VmConfig::default();
        // FACTORY WINDOW contents (docs/01's template table), as REAL
        // Pyrite: watchable, line-highlighted, costed, replaceable by the
        // player's own `on <signal>:` block. Hurt, bumped, and boot ship
        // empty — the forced prologue flinch IS their default reaction —
        // so they simply have no entry here.
        let factory_windows = [
            (pyrite::ast::SignalKind::Error, "upload_crash_dump()\n".to_string()),
            // + the 15-tick init flinch = the rammer's 50-tick at-fault stun.
            (
                pyrite::ast::SignalKind::Bump,
                format!(
                    "wait({})\n",
                    tuning.bump_freeze_ticks.saturating_sub(tuning.handler_init_ticks)
                ),
            ),
        ];
        for (kind, source) in factory_windows {
            let program =
                pyrite::parse(&source, &UnlockSet::all()).expect("factory windows parse");
            vm_config.default_handlers.insert(
                kind,
                pyrite::vm::DefaultHandler { source, program: Rc::new(program) },
            );
        }
        // Entity-kind constants for the generic queries: `closest(ore)`,
        // `exists(blueprint)`, ... They live in the config (not globals) so
        // they survive the post-fault VM reset; assignments can shadow them.
        for kind in crate::host::KINDS {
            vm_config.constants.insert(kind.to_string(), Value::Str(kind.to_string()));
        }
        // Log-level constants (docs/01): ordinary shadowable names, ints so
        // the same constants work as env values (`setenv(log_min_level,
        // warn)`) and as `log(x, level=warn)` arguments. One source —
        // `world::LEVEL_NAMES` — shared with the HUD's display prefixes.
        for (rank, name) in crate::world::LEVEL_NAMES.iter().enumerate() {
            vm_config.constants.insert(name.to_string(), Value::Int(rank as i64));
        }
        // Env keys as constants: `setenv(hurt_line, 30)` reads naturally.
        for key in crate::world::ENV_KEYS {
            vm_config.constants.insert(key.name.to_string(), Value::Str(key.name.to_string()));
        }
        // Host-domain fault ids (M7): comparable via last_error().
        vm_config.constants.insert(
            crate::host::UNKNOWN_CONTACT.to_string(),
            Value::Str(crate::host::UNKNOWN_CONTACT.to_string()),
        );
        // Quirk names as constants (docs/09: pre-bound like kind
        // constants, no enum): `has_quirk(overclocked)` reads naturally.
        let quirk_catalog = crate::quirks::QuirkCatalog::default();
        for quirk in &quirk_catalog.quirks {
            vm_config.constants.insert(quirk.name.clone(), Value::Str(quirk.name.clone()));
        }
        let mut sim = Self {
            world: World::from_spec(spec),
            costs: CostTable::default(),
            vm_config,
            tuning,
            stats: crate::stats::Stats::default(),
            upkeep: Upkeep::default(),
            xp: crate::xp::XpConfig::default(),
            quirks: quirk_catalog,
            printer_cfg: {
                let mut cfg = PrinterConfig::default();
                if let Some(cap) = spec.fleet_cap_override {
                    cfg.fleet_cap_per_printer = cap;
                }
                cfg
            },
            last_hash: 0,
        };
        // Map-authored structures place free; Generators start STOKED
        // (docs/03: the opening never brownouts before the player acts).
        for (pos, kind) in &spec.structures {
            let id = sim.world.alloc_entity();
            let mut input = std::collections::BTreeMap::new();
            if *kind == crate::world::StructureKind::Generator {
                input.insert(crate::resources::Resource::Coal, sim.upkeep.generator_stoke_deci);
            }
            sim.world.structures.insert(
                id,
                crate::world::Structure {
                    kind: *kind,
                    faction: 0,
                    pos: *pos,
                    hp: sim.tuning.structure_hp,
                    max_hp: sim.tuning.structure_hp,
                    input,
                    output: std::collections::BTreeMap::new(),
                    recipe: None,
                    batch: None,
                    pad: None,
                },
            );
        }
        // The VM's base stack depth is a chassis stat (stats.ron floor;
        // Stack extensions raise it per bot).
        sim.vm_config.stack_depth = sim.stats.stack_depth as usize;
        // Phase-0 upkeep seed (like the perception seed): tick 1 starts
        // with correct brownout/rust flags rather than a settlement-sized
        // grace window.
        if !sim.world.dev_free_power {
            sim.settle_upkeep();
        }
        // A printer is born with its color slot AND an empty program
        // file (M9, Q85): targets are settable immediately, and a bot
        // re-colored onto the empty program idles — visibly — until the
        // player writes something. Real deploys overwrite these.
        let slots: Vec<(u8, u8)> = sim
            .world
            .printers
            .values()
            .map(|p| (p.faction, p.color.0))
            .collect();
        for (faction, color) in slots {
            if !sim.world.color_programs.contains_key(&(faction, color)) {
                sim.apply(&Command::DeployProgram {
                    faction,
                    color: crate::world::Color(color),
                    source: String::new(),
                })
                .expect("the empty program parses");
            }
        }
        // The rest of the match settings land on the world (M13).
        sim.world.harm_enabled = spec.settings.harm != crate::map::HarmMode::NonPvp;
        sim.world.vote_cooldown_ticks = spec.settings.vote_cooldown_ticks;
        sim.world.vote_window_ticks = spec.settings.vote_window_ticks;
        // Feral nests (M12) — allocated last so existing fixtures' entity
        // ids stay put. The Ferals toggle (M13) is the "pure PvP" switch.
        if spec.settings.ferals {
            sim.build_nests(spec);
        }
        // Phase-0 perception seed (docs/07, round 4): tick 1's queries have
        // a "previous tick" to read, so the pre-deployed starter program
        // works from its first operation. A stub until M7, like phase 5.
        sim.run_perception();
        sim.last_hash = sim.state_hash();
        sim
    }

    /// The faction a command acts *as*, for relay authorization (Q86,
    /// docs/08). Prefers the operand ([`Command::actor_faction`]); for
    /// entity-scoped commands it resolves the target entity's owner against
    /// the world. `None` = no faction to check (cosmetic/ownerless, or a
    /// stale entity handle that `apply` will no-op on anyway). A pure
    /// function of (command, world), so every peer resolves it identically —
    /// authorization stays deterministic.
    pub fn command_actor_faction(&self, command: &Command) -> Option<u8> {
        if let Some(f) = command.actor_faction() {
            return Some(f);
        }
        match command {
            Command::EditPrinterRules { printer, .. }
            | Command::SetDesiredMax { printer, .. }
            | Command::RepairPrinter { printer } => {
                self.world.printers.get(printer).map(|p| p.faction)
            }
            Command::SetRecipe { structure, .. } => {
                self.world.structures.get(structure).map(|s| s.faction)
            }
            Command::QueueUpgrade { bot, .. } | Command::KillBot { bot } => {
                self.world.bots.get(bot).map(|b| b.data.faction)
            }
            _ => None,
        }
    }

    /// Phase 1: apply a command. Deterministic given identical call order.
    /// Returns the new bot's id for spawn commands.
    /// Function-block gating (M15, docs/06): reject a deploy that calls a
    /// builtin whose Cache the colony hasn't studied. Dev sandboxes unlock
    /// every block, so this is inert until a real match runs `dev_all_unlocks`
    /// off. Feral programs bypass it (they parse via feral.rs, not here — and
    /// docs/06 rule 3 has enemies preview unlocks anyway).
    fn check_functions(
        &self,
        faction: u8,
        program: &pyrite::Program,
    ) -> Result<(), PyriteError> {
        let called = pyrite::called_names(program);
        if let Some((func, block)) =
            crate::world::locked_builtins(&self.world, faction, &called).into_iter().next()
        {
            return Err(PyriteError {
                line: 0,
                col: 1,
                kind: pyrite::PyriteErrorKind::LockedFunction {
                    func,
                    block: block.display_name(),
                },
            });
        }
        Ok(())
    }

    pub fn apply(&mut self, command: &Command) -> Result<Option<BotId>, PyriteError> {
        match command {
            Command::SpawnBot { pos, source, cpu, cargo_cap, faction, hp, color } => {
                // Bots are solid: a spawn onto an impassable, structure, or
                // occupied tile is rejected (dev command or not — nothing
                // may stack two live bots on one tile). Spawnable, not
                // merely passable: nothing materializes on High Ground.
                let free = self.world.grid.get(*pos).is_some_and(|t| t.spawnable())
                    && !self.world.structure_at(*pos)
                    && !self.world.tile_occupied(*pos, BotId(u32::MAX));
                if !free {
                    return Ok(None);
                }
                let unlocks = crate::world::faction_unlocks(&self.world, *faction);
                let program = pyrite::parse(source, &unlocks)?;
                // Deploy-time window analysis (M3): caps, signal safety,
                // loop/recursion ban — rejected here, never at runtime.
                pyrite::check_windows(&program, &self.costs)?;
                // Function-block gating (M15, docs/06): calls to un-studied
                // Cache functions are rejected at deploy, like locked syntax.
                self.check_functions(*faction, &program)?;
                let vm = Vm::new(Rc::new(program), self.vm_config.clone());
                // Dev-spawn overrides arrive in human units (cycles/tick,
                // cargo units); stored units are centi/deci (Q56). Inputs
                // are clamped — a lockstep command must never panic the
                // sim (hostile peers, buggy replay files), and hp feeds
                // `hp * 2` / `hp * 100` comparisons downstream.
                let id = self.insert_bot(
                    *pos,
                    *faction,
                    *color,
                    (*hp).clamp(1, 1_000_000_000),
                    cpu.saturating_mul(100),
                    cargo_cap.saturating_mul(crate::resources::DECI),
                    vm,
                    false,
                );
                // The phase-0 perception seed extends to spawns (docs/07:
                // tick 1's queries must have a "previous tick" to read —
                // otherwise a spawned starter program eats one blind-crash
                // before its first perception pass). Deterministic:
                // commands apply in relay order.
                self.run_perception();
                Ok(Some(id))
            }
            Command::DeployProgram { faction, color, source } => {
                let unlocks = crate::world::faction_unlocks(&self.world, *faction);
                let program = pyrite::parse(source, &unlocks)?;
                pyrite::check_windows(&program, &self.costs)?;
                self.check_functions(*faction, &program)?;
                let slot = (*faction, color.0);
                // The artifact sets the color's HARDWARE BAR (M9, Q52):
                // its printer claims only bots whose bought hardware
                // fits. The remainder color must fit STOCK hardware — it
                // has to be able to receive ANY bot in the colony.
                let (req_lines, req_names) =
                    pyrite::analysis::artifact_requirements(source, &program);
                let remainder_color = self
                    .world
                    .remainder_printer(*faction)
                    .and_then(|id| self.world.printers.get(&id))
                    .map(|p| p.color);
                if remainder_color == Some(*color)
                    && (req_lines > self.stats.program_lines
                        || req_names > self.stats.variable_slots)
                {
                    return Err(PyriteError {
                        line: 0,
                        col: 0,
                        kind: pyrite::PyriteErrorKind::RemainderOverBar {
                            lines: req_lines,
                            names: req_names,
                            cap_lines: self.stats.program_lines,
                            cap_names: self.stats.variable_slots,
                        },
                    });
                }
                let hash = crate::world::program_hash(source);
                self.world.program_library.entry(hash).or_insert_with(|| source.clone());
                let program = Rc::new(program);
                self.world.color_programs.insert(
                    slot,
                    ColorProgram {
                        source: source.clone(),
                        program: Rc::clone(&program),
                        hash,
                        req_lines,
                        req_names,
                    },
                );
                // Hot-swap every live FITTING bot of this color at its
                // next loop boundary (docs/01: redeploy semantics). An
                // over-bar member never receives the new version — it is
                // a lame duck, visibly running the FINAL OLD VERSION
                // until its polite recall lands (Q52/Q85, round 4).
                let ids: Vec<crate::world::BotId> = self.world.bots.keys().copied().collect();
                for id in ids {
                    let data = &self.world.bots[&id].data;
                    if data.faction != *faction || data.color != *color {
                        continue;
                    }
                    let fits = self.ctx().program_lines_for(data) >= req_lines
                        && self.ctx().variable_slots_for(data) >= req_names;
                    if !fits {
                        continue;
                    }
                    if let Some(vm) = self.world.bots.get_mut(&id).and_then(|b| b.vm.as_mut()) {
                        vm.queue_program(Rc::clone(&program));
                    }
                }
                // A deploy is its own dispatch trigger, scoped to its
                // color (docs/01): assignments change at once, the drop/
                // claim recalls land politely — the lame-duck rule.
                self.allocate_fleet(
                    *faction,
                    crate::printers::RecallMode::Polite,
                    Some(*color),
                );
                Ok(None)
            }
            Command::EditPrinterRules {
                printer,
                target,
                key,
                best_first,
                priority,
                check_interval,
            } => {
                let Some(p) = self.world.printers.get(printer) else { return Ok(None) };
                let faction = p.faction;
                // The faction clock retunes regardless of WHICH printer
                // carried the command — only the RULES edit is ignored on
                // the remainder (its dials don't exist; the clock does).
                if let Some(interval) = check_interval {
                    self.world.check_interval.insert(faction, (*interval).max(1));
                }
                if self.world.remainder_printer(faction) == Some(*printer) {
                    return Ok(None); // the remainder bucket has no dials
                }
                if let Some(p) = self.world.printers.get_mut(printer) {
                    p.rules = Some(crate::world::PrinterRules {
                        target: *target,
                        key: *key,
                        best_first: *best_first,
                        priority: *priority,
                    });
                }
                // A rule edit fires the global, signal-like pass NOW —
                // mid-template bots double-handle (your clock, your risk).
                self.allocate_fleet(faction, crate::printers::RecallMode::Signal, None);
                Ok(None)
            }
            Command::SetDesiredMax { printer, value } => {
                // Legacy alias: forward to the v2 rules with defaults.
                let rules = crate::world::PrinterRules {
                    target: crate::world::PrintTarget::Count(*value),
                    ..Default::default()
                };
                self.apply(&Command::EditPrinterRules {
                    printer: *printer,
                    target: rules.target,
                    key: rules.key,
                    best_first: rules.best_first,
                    priority: rules.priority,
                    check_interval: None,
                })
            }
            Command::QueuePrint { faction } => {
                *self.world.reprint_queue.entry(*faction).or_insert(0) += 1;
                Ok(None)
            }
            Command::RepairPrinter { printer } => {
                // Re-priced in Data (docs/03: the first colony milestone).
                let cost = self.tuning.repair_cost_data;
                if let Some(p) = self.world.printers.get_mut(printer)
                    && p.state == PrinterState::Ruined
                {
                    let faction = p.faction;
                    let have = self.world.data.get(&faction).copied().unwrap_or(0);
                    if have >= cost {
                        p.state = PrinterState::Working;
                        self.world.data.insert(faction, have - cost);
                    }
                }
                Ok(None)
            }
            Command::PlaceBlueprint { pos, kind, faction } => {
                // Site + price + duration all come from the shared rule
                // set (BlueprintKind::site_ok / cost_stone / build_ticks)
                // so the build bar's ghost can't drift from what this
                // command accepts (review 2026-07-16).
                let valid_site = self.world.blueprint_site_ok(*kind, *pos);
                let occupied_by_blueprint =
                    self.world.blueprints.values().any(|b| b.pos == *pos);
                let cost = kind.cost_stone(&self.tuning);
                if valid_site
                    && !occupied_by_blueprint
                    && self.world.stock_take(*faction, crate::resources::Resource::Stone, cost)
                {
                    // Progress is deci-units (Q56): base build rate is 10
                    // deci/tick, so ticks-to-complete stays the tuning
                    // figure for an unleveled builder.
                    let needed = kind.build_ticks(&self.tuning) * crate::resources::DECI;
                    let id = self.world.alloc_entity();
                    self.world
                        .blueprints
                        .insert(id, Blueprint { pos: *pos, kind: *kind, progress: 0, needed });
                }
                Ok(None)
            }
            Command::PlaceOverlay { pos, overlay, faction } => {
                if self.world.grid.in_bounds(*pos) {
                    match overlay {
                        Some(kind) => {
                            let cost = self.tuning.overlay_cost_stone;
                            if self.world.stock_take(
                                *faction,
                                crate::resources::Resource::Stone,
                                cost,
                            ) {
                                self.world.overlays.insert(*pos, *kind);
                            }
                        }
                        None => {
                            self.world.overlays.remove(pos);
                        }
                    }
                }
                Ok(None)
            }
            Command::PlaceStructure { pos, kind, faction } => {
                use crate::resources::{Resource, DECI};
                use crate::world::StructureKind;
                let free = self.world.grid.get(*pos).is_some_and(|t| t.spawnable())
                    && !self.world.structure_at(*pos)
                    && !self.world.tile_occupied(*pos, BotId(u32::MAX))
                    // The Tap harnesses a vent — vent tiles only (docs/03).
                    && (*kind != StructureKind::GeothermalTap
                        || self.world.grid.get(*pos) == Some(TileKind::Vent));
                // Typed prices live in tuning.ron (docs/03 figures;
                // validated complete at load — every kind has an entry).
                let cost: &[(Resource, u32)] = self
                    .tuning
                    .structure_costs
                    .iter()
                    .find(|(k, _)| k == kind)
                    .map(|(_, c)| c.as_slice())
                    .expect("validated at load");
                let affordable = cost.iter().all(|(k, units)| {
                    self.world.stock_get(*faction, *k) >= (*units * DECI) as u64
                });
                if free && affordable {
                    for (k, units) in cost {
                        let taken =
                            self.world.stock_take(*faction, *k, (*units * DECI) as u64);
                        debug_assert!(taken, "checked affordable above");
                    }
                    let id = self.world.alloc_entity();
                    self.world.structures.insert(
                        id,
                        crate::world::Structure {
                            kind: *kind,
                            faction: *faction,
                            pos: *pos,
                            hp: self.tuning.structure_hp,
                            max_hp: self.tuning.structure_hp,
                            input: std::collections::BTreeMap::new(),
                            output: std::collections::BTreeMap::new(),
                            recipe: None,
                            batch: None,
                            pad: None,
                        },
                    );
                }
                Ok(None)
            }
            Command::ExchangeData { from, to, amount } => {
                if from != to && *to != crate::world::FERAL_FACTION {
                    let have = self.world.data.get(from).copied().unwrap_or(0);
                    let moved = (*amount).min(have);
                    if moved > 0 {
                        *self.world.data.entry(*from).or_insert(0) -= moved;
                        *self.world.data.entry(*to).or_insert(0) += moved;
                    }
                }
                Ok(None)
            }
            Command::PostRequest { faction, text } => {
                // Clamp hostile input ON A CHAR BOUNDARY — String::truncate
                // panics mid-codepoint, and a lockstep command must never
                // panic the sim. The board keeps the newest 64.
                let mut end = 200.min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let text = text[..end].to_string();
                let tick = self.world.tick;
                self.world.requests.push((tick, *faction, text));
                if self.world.requests.len() > 64 {
                    let overflow = self.world.requests.len() - 64;
                    self.world.requests.drain(..overflow);
                }
                Ok(None)
            }
            Command::Grant { from, to, what, revoke } => {
                if from != to {
                    if *revoke {
                        self.world.grants.remove(&(*from, *to, *what));
                    } else {
                        self.world.grants.insert((*from, *to, *what));
                    }
                }
                Ok(None)
            }
            Command::SetAlliance { a, b, allied } => {
                if a != b
                    && *a != crate::world::FERAL_FACTION
                    && *b != crate::world::FERAL_FACTION
                {
                    let pair = (*a.min(b), *a.max(b));
                    if *allied {
                        self.world.alliances.insert(pair);
                    } else if self.world.alliances.remove(&pair) {
                        // A broken alliance takes its grants with it —
                        // but ONLY a broken one: dissolving an alliance
                        // that never existed must not strip standing
                        // grants issued independently (review 2026-07-16).
                        self.world.grants.retain(|(f, t, _)| {
                            (*f, *t) != (pair.0, pair.1) && (*f, *t) != (pair.1, pair.0)
                        });
                    }
                }
                Ok(None)
            }
            Command::Vote { faction, proposal, approve } => {
                let tick = self.world.tick;
                if tick < self.world.vote_cooldown_until
                    || !self.world.live_factions().contains(faction)
                {
                    return Ok(None);
                }
                match &mut self.world.pending_vote {
                    None => {
                        if *approve {
                            let mut ayes = std::collections::BTreeSet::new();
                            ayes.insert(*faction);
                            self.world.pending_vote = Some(crate::world::PendingVote {
                                proposal: *proposal,
                                ayes,
                                opened: tick,
                            });
                        }
                    }
                    Some(vote) if vote.proposal == *proposal => {
                        if *approve {
                            vote.ayes.insert(*faction);
                        } else {
                            // One refusal kills a unanimity vote.
                            self.world.pending_vote = None;
                            self.world.vote_cooldown_until =
                                tick + self.world.vote_cooldown_ticks;
                        }
                    }
                    // A different proposal while one is open is ignored.
                    Some(_) => {}
                }
                if let Some(vote) = &self.world.pending_vote {
                    if vote.ayes.is_superset(&self.world.live_factions()) {
                        let Proposal::SetSpeed(permille) = vote.proposal;
                        self.world.sim_speed_permille = permille.min(10_000);
                        self.world.pending_vote = None;
                        self.world.vote_cooldown_until =
                            tick + self.world.vote_cooldown_ticks;
                    }
                }
                Ok(None)
            }
            Command::ClaimNest { nest, faction } => {
                if let Some(n) = self.world.nests.get_mut(nest) {
                    if n.state == crate::world::NestState::Defeated {
                        n.state = crate::world::NestState::Claimed(*faction);
                        n.hp = n.max_hp / 2;
                        // Re-claiming a nest can reactivate a printer bound to
                        // it (Q87) — its ghosts fold back into the fleet.
                        self.reconcile_dormancy(*faction);
                    }
                }
                Ok(None)
            }
            Command::RazeNest { nest, faction } => {
                let razeable = self
                    .world
                    .nests
                    .get(nest)
                    .is_some_and(|n| n.state == crate::world::NestState::Defeated);
                if razeable {
                    // Any printer built against this nest is now permanently
                    // unsupported — reconcile its owner so it goes Dormant and
                    // isn't left Working on a dangling handle (Q87).
                    let bound_owners: Vec<u8> = self
                        .world
                        .printers
                        .values()
                        .filter(|p| p.nest == Some(*nest))
                        .map(|p| p.faction)
                        .collect();
                    self.world.nests.remove(nest);
                    *self.world.data.entry(*faction).or_insert(0) +=
                        self.tuning.nest_data_bounty;
                    for owner in bound_owners {
                        self.reconcile_dormancy(owner);
                    }
                }
                Ok(None)
            }
            Command::PlacePrinter { pos, faction } => {
                use crate::resources::{Resource, DECI};
                let free = self.world.grid.get(*pos).is_some_and(|t| t.spawnable())
                    && !self.world.structure_at(*pos)
                    && !self.world.tile_occupied(*pos, BotId(u32::MAX))
                    && !self.world.printers.values().any(|p| p.pos == *pos);
                let owned =
                    self.world.printers.values().filter(|p| p.faction == *faction).count()
                        as u32;
                let claimed = self
                    .world
                    .nests
                    .values()
                    .filter(|n| n.state == crate::world::NestState::Claimed(*faction))
                    .count() as u32;
                let gated = owned < crate::feral::printers_allowed(claimed);
                let cost = self.tuning.printer_cost_steel * DECI as u64;
                if free && gated && self.world.stock_take(*faction, Resource::Steel, cost) {
                    // The new slot takes the lowest unused color index.
                    let mut color = 0u8;
                    while self
                        .world
                        .printers
                        .values()
                        .any(|p| p.faction == *faction && p.color.0 == color)
                    {
                        color += 1;
                    }
                    // Over-base printers (beyond the two free slots) bind to
                    // the nest they're built against (Q87): the lowest-id
                    // claimed nest not already bound to one of this faction's
                    // printers. Losing that nest sends this printer Dormant.
                    let bound_nest = if owned >= 2 {
                        let already: Vec<EntityId> = self
                            .world
                            .printers
                            .values()
                            .filter(|p| p.faction == *faction)
                            .filter_map(|p| p.nest)
                            .collect();
                        self.world
                            .nests
                            .iter()
                            .filter(|(_, n)| {
                                n.state == crate::world::NestState::Claimed(*faction)
                            })
                            .map(|(nid, _)| *nid)
                            .find(|nid| !already.contains(nid))
                    } else {
                        None
                    };
                    let id = self.world.alloc_entity();
                    self.world.printers.insert(
                        id,
                        crate::world::Printer {
                            pos: *pos,
                            faction: *faction,
                            color: crate::world::Color(color),
                            state: crate::world::PrinterState::Working,
                            // Never the remainder: map-born firsts keep
                            // that role; built printers start dialed.
                            rules: Some(crate::world::PrinterRules::default()),
                            job: None,
                            nest: bound_nest,
                        },
                    );
                    if !self.world.color_programs.contains_key(&(*faction, color)) {
                        self.apply(&Command::DeployProgram {
                            faction: *faction,
                            color: crate::world::Color(color),
                            source: String::new(),
                        })?;
                    }
                }
                Ok(None)
            }
            Command::SetRecipe { structure, recipe } => {
                if let Some(st) = self.world.structures.get_mut(structure) {
                    let valid = recipe.is_none_or(|idx| {
                        crate::resources::RECIPES
                            .get(idx as usize)
                            .is_some_and(|r| r.station == st.kind.name())
                    });
                    if valid && st.recipe != *recipe {
                        st.recipe = *recipe;
                        // Recipe change scraps the in-flight batch; its
                        // already-consumed inputs are LOST with it, and old
                        // leftovers stranded in `input` stay there (only
                        // `output` is withdrawable) — scrapping mid-batch
                        // deliberately wastes the feed.
                        st.batch = None;
                    }
                }
                Ok(None)
            }
            Command::QueueUpgrade { bot, order, replace } => {
                let resolved = if let Some((idx, _)) = self.stats.upgrade(order) {
                    // Compute orders never name a slot.
                    (replace.is_none()).then_some(crate::world::UpgradeOrder::Compute(idx))
                } else if let Some((idx, _)) = self.stats.module(order) {
                    Some(crate::world::UpgradeOrder::Module { idx, replace: *replace })
                } else {
                    None
                };
                if let (Some(order), Some(b)) = (resolved, self.world.bots.get_mut(bot)) {
                    if !b.data.dying {
                        b.data.upgrade_queue.push(order);
                    }
                }
                Ok(None)
            }
            Command::Research { faction, construct } => {
                let cost = crate::world::research_cost(*construct);
                let have = self.world.data.get(faction).copied().unwrap_or(0);
                let set = self.world.unlocks.entry(*faction).or_default();
                if !set.has(*construct) && have >= cost {
                    set.unlock(*construct);
                    self.world.data.insert(*faction, have - cost);
                }
                Ok(None)
            }
            Command::KillBot { bot } => {
                if let Some(b) = self.world.bots.get_mut(bot) {
                    b.data.dying = true;
                    let pos = b.data.pos;
                    self.world.unindex_bot(*bot, pos);
                }
                Ok(None)
            }
            Command::PlacePaint { pos, color } => {
                if self.world.grid.in_bounds(*pos) {
                    match color {
                        Some(c) => {
                            self.world.paint.insert(*pos, *c);
                        }
                        None => {
                            self.world.paint.remove(pos);
                        }
                    }
                }
                Ok(None)
            }
        }
    }

    /// Shared bot construction. Printed bots start in the Boot Sequence
    /// (an engine interrupt context); test spawns skip it. `cpu_centi` and
    /// `cargo_cap_deci` are stored-unit BASES (the stats floor, or a
    /// dev-spawn override); everything else on the statline comes from
    /// `stats.ron` — every print is the same machine (docs/02).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_bot(
        &mut self,
        pos: TilePos,
        faction: u8,
        color: Color,
        hp: i64,
        cpu_centi: u64,
        cargo_cap_deci: u32,
        mut vm: Vm,
        boot: bool,
    ) -> BotId {
        let id = self.world.alloc_bot_id();
        let entity = self.world.alloc_entity();
        // Latent quirk rolls (docs/09): at print, from the seeded
        // `quirk_roll` stream, gated by the match's expected-quirks-per-
        // bot dial (per-mille; slot n's chance is the dial minus n×1000,
        // clamped — 500 = 50% of one quirk, 2000 = both certain). Rolls
        // stay LATENT until total XP crosses the manifestation thresholds.
        let mut latent_quirks = Vec::new();
        if self.world.quirk_permille > 0 {
            for slot in 0..self.quirks.manifest_at.len() {
                let prob =
                    self.world.quirk_permille.saturating_sub(slot as u32 * 1000).min(1000);
                if prob == 0 {
                    continue;
                }
                if crate::world::next_rand(&mut self.world.rng.quirk_roll) % 1000
                    < prob as u64
                {
                    let r = crate::world::next_rand(&mut self.world.rng.quirk_roll);
                    latent_quirks.push(self.quirks.pick(r));
                }
            }
        }
        let booting = if boot {
            vm.set_engine_ctx(Some(pyrite::EngineCtx::Boot));
            Some(self.tuning.boot_ticks)
        } else {
            None
        };
        self.world.bot_entities.insert(entity, id);
        self.world.bots.insert(
            id,
            Bot {
                data: BotData {
                    id,
                    entity,
                    faction,
                    pos,
                    hp,
                    max_hp: hp,
                    hurt_fired: false,
                    cargo: std::collections::BTreeMap::new(),
                    cargo_cap: cargo_cap_deci,
                    cpu_centi,
                    move_rate_deci: self.stats.move_rate_deci,
                    sensors: self.stats.sensors,
                    module_slots: self.stats.module_slots,
                    log_cap: self.stats.log_buffer,
                    upgrades: Vec::new(),
                    modules: Vec::new(),
                    upgrade_queue: Vec::new(),
                    pad_sit: false,
                    survey_after_move: false,
                    withdrawn_aboard: 0,
                    color,
                    requested: None,
                    action: None,
                    booting,
                    recall: None,
                    bump_frozen: 0,
                    dying: false,
                    log_buf: Vec::new(),
                    xp: std::collections::BTreeMap::new(),
                    haul_accum: 0,
                    learning_carry: 0,
                    gain_carry: std::collections::BTreeMap::new(),
                    age_hp_levels: 0,
                    moved_tick: 0,
                    episodes: std::collections::BTreeMap::new(),
                    countdown_carry: None,
                    latent_quirks,
                    quirks: Vec::new(),
                    crash_seen: 0,
                    env: std::collections::BTreeMap::new(),
                    dune_idle: 0,
                    rng_program: crate::world::stream_seed(
                        self.world.seed ^ entity.0,
                        "program",
                    ),
                },
                vm: Some(vm),
            },
        );
        self.world.index_bot(id, pos);
        id
    }

    /// One fixed simulation tick — the nine-phase order of docs/07.
    /// Phase 1 (agreed Commands) happens outside, via [`Sim::apply`], in
    /// the relay's total order; everything from the VM grant to the
    /// snapshot hash lives here, in stable id order within each phase.
    pub fn step(&mut self) {
        self.world.tick += 1;

        // A sim-speed proposal that never reached unanimity expires
        // (M13, docs/08) — the attempt still starts the cooldown.
        if let Some(vote) = &self.world.pending_vote {
            if self.world.tick >= vote.opened + self.world.vote_window_ticks {
                self.world.pending_vote = None;
                self.world.vote_cooldown_until =
                    self.world.tick + self.world.vote_cooldown_ticks;
            }
        }

        // --- phase 2: grant + step VMs, stable id order ---
        let ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if bot.data.dying
                || bot.data.booting.is_some()
                || bot.data.recall.is_some()
                || bot.data.pad_sit
            {
                // Boot/recall/pad-sit are engine interrupt contexts: the
                // program is suspended and the engine drives the bot.
                continue;
            }
            if bot.data.bump_frozen > 0 {
                continue; // stunned by a bump — no thinking either
            }
            // The modifier pipeline (docs/02): base → hardware → XP →
            // quirks → state (Damaged, brownout — the Fabricator trickle
            // exempts one bot per faction) → clamp.
            let faction = bot.data.faction;
            let centi = crate::stats::cpu_centi(
                self.ctx(),
                &bot.data,
                self.world.brownout.contains(&faction),
                self.world.powered_bot.get(&faction) == Some(&id),
            );
            let bot = self.world.bots.get_mut(&id).expect("checked above");
            let mut vm = bot.vm.take().expect("vm present between phases");
            if vm.is_dead() {
                bot.vm = Some(vm);
                continue;
            }
            // Corruption's compute tax (M8, docs/05): every charged op
            // costs extra while the chassis stands on corrupted ground.
            // Set fresh before EVERY grant — the overlay is derived from
            // where the bot is now, never persisted. (Flat only; the
            // per-op-key generalization is flagged in TASKS.md.)
            let on_corruption = self.world.grid.is_corruption(bot.data.pos);
            vm.set_cost_overlay_centi(if on_corruption {
                self.tuning.corruption_op_tax as i64
            } else {
                0
            });
            // The grant itself enforces the VM rules: no banking while
            // Blocked (waiting burns the tick) and the bank_cap clamp.
            vm.grant_centi(centi, &self.costs);
            if vm.is_blocked() {
                bot.vm = Some(vm);
                continue;
            }
            let outcome = self.with_host(id, |host, costs| vm.run(host, costs));
            self.after_vm(id, vm, outcome);
        }

        // --- phases 3+4: collect issued actions, resolve them PER BOT in
        // stable id order (each bot advances whatever action it has — the
        // move → combat → mine/build sub-order of docs/07 is not split out;
        // flagged in TASKS.md for reconciliation), then the engine-driven
        // walks (boot countdowns, recall walks). Damage and signals these
        // produce are QUEUED for phase 6, not applied inline. ---
        for id in ids.iter().copied() {
            self.resolve_bot(id);
        }
        // Phase 4b (M11): the channel rendezvous settle — after every bot
        // resolved, so fresh blocks participate this very tick.
        self.settle_channels();
        for id in ids.iter().copied() {
            self.advance_engine(id);
        }

        // --- phase 5: perception recompute, then episode settlement
        // (split so seed passes never advance re-arm counters) ---
        self.run_perception();
        self.settle_episodes();

        // --- phase 6: damage, faults, deaths (countdowns and blasts join
        // in M10). Fault chip first: every unhandled crash this tick (from
        // stepping or action resolution) queues chassis damage, so its
        // hurt/death signals ride the same dispatch. ---
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            if bot.data.dying {
                continue;
            }
            let Some(vm) = bot.vm.as_ref() else { continue };
            let crashes = vm.crash_count();
            let delta = crashes.saturating_sub(bot.data.crash_seen);
            if delta > 0 {
                bot.data.crash_seen = crashes;
                // Per-bot chip (M6): Statically Typed halves it, `unsafe`
                // Block doubles it.
                let chip = crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }
                .fault_damage_for(&bot.data, self.tuning.fault_damage);
                self.queue_damage(id, delta as i64 * chip, None);
            }
        }
        // Wreck countdowns first (M10): expiries queue their blast damage
        // into this tick.s settle — blasts fire in the damage phase.
        self.tick_wrecks();
        self.settle_damage();
        self.dispatch_signals();

        // Deaths: dying bots become wrecks (the op boundary above may have
        // added to the pile — a Died outcome lands the same tick).
        for id in ids.iter().copied() {
            let Some(bot) = self.world.bots.get(&id) else { continue };
            if !bot.data.dying {
                continue;
            }
            // Already out of the occupancy index — dying bots leave it the
            // moment the flag is set.
            let bot = self.world.bots.remove(&id).expect("checked above");
            let mut data = bot.data;
            // (Escalation's ferals_killed counts in settle_damage, where
            // the kill's ATTRIBUTION is known — docs/04: player footprint,
            // so Feral fault-loops and blast chains never raise it.)
            // The entity handle stays live: the wreck is targetable
            // (attack, repair, salvage, analyze, hijack) — it is removed
            // for good only when the wreck itself is destroyed.
            // Carried cargo spills to the ground rather than entombing.
            let cargo = std::mem::take(&mut data.cargo);
            self.drop_cargo_to_ground(data.pos, cargo);
            data.dying = false; // the wreck may boot again (rescue/hijack)
            // The wreck race (M10): hull ~25% max HP; countdown = base +
            // per-XP bonus — or the RESUMED remainder after a failed
            // rescue (never resets).
            let hp =
                (data.max_hp * self.tuning.wreck_hp_pct as i64 / 100).max(1);
            let countdown = data.countdown_carry.take().unwrap_or_else(|| {
                self.tuning.wreck_countdown_base_ticks
                    + (data.xp_total() / 1000) as u32
                        * self.tuning.wreck_countdown_per_100xp_ticks
            });
            self.world.wrecks.insert(id, Wreck { data, hp, countdown });
        }

        // --- phase 7: XP settlement ---
        self.settle_xp();

        // --- phase 8: economy — upkeep settles FIRST (docs/07: energy,
        // upkeep; its brownout/rust flags feed this tick's self-repair and
        // the NEXT tick's cycle grants), then regen, refineries, printers.
        if !self.world.dev_free_power && self.world.tick.is_multiple_of(self.upkeep.interval_ticks)
        {
            self.settle_upkeep();
        }
        if self.world.tick.is_multiple_of(self.tuning.regen_interval_ticks) {
            // Regenerating nodes (Groves — docs/03: renewable but thin).
            let regen = self.tuning.node_regen_deci;
            let cap = self.tuning.node_regen_cap_deci;
            for node in self.world.nodes.values_mut() {
                if node.regen && node.amount < cap {
                    node.amount = (node.amount + regen).min(cap);
                }
            }
            for id in ids.iter() {
                let Some(bot) = self.world.bots.get_mut(id) else { continue };
                if bot.data.dying {
                    continue;
                }
                if bot.data.hp >= bot.data.max_hp {
                    // A fully mended chassis re-arms its self-destruct
                    // window (world.rs: None = never wrecked since the
                    // last FULL window) — without this, every rescue
                    // ratcheted the countdown toward insta-blast forever
                    // (review 2026-07-16).
                    bot.data.countdown_carry = None;
                    continue;
                }
                if self.world.rusting.contains(&bot.data.faction) {
                    continue; // unpaid Steel maintenance: self-repair halts (Q84)
                }
                // Seniority mends (docs/02): Age raises the trickle.
                let amount = crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }
                .regen_for(&bot.data, self.tuning.regen_amount);
                bot.data.hp = (bot.data.hp + amount).min(bot.data.max_hp);
                // The latch re-arms against the SAME line it fires on — the
                // bot's own hurt_line env — so a moved line can't make the
                // edge trigger re-fire mid-template or stick forever.
                let line = crate::world::env_read(&bot.data, "hurt_line", &self.tuning, &self.quirks);
                if bot.data.hurt_fired && bot.data.hp * 100 >= bot.data.max_hp * line {
                    bot.data.hurt_fired = false; // back over the Damaged line
                }
            }
        }
        self.run_refineries();
        self.run_printers();
        self.tick_nests();
        self.run_pads();
        self.settle_terrain();
        if self.world.tick.is_multiple_of(self.tuning.corruption_spread_ticks) {
            self.spread_corruption();
        }

        // Terrain hash refresh: set_tile only marks dirty (M8 made
        // terrain mutate routinely — per-mutation recompute was O(map)
        // each); one recompute per tick at most, before the snapshot.
        if self.world.terrain_dirty {
            self.world.terrain_hash = self.world.compute_terrain_hash();
            self.world.terrain_dirty = false;
        }

        // --- phase 9: snapshot hash for desync detection ---
        self.last_hash = self.state_hash();
    }

    /// Phase 8 (M5): the upkeep settlement — energy is a RATE, not a pile
    /// (docs/03): per-faction generation (Taps free, Generators burn fuel)
    /// vs. draw (bots + working refineries). Draw over generation =
    /// brownout (cycle budgets −50% next grant; the Fabricator trickle
    /// keeps ONE bot — lowest id — fully powered). Steel maintenance
    /// unpaid = rust: self-repair halts and hulls decay (Q84,
    /// `upkeep.ron`-configurable).
    pub(crate) fn settle_upkeep(&mut self) {
        use crate::resources::Resource;
        use crate::world::StructureKind;
        let mut factions: std::collections::BTreeSet<u8> = std::collections::BTreeSet::new();
        for bot in self.world.bots.values() {
            factions.insert(bot.data.faction);
        }
        for st in self.world.structures.values() {
            factions.insert(st.faction);
        }
        for faction in factions {
            // Draw first: every live bot (base + per-acquisition
            // increments — per-track-level joins with M6) + every
            // recipe-set refinery. A faction drawing NOTHING burns no
            // fuel and can't brown out — generators idle.
            let mut draw = 0u64;
            let mut bot_count = 0u64;
            for bot in
                self.world.bots.values().filter(|b| b.data.faction == faction && !b.data.dying)
            {
                bot_count += 1;
                let levels: u64 = bot
                    .data
                    .xp
                    .values()
                    .map(|&deci| self.xp.level(deci) as u64)
                    .sum();
                draw += self.upkeep.base_draw
                    + self.upkeep.draw_per_upgrade * bot.data.upgrades.len() as u64
                    + self.upkeep.draw_per_module * bot.data.modules.len() as u64
                    + self.upkeep.draw_per_track_level * levels;
            }
            draw += self.upkeep.draw_per_refinery
                * self
                    .world
                    .structures
                    .values()
                    .filter(|s| s.faction == faction && s.recipe.is_some())
                    .count() as u64;
            if draw == 0 {
                self.world.brownout.remove(&faction);
                self.world.powered_bot.remove(&faction);
                self.world.rusting.remove(&faction);
                continue;
            }
            // Generation. Generators burn the STRONG fuel first (Coal,
            // then Wood) — a deterministic preference; whether players
            // should choose is flagged in TASKS.md.
            let mut generation = 0u64;
            let st_ids: Vec<EntityId> = self
                .world
                .structures
                .iter()
                .filter(|(_, s)| s.faction == faction)
                .map(|(id, _)| *id)
                .collect();
            for id in st_ids {
                let fuel = self.upkeep.generator_fuel_deci;
                let st = self.world.structures.get_mut(&id).expect("collected above");
                match st.kind {
                    StructureKind::GeothermalTap => generation += self.upkeep.geothermal_output,
                    StructureKind::Generator => {
                        let mut burn = |kind: Resource| -> bool {
                            match st.input.get_mut(&kind) {
                                Some(have) if *have >= fuel => {
                                    *have -= fuel;
                                    if *have == 0 {
                                        st.input.remove(&kind);
                                    }
                                    true
                                }
                                _ => false,
                            }
                        };
                        if burn(Resource::Coal) {
                            generation += self.upkeep.generator_output_coal;
                        } else if burn(Resource::Wood) {
                            generation += self.upkeep.generator_output_wood;
                        }
                    }
                    _ => {}
                }
            }
            if draw > generation {
                self.world.brownout.insert(faction);
            } else {
                self.world.brownout.remove(&faction);
            }
            // The Fabricator backup trickle (Q84): one bot always powered
            // — deterministic pick, lowest id — while a working printer
            // exists to trickle from. Blackout can never deadlock the
            // colony: someone can always walk out for fuel.
            let has_printer = self
                .world
                .printers
                .values()
                .any(|p| p.faction == faction && p.state == PrinterState::Working);
            let pick = if has_printer {
                self.world
                    .bots
                    .iter()
                    .filter(|(_, b)| b.data.faction == faction && !b.data.dying)
                    .map(|(id, _)| *id)
                    .next()
            } else {
                None
            };
            match pick {
                Some(id) => {
                    self.world.powered_bot.insert(faction, id);
                }
                None => {
                    self.world.powered_bot.remove(&faction);
                }
            }
            // Steel maintenance (all-or-nothing from stock; partial
            // payment doesn't partially protect — flagged in TASKS.md).
            let need = bot_count * self.upkeep.steel_per_bot_deci;
            if need == 0 || self.world.stock_take(faction, Resource::Steel, need) {
                self.world.rusting.remove(&faction);
            } else {
                let sustained = !self.world.rusting.insert(faction);
                let bot_ids: Vec<BotId> = self
                    .world
                    .bots
                    .iter()
                    .filter(|(_, b)| b.data.faction == faction && !b.data.dying)
                    .map(|(id, _)| *id)
                    .collect();
                for id in bot_ids {
                    // Rust decay rides the ordinary damage phase (next
                    // tick's phase 6), like every other hurt.
                    self.queue_damage(id, self.upkeep.rust_decay_hp, None);
                }
                if sustained && self.upkeep.rust_scraps {
                    self.scrap_recall_lowest(faction);
                }
            }
        }
    }

    /// Phase 8: refineries (docs/03 — refinement is a logistics step).
    /// Each Smelter/Foundry with a recipe consumes its inputs from its
    /// physically-fed buffer, runs a batch timer, and emits into its
    /// output buffer for bots to withdraw(). Stable id order. Energy
    /// gating (M5): a browned-out faction's refineries stand idle —
    /// batches neither start nor advance until generation recovers.
    pub(crate) fn run_refineries(&mut self) {
        use crate::resources::{DECI, RECIPES};
        let ids: Vec<EntityId> = self.world.structures.keys().copied().collect();
        for id in ids {
            let st = self.world.structures.get_mut(&id).expect("structure exists");
            if self.world.brownout.contains(&st.faction) {
                continue; // needs energy (docs/03)
            }
            let Some(recipe_idx) = st.recipe else { continue };
            let Some(recipe) = RECIPES.get(recipe_idx as usize) else { continue };
            if let Some(ticks) = st.batch {
                if ticks > 1 {
                    st.batch = Some(ticks - 1);
                } else {
                    st.batch = None;
                    let (out_kind, out_units) = recipe.output;
                    *st.output.entry(out_kind).or_insert(0) += out_units * DECI;
                }
                continue;
            }
            // Start a batch when every input is buffered.
            let ready = recipe
                .inputs
                .iter()
                .all(|(k, units)| st.input.get(k).copied().unwrap_or(0) >= units * DECI);
            if ready {
                for (k, units) in recipe.inputs {
                    let have = st.input.get_mut(k).expect("checked ready");
                    *have -= units * DECI;
                    if *have == 0 {
                        st.input.remove(k);
                    }
                }
                st.batch = Some(self.tuning.recipe_batch_ticks);
            }
        }
    }

    /// Phase 5: perception — seeing/hearing recomputed from post-move
    /// positions, detection episodes, per-faction map knowledge, survey

    /// Phase 7: XP settlement — every award earned anywhere in the tick
    /// queued, then settled here in arrival order (phases queue in stable
    /// id order). The Learning multiplier applies at its start-of-tick
    /// level; it is IDENTITY until M6 lands the body tracks, so today this
    /// is a plain sum. Awards for bots that died in phase 6 are dropped
    /// with them.
    /// Phase 8 terrain settle (M8): Dune idle counters advance for every
    /// bot that stood still on sand this tick (Q35 — the counter feeds
    /// step_ticks' exit surcharge), and Scree worn past the crossing
    /// threshold collapses to Rubble (Q40). End-of-tick, so the Nth
    /// crosser finishes its own step on solid ground.
    pub(crate) fn settle_terrain(&mut self) {
        let tick = self.world.tick;
        for bot in self.world.bots.values_mut() {
            if bot.data.dying {
                continue;
            }
            if self.world.grid.get(bot.data.pos) == Some(crate::map::TileKind::Dunes)
                && bot.data.moved_tick != tick
            {
                bot.data.dune_idle = bot.data.dune_idle.saturating_add(1);
            }
        }
        let worn: Vec<crate::map::TilePos> = self
            .world
            .scree_wear
            .iter()
            .filter(|(_, n)| **n >= self.tuning.scree_crossings)
            .map(|(p, _)| *p)
            .collect();
        for p in worn {
            self.world.set_tile(p, crate::map::TileKind::Rubble);
        }
    }

    /// Corruption dynamics (M8-C, docs/05): each living Blight Core (id
    /// order) corrupts the nearest non-Corruption passable tile within
    /// its radius — nearest by (chebyshev, y, x), so the creep front is
    /// deterministic. Cleansed ground inside the radius is simply the
    /// nearest clean tile again: re-corruption falls out for free while
    /// the source lives. Bridges, Ramps, and Roads are spared: creep
    /// over a river would delete the crossing outright, a corrupted Ramp
    /// permanently traps every bot on its plateau (Cleanse yields
    /// Plains, never Ramp — review 2026-07-16), and Roads are paid civil
    /// works exactly like bridges.
    pub(crate) fn spread_corruption(&mut self) {
        let cores: Vec<(crate::map::TilePos, u32)> =
            self.world.blight_cores.values().map(|c| (c.pos, c.radius)).collect();
        for (pos, radius) in cores {
            let r = radius as i32;
            let mut best: Option<(u32, i32, i32)> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    let t = crate::map::TilePos::new(pos.x + dx, pos.y + dy);
                    let Some(kind) = self.world.grid.get(t) else { continue };
                    if matches!(
                        kind,
                        crate::map::TileKind::Corruption
                            | crate::map::TileKind::Bridge
                            | crate::map::TileKind::Ramp
                            | crate::map::TileKind::Road
                    ) || !kind.passable()
                    {
                        continue;
                    }
                    let cand = (pos.chebyshev(t), t.y, t.x);
                    if best.is_none_or(|b| cand < b) {
                        best = Some(cand);
                    }
                }
            }
            if let Some((_, y, x)) = best {
                self.world.set_tile(crate::map::TilePos::new(x, y), crate::map::TileKind::Corruption);
            }
        }
    }

    pub(crate) fn settle_xp(&mut self) {
        use std::collections::BTreeMap;
        let mut awards = std::mem::take(&mut self.world.pending_xp);
        // Age drips for every live bot (docs/02: its XP is literally time
        // — 1 deci per tick), through the same multiplier path as
        // everything else.
        for (id, bot) in &self.world.bots {
            if !bot.data.dying {
                awards.push((*id, XpTrack::Age, self.xp.age_deci_per_tick));
            }
        }
        let cap = self.xp.track_cap_deci();
        // The multiplier is the START-OF-SETTLE percent (Learning level +
        // quirk XP%), memoized per bot so this tick's own awards can't
        // compound into themselves.
        let mut pct_memo: BTreeMap<BotId, u64> = BTreeMap::new();
        // Learning feeds on every OTHER track's post-multiplier XP —
        // capped tracks included — and is never re-multiplied (docs/02).
        let mut feeds: BTreeMap<BotId, u64> = BTreeMap::new();
        for (id, track, deci) in awards {
            let Some(bot) = self.world.bots.get(&id) else { continue }; // died in phase 6
            if bot.data.dying {
                continue;
            }
            let pct = *pct_memo.entry(id).or_insert_with(|| self.ctx().xp_gain_pct(&bot.data));
            // Fractional carry per (bot, track), hundredths of a deci: a
            // sub-100% multiplier must REDUCE a 1-deci drip, not floor it
            // to zero forever (tech_debt froze the Age track outright).
            let bot = self.world.bots.get_mut(&id).expect("checked above");
            let carried =
                bot.data.gain_carry.remove(&track).unwrap_or(0) + deci * pct;
            let post = carried / 100;
            let rem = carried % 100;
            if rem > 0 {
                bot.data.gain_carry.insert(track, rem);
            }
            if post == 0 {
                continue;
            }
            if track != XpTrack::Learning {
                *feeds.entry(id).or_insert(0) += post;
            }
            let entry = bot.data.xp.entry(track).or_insert(0);
            *entry = (*entry + post).min(cap);
        }
        for (id, feed) in feeds {
            let Some(bot) = self.world.bots.get_mut(&id) else { continue };
            // Fractional carry (hundredths of a deci): 10% of a slow drip
            // accrues instead of flooring to zero every settlement.
            let carry = bot.data.learning_carry + feed * self.xp.learning_feed_pct;
            let gain = carry / 100;
            bot.data.learning_carry = carry % 100;
            if gain > 0 {
                let entry = bot.data.xp.entry(XpTrack::Learning).or_insert(0);
                *entry = (*entry + gain).min(cap);
            }
        }
        self.settle_milestones();
    }

    /// Phase 7b: total-XP milestones — module slots (+1 at each xp.ron
    /// threshold, capped) and quirk manifestation (docs/09: the nth latent
    /// roll comes alive when total XP crosses the nth threshold —
    /// deterministic check, no RNG).
    fn settle_milestones(&mut self) {
        let ids: Vec<BotId> = self.world.bots.keys().copied().collect();
        for id in ids {
            let (total, slots, latent, manifested) = {
                let d = &self.world.bots[&id].data;
                (d.xp_total(), d.module_slots, d.latent_quirks.len(), d.quirks.len())
            };
            let owed_slots = (1 + self
                .xp
                .slot_milestones
                .iter()
                .filter(|&&m| total >= m * 10)
                .count() as u32)
                .min(self.xp.slot_cap);
            if owed_slots > slots {
                self.world.bots.get_mut(&id).expect("collected").data.module_slots = owed_slots;
            }
            // Age body perk (xp.ron `age_hp_per_level`): each Age level
            // grows the hull — max HP and current HP both rise by the
            // delta (growing tougher never makes a bot instantly Damaged).
            let age_level = {
                let d = &self.world.bots[&id].data;
                self.xp.level(d.xp(XpTrack::Age))
            };
            {
                let d = &mut self.world.bots.get_mut(&id).expect("collected").data;
                if age_level > d.age_hp_levels {
                    let delta =
                        ((age_level - d.age_hp_levels) as i64) * self.xp.age_hp_per_level;
                    d.max_hp += delta;
                    d.hp += delta;
                    d.age_hp_levels = age_level;
                }
            }
            let owed_quirks = self
                .quirks
                .manifest_at
                .iter()
                .filter(|&&t| total >= t * 10)
                .count()
                .min(latent + manifested);
            for _ in manifested..owed_quirks {
                self.manifest_next_quirk(id);
            }
        }
    }

    /// Bring a bot's next latent quirk alive: move it to the manifested
    /// list and apply the one-time effects (max HP, log cap, live-VM stack
    /// depth). Pipeline effects need no action — they read the list.
    fn manifest_next_quirk(&mut self, id: BotId) {
        let Some(bot) = self.world.bots.get_mut(&id) else { return };
        if bot.data.latent_quirks.is_empty() {
            return;
        }
        let quirk = bot.data.latent_quirks.remove(0);
        bot.data.quirks.push(quirk);
        let Some(spec) = self.quirks.quirks.get(quirk as usize) else { return };
        for effect in &spec.effects {
            match effect {
                crate::quirks::QuirkEffect::MaxHpPct(p) => {
                    let bot = self.world.bots.get_mut(&id).expect("checked");
                    let delta = bot.data.max_hp * *p as i64 / 100;
                    bot.data.max_hp = (bot.data.max_hp + delta).max(1);
                    bot.data.hp = bot.data.hp.min(bot.data.max_hp);
                }
                crate::quirks::QuirkEffect::LogCapPct(p) => {
                    let bot = self.world.bots.get_mut(&id).expect("checked");
                    let delta = bot.data.log_cap as i64 * *p as i64 / 100;
                    bot.data.log_cap = (bot.data.log_cap as i64 + delta).max(1) as u32;
                }
                crate::quirks::QuirkEffect::StackDepth(_) => {
                    let depth = self.ctx().stack_depth_for(&self.world.bots[&id].data);
                    let bot = self.world.bots.get_mut(&id).expect("checked");
                    if let Some(vm) = bot.vm.as_mut() {
                        vm.set_stack_depth(depth);
                    }
                }
                _ => {} // pipeline- or read-side effects
            }
        }
    }

    /// The stat pipeline's read context (floor statline + XP magnitudes +
    /// quirk catalog). All shared borrows of disjoint Sim fields, so it
    /// composes with `world` reads.
    pub fn ctx(&self) -> crate::stats::StatCtx<'_> {
        crate::stats::StatCtx { stats: &self.stats, xp: &self.xp, quirks: &self.quirks, tuning: &self.tuning }
    }

    /// Run `f` with a freshly built per-bot [`BotHost`] and the cost
    /// table. Centralizes the disjoint-field borrow — the host takes
    /// `world` mutably while `stats`/`xp`/`quirks`/`tuning`/`costs` borrow
    /// `self` shared, which only type-checks inside one `&mut self` body
    /// (not via `self.ctx()`, whose whole-`self` borrow conflicts with the
    /// world). One place to touch when BotHost gains a field.
    pub(crate) fn with_host<R>(
        &mut self,
        id: BotId,
        f: impl FnOnce(&mut BotHost, &CostTable) -> R,
    ) -> R {
        let mut host = BotHost {
            world: &mut self.world,
            bot: id,
            tuning: &self.tuning,
            ctx: crate::stats::StatCtx {
                stats: &self.stats,
                xp: &self.xp,
                quirks: &self.quirks,
                tuning: &self.tuning,
            },
        };
        f(&mut host, &self.costs)
    }

    /// Per-bot VM config: the shared template with the hardware and quirk
    /// layers applied (Stack extensions / Memory Leak move the call-depth
    /// cap).
    pub(crate) fn vm_config_for(&self, data: &BotData) -> VmConfig {
        let mut config = self.vm_config.clone();
        config.stack_depth = self.ctx().stack_depth_for(data);
        config
    }

    /// Store the VM back. Every outcome keeps the VM (aborted bots are
    /// dying wrecks-to-be, not vaporized — no instant-destroy path).
    fn after_vm(&mut self, id: BotId, vm: Vm, outcome: Outcome) {
        let _ = outcome;
        if let Some(bot) = self.world.bots.get_mut(&id) {
            bot.vm = Some(vm);
        }
    }

    /// Phase 9: deterministic world hash for desync detection and golden
    /// replays. (VM internals are hashed shallowly for now — budget, line,
    /// blocked/dead — deep state hashing is a TODO.)
    pub fn state_hash(&self) -> u64 {
        let w = &self.world;
        let mut h = Fnv1a::new();
        h.write_u64(w.tick);
        h.write_i32(w.grid.width);
        h.write_i32(w.grid.height);
        // Cached: re-walking the map every tick made phase 9 O(map). Kept
        // fresh by World::set_tile on the rare terrain mutation.
        h.write_u64(w.terrain_hash);
        // Scree wear is real divergent state (M8, Q40): two peers with a
        // half-worn tile must agree before the collapse, not just after.
        h.write_u32(w.pending_recalls.len() as u32);
        for (bot, printer) in &w.pending_recalls {
            h.write_u32(bot.0);
            h.write_u64(printer.0);
        }
        h.write_u32(w.check_interval.len() as u32);
        for (faction, interval) in &w.check_interval {
            h.write_u8(*faction);
            h.write_u64(*interval);
        }
        h.write_u32(w.reprint_queue.len() as u32);
        for (faction, n) in &w.reprint_queue {
            h.write_u8(*faction);
            h.write_u32(*n);
        }
        h.write_u32(w.blight_cores.len() as u32);
        for (id, core) in &w.blight_cores {
            h.write_u64(id.0);
            h.write_i32(core.pos.x);
            h.write_i32(core.pos.y);
            h.write_u32(core.radius);
            h.write_i64(core.hp);
        }
        h.write_u32(w.scree_wear.len() as u32);
        for (pos, n) in &w.scree_wear {
            h.write_i32(pos.x);
            h.write_i32(pos.y);
            h.write_u32(*n);
        }
        for (id, node) in &w.nodes {
            h.write_u64(id.0);
            h.write_u8(node.kind.as_u8());
            h.write_i32(node.pos.x);
            h.write_i32(node.pos.y);
            h.write_u32(node.amount);
            h.write_u8(node.regen as u8);
        }
        for (id, depot) in &w.depots {
            h.write_u64(id.0);
            h.write_i32(depot.pos.x);
            h.write_i32(depot.pos.y);
        }
        // Template Caches (M15): id, position, and which block they hold.
        for (id, cache) in &w.caches {
            h.write_u64(id.0);
            h.write_i32(cache.pos.x);
            h.write_i32(cache.pos.y);
            h.write_u8(cache.block.as_u8());
        }
        for (id, st) in &w.structures {
            h.write_u64(id.0);
            h.write_u8(st.kind.as_u8());
            h.write_u8(st.faction);
            h.write_i32(st.pos.x);
            h.write_i32(st.pos.y);
            h.write_i64(st.hp);
            // Length-prefix the buffers so {input: X} and {output: X}
            // can't hash identically (collision quality, not a desync
            // vector — every peer runs the same computation).
            h.write_u32(st.input.len() as u32);
            for (k, deci) in &st.input {
                h.write_u8(k.as_u8());
                h.write_u32(*deci);
            }
            h.write_u32(st.output.len() as u32);
            for (k, deci) in &st.output {
                h.write_u8(k.as_u8());
                h.write_u32(*deci);
            }
            h.write_u8(st.recipe.map(|r| r + 1).unwrap_or(0));
            h.write_u32(st.batch.unwrap_or(0));
            match &st.pad {
                Some(job) => {
                    h.write_u8(1);
                    h.write_u32(job.bot.0);
                    hash_order(&mut h, &job.order);
                    h.write_u32(job.ticks_left);
                }
                None => h.write_u8(0),
            }
        }
        for (faction, set) in &w.unlocks {
            h.write_u8(*faction);
            for c in pyrite::Construct::ALL {
                h.write_u8(set.has(c) as u8);
            }
        }
        // Studied function blocks (M15): per faction, which blocks are learned.
        for (faction, blocks) in &w.studied {
            h.write_u8(*faction);
            for b in crate::progression::FunctionBlock::ALL {
                h.write_u8(blocks.contains(&b) as u8);
            }
        }
        for ((faction, kind), deci) in &w.stock {
            h.write_u8(*faction);
            h.write_u8(kind.as_u8());
            h.write_u64(*deci);
        }
        for (faction, data) in &w.data {
            h.write_u8(*faction);
            h.write_u64(*data);
        }
        for (faction, delivered) in &w.delivered {
            h.write_u8(*faction);
            h.write_u64(*delivered);
        }
        // Permanent map knowledge is real state (docs/05 Q70); the live
        // perception union is derived every tick and deliberately unhashed.
        for (faction, known) in &w.known_nodes {
            h.write_u8(*faction);
            h.write_u32(known.len() as u32);
            for (id, node) in known {
                h.write_u64(id.0);
                h.write_u8(node.kind.as_u8());
                h.write_u8(node.exhausted as u8);
            }
        }
        for (faction, paid) in &w.milestones_paid {
            h.write_u8(*faction);
            h.write_u64(*paid);
        }
        for faction in &w.first_kill_done {
            h.write_u8(*faction);
        }
        for faction in &w.brownout {
            h.write_u8(*faction);
        }
        for faction in &w.rusting {
            h.write_u8(*faction);
        }
        // The trickle pick controls per-bot cycle grants for a whole
        // settlement interval — a divergence here must trip the desync
        // alarm immediately, not once the extra compute moves a position.
        for (faction, bot) in &w.powered_bot {
            h.write_u8(*faction);
            h.write_u32(bot.0);
        }
        h.write_u64(w.rng.combat);
        h.write_u64(w.rng.wander);
        h.write_u64(w.rng.explore);
        h.write_u64(w.rng.sidestep);
        h.write_u64(w.rng.quirk_roll);
        h.write_u64(w.rng.feral_mutation);
        for (id, printer) in &w.printers {
            h.write_u64(id.0);
            h.write_i32(printer.pos.x);
            h.write_i32(printer.pos.y);
            h.write_u8(printer.faction);
            h.write_u8(printer.color.0);
            // Distinct discriminant per state so Dormant ≠ Ruined in the hash
            // (a hidden desync otherwise); bound nest included (Q87).
            h.write_u8(match printer.state {
                PrinterState::Working => 0,
                PrinterState::Ruined => 1,
                PrinterState::Dormant => 2,
            });
            h.write_u64(printer.nest.map_or(u64::MAX, |n| n.0));
            match &printer.rules {
                None => h.write_u8(0), // the remainder bucket
                Some(r) => {
                    h.write_u8(1);
                    match r.target {
                        crate::world::PrintTarget::Count(n) => {
                            h.write_u8(0);
                            h.write_u32(n);
                        }
                        crate::world::PrintTarget::CapPct(p) => {
                            h.write_u8(1);
                            h.write_u32(p);
                        }
                    }
                    h.write_u8(select_key_tag(r.key));
                    h.write_u8(r.best_first as u8);
                    h.write_u32(r.priority);
                }
            }
            h.write_u32(printer.job.unwrap_or(0));
        }
        for (pos, overlay) in &w.overlays {
            h.write_i32(pos.x);
            h.write_i32(pos.y);
            h.write_u8(overlay.as_u8());
        }
        for (pos, color) in &w.paint {
            h.write_i32(pos.x);
            h.write_i32(pos.y);
            h.write_u8(*color);
        }
        for (id, bp) in &w.blueprints {
            h.write_u64(id.0);
            // The kind was implied while Bridge was the only one; with
            // six kinds a divergence must desync NOW, not at completion
            // when the wrong tile lands (review 2026-07-16).
            h.write_u8(bp.kind.as_u8());
            h.write_i32(bp.pos.x);
            h.write_i32(bp.pos.y);
            h.write_u32(bp.progress);
            h.write_u32(bp.needed);
        }
        // Program versions ARE source-byte hashes (CLAUDE.md rule 7), so
        // hashing the stored u64s covers the sources without re-walking
        // every deployed program's bytes each tick.
        for ((faction, color), cp) in &w.color_programs {
            h.write_u8(*faction);
            h.write_u8(*color);
            h.write_u64(cp.hash);
        }
        for hash in w.program_library.keys() {
            h.write_u64(*hash);
        }
        for (id, bot) in &w.bots {
            h.write_u32(id.0);
            hash_bot_data(&mut h, &bot.data);
            if let Some(vm) = &bot.vm {
                h.write_i64(vm.budget());
                h.write_u64(vm.fault_count());
                h.write_u64(vm.crash_count());
                h.write_u32(vm.current_line());
                h.write_u8(vm.is_blocked() as u8);
                h.write_u8(vm.is_dead() as u8);
            }
        }
        for (id, wreck) in &w.wrecks {
            h.write_u32(id.0);
            h.write_i64(wreck.hp);
            h.write_u32(wreck.countdown);
            // The FULL BotData — it reboots into live play whole via
            // rescue/hijack, so every field is lockstep state.
            hash_bot_data(&mut h, &wreck.data);
        }
        // The intel ledgers (M10): decryption never goes down; comm keys
        // never expire — both are lockstep state.
        for ((viewer, owner, color), pct) in &w.decryption {
            h.write_u8(*viewer);
            h.write_u8(*owner);
            h.write_u8(*color);
            h.write_u32(*pct);
        }
        h.write_u32(w.comm_keys.len() as u32);
        for (viewer, keys) in &w.comm_keys {
            h.write_u8(*viewer);
            // Length-prefix the inner set (review 2026-07-17): without it
            // {1:{2},5:{6}} and {1:{2,5,6}} hash identically and a
            // comm-key divergence slips past the desync detector.
            h.write_u32(keys.len() as u32);
            for k in keys {
                h.write_u8(*k);
            }
        }
        // Ferals (M12): nests are lockstep state; so is the escalation
        // dial and its kill counter.
        h.write_u8(w.escalation);
        h.write_u32(w.ferals_killed);
        for (id, n) in &w.nests {
            h.write_u64(id.0);
            h.write_i32(n.pos.x);
            h.write_i32(n.pos.y);
            h.write_u8(n.arcanum);
            h.write_i64(n.hp);
            let (tag, claimant) = match n.state {
                crate::world::NestState::Active => (0u8, 0u8),
                crate::world::NestState::Defeated => (1, 0),
                crate::world::NestState::Claimed(f) => (2, f),
            };
            h.write_u8(tag);
            h.write_u8(claimant);
            h.write_u64(n.stock_deci);
            h.write_u64(n.job.unwrap_or(u64::MAX));
            h.write_u32(n.prints);
        }
        // Diplomacy & votes (M13): all lockstep state.
        for (a, b) in &w.alliances {
            h.write_u8(*a);
            h.write_u8(*b);
        }
        for (f, t, what) in &w.grants {
            h.write_u8(*f);
            h.write_u8(*t);
            h.write_u8(matches!(what, crate::world::GrantKind::Channels) as u8);
        }
        for (tick, faction, text) in &w.requests {
            h.write_u64(*tick);
            h.write_u8(*faction);
            h.write_str(text);
        }
        h.write_u32(w.sim_speed_permille);
        h.write_u64(w.vote_cooldown_until);
        // Match-settings-derived world state (M13): peers whose configs
        // disagree must diverge at tick 0, not when the first gate fires.
        h.write_u8(w.harm_enabled as u8);
        h.write_u64(w.vote_cooldown_ticks);
        h.write_u64(w.vote_window_ticks);
        if let Some(vote) = &w.pending_vote {
            let crate::sim::Proposal::SetSpeed(permille) = vote.proposal;
            h.write_u32(permille);
            h.write_u64(vote.opened);
            for f in &vote.ayes {
                h.write_u8(*f);
            }
        }
        for bb in &w.black_boxes {
            h.write_u64(bb.entity.0);
            h.write_i32(bb.pos.x);
            h.write_i32(bb.pos.y);
            h.write_u64(bb.tick);
            h.write_u32(bb.bot.0);
            h.write_str(&bb.cause);
            for (level, log) in &bb.logs {
                h.write_u8(*level);
                h.write_str(log);
            }
            for (key, value) in &bb.env {
                h.write_str(key);
                h.write_i64(*value);
            }
        }
        h.write_u64(w.archive.len() as u64);
        // BTreeMap iteration is faction-ordered (deterministic).
        for (faction, entries) in &w.archive {
            h.write_u8(*faction);
            h.write_u64(entries.len() as u64);
            for entry in entries {
                h.write_u64(entry.tick);
                h.write_u32(entry.bot.0);
                h.write_u8(entry.level);
                h.write_u32(entry.line);
                h.write_str(&entry.text);
            }
        }
        h.finish()
    }
}
