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
/// machine. `players` is clamped to at least 1.
///
/// Panics only if `retry_cap` candidate layouts all fail the playability
/// floor — a config bug (bands too dense), surfaced loudly rather than
/// shipping a soft-locked map.
pub fn generate(config: &MapgenConfig, seed: u64, players: u32) -> MapSpec {
    let players = players.max(1);
    // The base mapgen stream: every attempt's sub-seed derives from it, so
    // `seed S` deterministically resolves to the first passing candidate.
    let base = stream_seed(seed, "mapgen");
    for attempt in 0..config.retry_cap {
        // Fold the retry counter into the seed (a fresh named sub-stream
        // per attempt — decorrelated from its neighbors).
        let mut s = base;
        s = mix(s, attempt as u64);
        let spec = build_candidate(config, s, players);
        // The authoring floor first (a generator bug is a hard error, not a
        // retry), then the emergent playability floor.
        spec.validate().expect("mapgen emits a structurally valid MapSpec");
        if playability_floor(&spec, config, players).is_ok() {
            return spec;
        }
    }
    panic!(
        "mapgen: {} attempts all failed the playability floor for {} players — \
         config bands are too dense",
        config.retry_cap, players
    );
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
    /// square ring — integer perimeter walk, no trig, any player count.
    fn start_positions(&self, players: u32) -> Vec<TilePos> {
        let r = self.start_ring;
        let perimeter = 8 * r; // tiles on a Chebyshev ring of radius r >= 1
        (0..players).map(|i| self.ring_point(r, (i as i32 * perimeter) / players as i32)).collect()
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
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            self.reserve(TilePos::new(p.x + dx, p.y + dy));
        }
    }
}

/// Build one skeleton+fill candidate for the given (already-folded) seed.
fn build_candidate(config: &MapgenConfig, seed: u64, players: u32) -> MapSpec {
    let geo = Geometry::new(config, players);
    let mut sk = Skeleton::new(geo);
    let starts = sk.geo.start_positions(players);

    // --- Skeleton: place every guarantee, per start wedge -----------------
    for (i, &s) in starts.iter().enumerate() {
        place_start(&mut sk, config, i as u8, s);
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
fn place_start(sk: &mut Skeleton, config: &MapgenConfig, faction: u8, s: TilePos) {
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
    let coal = at(2, 0);
    let grove = at(1, -2);
    let stone = at(2, 1);
    for (p, kind) in [
        (iron, TileKind::IronVein),
        (coal, TileKind::CoalSeam),
        (grove, TileKind::Grove),
        (stone, TileKind::StoneOutcrop),
    ] {
        sk.reserve(p);
        sk.resource_tiles.push((p, kind));
    }
    debug_assert!(
        s.chebyshev(iron) <= config.start_vein_sight as u32,
        "kit Iron vein must sit within start sight"
    );

    // A start Vent (free-energy tap ground).
    let vent = at(3, 0);
    sk.reserve(vent);
    sk.vents.push(vent);

    // A reachable shore strip on the tangential side, kept beside the
    // reserved disc so it borders reachable ground. Water + a Sand flat
    // (coolant AND Glass feedstock — the two-birds guarantee). Tiles are
    // chosen off the printer/kit set so nothing collides.
    for w in [at(-1, 2), at(-2, 2)] {
        sk.reserve(w);
        sk.water.push(w);
    }
    let sand = at(-1, 1);
    sk.reserve(sand);
    sk.resource_tiles.push((sand, TileKind::Sand));

    // Midfield wedge: Copper + Tin along the radial (no Bronze soft-lock).
    let mid_r = (sk.geo.deep_r + sk.geo.mid_r) / 2;
    let copper = sk.geo.toward_center(s, mid_r);
    let tin = TilePos::new(copper.x + tx, copper.y + ty);
    for (p, kind) in [(copper, TileKind::CopperVein), (tin, TileKind::TinVein)] {
        sk.reserve_thick(p);
        sk.resource_tiles.push((p, kind));
    }

    // Deep wedge: Silver + Gold, richer material farther in (inside the
    // deep band, distinct radii so they never share a tile).
    let deep_local = sk.geo.deep_r.max(2);
    let silver = sk.geo.toward_center(s, deep_local);
    let gold = sk.geo.toward_center(s, (deep_local - 1).max(1));
    for (p, kind) in [(silver, TileKind::SilverVein), (gold, TileKind::GoldVein)] {
        sk.reserve(p);
        sk.resource_tiles.push((p, kind));
    }

    // A per-wedge Feral nest guarding the deep ore, pushed one tile off the
    // radial so it shares no tile with a vein; a mid arcanum here, the map's
    // peak arcanum sits at the shared core.
    let nest_base = sk.geo.toward_center(s, (sk.geo.deep_r / 2).max(2));
    let nest = TilePos::new(nest_base.x + tx, nest_base.y + ty);
    if sk.geo.in_bounds(nest) && nest != sk.geo.center {
        let arc = (config.nest_max_arcanum / 2).max(1);
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
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let corrupt = TilePos::new(c.x + dx, c.y + dy);
        if sk.geo.in_bounds(corrupt) {
            sk.corruption.push(corrupt);
            sk.reserve(corrupt);
        }
        let crystal = TilePos::new(c.x + dx * 2, c.y + dy * 2);
        if sk.geo.in_bounds(crystal) {
            sk.crystal.push(crystal);
            sk.reserve(crystal);
        }
    }
    // The apex nest, one tile off-center so it doesn't share the core tile.
    let nest = TilePos::new(c.x + 1, c.y + 1);
    if sk.geo.in_bounds(nest) {
        sk.nests.push((nest, config.nest_max_arcanum));
        sk.reserve(nest);
    }
    // A High Ground overlook nearby (an anchor for defensive programs).
    let hg = TilePos::new(c.x - 2, c.y - 2);
    if sk.geo.in_bounds(hg) {
        sk.high_ground.push(hg);
        sk.reserve(hg);
    }
    let _ = seed;
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

/// Flood-fill from each start printer and confirm the floor holds. Pure
/// integer BFS over 4-neighbors on passable, non-plateau ground (no ramps
/// are generated, so High Ground reads as a wall for connectivity).
pub fn playability_floor(
    spec: &MapSpec,
    config: &MapgenConfig,
    players: u32,
) -> Result<(), FloorError> {
    let grid = spec.paint_grid();
    let geo = Geometry::new(config, players);
    // The floor re-derives each start's checked tiles from the seed-
    // independent geometry (start positions, the mid-radius Copper/Tin) and
    // from the spec's own lists (printers, resource_tiles, water) — the spec
    // doesn't carry the skeleton's bookkeeping, and it doesn't need to.
    let starts = geo.start_positions(players);

    for (i, &s) in starts.iter().enumerate() {
        let faction = i as u8;
        let printer = spec
            .printers
            .iter()
            .find(|p| p.faction == faction && p.color == 0)
            .map(|p| p.pos)
            .expect("every start has a remainder (color 0) printer");
        let reach = flood_from(&grid, printer);

        // Kit: the four start-zone veins nearest the printer must be
        // reachable. We identify them as the resource_tiles within sight of
        // this printer.
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

        // Reachable shoreline: some Water tile borders a reachable tile.
        let shore_ok = spec.water.iter().any(|&w| {
            printer.chebyshev(w) <= geo.mid_r as u32 * 2
                && [(0, -1), (1, 0), (0, 1), (-1, 0)]
                    .iter()
                    .any(|(dx, dy)| reach.contains(TilePos::new(w.x + dx, w.y + dy)))
        });
        if !shore_ok {
            return Err(FloorError::NoReachableShore { faction });
        }

        // Copper + Tin: this start's midfield veins must be reachable. They
        // sit on the radial toward center at the shared mid radius.
        let mid_r = (geo.deep_r + geo.mid_r) / 2;
        let copper = geo.toward_center(s, mid_r);
        if !near_reachable(&reach, copper) {
            return Err(FloorError::CopperUnreachable { faction });
        }
        let (ix, iy) = geo.inward(s);
        let tin = TilePos::new(copper.x + (-iy), copper.y + ix);
        if !near_reachable(&reach, tin) {
            return Err(FloorError::TinUnreachable { faction });
        }

        // Not sealed: the reachable set must touch the midfield/deep bands.
        if !reach.touches_non_rim(&geo) {
            return Err(FloorError::StartSealed { faction });
        }
    }
    Ok(())
}

/// The reachable set as a flat bitmap — deterministic (index order is
/// positional) and O(1) membership, so the floor check scales to the large
/// grids high player counts produce.
struct Reach {
    w: i32,
    h: i32,
    hit: Vec<bool>,
}

impl Reach {
    fn contains(&self, p: TilePos) -> bool {
        p.x >= 0
            && p.y >= 0
            && p.x < self.w
            && p.y < self.h
            && self.hit[(p.y * self.w + p.x) as usize]
    }

    /// Does the reachable set include any tile outside the rim band? (The
    /// "start not sealed from the frontier" floor check.)
    fn touches_non_rim(&self, geo: &Geometry) -> bool {
        for y in 0..self.h {
            for x in 0..self.w {
                if self.hit[(y * self.w + x) as usize]
                    && geo.band(TilePos::new(x, y)) != Band::Rim
                {
                    return true;
                }
            }
        }
        false
    }
}

/// A tile counts as reachable-for-mining if it or a 4-neighbor is in the
/// flood set (miners work from an adjacent tile; the vein itself is
/// passable ground but may be surrounded — adjacency is the real test).
fn near_reachable(reach: &Reach, p: TilePos) -> bool {
    reach.contains(p)
        || [(0, -1), (1, 0), (0, 1), (-1, 0)]
            .iter()
            .any(|(dx, dy)| reach.contains(TilePos::new(p.x + dx, p.y + dy)))
}

/// BFS over passable, non-plateau ground from `start`. Deterministic: fixed
/// N/E/S/W neighbor order, positional bitmap visited set.
fn flood_from(grid: &Grid, start: TilePos) -> Reach {
    let (w, h) = (grid.width, grid.height);
    let mut hit = vec![false; (w * h) as usize];
    let idx = |p: TilePos| (p.y * w + p.x) as usize;
    let mut queue = VecDeque::new();
    if walkable(grid, start) {
        hit[idx(start)] = true;
        queue.push_back(start);
    }
    while let Some(p) = queue.pop_front() {
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let n = TilePos::new(p.x + dx, p.y + dy);
            // walkable(n) implies in-bounds, so idx(n) is valid.
            if walkable(grid, n) && !hit[idx(n)] {
                hit[idx(n)] = true;
                queue.push_back(n);
            }
        }
    }
    Reach { w, h, hit }
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
