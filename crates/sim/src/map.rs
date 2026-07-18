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
    // --- terrain v2 (M8, docs/05) ---
    /// Edge-costed range: climbing on is dear, descending moderate,
    /// ridge-running cheap; summits carry the High Ground sensor state.
    Mountain,
    /// The only doorway onto High Ground (docs/05: "enter only via Ramp").
    Ramp,
    /// Idling sinks (Q35): stand still and the exit cost escalates.
    Dunes,
    /// Deterministic slides (Q37): entering continues the move in the
    /// same direction until non-ice; arrows redirect; bumps end it.
    Ice,
    /// Mapgen shallow crossing (Q38): slow, and wading quiets signature.
    Ford,
    /// Terraformed artery (Q39): HALF plains cost — the ×2 scale exists
    /// for this tile.
    Road,
    /// Collapses to Rubble after N crossings (Q40 per-tile wear).
    Scree,
    /// Terraformed wall: blocks movement AND vision (docs/05).
    Barricade,
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
            TileKind::Mountain => 20,
            TileKind::Ramp => 21,
            TileKind::Dunes => 22,
            TileKind::Ice => 23,
            TileKind::Ford => 24,
            TileKind::Road => 25,
            TileKind::Scree => 26,
            TileKind::Barricade => 27,
        }
    }

    /// Can ground bots ever stand here? (Costs live in tuning's ×2 table;
    /// High Ground is passable but Ramp-gated — see `edge_allowed`.)
    /// Every kind, for exhaustive data validation (tuning's cost table
    /// must price exactly the passable set — Tuning::validate).
    pub const ALL: [TileKind; 28] = [
        TileKind::Plains,
        TileKind::Rubble,
        TileKind::Water,
        TileKind::Bridge,
        TileKind::Mud,
        TileKind::Corruption,
        TileKind::OreVein,
        TileKind::CrystalField,
        TileKind::HighGround,
        TileKind::Vent,
        TileKind::Snow,
        TileKind::Sand,
        TileKind::StoneOutcrop,
        TileKind::Grove,
        TileKind::CoalSeam,
        TileKind::IronVein,
        TileKind::CopperVein,
        TileKind::TinVein,
        TileKind::SilverVein,
        TileKind::GoldVein,
        TileKind::Mountain,
        TileKind::Ramp,
        TileKind::Dunes,
        TileKind::Ice,
        TileKind::Ford,
        TileKind::Road,
        TileKind::Scree,
        TileKind::Barricade,
    ];

    /// May things MATERIALIZE here — prints, recolors, dev spawns,
    /// structure placement, cargo spills? Passable ground that is not
    /// elevation-gated: High Ground is reached only by climbing a Ramp
    /// (docs/05), so nothing may pop into existence on the plateau from
    /// ground level. (Standing there after a legitimate climb is fine —
    /// this gates appearance, not presence.)
    pub fn spawnable(self) -> bool {
        self.passable() && self != TileKind::HighGround
    }

    pub fn passable(self) -> bool {
        !matches!(self, TileKind::Water | TileKind::Barricade)
    }
}

/// The ×2-scale move-cost table (Q39: Plains 2 so Road's ½× exists) plus
/// the edge parameters — resolved once from tuning at Sim construction.
/// PLANNING costs (A*) are bot-independent; per-bot state (loaded Mud,
/// Dune sink) rides `stats::step_ticks` on top.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TileCostTable {
    /// (kind, entry cost ×2). Kinds absent here are impassable.
    pub x2: Vec<(TileKind, u32)>,
    /// Entering a Mountain from non-mountain (the climb).
    pub mountain_climb_x2: u32,
    /// Leaving a Mountain onto ordinary ground (the descent).
    pub mountain_descend_x2: u32,
    /// Mud while carrying anything (docs/05: 3×, loaded 4×).
    pub mud_loaded_x2: u32,
}

impl TileCostTable {
    pub fn cost_x2(&self, kind: TileKind) -> Option<u32> {
        self.x2.iter().find(|(k, _)| *k == kind).map(|(_, c)| *c)
    }

    /// The bot-independent cost (×2) of entering `to` from `from` —
    /// Mountain edges replace the flat entry cost (docs/05 Q36).
    pub fn edge_cost_x2(&self, grid: &Grid, from: TilePos, to: TilePos) -> Option<u32> {
        let to_kind = grid.get(to)?;
        let from_kind = grid.get(from)?;
        Some(match (from_kind, to_kind) {
            (TileKind::Mountain, TileKind::Mountain) => self.cost_x2(TileKind::Mountain)?,
            (_, TileKind::Mountain) => self.mountain_climb_x2,
            (TileKind::Mountain, _) => self.mountain_descend_x2.max(self.cost_x2(to_kind)?),
            _ => self.cost_x2(to_kind)?,
        })
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

    /// Is this tile Corruption? The radio jam (M11) and the creep front
    /// both key on it — one predicate so "the static" is defined once.
    pub fn is_corruption(&self, pos: TilePos) -> bool {
        self.get(pos) == Some(TileKind::Corruption)
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
    /// (position, owning faction) — depots see/hear for their owner (Q89).
    pub depots: Vec<(TilePos, u8)>,
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
    /// Feral nests (M12, docs/04): (position, arcanum 0–21). Nests above
    /// the match's `max_arcanum` simply don't spawn.
    #[serde(default)]
    pub nests: Vec<(TilePos, u8)>,
    /// Match option (docs/04): the deepest arcanum this map spawns.
    /// Raising it deepens the frontier, never the neighborhood.
    #[serde(default = "default_max_arcanum")]
    pub max_arcanum: u8,
    /// The rest of the match-settings inventory (M13, docs/08 Q77).
    #[serde(default)]
    pub settings: MatchSettings,
    /// DEV KNOB (M9): override printers.ron's per-printer fleet-cap
    /// contribution for this map — tests and demo scenes need small,
    /// predictable populations, and the replay format carries only
    /// (spec, commands). Real matches leave it None.
    #[serde(default)]
    pub fleet_cap_override: Option<u32>,
    /// Blight Cores (M8-C, docs/05): (pos, spread radius, hp). The core's
    /// own tile is painted Corruption at build. Serde-defaulted so stored
    /// replays keep parsing.
    #[serde(default)]
    pub blight_cores: Vec<(TilePos, u32, i64)>,
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
    /// Expected quirks per bot in PER-MILLE (docs/09 match setting:
    /// 500 = 0.5 quirks/bot; 0 = quirks off; up to 2000 = both latent
    /// slots certain). Integer so the sim stays float-free.
    #[serde(default = "default_quirk_permille")]
    pub quirk_permille: u32,
}

fn default_quirk_permille() -> u32 {
    500
}

fn default_true() -> bool {
    true
}

fn default_node_amount() -> u32 {
    20
}

fn default_max_arcanum() -> u8 {
    21
}

/// The server harm setting (M13, docs/08 Modes): Open lets players ally,
/// trade, raid, or war freely; Non-PvP removes direct HARM (damage,
/// salvage/analyze/hijack of other players' property) but never indirect
/// competition; Duel is the stretch mirror-map mode (harm on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HarmMode {
    Open,
    NonPvp,
    Duel,
}

/// The match-settings inventory (M13, docs/08 Q77): every dial a match is
/// configured with, lockstep-shared, fixed at match start. Two dials from
/// the inventory live directly on MapSpec for historical reasons —
/// `quirk_permille` (quirk probability) and `max_arcanum` — and the
/// Ferals toggle, print cost, and decryption % land here. `None` on an
/// override means "use tuning.ron's figure".
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MatchSettings {
    pub harm: HarmMode,
    /// Ferals on/off ("pure" PvP turns them off) — docs/04.
    pub ferals: bool,
    /// Print cost override (docs/02: the build receipt is literal —
    /// refunds scale with what was actually spent).
    pub print_cost_steel: Option<u64>,
    /// Salvage decryption % override (docs/08, default 5).
    pub salvage_decrypt_pct: Option<u32>,
    /// Sim-speed vote plumbing (docs/08: unanimous consent, cooldown per
    /// attempt — no vote spam).
    pub vote_cooldown_ticks: u64,
    /// How long an open proposal waits for unanimity before it fails.
    pub vote_window_ticks: u64,
}

impl Default for MatchSettings {
    fn default() -> Self {
        Self {
            harm: HarmMode::Open,
            ferals: true,
            print_cost_steel: None,
            salvage_decrypt_pct: None,
            vote_cooldown_ticks: 300,
            vote_window_ticks: 100,
        }
    }
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
}

/// Why a `MapSpec` failed [`MapSpec::validate`]. Every variant names the
/// offending position so a generator bug points at the tile, not "somewhere".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapSpecError {
    /// width or height non-positive.
    BadDimensions { width: i32, height: i32 },
    /// A referenced tile lies outside the grid.
    OutOfBounds { what: &'static str, pos: TilePos },
    /// A printer / depot / structure was placed where nothing may
    /// materialize (Water, Barricade, or a Ramp-gated plateau).
    NotSpawnable { what: &'static str, pos: TilePos, kind: TileKind },
    /// A resource node sits on ground that yields no resource.
    NodeOnBareGround { pos: TilePos, kind: TileKind },
    /// Two printers claim the same tile.
    DuplicatePrinter { pos: TilePos },
}

impl MapSpec {
    /// Paint the terrain grid this spec describes — the single source of
    /// truth shared by [`crate::World::from_spec`] and [`Self::validate`]
    /// (painting order is load-bearing: later kinds overwrite earlier, e.g.
    /// a Mud tile drawn over the water strip). Assumes in-bounds positions;
    /// `validate` bounds-checks first, so only pre-validated specs reach the
    /// `set` calls here.
    pub fn paint_grid(&self) -> Grid {
        let mut grid = Grid::filled(self.width, self.height, TileKind::Plains);
        for &pos in &self.rubble {
            grid.set(pos, TileKind::Rubble);
        }
        for &pos in &self.water {
            grid.set(pos, TileKind::Water);
        }
        for &pos in &self.bridges {
            grid.set(pos, TileKind::Bridge);
        }
        for (tiles, kind) in [
            (&self.mud, TileKind::Mud),
            (&self.corruption, TileKind::Corruption),
            (&self.ore_veins, TileKind::OreVein),
            (&self.crystal, TileKind::CrystalField),
            (&self.high_ground, TileKind::HighGround),
            (&self.vents, TileKind::Vent),
            (&self.snow, TileKind::Snow),
        ] {
            for &pos in tiles {
                grid.set(pos, kind);
            }
        }
        for &(pos, kind) in &self.resource_tiles {
            grid.set(pos, kind);
        }
        // Blight Cores squat on corrupted ground from tick 0 (from_spec
        // paints these too, after entity allocation).
        for &(pos, _, _) in &self.blight_cores {
            grid.set(pos, TileKind::Corruption);
        }
        grid
    }

    /// Structural sanity for a spec before it builds a world: bounds, no
    /// fatal placements (a printer in a lake, a node on bare plains), no
    /// duplicate printers. This is the *authoring* floor — the emergent
    /// playability floor (walkable kit, reachable shoreline) lives in
    /// [`crate::mapgen`], which validates the finished layout by flood-fill.
    /// Returns the FIRST problem found (in a deterministic scan order).
    pub fn validate(&self) -> Result<(), MapSpecError> {
        self.validate_grid().map(|_| ())
    }

    /// [`Self::validate`] that also hands back the painted [`Grid`] on
    /// success, so a caller that needs the grid anyway (the generator's
    /// floor check) validates and paints in a single pass instead of
    /// repainting the same spec two or three times.
    pub fn validate_grid(&self) -> Result<Grid, MapSpecError> {
        if self.width <= 0 || self.height <= 0 {
            return Err(MapSpecError::BadDimensions { width: self.width, height: self.height });
        }
        let in_bounds = |p: TilePos| p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height;

        // 1. Every referenced position must be on the grid. Scanned in a
        //    fixed field order so the reported error is deterministic.
        let point_lists: [(&'static str, &[TilePos]); 10] = [
            ("rubble", &self.rubble),
            ("water", &self.water),
            ("bridges", &self.bridges),
            ("mud", &self.mud),
            ("corruption", &self.corruption),
            ("ore_veins", &self.ore_veins),
            ("crystal", &self.crystal),
            ("high_ground", &self.high_ground),
            ("vents", &self.vents),
            ("snow", &self.snow),
        ];
        for (what, list) in point_lists {
            for &pos in list {
                if !in_bounds(pos) {
                    return Err(MapSpecError::OutOfBounds { what, pos });
                }
            }
        }
        for &(pos, _) in &self.resource_tiles {
            if !in_bounds(pos) {
                return Err(MapSpecError::OutOfBounds { what: "resource_tiles", pos });
            }
        }
        for &(pos, _) in &self.ore_nodes {
            if !in_bounds(pos) {
                return Err(MapSpecError::OutOfBounds { what: "ore_nodes", pos });
            }
        }
        for &(pos, _) in &self.depots {
            if !in_bounds(pos) {
                return Err(MapSpecError::OutOfBounds { what: "depots", pos });
            }
        }
        for p in &self.printers {
            if !in_bounds(p.pos) {
                return Err(MapSpecError::OutOfBounds { what: "printers", pos: p.pos });
            }
        }
        for &(pos, _) in &self.structures {
            if !in_bounds(pos) {
                return Err(MapSpecError::OutOfBounds { what: "structures", pos });
            }
        }
        for &(pos, _) in &self.nests {
            if !in_bounds(pos) {
                return Err(MapSpecError::OutOfBounds { what: "nests", pos });
            }
        }
        for &(pos, _, _) in &self.blight_cores {
            if !in_bounds(pos) {
                return Err(MapSpecError::OutOfBounds { what: "blight_cores", pos });
            }
        }

        // 2. With bounds guaranteed, paint the grid and check placements.
        let grid = self.paint_grid();
        for &(pos, _) in &self.resource_tiles {
            let painted = grid.get(pos).expect("in-bounds");
            if crate::resources::Resource::for_tile(painted).is_none() {
                return Err(MapSpecError::NodeOnBareGround { pos, kind: painted });
            }
        }
        let mut printer_seen: BTreeSet<TilePos> = BTreeSet::new();
        for p in &self.printers {
            let kind = grid.get(p.pos).expect("in-bounds");
            if !kind.spawnable() {
                return Err(MapSpecError::NotSpawnable { what: "printer", pos: p.pos, kind });
            }
            if !printer_seen.insert(p.pos) {
                return Err(MapSpecError::DuplicatePrinter { pos: p.pos });
            }
        }
        for &(pos, _) in &self.depots {
            let kind = grid.get(pos).expect("in-bounds");
            if !kind.spawnable() {
                return Err(MapSpecError::NotSpawnable { what: "depot", pos, kind });
            }
        }
        for &(pos, _) in &self.structures {
            let kind = grid.get(pos).expect("in-bounds");
            if !kind.spawnable() {
                return Err(MapSpecError::NotSpawnable { what: "structure", pos, kind });
            }
        }
        Ok(grid)
    }

    pub fn empty(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            rubble: Vec::new(),
            water: Vec::new(),
            ore_nodes: Vec::new(),
            depots: Vec::new(),
            printers: Vec::new(),
            fleet_cap_override: None,
            blight_cores: Vec::new(),
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
            quirk_permille: default_quirk_permille(),
            nests: Vec::new(),
            max_arcanum: default_max_arcanum(),
            settings: MatchSettings::default(),
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
    if !to_kind.passable() {
        return false;
    }
    let from_kind = grid.get(from);
    // High Ground is entered and left ONLY via Ramp (or along the
    // plateau); Mountain summits are the soft-slope sibling — climbable
    // from anywhere at edge cost (docs/05).
    if to_kind == TileKind::HighGround
        && !matches!(
            from_kind,
            Some(TileKind::Ramp) | Some(TileKind::HighGround) | Some(TileKind::Mountain)
        )
    {
        return false;
    }
    if from_kind == Some(TileKind::HighGround)
        && !matches!(
            to_kind,
            TileKind::Ramp | TileKind::HighGround | TileKind::Mountain
        )
    {
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
    costs: &TileCostTable,
    start: TilePos,
    goals: &BTreeSet<TilePos>,
) -> Option<Vec<TilePos>> {
    astar_avoiding(grid, overlays, costs, start, goals, &BTreeSet::new())
}

/// A* that additionally refuses to enter `blocked` tiles (used for bump
/// re-planning: other bots' current positions are obstacles).
pub fn astar_avoiding(
    grid: &Grid,
    overlays: &BTreeMap<TilePos, OverlayKind>,
    costs: &TileCostTable,
    start: TilePos,
    goals: &BTreeSet<TilePos>,
    blocked: &BTreeSet<TilePos>,
) -> Option<Vec<TilePos>> {
    if goals.contains(&start) {
        return Some(Vec::new());
    }
    // Admissible under the ×2 table: the cheapest edge anywhere is
    // Road's 1, so plain manhattan (×1) never overestimates.
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
            let Some(step_cost) = costs.edge_cost_x2(grid, pos, next) else { continue };
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
