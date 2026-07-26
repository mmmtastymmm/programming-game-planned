//! Fog of war (M7, docs/05): the viewer sees the world as FACTION 0
//! perceives it. Pure view layer — no sim state, no replay exposure.
//! Three tile states, styled at the material level (no translucent
//! overlay planes):
//!
//! - **Undiscovered**: terrain and objects are hidden outright; a flat
//!   opaque near-black cover is all that renders. True black.
//! - **Discovered (memory)**: the terrain renders, swapped to frozen
//!   "memory" twins of its live materials — unlit, darkened blue-grey,
//!   no glow, ambient animation frozen (Q93: stillness is the contract,
//!   not per-tile frame fidelity — twins are shared per material).
//! - **In view**: live materials, nothing on top.
//!
//! Heard-only enemy contacts render as pulsing blips — a position, not a
//! picture — and a searching bot shows its expanding survey ring.

use bevy::prelude::*;
use sim::perception::los_clear;
use sim::TilePos;
use std::collections::{HashMap, HashSet};

use crate::palette::Palette;
use crate::scene::{tile_top_xyz, tile_xyz, TerrainTile};
use crate::view::ViewIndex;
use crate::GameSim;

/// The viewer faction whose perception the fog renders (and whose colony
/// cloud the telemetry panel reads — Q89).
pub const VIEWER: u8 = 0;

#[derive(Resource, Default)]
pub(crate) struct FogState {
    /// Tiles inside a viewer bot's seeing circle THIS tick.
    pub(crate) visible: HashSet<(i32, i32)>,
    /// Tiles ever seen (view-local memory — the greyed snapshot set).
    pub(crate) known: HashSet<(i32, i32)>,
    /// Heard-only enemy contacts (entity id → last heard tile).
    heard: Vec<(u64, TilePos)>,
    /// Searching viewer bots: (bot id, current survey reach in tiles).
    rings: Vec<(u32, u32)>,
}

impl FogState {
    /// Is the viewer *watching* this tile right now? The Q92 snapshot rule
    /// hangs off this: view mutations (spawns, despawns, state flips,
    /// scale/progress updates) for world objects happen only on watched
    /// tiles — everything else stays exactly as last seen. A fog-stripped
    /// dev capture counts as watching everything.
    pub(crate) fn watching(&self, pos: TilePos) -> bool {
        crate::screenshot_hides_fog() || self.visible.contains(&(pos.x, pos.y))
    }
}

/// A tile's fog state from the viewer's chair.
enum TileFog {
    Visible,
    Known,
    Unknown,
}

fn tile_fog(fog: &FogState, key: (i32, i32)) -> TileFog {
    if fog.visible.contains(&key) {
        TileFog::Visible
    } else if fog.known.contains(&key) {
        TileFog::Known
    } else {
        TileFog::Unknown
    }
}

/// One opaque black cover quad (per tile) — the undiscovered look.
#[derive(Component)]
pub(crate) struct FogTile(i32, i32);
/// A heard-only contact blip (marker; identity lives in `FogAssets.blips`).
#[derive(Component)]
pub(crate) struct Blip;
/// A search-stance survey ring (marker; keyed in `FogAssets.rings`).
#[derive(Component)]
pub(crate) struct SurveyRing;

#[derive(Resource, Default)]
pub(crate) struct FogAssets {
    quad: Handle<Mesh>,
    unknown: Handle<StandardMaterial>,
    blip: Handle<StandardMaterial>,
    ring: Handle<StandardMaterial>,
    tiles: HashMap<(i32, i32), Entity>,
    blips: HashMap<u64, Entity>,
    rings: HashMap<u32, Entity>,
    /// Live material → its frozen "memory" twin. All tiles sharing a live
    /// material share one twin, and the twin is never retargeted by
    /// `animate_terrain` — remembered ground holds still. Q93 ruled that
    /// stillness (not per-tile last-seen frames) is the contract, so one
    /// twin per material is the intended design, not an approximation.
    dim: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>,
    /// Memory twin → the live material it shadows (restored on reveal).
    bright: HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>,
}

pub(crate) fn setup_fog(
    mut assets: ResMut<FogAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    assets.quad = meshes.add(Cuboid::new(1.0, 0.02, 1.0));
    // Opaque and unlit: undiscovered ground is a void, not a tint.
    let mut dark = StandardMaterial::from(Color::srgb(0.008, 0.008, 0.016));
    dark.unlit = true;
    assets.unknown = materials.add(dark);
    let mut blip = StandardMaterial::from(Color::srgba(0.95, 0.55, 0.25, 0.9));
    blip.alpha_mode = AlphaMode::Blend;
    blip.unlit = true;
    assets.blip = materials.add(blip);
    let mut ring = StandardMaterial::from(Color::srgba(0.4, 0.85, 0.95, 0.5));
    ring.alpha_mode = AlphaMode::Blend;
    ring.unlit = true;
    assets.ring = materials.add(ring);
}

/// Mirror the sim's two-circle model for the viewer faction: chebyshev
/// seeing circles with LoS, from every live viewer bot (docs/05; the
/// renderer recomputes rather than reading sim internals — fog stays a
/// pure view concern).
pub(crate) fn recompute_fog(game: NonSend<GameSim>, mut fog: ResMut<FogState>) {
    let simulation = &game.0;
    let world = &simulation.world;
    let ctx = simulation.ctx();
    fog.visible.clear();
    // Mirror the sim's FULL perceiver set (bots + printers + structures +
    // the factionless depots), or tiles the sim treats as seen render
    // under fog and explore()'s picks disagree with the screen.
    let mut eyes: Vec<(TilePos, i32, bool)> = Vec::new();
    for bot in world.bots.values() {
        if bot.data.faction != VIEWER || bot.data.dying {
            continue;
        }
        let mut seeing = ctx.sensors_for(&bot.data)
            + sim::perception::high_ground_bonus(
                &world.grid,
                bot.data.pos,
                simulation.tuning.high_ground_sensor_bonus,
            );
        // The search stance widens real sight to its current ring.
        if let Some(sim::world::Action::Search { current, .. }) = &bot.data.action {
            seeing = seeing.max(*current);
        }
        eyes.push((
            bot.data.pos,
            seeing as i32,
            sim::perception::on_high_ground(&world.grid, bot.data.pos),
        ));
    }
    let s = simulation.tuning.structure_sensors as i32;
    for p in world.printers.values().filter(|p| p.faction == VIEWER) {
        eyes.push((p.pos, s, false));
    }
    for st in world.structures.values().filter(|st| st.faction == VIEWER) {
        eyes.push((st.pos, s, false));
    }
    for d in world.depots.values().filter(|d| d.faction == VIEWER) {
        eyes.push((d.pos, s, false)); // owner's eyes (Q89)
    }
    for (pos, seeing, elevated) in eyes {
        for dy in -seeing..=seeing {
            for dx in -seeing..=seeing {
                let t = TilePos::new(pos.x + dx, pos.y + dy);
                if !world.grid.in_bounds(t) {
                    continue;
                }
                if los_clear(&world.grid, pos, t, elevated) {
                    fog.visible.insert((t.x, t.y));
                }
            }
        }
    }
    let visible: Vec<(i32, i32)> = fog.visible.iter().copied().collect();
    fog.known.extend(visible);
    // Seed view memory from the sim's permanent map knowledge (Q70:
    // discovered nodes outlive any one viewing session — PRESTEP_TICKS,
    // future save/load). A known vein's tile is remembered ground, so its
    // gem neither floats over undiscovered black nor vanishes from the
    // player's own map (review 2026-07-25).
    if let Some(known) = world.known_nodes.get(&VIEWER) {
        fog.known.extend(known.values().map(|n| (n.pos.x, n.pos.y)));
    }

    // Heard-only contacts: the sim's own perception (entity handles the
    // programs also see) — positions only.
    fog.heard.clear();
    if let Some(p) = world.perception.get(&VIEWER) {
        for (entity, pos) in &p.heard {
            if !p.seen.contains(entity) {
                fog.heard.push((entity.0, *pos));
            }
        }
    }

    // Survey rings: searching viewer bots expose their current reach.
    fog.rings.clear();
    for (id, bot) in &world.bots {
        if bot.data.faction != VIEWER {
            continue;
        }
        if let Some(sim::world::Action::Search { current, .. }) = &bot.data.action {
            fog.rings.push((id.0, *current));
        }
    }
}

/// The frozen "memory" version of a live material: unlit (no sun, no
/// glow), darkened toward blue-grey, animation frame captured at swap
/// time. Cached per live material, with the reverse mapping recorded so
/// reveal can restore the live handle.
fn memory_twin(
    assets: &mut FogAssets,
    materials: &mut Assets<StandardMaterial>,
    live: &Handle<StandardMaterial>,
) -> Handle<StandardMaterial> {
    if let Some(dim) = assets.dim.get(&live.id()) {
        return dim.clone();
    }
    let Some(src) = materials.get(live) else { return live.clone() };
    let mut m = src.clone();
    // Linear-space multipliers ≈ sRGB 0.42/0.43/0.49: dark enough to read
    // as memory, blue-shifted enough to read as cold rather than dirty.
    let c = m.base_color.to_linear();
    m.base_color =
        LinearRgba::new(c.red * 0.14, c.green * 0.15, c.blue * 0.21, c.alpha).into();
    m.unlit = true;
    m.emissive = LinearRgba::BLACK;
    m.emissive_texture = None;
    let dim = materials.add(m);
    assets.dim.insert(live.id(), dim.clone());
    assets.bright.insert(dim.id(), live.clone());
    dim
}

/// Spawn-or-toggle the black cover quads, plus blips and rings.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_fog(
    mut commands: Commands,
    game: NonSend<GameSim>,
    fog: Res<FogState>,
    mut assets: ResMut<FogAssets>,
    palette: Res<Palette>,
    mut tiles: Query<(&FogTile, &mut Visibility)>,
    mut rings: Query<(&SurveyRing, &mut Transform), Without<Blip>>,
) {
    // Dev tool: skip fog so the whole map is lit for a SCREENSHOT_PATH capture.
    if crate::screenshot_hides_fog() {
        return;
    }
    let world = &game.0.world;
    // Lazily cover the whole grid once. The cover sits just above the flat
    // slab tops; everything taller on an undiscovered tile is HIDDEN by
    // shade_terrain / gate_fogged_views, so nothing pokes through.
    if assets.tiles.is_empty() {
        for y in 0..world.grid.height {
            for x in 0..world.grid.width {
                let entity = commands
                    .spawn((
                        FogTile(x, y),
                        Mesh3d(assets.quad.clone()),
                        MeshMaterial3d(assets.unknown.clone()),
                        Transform::from_translation(tile_xyz(
                            world,
                            TilePos::new(x, y),
                            0.06,
                        )),
                    ))
                    .id();
                assets.tiles.insert((x, y), entity);
            }
        }
    }
    for (tile, mut vis) in &mut tiles {
        // set_if_neq: an unconditional write would dirty change detection
        // for the whole cover grid every frame.
        vis.set_if_neq(if fog.known.contains(&(tile.0, tile.1)) {
            Visibility::Hidden
        } else {
            Visibility::Visible
        });
    }

    // Blips: one pulsing marker per heard-only contact.
    let mut live: HashSet<u64> = HashSet::new();
    for (id, pos) in &fog.heard {
        live.insert(*id);
        let at = tile_top_xyz(world, *pos, 0.8);
        match assets.blips.get(id) {
            Some(&e) => {
                commands.entity(e).insert(Transform::from_translation(at));
            }
            None => {
                let e = commands
                    .spawn((
                        Blip,
                        Mesh3d(palette.gem.clone()),
                        MeshMaterial3d(assets.blip.clone()),
                        Transform::from_translation(at).with_scale(Vec3::splat(0.4)),
                    ))
                    .id();
                assets.blips.insert(*id, e);
            }
        }
    }
    let stale: Vec<u64> = assets.blips.keys().filter(|k| !live.contains(k)).copied().collect();
    for id in stale {
        if let Some(e) = assets.blips.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    // Survey rings: expand with the stance's current reach.
    let mut live_rings: HashSet<u32> = HashSet::new();
    for (bot_id, reach) in &fog.rings {
        live_rings.insert(*bot_id);
        let Some(bot) = world.bots.get(&sim::world::BotId(*bot_id)) else { continue };
        let at = tile_top_xyz(world, bot.data.pos, 0.1);
        let scale = (*reach as f32).max(1.0) * 2.0;
        match assets.rings.get(bot_id) {
            Some(&e) => {
                if let Ok((_, mut tf)) = rings.get_mut(e) {
                    tf.translation = at;
                    tf.scale = Vec3::new(scale, 1.0, scale);
                }
            }
            None => {
                let e = commands
                    .spawn((
                        SurveyRing,
                        Mesh3d(palette.sel_ring.clone()),
                        MeshMaterial3d(assets.ring.clone()),
                        Transform::from_translation(at)
                            .with_scale(Vec3::new(scale, 1.0, scale)),
                    ))
                    .id();
                assets.rings.insert(*bot_id, e);
            }
        }
    }
    let stale: Vec<u32> =
        assets.rings.keys().filter(|k| !live_rings.contains(k)).copied().collect();
    for id in stale {
        if let Some(e) = assets.rings.remove(&id) {
            commands.entity(e).despawn();
        }
    }
}

/// Apply one tile's fog state to a view entity: visibility plus (when the
/// entity reads as ground — `dims`) the live↔memory material swap. Shared
/// by `shade_terrain` and `gate_fogged_views` so the two can't drift.
/// Visibility goes through `set_if_neq`: an unconditional write would
/// dirty change detection for the whole grid every frame.
fn apply_tile_fog(
    state: TileFog,
    dims: bool,
    mut vis: Mut<Visibility>,
    mat: Option<Mut<MeshMaterial3d<StandardMaterial>>>,
    assets: &mut FogAssets,
    materials: &mut Assets<StandardMaterial>,
) {
    match state {
        TileFog::Unknown => {
            vis.set_if_neq(Visibility::Hidden);
        }
        TileFog::Visible => {
            vis.set_if_neq(Visibility::Inherited);
            if let Some(mut mat) = mat
                && let Some(bright) = assets.bright.get(&mat.0.id())
            {
                mat.0 = bright.clone();
            }
        }
        TileFog::Known => {
            vis.set_if_neq(Visibility::Inherited);
            // Already a twin? (Twin handles live in `bright`'s keys.)
            if dims
                && let Some(mut mat) = mat
                && !assets.bright.contains_key(&mat.0.id())
            {
                mat.0 = memory_twin(assets, materials, &mat.0.clone());
            }
        }
    }
}

/// Restyle the terrain layer per fog state: live materials while seen,
/// frozen memory twins while only remembered, hidden while undiscovered
/// (the black cover is all that renders there). Idempotent per frame, so
/// tiles rebuilt by `resync_terrain` (which respawn bright) are re-dimmed
/// the same frame.
pub(crate) fn shade_terrain(
    fog: Res<FogState>,
    mut assets: ResMut<FogAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tiles: Query<(&TerrainTile, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    if crate::screenshot_hides_fog() {
        return;
    }
    for (tile, vis, mat) in &mut tiles {
        apply_tile_fog(
            tile_fog(&fog, (tile.0, tile.1)),
            true,
            vis,
            Some(mat),
            &mut assets,
            &mut materials,
        );
    }
}

/// Fogged-object visibility: world objects on undiscovered tiles don't
/// exist to the viewer; remembered tiles keep showing what was found
/// there. Flat ground decals (bridges, one-way arrows, wrecks) swap to
/// memory twins so they dim with the terrain under them; standing objects
/// (printers, depots, blueprints, black boxes, paint marks) just stay
/// visible as remembered landmarks. Bots are perception-gated in
/// sync_view; ore gems discovery-gated there too.
pub(crate) fn gate_fogged_views(
    game: NonSend<GameSim>,
    fog: Res<FogState>,
    index: Res<ViewIndex>,
    mut assets: ResMut<FogAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut views: Query<(&mut Visibility, Option<&mut MeshMaterial3d<StandardMaterial>>)>,
) {
    if crate::screenshot_hides_fog() {
        return;
    }
    let world = &game.0.world;
    // (view entity, tile, dims-with-the-ground?)
    let mut targets: Vec<(Entity, TilePos, bool)> = Vec::new();
    for (id, p) in &world.printers {
        if let Some(&(e, _)) = index.printers.get(&id.0) {
            targets.push((e, p.pos, false));
        }
    }
    for (id, bp) in &world.blueprints {
        if let Some(&e) = index.blueprints.get(&id.0) {
            targets.push((e, bp.pos, false));
        }
    }
    // Q92 snapshot classes iterate the INDEX, not the world: a ghost (sim
    // object gone, removal unseen) keeps dimming/hiding with its tile.
    for &(e, pos) in index.depots.values() {
        targets.push((e, pos, false));
    }
    for &(e, pos) in index.wrecks.values() {
        targets.push((e, pos, true));
    }
    for &(e, pos) in index.black_boxes.values() {
        targets.push((e, pos, false));
    }
    for (&(x, y), &e) in &index.bridges {
        targets.push((e, TilePos::new(x, y), true));
    }
    for (&(x, y), &(e, _)) in &index.overlays {
        targets.push((e, TilePos::new(x, y), true));
    }
    for (&(x, y), &(e, _)) in &index.paint {
        targets.push((e, TilePos::new(x, y), false));
    }
    for (entity, pos, dims) in targets {
        let Ok((vis, mat)) = views.get_mut(entity) else { continue };
        apply_tile_fog(
            tile_fog(&fog, (pos.x, pos.y)),
            dims,
            vis,
            mat,
            &mut assets,
            &mut materials,
        );
    }
}

/// The heard-contact pulse: a position, not a picture — it breathes.
pub(crate) fn pulse_blips(time: Res<Time>, mut blips: Query<&mut Transform, With<Blip>>) {
    let s = 0.4 + 0.15 * (time.elapsed_secs() * 5.0).sin().abs();
    for mut tf in &mut blips {
        tf.scale = Vec3::splat(s);
    }
}
