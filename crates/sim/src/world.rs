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

#[derive(Debug, Clone, PartialEq)]
pub struct OreNode {
    pub pos: TilePos,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Depot {
    pub pos: TilePos,
}

/// An action a builtin asked for this tick; started in the resolve phase.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionRequest {
    MoveTo(EntityId),
    Mine,
    Deposit,
    Attack(EntityId),
    /// Idle deliberately for N ticks — the Tier-0 traffic tool.
    Wait(u32),
    /// Work on a designated blueprint.
    Build(EntityId),
}

/// An in-flight world action.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// `path[0]` is the next tile to enter; `ticks_left` is the remaining
    /// cost of entering it (Rubble takes 2, Plains 1). `goals` is kept so
    /// the route can be re-planned after a bump.
    Move { path: Vec<TilePos>, ticks_left: u32, goals: BTreeSet<TilePos> },
    Mine { node: EntityId, ticks_left: u32 },
    Deposit { depot: EntityId, ticks_left: u32 },
    Attack { target: EntityId, ticks_left: u32 },
    Wait { ticks_left: u32 },
    /// Contributes 1 progress per tick while adjacent to the blueprint.
    Build { blueprint: EntityId },
}

pub const LOG_BUFFER_CAP: usize = 8;

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
    pub cargo: u32,
    pub cargo_cap: u32,
    /// Cycles granted per tick (CPU hardware).
    pub cpu: u64,
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
    /// Local log ring buffer (base 8 entries; hardware stat later).
    pub log_buf: Vec<String>,
    pub xp_mining: u64,
    pub xp_hauling: u64,
    pub xp_combat: u64,
    pub xp_building: u64,
    /// Last VM crash_count charged for (fault-damage bookkeeping).
    pub crash_seen: u64,
    /// This bot's `rng.program` stream state (docs/07): seeded from
    /// (match seed, entity ID) so identical programs desync deterministically.
    pub rng_program: u64,
}

#[derive(Debug)]
pub struct Bot {
    pub data: BotData,
    /// Taken out while running (borrow discipline); always `Some` between
    /// phases.
    pub vm: Option<Vm>,
}

impl Bot {
    /// Is the VM currently executing any signal handler (error, hurt,
    /// death, bump, bumped)? Drives the viewer's frustration cloud.
    pub fn in_signal_handler(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.phase() != pyrite::Phase::Main)
    }

    /// Name of the signal currently being handled ("error" / "hurt" /
    /// "bump" / "bumped" / "death"), if any — the VM tracks which signal
    /// the unified handler is serving.
    pub fn handler_name(&self) -> Option<&'static str> {
        self.vm.as_ref()?.active_signal()
    }

    /// Is the bot inside the forced handler-entry ritual?
    pub fn in_handler_init(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.in_handler_init())
    }

    /// Is the running handler an engine default?
    pub fn in_default_handler(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.handler_is_default())
    }

    /// Engine-default handler source for a handler kind ("signal" or
    /// "death"), if installed.
    pub fn default_handler_source(&self, which: &str) -> Option<&str> {
        use pyrite::ast::SignalKind;
        let kind = match which {
            "signal" => SignalKind::Signal,
            "death" => SignalKind::Death,
            _ => return None,
        };
        self.vm.as_ref().and_then(|vm| vm.default_handler(kind)).map(|d| d.source.as_str())
    }

    /// (handler name, source line if the program installed one) — the two
    /// handler kinds, inspector-ready.
    pub fn handler_summary(&self) -> [(&'static str, Option<u32>); 2] {
        use pyrite::ast::SignalKind;
        let line = |kind| self.vm.as_ref().and_then(|vm| vm.handler_line(kind));
        [("signal", line(SignalKind::Signal)), ("death", line(SignalKind::Death))]
    }
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

/// A disabled bot awaiting rescue/salvage (countdown comes later).
#[derive(Debug, Clone, PartialEq)]
pub struct Wreck {
    pub pos: TilePos,
    pub cargo: u32,
    pub logs: Vec<String>,
}

/// Dropped by every destruction (docs/02-agents.md): logs + cause.
#[derive(Debug, Clone, PartialEq)]
pub struct BlackBox {
    pub tick: u64,
    pub bot: BotId,
    pub pos: TilePos,
    pub cause: String,
    pub logs: Vec<String>,
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
    pub line: u32,
    pub text: String,
}

#[derive(Debug)]
pub struct World {
    pub tick: u64,
    pub grid: Grid,
    pub ore_nodes: BTreeMap<EntityId, OreNode>,
    pub depots: BTreeMap<EntityId, Depot>,
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
    pub stockpile_ore: u64,
    pub archive: Vec<ArchiveEntry>,
    /// The match seed (kept for seeding per-bot streams at spawn).
    pub seed: u64,
    /// Damage queued during phases 2–4, applied in phase 6 (docs/07: damage
    /// is a phase, not an inline side effect). Drained every tick.
    pub pending_damage: Vec<(BotId, i64)>,
    /// XP earned this tick, settled in phase 7 under the start-of-tick
    /// Learning multiplier (identity until M6). Drained every tick.
    pub pending_xp: Vec<(BotId, XpTrack, u64)>,
    /// Signals raised this tick, dispatched once per bot at the phase-6 op
    /// boundary: highest severity wins, extras dropped (Q81 — co-arrival is
    /// not a double-handle). Drained every tick.
    pub pending_signals: Vec<(BotId, pyrite::Signal)>,
    /// BLOCKING bots per tile — the spatial index (occupancy checks were
    /// O(bots) scans; perception multiplies query volume). Holds exactly
    /// the live, non-dying bots by construction: kept in sync by
    /// [`World::index_bot`]/[`World::unindex_bot`]/[`World::move_bot`],
    /// and a bot leaves the moment `dying` is set (wrecks don't block) —
    /// readers never need to re-filter. Derived state: excluded from the
    /// state hash.
    pub occupancy: BTreeMap<TilePos, std::collections::BTreeSet<BotId>>,
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

/// One XP track (the 4 task tracks; Scouting and the body tracks land
/// with their systems — M6/M7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XpTrack {
    Mining,
    Hauling,
    Combat,
    Building,
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
            ore_nodes: BTreeMap::new(),
            depots: BTreeMap::new(),
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
            stockpile_ore: 0,
            archive: Vec::new(),
            seed: spec.seed,
            pending_damage: Vec::new(),
            pending_xp: Vec::new(),
            pending_signals: Vec::new(),
            occupancy: BTreeMap::new(),
            rng: RngStreams::from_seed(spec.seed),
            terrain_hash: 0,
            next_entity: 1,
            next_bot: 1,
        };
        world.terrain_hash = world.compute_terrain_hash();
        for &(pos, amount) in &spec.ore_nodes {
            let id = world.alloc_entity();
            world.ore_nodes.insert(id, OreNode { pos, amount });
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
        world.stockpile_ore = spec.starting_ore;
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
        self.ore_nodes
            .iter()
            .filter(|(_, n)| n.amount > 0)
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

    /// Position of a targetable entity (ore node, depot, printer, or bot).
    pub fn entity_pos(&self, id: EntityId) -> Option<TilePos> {
        self.ore_nodes
            .get(&id)
            .map(|n| n.pos)
            .or_else(|| self.depots.get(&id).map(|d| d.pos))
            .or_else(|| self.printers.get(&id).map(|p| p.pos))
            .or_else(|| self.blueprints.get(&id).map(|b| b.pos))
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
        let Some(bot) = self.bots.get_mut(&id) else { return };
        let from = bot.data.pos;
        bot.data.pos = to;
        self.unindex_bot(id, from);
        self.index_bot(id, to);
    }

    /// Structures are solid: bots can neither stand on nor path through
    /// printer or depot tiles.
    pub fn structure_at(&self, pos: TilePos) -> bool {
        self.printers.values().any(|p| p.pos == pos)
            || self.depots.values().any(|d| d.pos == pos)
    }

    /// All structure tiles, for feeding A*'s blocked set.
    pub fn structure_tiles(&self) -> BTreeSet<TilePos> {
        self.printers
            .values()
            .map(|p| p.pos)
            .chain(self.depots.values().map(|d| d.pos))
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
