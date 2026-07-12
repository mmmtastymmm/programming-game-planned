//! World state: entities, bots, wrecks, black boxes, the colony stockpile.
//! Everything lives in BTree containers with stable IDs (determinism).

use crate::map::{Grid, MapSpec, OverlayKind, TileKind, TilePos};
use std::collections::BTreeSet;
use pyrite::ast::Program;
use pyrite::Vm;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BotId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityId(pub u64);

/// A program color slot (docs/01 "Program Colors"). One color = one printer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    pub version: u32,
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

    /// Name of the signal currently being handled, if any.
    pub fn handler_name(&self) -> Option<&'static str> {
        use pyrite::ast::SignalKind;
        match self.vm.as_ref()?.phase() {
            pyrite::Phase::Main => None,
            pyrite::Phase::Handler(kind) => Some(match kind {
                SignalKind::Error => "error",
                SignalKind::Hurt => "hurt",
                SignalKind::Death => "death",
                SignalKind::Bump => "bump",
                SignalKind::Bumped => "bumped",
            }),
        }
    }

    /// Is the running handler an engine default?
    pub fn in_default_handler(&self) -> bool {
        self.vm.as_ref().is_some_and(|vm| vm.handler_is_default())
    }

    /// Engine-default handler source for a signal name, if installed.
    pub fn default_handler_source(&self, signal: &str) -> Option<&str> {
        use pyrite::ast::SignalKind;
        let kind = match signal {
            "error" => SignalKind::Error,
            "hurt" => SignalKind::Hurt,
            "death" => SignalKind::Death,
            "bump" => SignalKind::Bump,
            "bumped" => SignalKind::Bumped,
            _ => return None,
        };
        self.vm.as_ref().and_then(|vm| vm.default_handler(kind)).map(|d| d.source.as_str())
    }

    /// (signal name, handler's source line if the program installed one) —
    /// for every player-facing signal, inspector-ready.
    pub fn handler_summary(&self) -> [(&'static str, Option<u32>); 5] {
        use pyrite::ast::SignalKind;
        let line = |kind| self.vm.as_ref().and_then(|vm| vm.handler_line(kind));
        [
            ("error", line(SignalKind::Error)),
            ("hurt", line(SignalKind::Hurt)),
            ("death", line(SignalKind::Death)),
            ("bump", line(SignalKind::Bump)),
            ("bumped", line(SignalKind::Bumped)),
        ]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub wrecks: BTreeMap<BotId, Wreck>,
    pub black_boxes: Vec<BlackBox>,
    pub stockpile_ore: u64,
    pub archive: Vec<ArchiveEntry>,
    /// SplitMix64 state — the sim's only randomness source (CLAUDE.md).
    pub rng_state: u64,
    next_entity: u64,
    next_bot: u32,
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
            wrecks: BTreeMap::new(),
            black_boxes: Vec::new(),
            stockpile_ore: 0,
            archive: Vec::new(),
            rng_state: spec.seed,
            next_entity: 1,
            next_bot: 1,
        };
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

    /// Deterministic SplitMix64. Advanced only by sim systems, in tick
    /// order — never from rendering or UI.
    pub fn next_rand(&mut self) -> u64 {
        self.rng_state = self.rng_state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
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

    /// Is a living bot (other than `exclude`) standing on `pos`?
    pub fn tile_occupied(&self, pos: TilePos, exclude: BotId) -> bool {
        self.bots
            .values()
            .any(|b| b.data.id != exclude && !b.data.dying && b.data.pos == pos)
    }

    /// First free, passable tile at/around `center`, in a fixed
    /// deterministic order. Used for print/re-color placement.
    pub fn free_spawn_tile(&self, center: TilePos) -> Option<TilePos> {
        const ORDER: [(i32, i32); 9] =
            [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];
        ORDER.iter().map(|(dx, dy)| TilePos::new(center.x + dx, center.y + dy)).find(|&p| {
            self.grid.get(p).is_some_and(|t| t.move_ticks().is_some())
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
