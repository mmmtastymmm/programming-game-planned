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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Working,
    /// Present but broken (the starting Red printer); repairable.
    Ruined,
}

/// A Fabricator: prints/reprints bots for exactly one color and carries the
/// desired-max population dial (docs/03-resources.md). Printers are also
/// "the cloud" — they always accept log traffic.
#[derive(Debug, Clone, PartialEq)]
pub struct Printer {
    pub pos: TilePos,
    pub faction: u8,
    pub color: Color,
    pub state: PrinterState,
    pub desired_max: u32,
    /// In-progress print job: ticks remaining.
    pub job: Option<u32>,
}

/// The deployed program for one (faction, color) slot.
#[derive(Debug, Clone)]
pub struct ColorProgram {
    pub source: String,
    pub program: Rc<Program>,
    /// Version identity = FNV-1a of the source bytes (docs/01: programs are
    /// byte-exact; versions are identified by hashing source bytes).
    pub hash: u64,
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
}

// (The old LOG_BUFFER_CAP const died with M5 — the cap is the per-bot
// `log_cap` stat: stats.ron floor + Memory banks.)

#[derive(Debug)]
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
}

/// A disabled bot awaiting rescue/salvage (countdown comes later). The
/// logs and env snapshot ride along — they become the Black Box when the
/// countdown expires (M10).
#[derive(Debug, Clone, PartialEq)]
pub struct Wreck {
    pub pos: TilePos,
    pub cargo: u32,
    pub logs: Vec<LogEntry>,
    /// Env snapshot at the moment of disablement (Q58: exact runtime
    /// values are read on murder — the game's oldest intel rule).
    pub env: BTreeMap<String, i64>,
}

/// Dropped by every destruction (docs/02-agents.md): the local log buffer
/// at the moment of destruction, the cause, and the env snapshot (Q58).
#[derive(Debug, Clone, PartialEq)]
pub struct BlackBox {
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

/// The colony cloud (printer-hosted): crash dumps and uploaded logs.
/// Printers always accept log traffic (docs/03-resources.md).
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
    /// Per-faction construct unlocks (docs/06: permanent knowledge),
    /// consumed at parse. Dev sandboxes get UnlockSet::all() via
    /// MapSpec.dev_all_unlocks.
    pub unlocks: BTreeMap<u8, pyrite::UnlockSet>,
    /// Dev flag (from MapSpec): parse with everything unlocked.
    pub dev_all_unlocks: bool,
    /// Dev flag (from MapSpec): skip energy/Steel upkeep entirely.
    pub dev_free_power: bool,
    /// Expected quirks per bot, per-mille (docs/09 match setting; 0 = off).
    pub quirk_permille: u32,
    pub archive: Vec<ArchiveEntry>,
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
    /// doesn't re-walk the whole map. Terrain mutates rarely (bridge
    /// builds); EVERY post-construction tile write goes through
    /// [`World::set_tile`], which refreshes this.
    pub terrain_hash: u64,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
        let mut grid = Grid::filled(spec.width, spec.height, TileKind::Plains);
        for &pos in &spec.rubble {
            grid.set(pos, TileKind::Rubble);
        }
        for &pos in &spec.water {
            grid.set(pos, TileKind::Water);
        }
        for &pos in &spec.bridges {
            grid.set(pos, TileKind::Bridge);
        }
        for (tiles, kind) in [
            (&spec.mud, TileKind::Mud),
            (&spec.corruption, TileKind::Corruption),
            (&spec.ore_veins, TileKind::OreVein),
            (&spec.crystal, TileKind::CrystalField),
            (&spec.high_ground, TileKind::HighGround),
            (&spec.vents, TileKind::Vent),
            (&spec.snow, TileKind::Snow),
        ] {
            for &pos in tiles {
                grid.set(pos, kind);
            }
        }
        for &(pos, kind) in &spec.resource_tiles {
            grid.set(pos, kind);
        }
        let mut world = Self {
            tick: 0,
            grid,
            nodes: BTreeMap::new(),
            depots: BTreeMap::new(),
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
            unlocks: BTreeMap::new(),
            dev_all_unlocks: spec.dev_all_unlocks,
            dev_free_power: spec.dev_free_power,
            quirk_permille: spec.quirk_permille,
            archive: Vec::new(),
            seed: spec.seed,
            pending_damage: Vec::new(),
            pending_xp: Vec::new(),
            pending_signals: Vec::new(),
            blight_cores: BTreeMap::new(),
            scree_wear: BTreeMap::new(),
            occupancy: BTreeMap::new(),
            rng: RngStreams::from_seed(spec.seed),
            terrain_hash: 0,
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
        for &pos in &spec.depots {
            let id = world.alloc_entity();
            world.depots.insert(id, Depot { pos });
        }
        for p in &spec.printers {
            let id = world.alloc_entity();
            world.printers.insert(
                id,
                Printer {
                    pos: p.pos,
                    faction: p.faction,
                    color: Color(p.color),
                    state: if p.ruined { PrinterState::Ruined } else { PrinterState::Working },
                    desired_max: p.desired_max,
                    job: None,
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

    /// Nearest ore node with ore remaining: (manhattan distance, id) order —
    /// fully deterministic tie-breaking.
    pub fn nearest_ore(&self, from: TilePos) -> Option<EntityId> {
        // `ore` is the family constant: any mineral vein or seam.
        self.nodes
            .iter()
            .filter(|(_, n)| n.amount > 0 && n.kind.is_ore_family())
            .map(|(id, n)| (from.manhattan(n.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    pub fn nearest_depot(&self, from: TilePos) -> Option<EntityId> {
        self.depots
            .iter()
            .map(|(id, d)| (from.manhattan(d.pos), *id))
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
            .map(|(id, n)| (from.manhattan(n.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    /// Position of a targetable entity (node, depot, printer, or bot).
    pub fn entity_pos(&self, id: EntityId) -> Option<TilePos> {
        self.nodes
            .get(&id)
            .map(|n| n.pos)
            .or_else(|| self.depots.get(&id).map(|d| d.pos))
            .or_else(|| self.printers.get(&id).map(|p| p.pos))
            .or_else(|| self.structures.get(&id).map(|s| s.pos))
            .or_else(|| self.blueprints.get(&id).map(|b| b.pos))
            .or_else(|| self.blight_cores.get(&id).map(|c| c.pos))
            .or_else(|| {
                self.bot_entities
                    .get(&id)
                    .and_then(|bid| self.bots.get(bid))
                    .map(|b| b.data.pos)
            })
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
    fn compute_terrain_hash(&self) -> u64 {
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
        self.terrain_hash = self.compute_terrain_hash();
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
    }

    /// All structure tiles, for feeding A*'s blocked set.
    pub fn structure_tiles(&self) -> BTreeSet<TilePos> {
        self.printers
            .values()
            .map(|p| p.pos)
            .chain(self.depots.values().map(|d| d.pos))
            .chain(self.structures.values().map(|s| s.pos))
            .chain(self.blight_cores.values().map(|c| c.pos))
            .collect()
    }

    /// First free, passable tile at/around `center`, in a fixed
    /// deterministic order. Used for print/re-color placement.
    pub fn free_spawn_tile(&self, center: TilePos) -> Option<TilePos> {
        const ORDER: [(i32, i32); 9] =
            [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];
        ORDER.iter().map(|(dx, dy)| TilePos::new(center.x + dx, center.y + dy)).find(|&p| {
            self.grid.get(p).is_some_and(|t| t.move_ticks().is_some())
                && !self.structure_at(p)
                && !self.tile_occupied(p, BotId(u32::MAX))
        })
    }

    pub fn nearest_blueprint(&self, from: TilePos) -> Option<EntityId> {
        self.blueprints
            .iter()
            .map(|(id, b)| (from.manhattan(b.pos), *id))
            .min()
            .map(|(_, id)| id)
    }

    /// Nearest living bot of a different faction: (manhattan, entity id)
    /// order — deterministic tie-breaking.
    pub fn nearest_enemy(&self, from: TilePos, faction: u8) -> Option<EntityId> {
        self.bots
            .values()
            .filter(|b| b.data.faction != faction && !b.data.dying)
            .map(|b| (from.manhattan(b.data.pos), b.data.entity))
            .min()
            .map(|(_, id)| id)
    }
}
