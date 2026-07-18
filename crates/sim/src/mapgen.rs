//! Procedural map generation (docs/05 *Map Generation*, answers Q71).
//!
//! A **deterministic, seeded, integer-only, setup-time** producer:
//! [`generate`] turns a [`MapgenConfig`] + a match seed into a concrete
//! [`MapSpec`], run ONCE when a match is created — never in the tick loop,
//! never in the phase-9 state hash. Because the *output* (a `MapSpec`) is
//! what ships and what the replay stores, a match is both seed-reproducible
//! across machines AND replay-proof against future changes to this code.
//!
//! Determinism contract (CLAUDE.md): no floats, no wall clock, all
//! randomness from the `mapgen` stream via [`crate::world::next_rand`], and
//! every collection iterated in a sorted/BTree order.
//!
//! Pipeline: **skeleton → fill → validate → regenerate**.
//! 1. *Skeleton* places every hard guarantee by construction (start-zone
//!    kit, center-out tier bands, Crystal-beside-Corruption, nest rings).
//! 2. *Fill* paints decorative biome variety from integer value-noise,
//!    never overwriting a placed guarantee.
//! 3. *Validate* flood-fills the finished layout and checks the playability
//!    floor; on failure the next sub-seed is tried, capped at `retry_cap`.
//!
//! v1 is **co-op-first**: asymmetric rim starts, richness/danger rising to a
//! shared contested center. PvP rotational symmetry is designed but deferred.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::hash::Fnv1a;
use crate::map::{Grid, MapSpec, PrinterSpec, TileKind, TilePos};
use crate::resources::Resource;
use crate::world::{next_rand, stream_seed, StructureKind};

/// Orthogonal neighbor offsets (N, E, S, W) — the connectivity the reserved
/// corridors thicken to and the floor floods over. One const so a change to
/// the movement model (e.g. 8-connectivity) touches a single place.
const NEIGHBORS4: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Minimum tiles between adjacent start zones on the rim ring: the start
/// footprint (a ±2 disc, 5 wide) plus a gap, so neighbours never collide.
/// Caps how many players a given map can seat (see [`max_supported_players`]).
const MIN_START_SPACING: i32 = 7;

/// Per-band decorative fill: what fraction of the band's open tiles gets
/// painted, and which biomes (weighted) it draws from.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct BandFill {
    /// Share of open tiles painted, in per-mille (0 = none, 1000 = all).
    pub fill_permille: u32,
    /// Weighted biome palette. Only *passable* kinds in the rim/mid bands
    /// (impassables there could seal a start); Water/High Ground are
    /// allowed in the deep band only, inward of every guarantee.
    pub biomes: Vec<(TileKind, u32)>,
}

/// Every figure the generator draws on — band geometry, densities, the
/// retry cap, scaling. Data, per the doc convention (`data/mapgen.ron`).
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct MapgenConfig {
    /// Square map side for a 1-player map.
    pub base_size: i32,
    /// Extra side length per additional player (keeps opening pace even).
    pub size_per_player: i32,
    /// Hard ceiling on map side.
    pub max_size: i32,
    /// Deep-field outer radius, as a percent of the half-size (Chebyshev).
    pub deep_pct: i32,
    /// Midfield outer radius, percent of half-size. Must exceed `deep_pct`.
    pub mid_pct: i32,
    /// Where start zones sit, percent of half-size. Must exceed `mid_pct`
    /// (starts live in the rim band, outside the midfield).
    pub start_ring_pct: i32,
    /// A guaranteed ore-family vein sits within this Chebyshev distance of
    /// each printer (base sensor range, docs/02) so tick-1 `closest(ore)`
    /// answers.
    pub start_vein_sight: i32,
    /// Units on every placed resource node.
    pub node_amount: u32,
    /// Max regeneration attempts before the config is declared too dense.
    pub retry_cap: u32,
    /// Coarse cell size for the value-noise (bigger = larger biome blobs).
    pub noise_cell: i32,
    pub rim_fill: BandFill,
    pub mid_fill: BandFill,
    pub deep_fill: BandFill,
    /// Blight-Core spread radius and hp (deep-field Corruption sources).
    pub blight_radius: u32,
    pub blight_hp: i64,
    /// Deepest arcanum this generator seeds (also written to the spec's
    /// `max_arcanum` frontier gate).
    pub nest_max_arcanum: u8,
    /// Per-faction opening stock, as (resource, whole units) — deci
    /// conversion happens at world build.
    pub starting_stock: Vec<(Resource, u64)>,
    /// v1 generated maps run as dev sandboxes (free power, all constructs
    /// unlocked) so they are immediately steppable; production wiring flips
    /// these once the economy is bootstrapped from generated Generators.
    pub dev_all_unlocks: bool,
    pub dev_free_power: bool,
}

impl Default for MapgenConfig {
    fn default() -> Self {
        let cfg: MapgenConfig = ron::from_str(include_str!("../data/mapgen.ron"))
            .expect("data/mapgen.ron parses (unknown fields are errors)");
        cfg.validate();
        cfg
    }
}

impl MapgenConfig {
    fn validate(&self) {
        assert!(self.base_size > 0, "mapgen: base_size must be > 0");
        assert!(self.max_size >= self.base_size, "mapgen: max_size < base_size");
        assert!(
            0 < self.deep_pct && self.deep_pct < self.mid_pct && self.mid_pct < self.start_ring_pct,
            "mapgen: require 0 < deep_pct < mid_pct < start_ring_pct (got {}/{}/{})",
            self.deep_pct,
            self.mid_pct,
            self.start_ring_pct
        );
        assert!(self.start_ring_pct < 100, "mapgen: start_ring_pct must leave rim margin");
        assert!(self.noise_cell > 0, "mapgen: noise_cell must be > 0");
        assert!(self.retry_cap > 0, "mapgen: retry_cap must be > 0");
        assert!(self.start_vein_sight > 0, "mapgen: start_vein_sight must be > 0");
    }
}

/// The single public entry point. Deterministic in `(config, seed,
/// players)`: the same inputs yield a byte-identical `MapSpec` on any
/// machine. `players` is clamped to `1..=max_supported_players` — a count too
/// large to seat on the rim degrades to a full map, never a startup panic.
///
/// Panics only if `retry_cap` candidate layouts all fail the playability
/// floor — a config bug (bands too dense), surfaced loudly rather than
/// shipping a soft-locked map.
pub fn generate(config: &MapgenConfig, seed: u64, players: u32) -> MapSpec {
    let players = players.clamp(1, max_supported_players(config));
    // The base mapgen stream: every attempt's sub-seed derives from it, so
    // `seed S` deterministically resolves to the first passing candidate.
    let base = stream_seed(seed, "mapgen");
    for attempt in 0..config.retry_cap {
        // Fold the retry counter into the seed (a fresh named sub-stream
        // per attempt — decorrelated from its neighbors).
        let s = mix(base, attempt as u64);
        let spec = build_candidate(config, s, players);
        // Paint the terrain ONCE and share it between the authoring floor (a
        // generator bug is a hard error, not a retry) and the emergent
        // playability floor. World-build repaints later — unavoidable.
        let grid = spec.validate_grid().expect("mapgen emits a structurally valid MapSpec");
        if floor_on_grid(&spec, &grid, config, players).is_ok() {
            return spec;
        }
    }
    panic!(
        "mapgen: {} attempts all failed the playability floor for {} players — \
         config bands are too dense",
        config.retry_cap, players
    );
}

/// The most players a map of this config can seat: the widest possible rim
/// ring (at `max_size`, where the ring is largest) divided by the minimum
/// start spacing. Beyond this, adjacent start zones would collide.
pub fn max_supported_players(config: &MapgenConfig) -> u32 {
    let half = config.max_size / 2;
    let ring = (half * config.start_ring_pct / 100).max(3);
    (8 * ring / MIN_START_SPACING).max(1) as u32
}

/// SplitMix64-style scramble of two words — used to fold the attempt
/// counter into the seed and to derive per-purpose sub-seeds.
fn mix(seed: u64, salt: u64) -> u64 {
    let mut h = Fnv1a::new();
    h.write_u64(seed);
    h.write_u64(salt);
    let mut state = h.finish();
    next_rand(&mut state)
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Which center-out band a tile falls in (by Chebyshev distance from the
/// map center). `Deep` is the richest, most dangerous core; `Rim` holds the
/// safe start zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Band {
    Deep,
    Mid,
    Rim,
}

struct Geometry {
    size: i32,
    center: TilePos,
    deep_r: i32,
    mid_r: i32,
    start_ring: i32,
}

impl Geometry {
    fn new(config: &MapgenConfig, players: u32) -> Self {
        let size = (config.base_size + config.size_per_player * (players as i32 - 1))
            .min(config.max_size);
        let center = TilePos::new(size / 2, size / 2);
        let half = size / 2;
        Self {
            size,
            center,
            deep_r: (half * config.deep_pct / 100).max(1),
            mid_r: (half * config.mid_pct / 100).max(2),
            start_ring: (half * config.start_ring_pct / 100).max(3),
        }
    }

    fn band(&self, pos: TilePos) -> Band {
        let d = pos.chebyshev(self.center) as i32;
        if d <= self.deep_r {
            Band::Deep
        } else if d <= self.mid_r {
            Band::Mid
        } else {
            Band::Rim
        }
    }

    /// The N start-zone tiles, evenly spaced clockwise around a Chebyshev
    /// square ring — integer perimeter walk, no trig, any player count. The
    /// `rotation` (a seed-derived perimeter offset) turns the whole ring so
    /// different seeds seat starts at different rim positions.
    fn start_positions(&self, players: u32, rotation: i32) -> Vec<TilePos> {
        let r = self.start_ring;
        let perimeter = 8 * r; // tiles on a Chebyshev ring of radius r >= 1
        (0..players)
            .map(|i| self.ring_point(r, rotation + (i as i32 * perimeter) / players as i32))
            .collect()
    }

    /// Tile at perimeter index `idx` on the square ring of radius `r`,
    /// walking clockwise from the top-left corner.
    fn ring_point(&self, r: i32, idx: i32) -> TilePos {
        let perimeter = 8 * r;
        let idx = idx.rem_euclid(perimeter);
        let side = 2 * r;
        let (cx, cy) = (self.center.x, self.center.y);
        let (s, o) = (idx / side, idx % side);
        match s {
            0 => TilePos::new(cx - r + o, cy - r), // top, →
            1 => TilePos::new(cx + r, cy - r + o), // right, ↓
            2 => TilePos::new(cx + r - o, cy + r), // bottom, ←
            _ => TilePos::new(cx - r, cy + r - o), // left, ↑
        }
    }

    /// The tile on the radial from center through `s`, at Chebyshev radius
    /// `target_r` from center — how the generator lays each start's veins
    /// out along its own wedge (kit near the rim, richer ore toward core).
    fn toward_center(&self, s: TilePos, target_r: i32) -> TilePos {
        let dx = s.x - self.center.x;
        let dy = s.y - self.center.y;
        let d = dx.abs().max(dy.abs());
        if d == 0 {
            self.center
        } else {
            TilePos::new(self.center.x + dx * target_r / d, self.center.y + dy * target_r / d)
        }
    }

    /// Unit inward step (toward center), each component in {-1,0,1}.
    fn inward(&self, s: TilePos) -> (i32, i32) {
        ((self.center.x - s.x).signum(), (self.center.y - s.y).signum())
    }

    fn in_bounds(&self, p: TilePos) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.size && p.y < self.size
    }
}

// ---------------------------------------------------------------------------
// Value-noise (integer, coherent)
// ---------------------------------------------------------------------------

/// A corner value in 0..=255 for coarse cell `(cx, cy)` on noise channel
/// `salt`, hashed from the seed — deterministic, no state.
fn cell_value(seed: u64, salt: u32, cx: i32, cy: i32) -> i64 {
    let mut h = Fnv1a::new();
    h.write_u64(seed);
    h.write_u32(salt);
    h.write_i32(cx);
    h.write_i32(cy);
    (h.finish() & 0xFF) as i64
}

/// Coherent integer value-noise at `(x, y)`, in 0..=999. Coarse cell
/// corners are hashed then integer-bilinear-blended — same shape on every
/// machine, floats never involved.
fn value_noise(seed: u64, salt: u32, cell: i32, x: i32, y: i32) -> u32 {
    let cx = x.div_euclid(cell);
    let cy = y.div_euclid(cell);
    let fx = x.rem_euclid(cell) as i64;
    let fy = y.rem_euclid(cell) as i64;
    let c = cell as i64;
    let v00 = cell_value(seed, salt, cx, cy);
    let v10 = cell_value(seed, salt, cx + 1, cy);
    let v01 = cell_value(seed, salt, cx, cy + 1);
    let v11 = cell_value(seed, salt, cx + 1, cy + 1);
    let top = v00 * (c - fx) + v10 * fx; // scaled by c
    let bot = v01 * (c - fx) + v11 * fx; // scaled by c
    let val = top * (c - fy) + bot * fy; // scaled by c*c, range 0..=255*c*c
    (val * 1000 / (255 * c * c)) as u32
}

/// A deterministic integer in `0..modulo`, keyed by `(seed, salt, faction)`
/// — the skeleton's source of per-wedge seed variation (vein radii, nest
/// arcanum), so the strategic layout differs seed-to-seed, not just the
/// decorative fill.
fn seed_pick(seed: u64, salt: u32, faction: u8, modulo: i32) -> i32 {
    let mut h = Fnv1a::new();
    h.write_u64(seed);
    h.write_u32(salt);
    h.write_u8(faction);
    (h.finish() % modulo.max(1) as u64) as i32
}

/// Pick a biome from a weighted palette using a 0..=999 selector.
fn weighted_pick(selector: u32, biomes: &[(TileKind, u32)]) -> Option<TileKind> {
    let total: u32 = biomes.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return None;
    }
    let mut r = selector.min(999) * total / 1000;
    for &(kind, w) in biomes {
        if r < w {
            return Some(kind);
        }
        r -= w;
    }
    biomes.last().map(|(k, _)| *k)
}

// ---------------------------------------------------------------------------
// Skeleton + fill: build one candidate
// ---------------------------------------------------------------------------

/// Everything the skeleton reserves (kit, veins, shore, printers, radial
/// corridors, the start-clear disc) so the fill stage cannot overwrite a
/// guarantee — the discipline that keeps the floor a floor.
struct Skeleton {
    geo: Geometry,
    reserved: BTreeSet<TilePos>,
    // The lists that become the MapSpec.
    water: Vec<TilePos>,
    mud: Vec<TilePos>,
    snow: Vec<TilePos>,
    rubble: Vec<TilePos>,
    corruption: Vec<TilePos>,
    high_ground: Vec<TilePos>,
    vents: Vec<TilePos>,
    crystal: Vec<TilePos>,
    resource_tiles: Vec<(TilePos, TileKind)>,
    depots: Vec<(TilePos, u8)>,
    printers: Vec<PrinterSpec>,
    structures: Vec<(TilePos, StructureKind)>,
    nests: Vec<(TilePos, u8)>,
    blight_cores: Vec<(TilePos, u32, i64)>,
}

impl Skeleton {
    fn new(geo: Geometry) -> Self {
        Self {
            geo,
            reserved: BTreeSet::new(),
            water: Vec::new(),
            mud: Vec::new(),
            snow: Vec::new(),
            rubble: Vec::new(),
            corruption: Vec::new(),
            high_ground: Vec::new(),
            vents: Vec::new(),
            crystal: Vec::new(),
            resource_tiles: Vec::new(),
            depots: Vec::new(),
            printers: Vec::new(),
            structures: Vec::new(),
            nests: Vec::new(),
            blight_cores: Vec::new(),
        }
    }

    /// Reserve a tile (fill will skip it). Clamped-away tiles are ignored.
    fn reserve(&mut self, p: TilePos) {
        if self.geo.in_bounds(p) {
            self.reserved.insert(p);
        }
    }

    /// Reserve a tile and its 4-neighbors — thickens a diagonal corridor
    /// into a 4-connected one.
    fn reserve_thick(&mut self, p: TilePos) {
        self.reserve(p);
        for (dx, dy) in NEIGHBORS4 {
            self.reserve(TilePos::new(p.x + dx, p.y + dy));
        }
    }

    /// Place a guarantee tile: reserve it AND route it to its MapSpec list,
    /// skipping anything off-grid — parity with [`Self::reserve`], which also
    /// clamps. A guarantee that can't fit is dropped, and the floor catches
    /// the resulting gap and regenerates (now that the skeleton is
    /// seed-varied, a retry actually moves things); the spec is never
    /// out-of-bounds, which would panic world-build.
    fn place(&mut self, kind: TileKind, p: TilePos) {
        if !self.geo.in_bounds(p) {
            return;
        }
        self.reserve(p);
        match kind {
            TileKind::Water => self.water.push(p),
            TileKind::Mud => self.mud.push(p),
            TileKind::Snow => self.snow.push(p),
            TileKind::Corruption => self.corruption.push(p),
            TileKind::HighGround => self.high_ground.push(p),
            TileKind::Vent => self.vents.push(p),
            TileKind::CrystalField => self.crystal.push(p),
            TileKind::Sand
            | TileKind::StoneOutcrop
            | TileKind::Grove
            | TileKind::CoalSeam
            | TileKind::IronVein
            | TileKind::CopperVein
            | TileKind::TinVein
            | TileKind::SilverVein
            | TileKind::GoldVein => self.resource_tiles.push((p, kind)),
            _ => self.rubble.push(p),
        }
    }
}

/// Build one skeleton+fill candidate for the given (already-folded) seed.
fn build_candidate(config: &MapgenConfig, seed: u64, players: u32) -> MapSpec {
    let geo = Geometry::new(config, players);
    let mut sk = Skeleton::new(geo);
    // Rotate the whole start ring by a seed-derived amount, so different
    // seeds seat the starts — and their entire wedges of ore/nests — at
    // different rim positions, not just a different decorative scatter.
    let perimeter = (8 * sk.geo.start_ring).max(1);
    let rotation = (mix(seed, 101) % perimeter as u64) as i32;
    let starts = sk.geo.start_positions(players, rotation);

    // --- Skeleton: place every guarantee, per start wedge -----------------
    for (i, &s) in starts.iter().enumerate() {
        place_start(&mut sk, config, seed, i as u8, s);
    }
    // A shared, contested deep-field core: a big Blight Core + Crystal + a
    // top-arcanum nest at the very center (the frontier the team pushes).
    place_core(&mut sk, config, seed);

    // --- Fill: decorative biome variety, never over a reserved tile -------
    let mut spec = fill_and_assemble(sk, config, seed, players);
    spec.seed = stream_seed(seed, "sim");
    spec
}

/// Lay out one start zone and its radial wedge (kit → mid ore → deep ore).
/// Vein radii and nest arcanum are jittered per `(seed, faction)` so seeds
/// vary the strategic layout, not just the fill.
fn place_start(sk: &mut Skeleton, config: &MapgenConfig, seed: u64, faction: u8, s: TilePos) {
    let (ix, iy) = sk.geo.inward(s);
    // Tangent (perpendicular to inward), for spacing kit off the corridor.
    let (tx, ty) = (-iy, ix);
    let at = |a: i32, b: i32| TilePos::new(s.x + ix * a + tx * b, s.y + iy * a + ty * b);

    // Reserve a clear disc around the printer (legible start, docs/05) and
    // the radial corridor inward to the center (guarantees the start is
    // never sealed and its mid/deep veins are reachable).
    for a in -2..=2 {
        for b in -2..=2 {
            sk.reserve(at(a, b));
        }
    }
    corridor(sk, s);

    // Printers: Green (remainder, working) + a ruined Red, per docs/03's
    // opening. Green is the faction's first-born → indestructible remainder.
    // (Both sit at a=0, always inside the reserved disc / margin, so they are
    // placed directly rather than via the OOB-skipping `place`.)
    let green = at(0, 0);
    let red = at(0, 2);
    sk.reserve(green);
    sk.reserve(red);
    sk.printers.push(PrinterSpec { pos: green, faction, color: 0, ruined: false });
    sk.printers.push(PrinterSpec { pos: red, faction, color: 1, ruined: true });

    // A depot and a stoked Generator beside the printer.
    let depot = at(1, -1);
    sk.reserve(depot);
    sk.depots.push((depot, faction));
    let generator = at(2, -1);
    sk.reserve(generator);
    sk.structures.push((generator, StructureKind::Generator));

    // The guaranteed kit — Iron + Coal + Wood + Stone — within sight of the
    // printer (all inside the reserved disc / corridor, so reachable).
    let iron = at(1, 1);
    sk.place(TileKind::IronVein, iron);
    sk.place(TileKind::CoalSeam, at(2, 0));
    sk.place(TileKind::Grove, at(1, -2));
    sk.place(TileKind::StoneOutcrop, at(2, 1));
    debug_assert!(
        s.chebyshev(iron) <= config.start_vein_sight as u32,
        "kit Iron vein must sit within start sight"
    );

    // A start Vent (free-energy tap ground) and a reachable shore strip on
    // the tangential side — Water + a Sand flat (coolant AND Glass feedstock,
    // the two-birds guarantee), kept beside the reserved disc so the water
    // borders reachable ground.
    sk.place(TileKind::Vent, at(3, 0));
    sk.place(TileKind::Water, at(-1, 2));
    sk.place(TileKind::Water, at(-2, 2));
    sk.place(TileKind::Sand, at(-1, 1));

    // Midfield wedge: Copper + Tin along the radial (no Bronze soft-lock),
    // at a seed-jittered radius. They stay on the reserved (thickened)
    // corridor, so reachability holds; the floor finds them by kind, not by
    // recomputing this radius.
    let mid_base = (sk.geo.deep_r + sk.geo.mid_r) / 2;
    let mid_r = (mid_base + seed_pick(seed, 20, faction, 3) - 1).clamp(sk.geo.deep_r + 1, sk.geo.mid_r);
    let copper = sk.geo.toward_center(s, mid_r);
    let tin = TilePos::new(copper.x + tx, copper.y + ty);
    sk.reserve_thick(copper);
    sk.reserve_thick(tin);
    sk.place(TileKind::CopperVein, copper);
    sk.place(TileKind::TinVein, tin);

    // Deep wedge: Silver + Gold, richer material farther in (inside the deep
    // band, distinct radii so they never share a tile), radius seed-jittered.
    let deep_local = (sk.geo.deep_r + seed_pick(seed, 21, faction, 3) - 1).clamp(2, sk.geo.mid_r);
    let silver = sk.geo.toward_center(s, deep_local);
    let gold = sk.geo.toward_center(s, (deep_local - 1).max(1));
    sk.place(TileKind::SilverVein, silver);
    sk.place(TileKind::GoldVein, gold);

    // A per-wedge Feral nest guarding the deep ore, pushed one tile off the
    // radial so it shares no tile with a vein; a seed-jittered mid arcanum
    // here, the map's peak arcanum sits at the shared core.
    let nest_base = sk.geo.toward_center(s, (sk.geo.deep_r / 2).max(2));
    let nest = TilePos::new(nest_base.x + tx, nest_base.y + ty);
    if sk.geo.in_bounds(nest) && nest != sk.geo.center {
        let mid_arc = (config.nest_max_arcanum / 2).max(1) as i32;
        let arc = (mid_arc + seed_pick(seed, 22, faction, 3) - 1)
            .clamp(1, config.nest_max_arcanum as i32) as u8;
        sk.reserve(nest);
        sk.nests.push((nest, arc));
    }
}

/// Reserve a 4-connected corridor from a start to the center, guaranteeing
/// the start is never walled off from the frontier.
fn corridor(sk: &mut Skeleton, s: TilePos) {
    let c = sk.geo.center;
    let dx = c.x - s.x;
    let dy = c.y - s.y;
    let steps = dx.abs().max(dy.abs()).max(1);
    for i in 0..=steps {
        let p = TilePos::new(s.x + dx * i / steps, s.y + dy * i / steps);
        sk.reserve_thick(p);
    }
}

/// The shared deep-field core: a Blight Core radiating Corruption, a Crystal
/// cluster on its doorstep (docs/05: Crystal spawns near Corruption), and
/// the map's highest-arcanum nest.
fn place_core(sk: &mut Skeleton, config: &MapgenConfig, seed: u64) {
    let c = sk.geo.center;
    sk.blight_cores.push((c, config.blight_radius, config.blight_hp));
    // A ring of Corruption + Crystal around the core (inside the deep band).
    // `place` skips any tile a tiny map would push off-grid.
    for (dx, dy) in NEIGHBORS4 {
        sk.place(TileKind::Corruption, TilePos::new(c.x + dx, c.y + dy));
        sk.place(TileKind::CrystalField, TilePos::new(c.x + dx * 2, c.y + dy * 2));
    }
    // The apex nest sits at a seed-chosen diagonal off-center (never the core
    // tile), with the High Ground overlook on the opposite diagonal — so the
    // core's shape varies with the seed too.
    let diag = [(1, 1), (1, -1), (-1, 1), (-1, -1)][(mix(seed, 202) % 4) as usize];
    let nest = TilePos::new(c.x + diag.0, c.y + diag.1);
    if sk.geo.in_bounds(nest) {
        sk.nests.push((nest, config.nest_max_arcanum));
        sk.reserve(nest);
    }
    sk.place(TileKind::HighGround, TilePos::new(c.x - diag.0 * 2, c.y - diag.1 * 2));
}

/// Paint decorative biomes from value-noise over unreserved open tiles,
/// then assemble the final `MapSpec`.
fn fill_and_assemble(
    mut sk: Skeleton,
    config: &MapgenConfig,
    seed: u64,
    players: u32,
) -> MapSpec {
    let size = sk.geo.size;
    // Two independent noise channels: density decides IF a tile paints,
    // choice decides WHICH biome. Salts keep them decorrelated.
    for y in 0..size {
        for x in 0..size {
            let p = TilePos::new(x, y);
            if sk.reserved.contains(&p) {
                continue;
            }
            let band = sk.geo.band(p);
            let fill = match band {
                Band::Rim => &config.rim_fill,
                Band::Mid => &config.mid_fill,
                Band::Deep => &config.deep_fill,
            };
            let density = value_noise(seed, 1, config.noise_cell, x, y);
            if density >= fill.fill_permille {
                continue;
            }
            let choice = value_noise(seed, 2, config.noise_cell, x, y);
            let Some(kind) = weighted_pick(choice, &fill.biomes) else { continue };
            push_biome(&mut sk, p, kind);
        }
    }

    // Canonicalize every list: sorted and deduped by position, so the
    // output is byte-stable for a given seed and no tile is double-listed
    // (converging wedges can nominate the same deep tile twice). Later
    // pushes win for typed tiles — matching paint_grid's last-writer order.
    let dedup_points = |v: Vec<TilePos>| -> Vec<TilePos> {
        v.into_iter().collect::<BTreeSet<_>>().into_iter().collect()
    };
    let mut res: BTreeMap<TilePos, TileKind> = BTreeMap::new();
    for (pos, kind) in sk.resource_tiles {
        res.insert(pos, kind);
    }
    let mut nests: BTreeMap<TilePos, u8> = BTreeMap::new();
    for (pos, arc) in sk.nests {
        let e = nests.entry(pos).or_insert(arc);
        *e = (*e).max(arc);
    }

    let mut spec = MapSpec::empty(size, size);
    spec.water = dedup_points(sk.water);
    spec.mud = dedup_points(sk.mud);
    spec.snow = dedup_points(sk.snow);
    spec.rubble = dedup_points(sk.rubble);
    spec.corruption = dedup_points(sk.corruption);
    spec.high_ground = dedup_points(sk.high_ground);
    spec.vents = dedup_points(sk.vents);
    spec.crystal = dedup_points(sk.crystal);
    spec.resource_tiles = res.into_iter().collect();
    spec.depots = sk.depots;
    spec.printers = sk.printers;
    spec.structures = sk.structures;
    spec.nests = nests.into_iter().collect();
    spec.blight_cores = sk.blight_cores;
    spec.node_amount = config.node_amount;
    spec.max_arcanum = config.nest_max_arcanum;
    spec.dev_all_unlocks = config.dev_all_unlocks;
    spec.dev_free_power = config.dev_free_power;
    // Per-faction opening stock.
    for faction in 0..players as u8 {
        for &(kind, units) in &config.starting_stock {
            spec.starting_stock.push((faction, kind, units));
        }
    }
    spec
}

/// Route a painted biome into the right MapSpec list. `MapSpec` exposes
/// per-kind lists for only a subset of `TileKind` (the v2 loose-ground
/// tiles Dunes/Ice/Scree have no field yet), so a fill palette must name
/// only representable kinds; anything else degrades to Rubble rather than
/// silently vanishing.
fn push_biome(sk: &mut Skeleton, p: TilePos, kind: TileKind) {
    match kind {
        TileKind::Mud => sk.mud.push(p),
        TileKind::Snow => sk.snow.push(p),
        TileKind::Water => sk.water.push(p),
        TileKind::HighGround => sk.high_ground.push(p),
        _ => sk.rubble.push(p),
    }
}

// ---------------------------------------------------------------------------
// Validate: the playability floor
// ---------------------------------------------------------------------------

/// Why a candidate layout failed the emergent floor (post-fill).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorError {
    KitUnreachable { faction: u8, pos: TilePos },
    NoVeinInSight { faction: u8 },
    NoReachableShore { faction: u8 },
    CopperUnreachable { faction: u8 },
    TinUnreachable { faction: u8 },
    StartSealed { faction: u8 },
}

/// Flood-fill from each start printer and confirm the floor holds. Paints the
/// terrain and delegates to [`floor_on_grid`]; the generator's hot path calls
/// `floor_on_grid` directly with the grid it already painted.
pub fn playability_floor(
    spec: &MapSpec,
    config: &MapgenConfig,
    players: u32,
) -> Result<(), FloorError> {
    let grid = spec.paint_grid();
    floor_on_grid(spec, &grid, config, players)
}

/// The floor check over an already-painted grid. Pure integer BFS over
/// 4-neighbors on passable, non-plateau ground (no ramps are generated, so
/// High Ground reads as a wall for connectivity).
///
/// Every checked tile comes from the SPEC (the faction's remainder printer,
/// its in-sight veins, its water, its nearest Copper/Tin), so the check is
/// independent of *how* the skeleton placed them — which is what lets the
/// skeleton vary placement by seed. Band membership (for the sealed-start
/// check) is the only geometry it reads, and bands are seed-independent.
fn floor_on_grid(
    spec: &MapSpec,
    grid: &Grid,
    config: &MapgenConfig,
    players: u32,
) -> Result<(), FloorError> {
    let geo = Geometry::new(config, players);
    for faction in 0..players as u8 {
        let Some(printer) = spec
            .printers
            .iter()
            .find(|p| p.faction == faction && p.color == 0)
            .map(|p| p.pos)
        else {
            continue; // a faction with no remainder printer — nothing to seat
        };
        let reach = flood_from(grid, printer, &geo);

        // Kit: every resource tile within the printer's sight must be
        // reachable, and at least one must be an ore-family vein (so tick-1
        // `closest(ore)` answers).
        let sight = config.start_vein_sight as u32;
        let mut kit_ore_family_in_sight = false;
        for &(pos, kind) in &spec.resource_tiles {
            if printer.chebyshev(pos) > sight {
                continue;
            }
            if !reach.contains(pos) {
                return Err(FloorError::KitUnreachable { faction, pos });
            }
            if let Some((res, _)) = Resource::for_tile(kind)
                && res.is_ore_family()
            {
                kit_ore_family_in_sight = true;
            }
        }
        if !kit_ore_family_in_sight {
            return Err(FloorError::NoVeinInSight { faction });
        }

        // Reachable shoreline: some nearby Water tile borders a reachable one.
        let shore_ok = spec.water.iter().any(|&w| {
            printer.chebyshev(w) <= geo.mid_r as u32 * 2
                && NEIGHBORS4.iter().any(|(dx, dy)| reach.contains(TilePos::new(w.x + dx, w.y + dy)))
        });
        if !shore_ok {
            return Err(FloorError::NoReachableShore { faction });
        }

        // Copper + Tin: the printer's nearest vein of each kind must be
        // reachable (no Bronze soft-lock). Found by kind, not by re-deriving
        // a radius, so seed-jittered vein placement is respected.
        match nearest_kind(spec, printer, TileKind::CopperVein) {
            Some(p) if near_reachable(&reach, p) => {}
            _ => return Err(FloorError::CopperUnreachable { faction }),
        }
        match nearest_kind(spec, printer, TileKind::TinVein) {
            Some(p) if near_reachable(&reach, p) => {}
            _ => return Err(FloorError::TinUnreachable { faction }),
        }

        // Not sealed: the flood reached beyond the rim band (computed during
        // the BFS — no extra whole-grid scan).
        if !reach.non_rim {
            return Err(FloorError::StartSealed { faction });
        }
    }
    Ok(())
}

/// The tile of the resource-ground `kind` nearest `from` (manhattan, ties by
/// position for determinism), or `None` if the spec has none.
fn nearest_kind(spec: &MapSpec, from: TilePos, kind: TileKind) -> Option<TilePos> {
    spec.resource_tiles
        .iter()
        .filter(|(_, k)| *k == kind)
        .map(|&(p, _)| (from.manhattan(p), p))
        .min()
        .map(|(_, p)| p)
}

/// The reachable set as a flat bitmap — deterministic (index order is
/// positional) and O(1) membership, so the floor check scales to the large
/// grids high player counts produce. `non_rim` records, during the flood,
/// whether any reached tile lay outside the rim band (the sealed-start check,
/// folded in so it costs no extra whole-grid scan).
struct Reach {
    w: i32,
    h: i32,
    hit: Vec<bool>,
    non_rim: bool,
}

impl Reach {
    fn contains(&self, p: TilePos) -> bool {
        p.x >= 0
            && p.y >= 0
            && p.x < self.w
            && p.y < self.h
            && self.hit[(p.y * self.w + p.x) as usize]
    }
}

/// A tile counts as reachable-for-mining if it or a 4-neighbor is in the
/// flood set (miners work from an adjacent tile; the vein itself is
/// passable ground but may be surrounded — adjacency is the real test).
fn near_reachable(reach: &Reach, p: TilePos) -> bool {
    reach.contains(p)
        || NEIGHBORS4.iter().any(|(dx, dy)| reach.contains(TilePos::new(p.x + dx, p.y + dy)))
}

/// BFS over passable, non-plateau ground from `start`. Deterministic: fixed
/// N/E/S/W neighbor order, positional bitmap visited set. Tracks whether the
/// flood escaped the rim band as it goes.
fn flood_from(grid: &Grid, start: TilePos, geo: &Geometry) -> Reach {
    let (w, h) = (grid.width, grid.height);
    let mut hit = vec![false; (w * h) as usize];
    let idx = |p: TilePos| (p.y * w + p.x) as usize;
    let mut queue = VecDeque::new();
    let mut non_rim = false;
    if walkable(grid, start) {
        hit[idx(start)] = true;
        non_rim |= geo.band(start) != Band::Rim;
        queue.push_back(start);
    }
    while let Some(p) = queue.pop_front() {
        for (dx, dy) in NEIGHBORS4 {
            let n = TilePos::new(p.x + dx, p.y + dy);
            // walkable(n) implies in-bounds, so idx(n) is valid.
            if walkable(grid, n) && !hit[idx(n)] {
                hit[idx(n)] = true;
                non_rim |= geo.band(n) != Band::Rim;
                queue.push_back(n);
            }
        }
    }
    Reach { w, h, hit, non_rim }
}

/// Ground a bot can stand on for connectivity: passable, and not a
/// Ramp-gated plateau (no ramps exist in generated maps, so High Ground is
/// a wall). Uses the same passability predicate as the sim.
fn walkable(grid: &Grid, p: TilePos) -> bool {
    match grid.get(p) {
        Some(k) => k.passable() && k != TileKind::HighGround,
        None => false,
    }
}
