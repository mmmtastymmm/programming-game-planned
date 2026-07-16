//! Tile grid, positions, and deterministic A* pathfinding.
//!
//! Determinism rules apply (CLAUDE.md): fixed neighbor order, total
//! tie-breaking on (f, g, y, x), BTree containers only.

use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
}

impl TilePos {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn manhattan(self, other: TilePos) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }

    pub fn chebyshev(self, other: TilePos) -> u32 {
        self.x.abs_diff(other.x).max(self.y.abs_diff(other.y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub fn delta(self) -> (i32, i32) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
        }
    }

    pub fn clockwise(self) -> Direction {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
        }
    }

    pub fn arrow(self) -> &'static str {
        match self {
            Direction::North => "↑",
            Direction::East => "→",
            Direction::South => "↓",
            Direction::West => "←",
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Direction::North => 0,
            Direction::East => 1,
            Direction::South => 2,
            Direction::West => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TileKind {
    Plains,
    Rubble,
    Water,
    /// Built over Water by terraforming (docs/05): ground-passable.
    Bridge,
    /// Bog (docs/05): slow going. (The loaded-hauler 4x surcharge and
    /// per-biome cost overlays are future work — flat 3x for now.)
    Mud,
    /// The compute-tax biome (docs/05). Walkable like plains; the cycle
    /// tax and channel jamming are future work — terrain kind first so
    /// maps can paint it and the renderer can show it.
    Corruption,
    /// Minable outcrop (docs/05): the vein an ore node sits in.
    OreVein,
    /// Minable Crystal ground (docs/05); mapgen puts it near Corruption.
    CrystalField,
    /// Plateau (docs/05): enter only via Ramp tiles. No ramps exist yet,
    /// so it is impassable for now — a natural wall you can see over.
    HighGround,
    /// Geothermal vent (docs/05): the only tile allowing a Geothermal
    /// Tap. Ordinary ground to walk on.
    Vent,
    /// Snowfield. Plains-cost for now; whether it slows, conceals, or
    /// holds tracks is open (docs/QUESTIONS Q67).
    Snow,
    /// Raw-resource terrains (docs/03: the eleven-raw split — Water is
    /// water tiles, Crystal is CrystalField, these are the other nine).
    /// Ground kinds only for now: plains-cost, no node/recipe wiring —
    /// the sim-side resource migration is open (docs/QUESTIONS Q69).
    Sand,
    StoneOutcrop,
    Grove,
    CoalSeam,
    IronVein,
    CopperVein,
    TinVein,
    SilverVein,
    GoldVein,
}

/// A traffic rule painted onto any tile — independent of terrain.
/// Arrows make the tile one-way (two opposing arrowed bridges = a
/// deadlock-free crossing; an arrowed corridor = a dedicated lane).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum OverlayKind {
    Arrow(Direction),
}

impl OverlayKind {
    pub fn as_u8(self) -> u8 {
        match self {
            OverlayKind::Arrow(d) => d.as_u8(),
        }
    }
}

impl TileKind {
    /// Ticks required to *enter* a tile of this kind; `None` = impassable.
    pub fn move_ticks(self) -> Option<u32> {
        match self {
            TileKind::Plains => Some(1),
            TileKind::Rubble => Some(2),
            TileKind::Water => None,
            TileKind::Bridge => Some(1),
            TileKind::Mud => Some(3),
            TileKind::Corruption => Some(1),
            TileKind::OreVein => Some(1),
            TileKind::CrystalField => Some(1),
            TileKind::HighGround => None,
            TileKind::Vent => Some(1),
            TileKind::Snow => Some(1),
            // Resource ground is cost-neutral until Q69 decides otherwise.
            TileKind::Sand
            | TileKind::StoneOutcrop
            | TileKind::Grove
            | TileKind::CoalSeam
            | TileKind::IronVein
            | TileKind::CopperVein
            | TileKind::TinVein
            | TileKind::SilverVein
            | TileKind::GoldVein => Some(1),
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            TileKind::Plains => 0,
            TileKind::Rubble => 1,
            TileKind::Water => 2,
            TileKind::Bridge => 3,
            TileKind::Mud => 4,
            TileKind::Corruption => 5,
            TileKind::OreVein => 6,
            TileKind::CrystalField => 7,
            TileKind::HighGround => 8,
            TileKind::Vent => 9,
            TileKind::Snow => 10,
            TileKind::Sand => 11,
            TileKind::StoneOutcrop => 12,
            TileKind::Grove => 13,
            TileKind::CoalSeam => 14,
            TileKind::IronVein => 15,
            TileKind::CopperVein => 16,
            TileKind::TinVein => 17,
            TileKind::SilverVein => 18,
            TileKind::GoldVein => 19,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Grid {
    pub width: i32,
    pub height: i32,
    tiles: Vec<TileKind>,
}

impl Grid {
    pub fn filled(width: i32, height: i32, kind: TileKind) -> Self {
        assert!(width > 0 && height > 0);
        Self { width, height, tiles: vec![kind; (width * height) as usize] }
    }

    pub fn in_bounds(&self, pos: TilePos) -> bool {
        pos.x >= 0 && pos.y >= 0 && pos.x < self.width && pos.y < self.height
    }

    pub fn get(&self, pos: TilePos) -> Option<TileKind> {
        if self.in_bounds(pos) {
            Some(self.tiles[(pos.y * self.width + pos.x) as usize])
        } else {
            None
        }
    }

    pub fn set(&mut self, pos: TilePos, kind: TileKind) {
        assert!(self.in_bounds(pos), "set out of bounds: {pos:?}");
        self.tiles[(pos.y * self.width + pos.x) as usize] = kind;
    }

    pub fn tiles(&self) -> &[TileKind] {
        &self.tiles
    }
}

/// Declarative map description; the replay format stores one of these plus
/// the command log (see [`crate::replay`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MapSpec {
    pub width: i32,
    pub height: i32,
    pub rubble: Vec<TilePos>,
    pub water: Vec<TilePos>,
    /// (position, ore amount)
    pub ore_nodes: Vec<(TilePos, u32)>,
    pub depots: Vec<TilePos>,
    pub printers: Vec<PrinterSpec>,
    /// Seed stockpile (per docs/03 the starting state includes a buffer).
    pub starting_ore: u64,
    /// Seed for the sim's RNG stream (dodge picks, later wander/combat).
    pub seed: u64,
    /// Pre-built bridge tiles (placed over water at world build).
    pub bridges: Vec<TilePos>,
    pub mud: Vec<TilePos>,
    pub corruption: Vec<TilePos>,
    pub ore_veins: Vec<TilePos>,
    pub crystal: Vec<TilePos>,
    pub high_ground: Vec<TilePos>,
    pub vents: Vec<TilePos>,
    pub snow: Vec<TilePos>,
    /// Raw-resource ground painted at build, as (pos, kind) pairs — one
    /// list for all nine kinds rather than nine vecs. Each carries a node
    /// of its tile's resource (docs/03; `node_amount` units each).
    pub resource_tiles: Vec<(TilePos, TileKind)>,
    /// Units per placed resource node (serde-defaulted so stored replays
    /// keep parsing; deci conversion happens at world build).
    #[serde(default = "default_node_amount")]
    pub node_amount: u32,
    /// Typed starting stock: (faction, kind, units). docs/03's opening kit
    /// is 30 Steel + 10 Iron + 5 Coal.
    #[serde(default)]
    pub starting_stock: Vec<(u8, crate::resources::Resource, u64)>,
    /// Parse deploys with every construct unlocked (dev/test sandboxes and
    /// every existing map — real matches set this false and Research
    /// buys the tree). Serde default TRUE keeps stored replays working.
    #[serde(default = "default_true")]
    pub dev_all_unlocks: bool,
    /// Skip energy/Steel upkeep entirely (dev/test sandboxes and every
    /// existing map — real maps set this false and run on Generators).
    /// Default TRUE for the same reason as dev_all_unlocks (M5).
    #[serde(default = "default_true")]
    pub dev_free_power: bool,
    /// Map-authored structures, placed free at world build. Generators
    /// placed here start STOKED (docs/03: the opening never brownouts
    /// before the player acts).
    #[serde(default)]
    pub structures: Vec<(TilePos, crate::world::StructureKind)>,
}

fn default_true() -> bool {
    true
}

fn default_node_amount() -> u32 {
    20
}

/// A printer placed by the map (docs/03: colonies start with a working
/// Green printer and a ruined Red one).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PrinterSpec {
    pub pos: TilePos,
    pub faction: u8,
    /// Color slot index (0 = Green, 1 = Red, ...).
    pub color: u8,
    pub ruined: bool,
    pub desired_max: u32,
}

impl MapSpec {
    pub fn empty(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            rubble: Vec::new(),
            water: Vec::new(),
            ore_nodes: Vec::new(),
            depots: Vec::new(),
            printers: Vec::new(),
            starting_ore: 0,
            seed: 0x5EED_0001,
            bridges: Vec::new(),
            mud: Vec::new(),
            corruption: Vec::new(),
            ore_veins: Vec::new(),
            crystal: Vec::new(),
            high_ground: Vec::new(),
            vents: Vec::new(),
            snow: Vec::new(),
            resource_tiles: Vec::new(),
            node_amount: default_node_amount(),
            starting_stock: Vec::new(),
            dev_all_unlocks: true,
            dev_free_power: true,
            structures: Vec::new(),
        }
    }
}

/// May a bot step from `from` onto adjacent `to`? Tile passability plus
/// arrow-overlay constraints on either end (you can neither enter an
/// arrowed tile against its arrow nor leave one against it).
pub fn edge_allowed(
    grid: &Grid,
    overlays: &BTreeMap<TilePos, OverlayKind>,
    from: TilePos,
    to: TilePos,
) -> bool {
    let Some(to_kind) = grid.get(to) else { return false };
    if to_kind.move_ticks().is_none() {
        return false;
    }
    let delta = (to.x - from.x, to.y - from.y);
    if let Some(OverlayKind::Arrow(d)) = overlays.get(&to)
        && delta != d.delta()
    {
        return false;
    }
    if let Some(OverlayKind::Arrow(d)) = overlays.get(&from)
        && delta != d.delta()
    {
        return false;
    }
    true
}

/// Deterministic A*. Returns the path as the sequence of tiles to *enter*
/// (start excluded, goal included). `None` if unreachable. An empty path
/// means the start already satisfies a goal.
pub fn astar(
    grid: &Grid,
    overlays: &BTreeMap<TilePos, OverlayKind>,
    start: TilePos,
    goals: &BTreeSet<TilePos>,
) -> Option<Vec<TilePos>> {
    astar_avoiding(grid, overlays, start, goals, &BTreeSet::new())
}

/// A* that additionally refuses to enter `blocked` tiles (used for bump
/// re-planning: other bots' current positions are obstacles).
pub fn astar_avoiding(
    grid: &Grid,
    overlays: &BTreeMap<TilePos, OverlayKind>,
    start: TilePos,
    goals: &BTreeSet<TilePos>,
    blocked: &BTreeSet<TilePos>,
) -> Option<Vec<TilePos>> {
    if goals.contains(&start) {
        return Some(Vec::new());
    }
    let h = |p: TilePos| goals.iter().map(|g| p.manhattan(*g)).min().unwrap_or(u32::MAX);

    // Reverse ordering on (f, g, y, x): BinaryHeap is a max-heap, so wrap in
    // Reverse for min-first with a total, deterministic order.
    use std::cmp::Reverse;
    let mut open: BinaryHeap<Reverse<(u32, u32, i32, i32)>> = BinaryHeap::new();
    let mut g_score: BTreeMap<TilePos, u32> = BTreeMap::new();
    let mut parent: BTreeMap<TilePos, TilePos> = BTreeMap::new();

    g_score.insert(start, 0);
    open.push(Reverse((h(start), 0, start.y, start.x)));

    while let Some(Reverse((_, g, y, x))) = open.pop() {
        let pos = TilePos::new(x, y);
        if g > *g_score.get(&pos).unwrap_or(&u32::MAX) {
            continue; // stale entry
        }
        if goals.contains(&pos) {
            // Reconstruct.
            let mut path = vec![pos];
            let mut cur = pos;
            while let Some(&p) = parent.get(&cur) {
                if p == start {
                    break;
                }
                path.push(p);
                cur = p;
            }
            path.reverse();
            return Some(path);
        }
        // Fixed neighbor order: N, E, S, W.
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let next = TilePos::new(pos.x + dx, pos.y + dy);
            if blocked.contains(&next) {
                continue;
            }
            if !edge_allowed(grid, overlays, pos, next) {
                continue;
            }
            let step_cost = grid.get(next).and_then(|k| k.move_ticks()).expect("edge checked");
            let ng = g + step_cost;
            if ng < *g_score.get(&next).unwrap_or(&u32::MAX) {
                g_score.insert(next, ng);
                parent.insert(next, pos);
                open.push(Reverse((ng + h(next), ng, next.y, next.x)));
            }
        }
    }
    None
}
