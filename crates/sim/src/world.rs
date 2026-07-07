//! World state: entities, bots, wrecks, black boxes, the colony stockpile.
//! Everything lives in BTree containers with stable IDs (determinism).

use crate::map::{Grid, MapSpec, TileKind, TilePos};
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
}

/// An in-flight world action.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// `path[0]` is the next tile to enter; `ticks_left` is the remaining
    /// cost of entering it (Rubble takes 2, Plains 1).
    Move { path: Vec<TilePos>, ticks_left: u32 },
    Mine { node: EntityId, ticks_left: u32 },
    Deposit { depot: EntityId, ticks_left: u32 },
    Attack { target: EntityId, ticks_left: u32 },
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
    /// Set by the forced `become_disabled()`; the death phase wrecks the bot.
    pub dying: bool,
    /// Local log ring buffer (base 8 entries; hardware stat later).
    pub log_buf: Vec<String>,
    pub xp_mining: u64,
    pub xp_hauling: u64,
    pub xp_combat: u64,
}

#[derive(Debug)]
pub struct Bot {
    pub data: BotData,
    /// Taken out while running (borrow discipline); always `Some` between
    /// phases.
    pub vm: Option<Vm>,
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
    /// Deployed program per (faction, color slot).
    pub color_programs: BTreeMap<(u8, u8), ColorProgram>,
    pub wrecks: BTreeMap<BotId, Wreck>,
    pub black_boxes: Vec<BlackBox>,
    pub stockpile_ore: u64,
    pub archive: Vec<ArchiveEntry>,
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
        let mut world = Self {
            tick: 0,
            grid,
            ore_nodes: BTreeMap::new(),
            depots: BTreeMap::new(),
            bots: BTreeMap::new(),
            bot_entities: BTreeMap::new(),
            printers: BTreeMap::new(),
            color_programs: BTreeMap::new(),
            wrecks: BTreeMap::new(),
            black_boxes: Vec::new(),
            stockpile_ore: 0,
            archive: Vec::new(),
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
            .or_else(|| {
                self.bot_entities
                    .get(&id)
                    .and_then(|bid| self.bots.get(bid))
                    .map(|b| b.data.pos)
            })
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
