//! World state: entities, bots, wrecks, black boxes, the colony stockpile.
//! Everything lives in BTree containers with stable IDs (determinism).

use crate::map::{Grid, MapSpec, OverlayKind, TileKind, TilePos};
use std::collections::BTreeSet;
use pyrite::ast::Program;
use pyrite::Vm;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct BotId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct EntityId(pub u64);

/// A program color slot (docs/01 "Program Colors"). One color = one printer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Color(pub u8);

impl Color {
    pub const GREEN: Color = Color(0);
    pub const RED: Color = Color(1);
    pub const BLUE: Color = Color(2);

    /// The named palette (docs/01: nine names, then procedurally
    /// patterned tints — the count is uncapped).
    pub const NAMES: [&'static str; 9] =
        ["Green", "Red", "Blue", "Yellow", "Cyan", "Magenta", "Orange", "Purple", "White"];

    pub fn name(self) -> String {
        match Self::NAMES.get(self.0 as usize) {
            Some(n) => (*n).to_string(),
            None => format!("Color {}", self.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Working,
    /// Present but broken (the starting Red printer); repairable via
    /// `RepairPrinter`.
    Ruined,
    /// The nest this over-base printer was built against was lost (Q65/Q87):
    /// fleet-cap contribution withdrawn, color frozen, its bots become ghost
    /// machines. Re-claiming the bound nest reactivates it — NOT repairable
    /// (repair is for Ruined). The remainder printer never enters this state
    /// (Q88).
    Dormant,
}

/// One dialed printer's share of the fleet (M9, docs/01 "Target
/// shares"): a target, a selection key with direction, and a rank in the
/// player's priority order. The FIRST printer of a faction (lowest
/// entity id) is the remainder bucket — no rules, not editable, holds
/// every bot no other printer claims, implicitly last in priority.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PrinterRules {
    pub target: PrintTarget,
    pub key: SelectKey,
    /// Best-first sorts by the key's IMPROVEMENT direction (docs/01 Q64:
    /// move rate improves downward); worst-first inverts.
    pub best_first: bool,
    /// Rank in the player's priority order (lower claims first; ties
    /// break by printer entity id; the remainder is implicitly last).
    pub priority: u32,
}

impl Default for PrinterRules {
    fn default() -> Self {
        // A fresh dialed printer wants nothing until the player says so.
        PrinterRules {
            target: PrintTarget::Count(0),
            key: SelectKey::TotalXp,
            best_first: true,
            priority: u32::MAX,
        }
    }
}

/// An absolute bot count, or a percentage OF THE FLEET CAP (floored —
/// Q64: of the cap, never the live fleet, so targets don't reshuffle on
/// every death).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PrintTarget {
    Count(u32),
    CapPct(u32),
}

/// A selection key: any stat-sheet row or ledger number is a legal sort
/// (docs/01 Q64 — no composite keys in v1; key + entity-id tiebreak is
/// the whole sort).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum SelectKey {
    TotalXp,
    Xp(XpTrack),
    Hp,
    MaxHp,
    CpuCenti,
    Sensors,
    CargoCap,
    MoveRate,
    ModuleSlots,
}

impl SelectKey {
    /// The key's raw per-bot value (base + hardware — deploy-time stats;
    /// quirks never enter, Q52's rule extended to keys).
    pub fn value(self, data: &BotData) -> i64 {
        match self {
            SelectKey::TotalXp => data.xp_total() as i64,
            SelectKey::Xp(track) => data.xp(track) as i64,
            SelectKey::Hp => data.hp,
            SelectKey::MaxHp => data.max_hp,
            SelectKey::CpuCenti => data.cpu_centi as i64,
            SelectKey::Sensors => data.sensors as i64,
            SelectKey::CargoCap => data.cargo_cap as i64,
            SelectKey::MoveRate => data.move_rate_deci as i64,
            SelectKey::ModuleSlots => data.module_slots as i64,
        }
    }

    /// Does a HIGHER raw value mean a BETTER machine? (Move rate is
    /// ticks-per-tile: lower is better.)
    pub fn higher_is_better(self) -> bool {
        !matches!(self, SelectKey::MoveRate)
    }
}

/// A Fabricator: prints/reprints bots for exactly one color and carries
/// its target-share dials (M9, docs/01). Printers are also "the cloud" —
/// they always accept log traffic.
#[derive(Debug, Clone, PartialEq)]
pub struct Printer {
    pub pos: TilePos,
    pub faction: u8,
    pub color: Color,
    pub state: PrinterState,
    /// The target-share dials (M9). `None` on the faction's FIRST
    /// printer — the remainder bucket takes what nobody claims.
    pub rules: Option<PrinterRules>,
    /// In-progress print job: ticks remaining.
    pub job: Option<u32>,
    /// The nest this over-base printer was built against (Q87). `None` for
    /// the two free base slots (never nest-bound, never dormant). When the
    /// bound nest stops being claimed by this faction, the printer goes
    /// `Dormant`; re-claiming it reactivates the printer.
    pub nest: Option<EntityId>,
}

/// The deployed program for one (faction, color) slot.
#[derive(Debug, Clone)]
pub struct ColorProgram {
    pub source: String,
    pub program: Rc<Program>,
    /// Version identity = FNV-1a of the source bytes (docs/01: programs are
    /// byte-exact; versions are identified by hashing source bytes).
    pub hash: u64,
    /// The artifact's HARDWARE BAR (M9, Q52): program memory in lines and
    /// distinct variable names — its printer claims only bots whose bought
    /// hardware meets both. Derived from the source at deploy.
    pub req_lines: u32,
    pub req_names: u32,
}

/// FNV-1a over source bytes — the program-version identity (docs/01).
pub fn program_hash(source: &str) -> u64 {
    let mut h = crate::hash::Fnv1a::new();
    h.write_bytes(source.as_bytes());
    h.finish()
}

/// The engine-fixed recall interrupt (docs/01): walk home, then re-color
/// or scrap. Unwritable by player code; double-handle applies throughout.
#[derive(Debug, Clone, PartialEq)]
pub struct Recall {
    /// Remaining tiles to enter on the way to the home printer.
    pub path: Vec<TilePos>,
    /// Ticks left to enter `path[0]`.
    pub ticks_left: u32,
    /// The printer being walked to (for bump re-planning).
    pub home: EntityId,
    pub purpose: RecallPurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallPurpose {
    /// Transported to the destination printer and re-colored (XP kept).
    Recolor { dest: EntityId },
    /// Decommissioned for a partial refund (over-capacity).
    Scrap,
}

/// A typed resource node (docs/03): sits on its ground tile, yields its
/// kind to `mine()`. Amounts are deci-units.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceNode {
    pub kind: crate::resources::Resource,
    pub pos: TilePos,
    pub amount: u32,
    /// Per-node-type regeneration flag (Wood groves are the flagship
    /// exception; the rate is tuning `node_regen_deci`).
    pub regen: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Depot {
    pub pos: TilePos,
    /// Owning colony (Q89): a Depot sees and hears for its faction, not the
    /// old faction-0 hardcode. Haul deposits/withdrawals stay open to anyone
    /// standing on it — ownership governs perception, not access.
    pub faction: u8,
}

/// A Template Cache (docs/06): a ruined installation holding an old-world
/// function block. `study()` at an adjacent tile unlocks its block for the
/// studying bot's colony — non-consumable (a school, not a pickup), so any
/// colony can learn from the same site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Cache {
    pub pos: TilePos,
    pub block: crate::progression::FunctionBlock,
}

/// Structure kinds with generic state (docs/03). Printers stay their own
/// specialized type until M9 reworks them — flagged for discussion in
/// TASKS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum StructureKind {
    Smelter,
    Foundry,
    /// The Research Archive: the Data bank (research itself is
    /// structure-free — a Command; the Exchange lands later).
    Archive,
    /// Burns Wood (weak) or Coal (strong) from its physical intake into
    /// the faction's energy rate (docs/03; fed by `deposit()`).
    Generator,
    /// Free steady energy, placeable only on Vent tiles (docs/03).
    GeothermalTap,
    /// The compute shop (M5): bots stand adjacent with a queued order;
    /// the pad pulls them, they sit inert, they step off upgraded.
    /// Coolant (Water) is a physical feed into its input buffer.
    UpgradeStation,
}

impl StructureKind {
    pub const ALL: [StructureKind; 6] = [
        StructureKind::Smelter,
        StructureKind::Foundry,
        StructureKind::Archive,
        StructureKind::Generator,
        StructureKind::GeothermalTap,
        StructureKind::UpgradeStation,
    ];

    pub fn name(self) -> &'static str {
        match self {
            StructureKind::Smelter => "smelter",
            StructureKind::Foundry => "foundry",
            StructureKind::Archive => "archive",
            StructureKind::Generator => "generator",
            StructureKind::GeothermalTap => "geothermal_tap",
            StructureKind::UpgradeStation => "upgrade_station",
        }
    }

    /// Kinds a `deposit()` may feed into this structure's input buffer —
    /// the PHYSICAL flows (docs/03 Q84): refinery inputs are recipe-driven
    /// (handled separately), Generator fuel and Station coolant are fixed.
    pub fn feed_kinds(self) -> &'static [crate::resources::Resource] {
        use crate::resources::Resource;
        match self {
            StructureKind::Generator => &[Resource::Wood, Resource::Coal],
            StructureKind::UpgradeStation => &[Resource::Water],
            _ => &[],
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            StructureKind::Smelter => 0,
            StructureKind::Foundry => 1,
            StructureKind::Archive => 2,
            StructureKind::Generator => 3,
            StructureKind::GeothermalTap => 4,
            StructureKind::UpgradeStation => 5,
        }
    }
}

/// What one faction can grant another (M13, docs/08's allied-colony
/// scaffolding): pooled eyes or an open channel namespace. Grants are
/// unilateral and revocable; alliance itself is a separate declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum GrantKind {
    Vision,
    Channels,
}

/// An open sim-speed proposal (M13, docs/08: unanimous consent with a
/// per-attempt cooldown).
#[derive(Debug, Clone, PartialEq)]
pub struct PendingVote {
    pub proposal: crate::sim::Proposal,
    pub ayes: BTreeSet<u8>,
    pub opened: u64,
}

/// The PvE faction id (M12, docs/04): Ferals are just another faction to
/// every system — perception, decryption, comm keys, combat — reserved at
/// the top of the u8 range so player factions count up from 0.
pub const FERAL_FACTION: u8 = 255;

/// A Feral nest's life stage (M12, docs/04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestState {
    /// Printing Ferals, hostile.
    Active,
    /// Beaten to 0 hp: dormant site, awaiting a raze (Data now) or a
    /// claim (printer-slot credit forever) — docs/04's pick-one.
    Defeated,
    /// Converted by a player faction: counts toward that faction's
    /// quadratic printer gate (docs/01).
    Claimed(u8),
}

/// A Feral nest (M12, docs/04): prints archetype programs from harvested
/// stock, exactly like a player Fabricator in spirit — but its economy is
/// its own (Harvesters `deposit()` into it; starving it starves the
/// prints).
#[derive(Debug, Clone, PartialEq)]
pub struct Nest {
    pub entity: EntityId,
    pub pos: TilePos,
    /// Allegiance 0–21 (Major Arcana): the difficulty-and-personality
    /// axis. v1 flags: 1 (Magician) and 18 (Moon) mutate per print.
    pub arcanum: u8,
    pub hp: i64,
    pub max_hp: i64,
    pub state: NestState,
    /// The nest's private stock, deci-units (fed by its Harvesters +
    /// a slow trickle; prints draw from it).
    pub stock_deci: u64,
    /// Ticks left on the print in progress.
    pub job: Option<u64>,
    /// Lifetime prints — drives the deterministic round-robin archetype
    /// mix (no RNG spent on the pick; only mutation draws a stream).
    pub prints: u32,
}

impl Nest {
    /// The faction that controls this nest and sees for it: its claimant,
    /// or the Ferals (Active/Defeated sites are theirs). The single source
    /// of the "who owns this nest" rule that perception, `find_kind`,
    /// `move_to`, and the harm gate all consult.
    pub fn owner(&self) -> u8 {
        match self.state {
            NestState::Claimed(f) => f,
            _ => FERAL_FACTION,
        }
    }
}

/// A generic structure (docs/03): solid, damageable, with physical
/// input/output buffers where its kind refines (feeds are physical;
/// payments are abstract — Q84). Energy gating lands with M5.
#[derive(Debug, Clone, PartialEq)]
pub struct Structure {
    pub kind: StructureKind,
    pub faction: u8,
    pub pos: TilePos,
    pub hp: i64,
    pub max_hp: i64,
    /// Physical input buffer bots feed with deposit() (deci-units).
    pub input: BTreeMap<crate::resources::Resource, u32>,
    /// Physical output buffer bots empty with withdraw() (deci-units).
    pub output: BTreeMap<crate::resources::Resource, u32>,
    /// The recipe this refinery is set to (SetRecipe command), if any.
    pub recipe: Option<u8>,
    /// Ticks remaining on the in-progress batch.
    pub batch: Option<u32>,
    /// Upgrade Station only: the bot currently sitting the pad, its order,
    /// and the sit ticks left (docs/03: one pad, one bot — the queue is
    /// physical, everyone in it is exposed).
    pub pad: Option<PadJob>,
}

/// A mounted Station order (payment already charged — at mount, Q84).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadJob {
    pub bot: BotId,
    pub order: UpgradeOrder,
    pub ticks_left: u32,
}

/// A player-queued Station order, resolved against the stats catalog at
/// `QueueUpgrade` time (docs/03: designation is the player's; the PROGRAM
/// must bring the bot to a pad).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpgradeOrder {
    /// Index into `stats.upgrades` (compute — coolant applies).
    Compute(u8),
    /// Index into `stats.modules`; `replace` names the slot whose module
    /// the swap DESTROYS (no refund, Q72) — None fills an open slot.
    Module { idx: u8, replace: Option<u8> },
}

impl Structure {
    /// Kinds a `deposit()` may move into this structure's input buffer:
    /// the set recipe's inputs (refineries) plus the kind's fixed feeds
    /// (Generator fuel, Station coolant — docs/03: feeds are physical).
    pub fn accepted_feed(&self) -> Vec<crate::resources::Resource> {
        let mut kinds: Vec<crate::resources::Resource> = self
            .recipe
            .map(|idx| {
                crate::resources::RECIPES[idx as usize]
                    .inputs
                    .iter()
                    .map(|(k, _)| *k)
                    .collect()
            })
            .unwrap_or_default();
        kinds.extend_from_slice(self.kind.feed_kinds());
        kinds
    }
}

/// An action a builtin asked for this tick; started in the resolve phase.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionRequest {
    MoveTo(EntityId),
    Mine,
    Deposit { fault_on_fail: bool },
    Attack(EntityId),
    /// Idle deliberately for N ticks — the Tier-0 traffic tool.
    Wait(u32),
    /// Work on a designated blueprint.
    Build(EntityId),
    /// The scouting stance (M7): root, seeing expands to the hearing
    /// radius, resolves at full reach.
    Search,
    /// A seeded random walk leg (rng.wander) — the dumb explorer.
    Wander,
    /// Pick a random fogged tile within ~15, walk there, survey it.
    Explore,
    /// Study an adjacent Template Cache (docs/06): root ~10s, then unlock its
    /// function block colony-wide.
    Study,
    // --- the wreck race + guard duty (M10) ---
    /// Field repair a wreck (the rescue) or mend a structure/bot.
    Repair(EntityId),
    /// The race verbs, timed salvage < analyze < hijack (Q84).
    Salvage(EntityId),
    Analyze(EntityId),
    Hijack(EntityId),
    /// Bank a black box to the cloud.
    Recover(EntityId),
    /// Entity-anchored guard duty; `escort` follows farther afield.
    Guard { target: EntityId, escort: bool },
    /// A blocking channel op (M11): rendezvous send/receive/broadcast.
    Channel { op: ChannelOp, ch: String, namespace: u8, timeout: Option<u32> },
}

/// What a blocked channel participant is doing (M11, docs/01).
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelOp {
    Send(pyrite::Value),
    Receive,
    Broadcast(pyrite::Value),
}

/// Which race verb a timed wreck action resolves into (M10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RaceKind {
    Salvage,
    Analyze,
    Hijack,
}

/// An in-flight world action.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// `path[0]` is the next tile to enter; `ticks_left` is the remaining
    /// cost of entering it (Rubble takes 2, Plains 1). `goals` is kept so
    /// the route can be re-planned after a bump.
    Move { path: Vec<TilePos>, ticks_left: u32, goals: BTreeSet<TilePos> },
    Mine { node: EntityId, ticks_left: u32 },
    /// `depot` is the acceptor picked at request time (a depot or a
    /// refinery); `fault_on_fail` carries the deposit/try_deposit choice
    /// so an acceptor destroyed mid-action resolves per the caller's verb.
    Deposit { depot: EntityId, ticks_left: u32, fault_on_fail: bool },
    Attack { target: EntityId, ticks_left: u32 },
    Wait { ticks_left: u32 },
    /// Contributes 1 progress per tick while adjacent to the blueprint.
    Build { blueprint: EntityId },
    /// The scouting stance (M7): rooted; `current` is the survey ring
    /// reached so far, expanding one tile per interval out to `reach`
    /// (the hearing radius at stance start). Resolves at full reach.
    Search { reach: u32, current: u32, ticks_left: u32 },
    /// Studying a Template Cache (docs/06): rooted for `ticks_left`, then the
    /// `cache`'s function block unlocks colony-wide.
    Study { cache: EntityId, ticks_left: u32 },
    /// Field repair / mending (M10): deci-progress accumulated so far.
    Repair { target: EntityId, done_deci: u32 },
    /// A timed race verb against a wreck (M10).
    Race { wreck: BotId, kind: RaceKind, ticks_left: u32 },
    /// Picking up a black box (M10).
    Recover { target: EntityId, ticks_left: u32 },
    /// Guard duty (M10): stay near `target`, swing at adjacent enemies;
    /// `step_wait` paces the follow-walk, `cooldown` paces the swings.
    Guard { target: EntityId, escort: bool, step_wait: u32, cooldown: u32 },
    /// A channel rendezvous in flight (M11): no queues, no mailboxes —
    /// the message exists only at the delivery instant. `delivered` parks
    /// a completed handoff for the settle to resolve; `waited` drives the
    /// longest-blocked-receiver selection and the timeout.
    Channel {
        op: ChannelOp,
        ch: String,
        namespace: u8,
        waited: u32,
        timeout: Option<u32>,
        delivered: Option<pyrite::Value>,
    },
}

// (The old LOG_BUFFER_CAP const died with M5 — the cap is the per-bot
// `log_cap` stat: stats.ron floor + Memory banks.)

#[derive(Debug, Clone, PartialEq)]
pub struct BotData {
    pub id: BotId,
    /// This bot's world entity handle (targetable by other bots).
    pub entity: EntityId,
    pub faction: u8,
    pub pos: TilePos,
    pub hp: i64,
    pub max_hp: i64,
    /// Edge-trigger latch for the hurt signal; re-arms when repaired above
    /// the threshold (no repair yet, so it fires at most once).
    pub hurt_fired: bool,
    /// Typed cargo manifest in deci-units (docs/03: hauling routes by
    /// kind; refinery buffers accept only their inputs).
    pub cargo: BTreeMap<crate::resources::Resource, u32>,
    /// Capacity in deci-units (total across kinds).
    pub cargo_cap: u32,
    /// BASE centicycles granted per tick (the stats floor, or a dev-spawn
    /// override) — the modifier pipeline produces the effective grant.
    pub cpu_centi: u64,
    /// BASE move rate, deci-ticks per tile (stats floor; terrain
    /// multiplies, the pipeline modifies; M6's Mileage grows it).
    pub move_rate_deci: u32,
    /// BASE sensor range in tiles (M7 perception consumes; Optics adds).
    pub sensors: u32,
    /// Module slots (M6's total-XP milestones grow it, cap 3).
    pub module_slots: u32,
    /// Log ring-buffer cap (stats floor; Memory banks grow it).
    pub log_cap: u32,
    /// Station-bought compute upgrades, purchase order (indices into
    /// `stats.upgrades` — the hardware layer of the pipeline).
    pub upgrades: Vec<u8>,
    /// Slotted modules, slot order (indices into `stats.modules`).
    pub modules: Vec<u8>,
    /// Player-queued Station orders, FIFO (served at pad mount; an
    /// unaffordable front order is SKIPPED, not dropped — it re-arms).
    pub upgrade_queue: Vec<UpgradeOrder>,
    /// Deci-units aboard that were withdrawn FROM COLONY STOCK (cargo
    /// provenance): re-depositing them at a depot earns no delivery-
    /// milestone credit — recycling stock is zero net delivery. Clamped
    /// to the manifest whenever cargo leaves by another path.
    pub withdrawn_aboard: u32,
    /// Sitting an Upgrade Station pad (an engine interrupt context —
    /// inert, double-handle exposed).
    pub pad_sit: bool,
    /// explore() chains a survey onto its walk (M7): set at the walk's
    /// start, consumed when the move completes.
    pub survey_after_move: bool,
    pub color: Color,
    pub requested: Option<ActionRequest>,
    pub action: Option<Action>,
    /// Boot Sequence countdown (docs/02): an engine interrupt context.
    pub booting: Option<u32>,
    /// In-progress recall (engine interrupt context).
    pub recall: Option<Recall>,
    /// Ticks left in a collision-bump freeze (docs/02: bots are solid).
    pub bump_frozen: u32,
    /// Set by the forced `become_disabled()`; the death phase wrecks the bot.
    pub dying: bool,
    /// Local log ring buffer (base 8 entries; hardware stat later). Each
    /// entry carries its severity level (trace=0 … error=4, docs/01).
    pub log_buf: Vec<LogEntry>,
    /// The environment (docs/01): engine-defined `key → int` policy slots
    /// ([`ENV_KEYS`]). Survives restarts/faults/redeploys; dies with the
    /// bot. Unset means the key's default.
    pub env: BTreeMap<String, i64>,
    /// DECI-XP per track (M6, round 4: awards and multipliers compute in
    /// tenths so Learning's 10% of a 1-XP drip is real). Absent = 0.
    /// Tables in docs read whole XP — the human unit; divide by 10.
    pub xp: BTreeMap<XpTrack, u64>,
    /// Hauling income accumulator (deci-XP): cargo-distance carried since
    /// the last delivery — credited at deposit, forfeited by drops/spills
    /// (docs/02: "cargo-distance DELIVERED").
    pub haul_accum: u64,
    /// Learning-feed fractional carry (hundredths of a deci-XP): 10% of a
    /// 1-deci Age drip is real over time instead of flooring to zero
    /// every settlement (docs/02's deci-XP intent, one unit finer).
    pub learning_carry: u64,
    /// Age levels whose max-HP perk has been granted (idempotent
    /// level-up application; xp.ron `age_hp_per_level`).
    pub age_hp_levels: u32,
    /// Fractional XP-gain carry per track (hundredths of a deci): keeps
    /// sub-100% multipliers reducing slow drips instead of zeroing them.
    pub gain_carry: BTreeMap<XpTrack, u64>,
    /// The tick this bot last entered a tile (M7: ONLY MOVING THINGS MAKE
    /// NOISE — hearing checks it against the current tick).
    pub moved_tick: u64,
    /// Open detection episodes (M7, docs/05): enemy faction → ticks since
    /// last seen-or-heard by that faction; the episode re-arms (closes)
    /// after the re-arm window fully unobserved. Opening one pays Hiding
    /// XP — the machine gets good at what keeps happening to it.
    pub episodes: BTreeMap<u8, u32>,
    /// Remaining self-destruct ticks carried out of a RESCUE (M10: a
    /// re-wreck's countdown resumes, never resets — failed rescues burn
    /// the window). None = never wrecked since the last full window.
    pub countdown_carry: Option<u32>,
    /// Latent quirk rolls, print order (indices into quirks.ron). No
    /// effect, not visible, not introspectable — until total XP crosses a
    /// manifestation threshold (docs/09).
    pub latent_quirks: Vec<u8>,
    /// Manifested quirks, manifestation order — live effects, enemy-
    /// visible, introspectable via my_quirks()/has_quirk().
    pub quirks: Vec<u8>,
    /// Last VM crash_count charged for (fault-damage bookkeeping).
    pub crash_seen: u64,
    /// This bot's `rng.program` stream state (docs/07): seeded from
    /// (match seed, entity ID) so identical programs desync deterministically.
    pub rng_program: u64,
    /// Ticks spent standing on Dunes since the last move (M8, Q35): the
    /// idle-sink counter — full sink intervals surcharge the next step.
    /// Reset by every move; zero everywhere else.
    pub dune_idle: u32,
}

impl BotData {
    /// This track's deci-XP.
    pub fn xp(&self, track: XpTrack) -> u64 {
        self.xp.get(&track).copied().unwrap_or(0)
    }

    /// Total deci-XP across every track — the milestone scale (quirk
    /// manifestation, module slots).
    pub fn xp_total(&self) -> u64 {
        self.xp.values().sum()
    }
}

#[derive(Debug)]
pub struct Bot {
    pub data: BotData,
    /// Taken out while running (borrow discipline); always `Some` between
    /// phases.
    pub vm: Option<Vm>,
}

impl BotData {
    /// Total carried deci-units across all kinds.
    pub fn cargo_total(&self) -> u32 {
        self.cargo.values().sum()
    }

    /// Add up to `deci` of `kind`, clamped to `cap` (the EFFECTIVE
    /// capacity — the caller runs the pipeline: Hauling levels and cargo
    /// quirks move it); returns the amount actually loaded.
    pub fn cargo_add(&mut self, kind: crate::resources::Resource, deci: u32, cap: u32) -> u32 {
        let space = cap.saturating_sub(self.cargo_total());
        let take = deci.min(space);
        if take > 0 {
            *self.cargo.entry(kind).or_insert(0) += take;
        }
        take
    }

    /// Remove up to `deci` of `kind`; returns the amount actually removed.
    pub fn cargo_remove(&mut self, kind: crate::resources::Resource, deci: u32) -> u32 {
        let Some(have) = self.cargo.get_mut(&kind) else { return 0 };
        let take = deci.min(*have);
        *have -= take;
        if *have == 0 {
            self.cargo.remove(&kind);
        }
        take
    }
}

impl Bot {
    /// Is the VM currently executing any signal template (error, hurt,
    /// bump, bumped, boot)? Drives the viewer's frustration cloud.
    pub fn in_signal_handler(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.phase() != pyrite::Phase::Main)
    }

    /// Name of the signal currently being handled ("error" / "hurt" /
    /// "bump" / "bumped" / "boot"), if any — the VM tracks which signal's
    /// template is running.
    pub fn handler_name(&self) -> Option<&'static str> {
        self.vm.as_ref()?.active_signal()
    }

    /// Is the bot inside the forced handler-entry ritual?
    pub fn in_handler_init(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.in_handler_init())
    }

    /// Is the running window FACTORY contents?
    pub fn in_default_handler(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.handler_is_default())
    }

    /// Did this bot exit through the abort sequence? (The skull cloud.)
    pub fn aborted(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.aborted())
    }

    /// The factory window source for a signal name, if installed.
    pub fn default_handler_source(&self, which: &str) -> Option<&str> {
        let kind = pyrite::ast::SignalKind::ALL
            .into_iter()
            .find(|k| k.name() == which)?;
        self.vm.as_ref().and_then(|vm| vm.default_handler(kind)).map(|d| d.source.as_str())
    }

    /// (signal name, source line if the program wrote that window) — all
    /// five player windows, inspector-ready.
    pub fn handler_summary(&self) -> [(&'static str, Option<u32>); 5] {
        pyrite::ast::SignalKind::ALL.map(|kind| {
            (kind.name(), self.vm.as_ref().and_then(|vm| vm.handler_line(kind)))
        })
    }
}

/// One local log line: (severity level, text). Levels are trace=0 …
/// error=4 (docs/01); entries below the bot's `log_min_level` env are
/// discarded at the call.
pub type LogEntry = (u8, String);

/// Where an env key's default comes from: a fixed registry value, or a
/// tuning field (so the number keeps living in `tuning.ron` — no second
/// copy that can drift).
pub enum EnvDefault {
    Fixed(i64),
    /// `tuning.hurt_line_pct`.
    HurtLine,
}

/// One engine-defined environment key (docs/01 "The Environment"): a
/// bounded int policy slot.
pub struct EnvKey {
    pub name: &'static str,
    pub default: EnvDefault,
    pub min: i64,
    pub max: i64,
}

/// The v1 key set — grows like the function catalog.
pub const ENV_KEYS: &[EnvKey] = &[
    EnvKey { name: "hurt_line", default: EnvDefault::HurtLine, min: 1, max: 99 },
    EnvKey { name: "log_min_level", default: EnvDefault::Fixed(0), min: 0, max: 4 },
];

/// The log severity ladder (docs/01): the ONE (name, rank) source — bound
/// as VM constants by `Sim::new`, indexed by the HUD for display, used by
/// `log`'s range check. Ranks are the array indices.
pub const LEVEL_NAMES: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

/// One-time construct research prices in Data (docs/06's tree).
pub fn research_cost(construct: pyrite::Construct) -> u64 {
    use pyrite::Construct as C;
    match construct {
        C::Variables => 10,
        C::If => 20,
        C::WhileLoop => 35,
        C::OnError => 40,
        C::OnHurt => 55,
        C::OnBumpBumped => 30,
        C::OnBoot => 45,
        C::Functions => 50,
        C::Import => 65,
        C::Lists => 60,
        C::Enums => 70,
        C::Channels => 80,
    }
}

/// The UnlockSet a faction's deploys parse against.
pub fn faction_unlocks(world: &World, faction: u8) -> pyrite::UnlockSet {
    if world.dev_all_unlocks {
        pyrite::UnlockSet::all()
    } else {
        world.unlocks.get(&faction).cloned().unwrap_or_default()
    }
}

/// The function blocks a faction may call this match (docs/06). Dev sandboxes
/// (`dev_all_unlocks`) act as if every Cache has been studied, so existing
/// maps and tests deploy freely; a real match starts empty and studies up.
pub fn studied_blocks(
    world: &World,
    faction: u8,
) -> std::collections::BTreeSet<crate::progression::FunctionBlock> {
    if world.dev_all_unlocks {
        crate::progression::FunctionBlock::ALL.into_iter().collect()
    } else {
        world.studied.get(&faction).cloned().unwrap_or_default()
    }
}

/// Which called builtins a `faction` may NOT yet call: those whose gating
/// block (progression::block_of) is not in the faction's studied set. Empty
/// means the program is clear to deploy. Names not gated by any block, and
/// user `def`s / module functions, are ignored here.
pub fn locked_builtins(
    world: &World,
    faction: u8,
    called: &std::collections::BTreeSet<String>,
) -> Vec<(String, crate::progression::FunctionBlock)> {
    let studied = studied_blocks(world, faction);
    let mut locked = Vec::new();
    for name in called {
        if let Some(block) = crate::progression::block_of(name)
            && !studied.contains(&block)
        {
            locked.push((name.clone(), block));
        }
    }
    locked
}

/// Read an env key with defaulting and quirk policy (docs/01 + docs/09):
/// unset means the default — a TEMPERAMENT quirk shifts that default —
/// and the value always lands inside any COMPULSION clamp (a `setenv`
/// past the clamp clips quietly; `getenv` reports where it landed).
/// Latent quirks do not exist yet, so only `data.quirks` applies.
pub fn env_read(
    data: &BotData,
    key: &str,
    tuning: &crate::sim::Tuning,
    quirks: &crate::quirks::QuirkCatalog,
) -> i64 {
    let mut value = match data.env.get(key) {
        Some(v) => Some(*v),
        None => {
            // Temperament: the quirk's shifted default (first manifested
            // quirk naming this key wins — manifestation order).
            quirks.effects_of(data).find_map(|e| match e {
                crate::quirks::QuirkEffect::EnvDefault { key: k, value }
                    if k.as_str() == key =>
                {
                    Some(value)
                }
                _ => None,
            })
        }
    }
    .unwrap_or_else(|| match ENV_KEYS.iter().find(|k| k.name == key).map(|k| &k.default) {
        Some(EnvDefault::Fixed(v)) => *v,
        Some(EnvDefault::HurtLine) => tuning.hurt_line_pct,
        None => 0,
    });
    // The key's hard range (the same bound `setenv` enforces on writes) is a
    // floor under EVERY source — including a quirk's EnvDefault, which does
    // not go through setenv, so an out-of-range temperament can't smuggle in
    // a value setenv would have rejected (e.g. a 0 hurt_line that silences
    // the Hurt signal forever).
    if let Some(spec) = ENV_KEYS.iter().find(|k| k.name == key) {
        value = value.clamp(spec.min, spec.max);
    }
    // Compulsion: the hardware refuses, deterministically.
    for effect in quirks.effects_of(data) {
        if let crate::quirks::QuirkEffect::EnvClamp { key: k, min, max } = effect
            && k.as_str() == key
        {
            value = value.clamp(min, max);
        }
    }
    value
}

/// One faction's live perception (M7, docs/05: vision is the live union
/// of every friendly bot's and structure's sensor range — the colony
/// cloud pools eyes, so queries are faction-scoped).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Perception {
    /// Entities fully SEEN: total information — fog lifted, properties
    /// readable, geology known.
    pub seen: BTreeSet<EntityId>,
    /// Heard-only MOVING bots: position-only handles (docs/05 — a
    /// contact, not a picture); property reads fault err_unknown_contact.
    pub heard: BTreeMap<EntityId, TilePos>,
}

/// A permanently-discovered resource node (docs/05 Q70): existence and
/// kind persist; `exhausted` updates only when OBSERVED empty — you learn
/// your vein ran dry when you look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KnownNode {
    pub kind: crate::resources::Resource,
    pub pos: TilePos,
    pub exhausted: bool,
}

/// A player-designated terraform site (docs/05): the player places it
/// (a lockstep Command); bots do the labor via `build()`.
#[derive(Debug, Clone, PartialEq)]
pub struct Blueprint {
    pub pos: TilePos,
    pub kind: BlueprintKind,
    pub progress: u32,
    pub needed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BlueprintKind {
    Bridge,
    // --- terraform set (M8-D, docs/05) ---
    /// Rubble → Plains; completing yields Stone to the BUILDER's faction.
    Clear,
    /// Plains → Barricade (Stone): solid built mass, blocks LoS too.
    Barricade,
    /// Un-build civil works: Bridge → Water, Barricade → Plains.
    Demolish,
    /// Corruption → Plains — slow, and a living Blight Core in range
    /// will re-corrupt it (kill the source first).
    Cleanse,
    /// Plains/Rubble → Road (Stone): the half-cost artery.
    Road,
}

impl BlueprintKind {
    /// Stable hash tag (phase-9 snapshot).
    pub fn as_u8(self) -> u8 {
        match self {
            BlueprintKind::Bridge => 0,
            BlueprintKind::Clear => 1,
            BlueprintKind::Barricade => 2,
            BlueprintKind::Demolish => 3,
            BlueprintKind::Cleanse => 4,
            BlueprintKind::Road => 5,
        }
    }

    /// Which ground this kind works (M8-D) — ONE rule shared by the
    /// placement command, the completion re-check, and the build bar's
    /// ghost, so they cannot drift.
    pub fn site_ok(self, tile: Option<TileKind>) -> bool {
        match self {
            BlueprintKind::Bridge => tile == Some(TileKind::Water),
            BlueprintKind::Clear => tile == Some(TileKind::Rubble),
            BlueprintKind::Barricade => tile == Some(TileKind::Plains),
            BlueprintKind::Demolish => {
                matches!(tile, Some(TileKind::Bridge | TileKind::Barricade))
            }
            BlueprintKind::Cleanse => tile == Some(TileKind::Corruption),
            BlueprintKind::Road => {
                matches!(tile, Some(TileKind::Plains | TileKind::Rubble))
            }
        }
    }
}

/// A disabled bot in the wreck race (M10, docs/02): field-repair it (XP
/// preserved), salvage it (receipt + decryption), analyze it (Data +
/// intel + comm key), or hijack it (the bot itself) — before the
/// countdown expires into the game's ONLY explosion. The whole BotData
/// rides along: rescue and hijack rebuild the bot from it, salvage reads
/// its receipt, analyze reads its logs and env.
#[derive(Debug, Clone, PartialEq)]
pub struct Wreck {
    pub data: BotData,
    /// Wreck hull (~25% of the bot's max HP): attackable; 0 destroys it
    /// (black box, NO blast — expiry is the only explosion).
    pub hp: i64,
    /// Self-destruct ticks left (base + per-XP bonus; a re-wreck RESUMES
    /// where the failed rescue left off, never resets).
    pub countdown: u32,
}

impl Wreck {
    pub fn pos(&self) -> TilePos {
        self.data.pos
    }
}

/// Dropped by every destruction (docs/02-agents.md): the local log buffer
/// at the moment of destruction, the cause, and the env snapshot (Q58).
/// Carries an entity id so `recover_black_box(entity)` can target it.
#[derive(Debug, Clone, PartialEq)]
pub struct BlackBox {
    pub entity: EntityId,
    pub tick: u64,
    pub bot: BotId,
    pub pos: TilePos,
    pub cause: String,
    pub logs: Vec<LogEntry>,
    pub env: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArchiveKind {
    CrashDump,
    Log,
}

/// A colony cloud (printer-hosted): crash dumps and uploaded logs.
/// Printers always accept log traffic (docs/03-resources.md). Each colony
/// has its OWN cloud (Q89, docs/08) — the archive is keyed by faction, so
/// `analyze()`d victim logs file into the analyzer's cloud, never a shared
/// list every faction can read.
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveEntry {
    pub tick: u64,
    pub bot: BotId,
    pub kind: ArchiveKind,
    /// Severity (trace=0 … error=4); crash dumps archive at error.
    pub level: u8,
    pub line: u32,
    pub text: String,
}

#[derive(Debug)]
pub struct World {
    pub tick: u64,
    pub grid: Grid,
    pub nodes: BTreeMap<EntityId, ResourceNode>,
    pub depots: BTreeMap<EntityId, Depot>,
    /// Template Caches (docs/06): study sites unlocking function blocks.
    pub caches: BTreeMap<EntityId, Cache>,
    /// Generic structures (Smelter/Foundry/Archive — docs/03). Printers
    /// stay separate until M9's rework.
    pub structures: BTreeMap<EntityId, Structure>,
    pub bots: BTreeMap<BotId, Bot>,
    /// Entity handle -> bot, for targeting.
    pub bot_entities: BTreeMap<EntityId, BotId>,
    pub printers: BTreeMap<EntityId, Printer>,
    pub blueprints: BTreeMap<EntityId, Blueprint>,
    /// Traffic rules painted per tile (arrows) — affects pathfinding.
    pub overlays: BTreeMap<TilePos, OverlayKind>,
    /// Cosmetic tile paint (color index) — player markings; a future
    /// paint_at() sensor can make programs read these.
    pub paint: BTreeMap<TilePos, u8>,
    /// Deployed program per (faction, color slot).
    pub color_programs: BTreeMap<(u8, u8), ColorProgram>,
    /// Every source version ever deployed, by source hash — decryption,
    /// the Codex, and stale-version display all read from here (docs/01).
    pub program_library: BTreeMap<u64, String>,
    pub wrecks: BTreeMap<BotId, Wreck>,
    pub black_boxes: Vec<BlackBox>,
    /// Typed colony stock, per faction, in deci-units (docs/03: payments
    /// are abstract — they draw from here; feeds stay physical).
    pub stock: BTreeMap<(u8, crate::resources::Resource), u64>,
    /// Data currency per faction (docs/03: earned by doing, not mining).
    pub data: BTreeMap<u8, u64>,
    /// Lifetime NET deci-units delivered per faction: depot deposits only
    /// (refinery feeds are production logistics, not delivery), and cargo
    /// withdrawn from stock is excluded at deposit via the per-bot
    /// `withdrawn_aboard` provenance — so withdraw/deposit cycling mints
    /// nothing while seeded-stock withdrawals never suppress real income.
    pub delivered: BTreeMap<u8, u64>,
    /// Delivery milestones already paid per faction — a high-water mark
    /// against re-crossing.
    pub milestones_paid: BTreeMap<u8, u64>,
    /// Factions that already earned their first-kill Data.
    pub first_kill_done: BTreeSet<u8>,
    /// Per-faction perception (M7, docs/05): recomputed every phase 5 from
    /// post-move positions — a pure function of hashed state, so it is
    /// deliberately NOT hashed itself. Queries read LAST tick's compute
    /// (the phase-0 seed gives tick 1 something to read).
    pub perception: BTreeMap<u8, Perception>,
    /// Per-faction PERMANENT map knowledge (docs/05: discovered nodes are
    /// the deliberate exception to "no persistent intel"; amounts stay
    /// live-only — only existence and observed exhaustion persist). Hashed.
    pub known_nodes: BTreeMap<u8, BTreeMap<EntityId, KnownNode>>,
    /// Factions currently browning out (energy draw > generation, set by
    /// the phase-8 upkeep settlement; read by next tick's grant pipeline).
    pub brownout: BTreeSet<u8>,
    /// Per-faction Fabricator-trickle pick: the ONE bot that stays fully
    /// powered through a brownout (lowest entity id owning a working
    /// printer's faction — recomputed every upkeep settlement).
    pub powered_bot: BTreeMap<u8, BotId>,
    /// Factions whose Steel maintenance went unpaid last settlement —
    /// self-repair halts and rust decay applies (docs/03 Q84).
    pub rusting: BTreeSet<u8>,
    /// Decryption levels (M10, docs/08): (viewer faction, owner faction,
    /// color) → percent of that color's source readable. Grows +N% per
    /// salvage of that color's wrecks; NEVER goes down; alliances never
    /// share it.
    pub decryption: BTreeMap<(u8, u8, u8), u32>,
    /// Comm keys held (M10/M11): viewer faction → factions whose channel
    /// namespaces it can address. `analyze()` steals one.
    pub comm_keys: BTreeMap<u8, BTreeSet<u8>>,
    /// Feral nests (M12, docs/04), keyed by entity handle.
    pub nests: BTreeMap<EntityId, Nest>,
    /// Server harm setting (M13, docs/08): false = Non-PvP — no direct
    /// damage to other players' property, no salvage/analyze/hijack of
    /// their wrecks. Wreck BLASTS hit friend and foe regardless (Q55).
    pub harm_enabled: bool,
    /// Declared alliances (M13), stored as normalized (low, high) pairs.
    /// Allies share decryption progress (docs/08) and may grant further.
    pub alliances: BTreeSet<(u8, u8)>,
    /// Standing grants (M13): (granter, grantee, what). Vision pools the
    /// granter's eyes into the grantee's perception; Channels opens the
    /// granter's namespace without a stolen comm key.
    pub grants: BTreeSet<(u8, u8, GrantKind)>,
    /// The Request Box (M13, docs/08): a small cross-faction message
    /// board — (tick, poster faction, text). Capped; oldest entries fall
    /// off. (Docs sketch a physical structure; flagged in TASKS.md.)
    pub requests: Vec<(u64, u8, String)>,
    /// Agreed sim speed, per-mille of base (M13 vote outcome; 0 = paused).
    /// The GAME layer reads this to pace real time — sim state so peers
    /// agree, but the tick loop itself is the harness's.
    pub sim_speed_permille: u32,
    /// The open sim-speed proposal, if any.
    pub pending_vote: Option<PendingVote>,
    /// No new proposal before this tick (docs/08: each attempt, pass or
    /// fail, starts the cooldown).
    pub vote_cooldown_until: u64,
    /// Vote plumbing constants, from MatchSettings.
    pub vote_cooldown_ticks: u64,
    pub vote_window_ticks: u64,
    /// Global Feral escalation tier 0–3 (M12, docs/04): driven by player
    /// FOOTPRINT (structures, printers, claims, kills), never wall-clock.
    pub escalation: u8,
    /// Lifetime Feral bot deaths — one input to the footprint metric.
    pub ferals_killed: u32,
    /// Per-faction construct unlocks (docs/06: permanent knowledge),
    /// consumed at parse. Dev sandboxes get UnlockSet::all() via
    /// MapSpec.dev_all_unlocks.
    pub unlocks: BTreeMap<u8, pyrite::UnlockSet>,
    /// Per-faction, per-match function-block unlocks (docs/06): the blocks a
    /// colony has STUDIED at Template Caches. Gates which builtins its
    /// programs may call. Dev sandboxes act as if every block is studied.
    pub studied: BTreeMap<u8, std::collections::BTreeSet<crate::progression::FunctionBlock>>,
    /// Dev flag (from MapSpec): parse with everything unlocked.
    pub dev_all_unlocks: bool,
    /// Dev flag (from MapSpec): skip energy/Steel upkeep entirely.
    pub dev_free_power: bool,
    /// Expected quirks per bot, per-mille (docs/09 match setting; 0 = off).
    pub quirk_permille: u32,
    /// Per-faction colony clouds (Q89): faction → its crash dumps + logs.
    /// Each colony reads only its own cloud; `analyze()` files a victim's
    /// logs into the *analyzer's* entry, so intel never leaks across factions.
    /// Read one cloud with `archive.get(&faction)`, or all of them (tests /
    /// debug) with [`World::archive_all`].
    pub archive: BTreeMap<u8, Vec<ArchiveEntry>>,
    /// The match seed (kept for seeding per-bot streams at spawn).
    pub seed: u64,
    /// Damage queued during phases 2–4, applied in phase 6 (docs/07: damage
    /// is a phase, not an inline side effect). The attacker rides along as
    /// (bot, faction) — kill XP credits the bot, first-kill Data and
    /// hostile-source filters read the faction. Drained every tick.
    pub pending_damage: Vec<(BotId, i64, Option<(BotId, u8)>)>,
    /// XP earned this tick, settled in phase 7 under the start-of-tick
    /// Learning multiplier (identity until M6). Drained every tick.
    pub pending_xp: Vec<(BotId, XpTrack, u64)>,
    /// Signals raised this tick, dispatched once per bot at the phase-6 op
    /// boundary: highest severity wins, extras dropped (Q81 — co-arrival is
    /// not a double-handle). The source faction rides along for the Flinch
    /// body track's hostile-source filter (docs/02: self-inflicted signals
    /// grant nothing). Drained every tick.
    pub pending_signals: Vec<(BotId, pyrite::Signal, Option<u8>)>,
    /// BLOCKING bots per tile — the spatial index (occupancy checks were
    /// O(bots) scans; perception multiplies query volume). Holds exactly
    /// the live, non-dying bots by construction: kept in sync by
    /// [`World::index_bot`]/[`World::unindex_bot`]/[`World::move_bot`],
    /// and a bot leaves the moment `dying` is set (wrecks don't block) —
    /// readers never need to re-filter. Derived state: excluded from the
    /// state hash.
    pub occupancy: BTreeMap<TilePos, std::collections::BTreeSet<BotId>>,
    /// Polite recall queue (M9, docs/01 Q85): engine-fired re-colorings
    /// (deploy drops/claims) wait here until their bot is out of every
    /// template phase — entry never double-handles. bot → claiming
    /// printer. Hashed.
    pub pending_recalls: BTreeMap<BotId, EntityId>,
    /// Per-faction re-allocation clock (M9): every X ticks the printers
    /// recompute which bots they own. Player-set (EditPrinterRules);
    /// absent = the printers.ron default. Hashed.
    pub check_interval: BTreeMap<u8, u64>,
    /// Reprint queue (M9): a convenience counter per faction — queued
    /// stock prints, consumed as print jobs start. Hashed.
    pub reprint_queue: BTreeMap<u8, u32>,
    /// Blight Cores (M8-C, docs/05): the living sources of Corruption.
    /// Attackable world entities; while one lives it re-corrupts cleansed
    /// ground in its radius every spread interval. Hashed.
    pub blight_cores: BTreeMap<EntityId, BlightCore>,
    /// Scree crossing counters (M8, Q40): entries per Scree tile, bumped
    /// by [`World::move_bot`]; the end-of-tick terrain settle collapses a
    /// tile to Rubble at the tuning threshold and drops its counter.
    /// Hashed — mid-wear tiles are real divergent state.
    pub scree_wear: BTreeMap<TilePos, u32>,
    /// Named seeded RNG streams — the sim's only randomness (CLAUDE.md;
    /// inventory in docs/07). Advanced only by sim systems, in tick order.
    pub rng: RngStreams,
    /// FNV-1a over the tile grid, cached so the per-tick snapshot hash
    /// doesn't re-walk the whole map. EVERY post-construction tile write
    /// goes through [`World::set_tile`], which marks it dirty; the
    /// refresh runs once per tick before the phase-9 snapshot (M8 made
    /// terrain mutate routinely — one recompute per mutation was O(map)
    /// each). Derived state: excluded from the hash itself.
    pub terrain_hash: u64,
    /// Set by [`World::set_tile`]; cleared by the per-tick refresh.
    pub(crate) terrain_dirty: bool,
    next_entity: u64,
    next_bot: u32,
}

/// A Blight Core (M8-C, docs/05): the living source of Corruption. It
/// spreads creep to the nearest clean passable tile in its radius every
/// spread interval, re-corrupting cleansed ground while it lives. Solid,
/// attackable; killing it stops the spread — the creep it made stays
/// until cleansed (M8-D).
#[derive(Debug, Clone)]
pub struct BlightCore {
    pub pos: TilePos,
    /// Spread reach in tiles (chebyshev — the game's circle).
    pub radius: u32,
    pub hp: i64,
}

/// One XP track (M6, docs/02): five task tracks earned by what the
/// program chooses to do, six body tracks earned by what happens to the
/// machine. Scouting/Hiding income lands with M7 perception, Boot income
/// with M10 rescues — the tracks exist now so storage never migrates
/// again. Ordered for deterministic iteration/hashing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum XpTrack {
    // --- task tracks ---
    Mining,
    Hauling,
    Combat,
    Building,
    Scouting,
    // --- body tracks (use-based; source-filtered against farming) ---
    Age,
    Mileage,
    Hiding,
    Flinch,
    Boot,
    Learning,
}

impl XpTrack {
    pub const ALL: [XpTrack; 11] = [
        XpTrack::Mining,
        XpTrack::Hauling,
        XpTrack::Combat,
        XpTrack::Building,
        XpTrack::Scouting,
        XpTrack::Age,
        XpTrack::Mileage,
        XpTrack::Hiding,
        XpTrack::Flinch,
        XpTrack::Boot,
        XpTrack::Learning,
    ];

    pub fn name(self) -> &'static str {
        match self {
            XpTrack::Mining => "mining",
            XpTrack::Hauling => "hauling",
            XpTrack::Combat => "combat",
            XpTrack::Building => "building",
            XpTrack::Scouting => "scouting",
            XpTrack::Age => "age",
            XpTrack::Mileage => "mileage",
            XpTrack::Hiding => "hiding",
            XpTrack::Flinch => "flinch",
            XpTrack::Boot => "boot",
            XpTrack::Learning => "learning",
        }
    }

    pub fn as_u8(self) -> u8 {
        Self::ALL.iter().position(|t| *t == self).expect("in ALL") as u8
    }
}

/// The named RNG streams from docs/07's inventory. One consumer domain per
/// stream, so e.g. a bot calling `rng()` can never perturb dodge picks.
/// (`rng.program` is per-bot state on [`BotData`], not listed here.)
#[derive(Debug, Clone, PartialEq)]
pub struct RngStreams {
    pub combat: u64,
    pub wander: u64,
    pub explore: u64,
    pub sidestep: u64,
    pub quirk_roll: u64,
    pub feral_mutation: u64,
}

impl RngStreams {
    fn from_seed(seed: u64) -> Self {
        Self {
            combat: stream_seed(seed, "combat"),
            wander: stream_seed(seed, "wander"),
            explore: stream_seed(seed, "explore"),
            sidestep: stream_seed(seed, "sidestep"),
            quirk_roll: stream_seed(seed, "quirk_roll"),
            feral_mutation: stream_seed(seed, "feral_mutation"),
        }
    }
}

/// Derive a stream's initial state from the match seed and its name, then
/// scramble once so streams with related names still decorrelate.
pub fn stream_seed(seed: u64, name: &str) -> u64 {
    let mut h = crate::hash::Fnv1a::new();
    h.write_u64(seed);
    h.write_str(name);
    let mut state = h.finish();
    next_rand(&mut state)
}

/// Deterministic SplitMix64 step. Every sim randomness draw goes through
/// here, against one named stream's state.
pub fn next_rand(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

impl World {
    pub fn from_spec(spec: &MapSpec) -> Self {
        // Terrain painting is shared with MapSpec::validate (map.rs) — the
        // painting order lives in one place. Blight-Core Corruption is
        // re-stamped below after entity allocation (paint_grid already
        // includes it, but the cores are allocated as entities here).
        let grid = spec.paint_grid();
        let mut world = Self {
            tick: 0,
            grid,
            nodes: BTreeMap::new(),
            depots: BTreeMap::new(),
            caches: BTreeMap::new(),
            structures: BTreeMap::new(),
            bots: BTreeMap::new(),
            bot_entities: BTreeMap::new(),
            printers: BTreeMap::new(),
            blueprints: BTreeMap::new(),
            overlays: BTreeMap::new(),
            paint: BTreeMap::new(),
            color_programs: BTreeMap::new(),
            program_library: BTreeMap::new(),
            wrecks: BTreeMap::new(),
            black_boxes: Vec::new(),
            stock: BTreeMap::new(),
            data: BTreeMap::new(),
            delivered: BTreeMap::new(),
            milestones_paid: BTreeMap::new(),
            first_kill_done: BTreeSet::new(),
            perception: BTreeMap::new(),
            known_nodes: BTreeMap::new(),
            brownout: BTreeSet::new(),
            powered_bot: BTreeMap::new(),
            rusting: BTreeSet::new(),
            decryption: BTreeMap::new(),
            comm_keys: BTreeMap::new(),
            nests: BTreeMap::new(),
            escalation: 0,
            ferals_killed: 0,
            harm_enabled: true,
            alliances: BTreeSet::new(),
            grants: BTreeSet::new(),
            requests: Vec::new(),
            sim_speed_permille: 1000,
            pending_vote: None,
            vote_cooldown_until: 0,
            vote_cooldown_ticks: 300,
            vote_window_ticks: 100,
            unlocks: BTreeMap::new(),
            studied: BTreeMap::new(),
            dev_all_unlocks: spec.dev_all_unlocks,
            dev_free_power: spec.dev_free_power,
            quirk_permille: spec.quirk_permille,
            archive: BTreeMap::new(),
            seed: spec.seed,
            pending_damage: Vec::new(),
            pending_xp: Vec::new(),
            pending_signals: Vec::new(),
            pending_recalls: BTreeMap::new(),
            check_interval: BTreeMap::new(),
            reprint_queue: BTreeMap::new(),
            blight_cores: BTreeMap::new(),
            scree_wear: BTreeMap::new(),
            occupancy: BTreeMap::new(),
            rng: RngStreams::from_seed(spec.seed),
            terrain_hash: 0,
            terrain_dirty: false,
            next_entity: 1,
            next_bot: 1,
        };
        world.terrain_hash = world.compute_terrain_hash();
        // Legacy generic-ore specs become Iron nodes; typed nodes ride the
        // resource-ground tiles (docs/03). Amounts are deci-units.
        use crate::resources::{Resource, DECI};
        for &(pos, amount) in &spec.ore_nodes {
            let id = world.alloc_entity();
            world.nodes.insert(
                id,
                ResourceNode { kind: Resource::Iron, pos, amount: amount * DECI, regen: false },
            );
        }
        let mut node_tiles: Vec<(TilePos, TileKind)> = spec.resource_tiles.clone();
        node_tiles.extend(spec.ore_veins.iter().map(|&p| (p, TileKind::OreVein)));
        node_tiles.extend(spec.crystal.iter().map(|&p| (p, TileKind::CrystalField)));
        node_tiles.sort_by_key(|(pos, tile)| (*pos, tile.as_u8()));
        for (pos, tile) in node_tiles {
            let Some((kind, regen)) = Resource::for_tile(tile) else { continue };
            let id = world.alloc_entity();
            let amount = spec.node_amount * DECI;
            world.nodes.insert(id, ResourceNode { kind, pos, amount, regen });
        }
        for &(pos, faction) in &spec.depots {
            let id = world.alloc_entity();
            world.depots.insert(id, Depot { pos, faction });
        }
        // Template Caches (docs/06): non-consumable study sites.
        for &(pos, block) in &spec.caches {
            let id = world.alloc_entity();
            world.caches.insert(id, Cache { pos, block });
        }
        let mut first_printer_of: BTreeSet<u8> = BTreeSet::new();
        for p in &spec.printers {
            let id = world.alloc_entity();
            world.printers.insert(
                id,
                {
                    // The faction's FIRST-BORN printer is the remainder
                    // bucket (docs/01): no dials, and INDESTRUCTIBLE (Q88) —
                    // never Ruined even if the spec marks it so.
                    let is_remainder = first_printer_of.insert(p.faction);
                    Printer {
                        pos: p.pos,
                        faction: p.faction,
                        color: Color(p.color),
                        state: if p.ruined && !is_remainder {
                            PrinterState::Ruined
                        } else {
                            PrinterState::Working
                        },
                        rules: if is_remainder { None } else { Some(PrinterRules::default()) },
                        job: None,
                        // Seeded printers are the starting slots — not bound to
                        // a nest (never dormant); over-base printers bind at
                        // PlacePrinter time.
                        nest: None,
                    }
                },
            );
        }
        // Blight Cores (M8-C) — allocated AFTER printers so every entity
        // id in existing fixtures stays put. The core squats on corrupted
        // ground from tick 0.
        for &(pos, radius, hp) in &spec.blight_cores {
            let id = world.alloc_entity();
            world.grid.set(pos, TileKind::Corruption);
            world.blight_cores.insert(id, BlightCore { pos, radius, hp });
        }
        if !spec.blight_cores.is_empty() {
            world.terrain_hash = world.compute_terrain_hash();
        }
        // Typed per-faction colony stock. The legacy `starting_ore` seeds
        // Iron for faction 0 (tests); real matches use `starting_stock`.
        if spec.starting_ore > 0 {
            world.stock.insert((0, Resource::Iron), spec.starting_ore * DECI as u64);
        }
        for &(faction, kind, units) in &spec.starting_stock {
            *world.stock.entry((faction, kind)).or_insert(0) += units * DECI as u64;
        }
        world
    }

    /// Live population of a (faction, color): excludes dying bots and bots
    /// mid-recall (a recalled bot counts toward its *destination* color).
    pub fn color_population(&self, faction: u8, color: Color) -> u32 {
        let mut count = 0;
        for bot in self.bots.values() {
            if bot.data.faction != faction || bot.data.dying {
                continue;
            }
            match &bot.data.recall {
                Some(Recall { purpose: RecallPurpose::Recolor { dest }, .. }) => {
                    if let Some(p) = self.printers.get(dest)
                        && p.color == color
                    {
                        count += 1;
                    }
                }
                Some(Recall { purpose: RecallPurpose::Scrap, .. }) => {}
                None => {
                    if bot.data.color == color {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    pub fn alloc_entity(&mut self) -> EntityId {
        let id = EntityId(self.next_entity);
        self.next_entity += 1;
        id
    }

    pub fn alloc_bot_id(&mut self) -> BotId {
        let id = BotId(self.next_bot);
        self.next_bot += 1;
        id
    }

    /// Nearest ore node with ore remaining: (chebyshev distance, id) order —
    /// fully deterministic tie-breaking.
    pub fn nearest_ore(&self, from: TilePos) -> Option<EntityId> {
        // `ore` is the family constant: any mineral vein or seam.
        self.nodes
            .iter()
            .filter(|(_, n)| n.amount > 0 && n.kind.is_ore_family())
            .map(|(id, n)| (from.chebyshev(n.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    pub fn nearest_depot(&self, from: TilePos) -> Option<EntityId> {
        self.depots
            .iter()
            .map(|(id, d)| (from.chebyshev(d.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    /// Read a faction's stock of one kind (deci-units).
    pub fn stock_get(&self, faction: u8, kind: crate::resources::Resource) -> u64 {
        self.stock.get(&(faction, kind)).copied().unwrap_or(0)
    }

    /// Add deci-units to a faction's stock.
    pub fn stock_add(&mut self, faction: u8, kind: crate::resources::Resource, deci: u64) {
        if deci > 0 {
            *self.stock.entry((faction, kind)).or_insert(0) += deci;
        }
    }

    /// All-or-nothing withdrawal from stock (abstract payments, docs/03).
    #[must_use]
    pub fn stock_take(&mut self, faction: u8, kind: crate::resources::Resource, deci: u64) -> bool {
        let Some(have) = self.stock.get_mut(&(faction, kind)) else { return deci == 0 };
        if *have < deci {
            return false;
        }
        *have -= deci;
        if *have == 0 {
            self.stock.remove(&(faction, kind));
        }
        true
    }

    /// Nearest live node of one specific kind, (distance, id) order.
    pub fn nearest_node_of(&self, from: TilePos, kind: crate::resources::Resource) -> Option<EntityId> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.amount > 0 && n.kind == kind)
            .map(|(id, n)| (from.chebyshev(n.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    /// Position of a targetable entity (node, depot, printer, or bot).
    pub fn entity_pos(&self, id: EntityId) -> Option<TilePos> {
        self.nodes
            .get(&id)
            .map(|n| n.pos)
            .or_else(|| self.depots.get(&id).map(|d| d.pos))
            .or_else(|| self.caches.get(&id).map(|c| c.pos))
            .or_else(|| self.printers.get(&id).map(|p| p.pos))
            .or_else(|| self.structures.get(&id).map(|s| s.pos))
            .or_else(|| self.blueprints.get(&id).map(|b| b.pos))
            .or_else(|| self.blight_cores.get(&id).map(|c| c.pos))
            .or_else(|| self.nests.get(&id).map(|n| n.pos))
            .or_else(|| {
                self.bot_entities
                    .get(&id)
                    .and_then(|bid| self.bots.get(bid))
                    .map(|b| b.data.pos)
            })
            // The entity handle outlives the bot into its wreck (M10:
            // the wreck race targets it), and black boxes are clickable
            // field objects.
            .or_else(|| {
                self.bot_entities
                    .get(&id)
                    .and_then(|bid| self.wrecks.get(bid))
                    .map(|w| w.data.pos)
            })
            .or_else(|| self.black_boxes.iter().find(|bb| bb.entity == id).map(|bb| bb.pos))
    }

    /// Are two factions allied (M13)? Symmetric; a faction is its own ally.
    pub fn allied(&self, a: u8, b: u8) -> bool {
        a == b || self.alliances.contains(&(a.min(b), a.max(b)))
    }

    /// Does `from` currently grant `what` to `to` (M13)?
    pub fn granted(&self, from: u8, to: u8, what: GrantKind) -> bool {
        self.grants.contains(&(from, to, what))
    }

    /// May `actor` harm `victim`'s property (M13, docs/08)? Ferals are
    /// fair game on every server; Non-PvP blocks player-vs-player harm.
    pub fn harm_allowed(&self, actor: u8, victim: u8) -> bool {
        actor == victim
            || actor == FERAL_FACTION
            || victim == FERAL_FACTION
            || self.harm_enabled
    }

    /// The factions with a live stake in the match (M13 votes need
    /// unanimity ACROSS these): anyone owning a bot or a printer.
    pub fn live_factions(&self) -> BTreeSet<u8> {
        let mut out: BTreeSet<u8> = self
            .bots
            .values()
            .filter(|b| !b.data.dying)
            .map(|b| b.data.faction)
            .collect();
        out.extend(self.printers.values().map(|p| p.faction));
        out.remove(&FERAL_FACTION);
        out
    }

    /// The wreck a live entity handle points at, if any (M10).
    pub fn wreck_of(&self, id: EntityId) -> Option<BotId> {
        self.bot_entities.get(&id).copied().filter(|bid| self.wrecks.contains_key(bid))
    }

    /// Is a blocking bot (other than `exclude`) standing on `pos`?
    /// Backed by the occupancy index — O(log tiles), not O(bots); the
    /// index holds only live, non-dying bots, so membership is the answer.
    pub fn tile_occupied(&self, pos: TilePos, exclude: BotId) -> bool {
        self.occupancy
            .get(&pos)
            .is_some_and(|ids| ids.iter().any(|id| *id != exclude))
    }

    /// Register a bot's tile in the occupancy index (spawn).
    pub(crate) fn index_bot(&mut self, id: BotId, pos: TilePos) {
        self.occupancy.entry(pos).or_default().insert(id);
    }

    /// Remove a bot from the occupancy index (death/explosion).
    pub(crate) fn unindex_bot(&mut self, id: BotId, pos: TilePos) {
        if let Some(ids) = self.occupancy.get_mut(&pos) {
            ids.remove(&id);
            if ids.is_empty() {
                self.occupancy.remove(&pos);
            }
        }
    }

    /// FNV-1a over every tile, in row order. O(map) — called once at
    /// construction and after each (rare) terrain mutation, never per tick.
    pub(crate) fn compute_terrain_hash(&self) -> u64 {
        let mut h = crate::hash::Fnv1a::new();
        for tile in self.grid.tiles() {
            h.write_u8(tile.as_u8());
        }
        h.finish()
    }

    /// Mutate terrain, keeping the cached grid hash fresh. EVERY
    /// post-construction tile write goes through here (the phase-9
    /// snapshot hashes `terrain_hash`, not the grid).
    pub(crate) fn set_tile(&mut self, pos: TilePos, kind: TileKind) {
        self.grid.set(pos, kind);
        // A rewritten tile starts fresh: wear belongs to the Scree that
        // was, not whatever stands there now (M8, Q40).
        self.scree_wear.remove(&pos);
        // Terrain mutates routinely now (M8: scree settles, creep
        // spreads, works land) — mark dirty; the cached hash refreshes
        // at most once per tick, right before the phase-9 snapshot.
        self.terrain_dirty = true;
    }

    /// Tiles holding a blocking bot other than `exclude` — the obstacle
    /// set for path replanning, read straight off the spatial index.
    pub fn occupied_tiles(&self, exclude: BotId) -> std::collections::BTreeSet<TilePos> {
        self.occupancy
            .iter()
            .filter(|(_, ids)| ids.iter().any(|b| *b != exclude))
            .map(|(pos, _)| *pos)
            .collect()
    }

    /// Move a bot to a new tile, keeping the occupancy index in sync.
    /// EVERY `data.pos` write goes through here.
    pub(crate) fn move_bot(&mut self, id: BotId, to: TilePos) {
        let tick = self.tick;
        let Some(bot) = self.bots.get_mut(&id) else { return };
        let from = bot.data.pos;
        bot.data.pos = to;
        // Only moving things make noise (M7): hearing checks this stamp.
        bot.data.moved_tick = tick;
        // Moving shakes the sand off (M8, Q35).
        bot.data.dune_idle = 0;
        // Scree wear (M8, Q40): every entry counts a crossing; the
        // end-of-tick terrain settle collapses worn tiles to Rubble.
        if self.grid.get(to) == Some(crate::map::TileKind::Scree) {
            *self.scree_wear.entry(to).or_insert(0) += 1;
        }
        self.unindex_bot(id, from);
        self.index_bot(id, to);
    }

    /// Structures are solid: bots can neither stand on nor path through
    /// printer or depot tiles.
    pub fn structure_at(&self, pos: TilePos) -> bool {
        self.printers.values().any(|p| p.pos == pos)
            || self.depots.values().any(|d| d.pos == pos)
            || self.structures.values().any(|s| s.pos == pos)
            || self.blight_cores.values().any(|c| c.pos == pos)
            // Nests are solid, attackable sites (M12) — in every state
            // (a Defeated site still occupies its ground until razed).
            || self.nests.values().any(|n| n.pos == pos)
    }

    /// All structure tiles, for feeding A*'s blocked set.
    pub fn structure_tiles(&self) -> BTreeSet<TilePos> {
        self.printers
            .values()
            .map(|p| p.pos)
            .chain(self.depots.values().map(|d| d.pos))
            .chain(self.structures.values().map(|s| s.pos))
            .chain(self.blight_cores.values().map(|c| c.pos))
            .chain(self.nests.values().map(|n| n.pos))
            .collect()
    }

    /// First free, SPAWNABLE tile at/around `center`, in a fixed
    /// deterministic order. Used for print/re-color placement — a print
    /// may not materialize on High Ground (the Ramp is the only way up,
    /// docs/05; review 2026-07-16).
    pub fn free_spawn_tile(&self, center: TilePos) -> Option<TilePos> {
        const ORDER: [(i32, i32); 9] =
            [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];
        ORDER.iter().map(|(dx, dy)| TilePos::new(center.x + dx, center.y + dy)).find(|&p| {
            self.grid.get(p).is_some_and(|t| t.spawnable())
                && !self.structure_at(p)
                && !self.tile_occupied(p, BotId(u32::MAX))
        })
    }

    /// The faction's remainder printer (M9, docs/01): its first-born —
    /// the lowest-entity-id printer, ANY state (a ruined first printer
    /// still anchors the remainder color; its bots are ghosts until it
    /// works again).
    pub fn remainder_printer(&self, faction: u8) -> Option<EntityId> {
        self.printers
            .iter()
            .filter(|(_, p)| p.faction == faction)
            .map(|(id, _)| *id)
            .next() // BTreeMap: lowest id first
    }

    /// Every archive entry across all faction clouds, faction-ordered
    /// (Q89). A test/debug convenience — gameplay always reads one specific
    /// colony's cloud via `archive.get(&faction)`.
    pub fn archive_all(&self) -> impl Iterator<Item = &ArchiveEntry> {
        self.archive.values().flatten()
    }

    /// Ghost machines (M9, Q65): a bot whose color has NO working printer
    /// in its faction is no longer owned by the fleet — excluded from the
    /// allocation (no claims, no remainder absorb, no recalls, no scrap),
    /// still drawing upkeep. Derived, never stored: repairing the printer
    /// re-uploads the survivors by construction.
    pub fn is_ghost(&self, data: &BotData) -> bool {
        !self.printers.values().any(|p| {
            p.faction == data.faction
                && p.color == data.color
                && p.state == PrinterState::Working
        })
    }

    /// The faction's live FLEET (non-ghost, non-dying bot count) — what
    /// the fleet cap bounds and scrap trims. A bot already walking a
    /// SCRAP recall is leaving and no longer counts: the over-capacity
    /// valve must fire once per surplus body, not once per tick of the
    /// victim's walk home.
    pub fn fleet_size(&self, faction: u8) -> u32 {
        self.bots
            .values()
            .filter(|b| {
                b.data.faction == faction
                    && !b.data.dying
                    && !self.is_ghost(&b.data)
                    && !matches!(
                        b.data.recall,
                        Some(Recall { purpose: RecallPurpose::Scrap, .. })
                    )
            })
            .count() as u32
    }

    /// The full terraform-site predicate: the kind's ground rule plus
    /// the structure guard (walling a depot in stone is refused; Bridge
    /// keeps its water-only rule where structures cannot stand anyway).
    pub fn blueprint_site_ok(&self, kind: BlueprintKind, pos: TilePos) -> bool {
        kind.site_ok(self.grid.get(pos))
            && (kind == BlueprintKind::Bridge || !self.structure_at(pos))
    }

    pub fn nearest_blueprint(&self, from: TilePos) -> Option<EntityId> {
        self.blueprints
            .iter()
            .map(|(id, b)| (from.chebyshev(b.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    /// Nearest living bot of a different faction: (chebyshev, entity id)
    /// order — deterministic tie-breaking.
    pub fn nearest_enemy(&self, from: TilePos, faction: u8) -> Option<EntityId> {
        self.bots
            .values()
            .filter(|b| b.data.faction != faction && !b.data.dying)
            .map(|b| (from.chebyshev(b.data.pos), b.data.entity))
            .min()
            .map(|(_, id)| id)
    }
}

impl World {
    /// try_send / try_broadcast (M11, instant): pick the eligible blocked
    /// receiver(s) NOW and park the delivery for the settle to resolve.
    /// Returns how many receivers took a copy. Corruption jams both ends.
    pub(crate) fn try_deliver(
        &mut self,
        namespace: u8,
        ch: &str,
        value: &pyrite::Value,
        all: bool,
        exclude: BotId,
    ) -> u32 {
        let jammed = |pos: TilePos, grid: &crate::map::Grid| grid.is_corruption(pos);
        // The jam blocks BOTH ways (M11): a caller standing in Corruption
        // transmits nothing — the message is lost, like everything else
        // in the static.
        if self.bots.get(&exclude).is_some_and(|b| jammed(b.data.pos, &self.grid)) {
            return 0;
        }
        let mut eligible: Vec<(u32, u64, BotId)> = self
            .bots
            .iter()
            .filter(|(id, b)| {
                **id != exclude
                    && !b.data.dying
                    && !jammed(b.data.pos, &self.grid)
                    && matches!(
                        &b.data.action,
                        Some(Action::Channel {
                            op: ChannelOp::Receive,
                            ch: c,
                            namespace: n,
                            delivered: None,
                            ..
                        }) if c == ch && *n == namespace
                    )
            })
            .map(|(id, b)| {
                let waited = match &b.data.action {
                    Some(Action::Channel { waited, .. }) => *waited,
                    _ => 0,
                };
                (waited, b.data.entity.0, *id)
            })
            .collect();
        eligible.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let take = if all { eligible.len() } else { 1.min(eligible.len()) };
        for (_, _, id) in eligible.into_iter().take(take) {
            if let Some(Action::Channel { delivered, .. }) =
                &mut self.bots.get_mut(&id).expect("collected").data.action
            {
                *delivered = Some(value.clone());
            }
        }
        take as u32
    }

    /// try_receive (M11, instant): take a blocked sender's message —
    /// longest-blocked sender, ties by lowest entity id — parking Unit for
    /// its resolution. None when nobody is offering.
    pub(crate) fn try_take(&mut self, namespace: u8, ch: &str, exclude: BotId) -> Option<pyrite::Value> {
        let jammed = |pos: TilePos, grid: &crate::map::Grid| grid.is_corruption(pos);
        // Jammed both ways (M11): a poller inside Corruption hears nothing.
        if self.bots.get(&exclude).is_some_and(|b| jammed(b.data.pos, &self.grid)) {
            return None;
        }
        let mut offering: Vec<(u32, u64, BotId)> = self
            .bots
            .iter()
            .filter(|(id, b)| {
                **id != exclude
                    && !b.data.dying
                    && !jammed(b.data.pos, &self.grid)
                    && matches!(
                        &b.data.action,
                        Some(Action::Channel {
                            op: ChannelOp::Send(_),
                            ch: c,
                            namespace: n,
                            delivered: None,
                            ..
                        }) if c == ch && *n == namespace
                    )
            })
            .map(|(id, b)| {
                let waited = match &b.data.action {
                    Some(Action::Channel { waited, .. }) => *waited,
                    _ => 0,
                };
                (waited, b.data.entity.0, *id)
            })
            .collect();
        offering.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let (_, _, sender) = offering.first().copied()?;
        let action = &mut self.bots.get_mut(&sender).expect("collected").data.action;
        if let Some(Action::Channel { op: ChannelOp::Send(v), delivered, .. }) = action {
            let value = v.clone();
            *delivered = Some(pyrite::Value::Unit);
            Some(value)
        } else {
            None
        }
    }
}
