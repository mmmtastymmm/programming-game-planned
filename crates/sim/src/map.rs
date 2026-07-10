//! Tile grid, positions, and deterministic A* pathfinding.
//!
//! Determinism rules apply (CLAUDE.md): fixed neighbor order, total
//! tie-breaking on (f, g, y, x), BTree containers only.

use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    Plains,
    Rubble,
    Water,
    /// Built over Water by terraforming (docs/05): ground-passable.
    Bridge,
}

impl TileKind {
    /// Ticks required to *enter* a tile of this kind; `None` = impassable.
    pub fn move_ticks(self) -> Option<u32> {
        match self {
            TileKind::Plains => Some(1),
            TileKind::Rubble => Some(2),
            TileKind::Water => None,
            TileKind::Bridge => Some(1),
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            TileKind::Plains => 0,
            TileKind::Rubble => 1,
            TileKind::Water => 2,
            TileKind::Bridge => 3,
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
/// the command log.
#[derive(Debug, Clone)]
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
}

/// A printer placed by the map (docs/03: colonies start with a working
/// Green printer and a ruined Red one).
#[derive(Debug, Clone)]
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
        }
    }
}

/// Deterministic A*. Returns the path as the sequence of tiles to *enter*
/// (start excluded, goal included). `None` if unreachable. An empty path
/// means the start already satisfies a goal.
pub fn astar(grid: &Grid, start: TilePos, goals: &BTreeSet<TilePos>) -> Option<Vec<TilePos>> {
    astar_avoiding(grid, start, goals, &BTreeSet::new())
}

/// A* that additionally refuses to enter `blocked` tiles (used for bump
/// re-planning: other bots' current positions are obstacles).
pub fn astar_avoiding(
    grid: &Grid,
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
            let Some(kind) = grid.get(next) else { continue };
            let Some(step_cost) = kind.move_ticks() else { continue };
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
