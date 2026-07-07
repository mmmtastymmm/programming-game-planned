//! World state: entities, bots, wrecks, black boxes, the colony stockpile.
//! Everything lives in BTree containers with stable IDs (determinism).

use crate::map::{Grid, MapSpec, TileKind, TilePos};
use pyrite::Vm;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BotId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityId(pub u64);

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
}

/// An in-flight world action.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// `path[0]` is the next tile to enter; `ticks_left` is the remaining
    /// cost of entering it (Rubble takes 2, Plains 1).
    Move { path: Vec<TilePos>, ticks_left: u32 },
    Mine { node: EntityId, ticks_left: u32 },
    Deposit { depot: EntityId, ticks_left: u32 },
}

pub const LOG_BUFFER_CAP: usize = 8;

#[derive(Debug)]
pub struct BotData {
    pub id: BotId,
    pub pos: TilePos,
    pub cargo: u32,
    pub cargo_cap: u32,
    /// Cycles granted per tick (CPU hardware).
    pub cpu: u64,
    pub requested: Option<ActionRequest>,
    pub action: Option<Action>,
    /// Set by the forced `become_disabled()`; the death phase wrecks the bot.
    pub dying: bool,
    /// Local log ring buffer (base 8 entries; hardware stat later).
    pub log_buf: Vec<String>,
    pub xp_mining: u64,
    pub xp_hauling: u64,
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

/// The colony Log Archive: crash dumps and uploaded logs.
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
        world
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

    /// Position of a targetable entity (ore node or depot).
    pub fn entity_pos(&self, id: EntityId) -> Option<TilePos> {
        self.ore_nodes
            .get(&id)
            .map(|n| n.pos)
            .or_else(|| self.depots.get(&id).map(|d| d.pos))
    }
}
