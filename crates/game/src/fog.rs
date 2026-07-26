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
use sim::TilePos;
use std::collections::{HashMap, HashSet};

use crate::palette::Palette;
use crate::scene::{tile_top_xyz, tile_xyz, TerrainTile};
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

/// Fog gating contract for a world-object view (2026-07-25 review):
/// attached AT SPAWN in `sync_view`, so a new spawn path can't silently
/// skip fog gating — `gate_fogged_views` queries the component instead
/// of hand-enumerated registries. `dims` marks flat ground decals that
/// swap to memory-twin materials; standing objects stay visible as
/// remembered landmarks. Ghosts (sim object gone, removal unseen) keep
/// their component and so keep dimming/hiding with their tile.
#[derive(Component)]
pub(crate) struct FogGated {
    pub(crate) pos: TilePos,
    pub(crate) dims: bool,
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

/// Read the viewer faction's tile knowledge straight from the sim (Q94):
/// the perception phase owns both unions now — the live seeing circle
/// (`visible_tiles`, derived per tick) and the durable known-tiles chart
/// (sim state, hashed, survives save/load). The renderer stopped
/// mirroring the eye math the day the sim started publishing it.
pub(crate) fn recompute_fog(game: NonSend<GameSim>, mut fog: ResMut<FogState>) {
    let world = &game.0.world;
    fog.visible.clear();
    if let Some(visible) = world.visible_tiles.get(&VIEWER) {
        fog.visible.extend(visible.iter().map(|t| (t.x, t.y)));
    }
    fog.known.clear();
    if let Some(known) = world.known_tiles.get(&VIEWER) {
        fog.known.extend(known.iter().map(|t| (t.x, t.y)));
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
/// visible as remembered landmarks. Every gated view carries a
/// [`FogGated`] component attached at spawn (2026-07-25 review — a spawn
/// path that forgets it is now the exception, not the default), ghosts
/// included. Bots are perception-gated in sync_view; ore gems
/// discovery-gated there too.
pub(crate) fn gate_fogged_views(
    fog: Res<FogState>,
    mut assets: ResMut<FogAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut views: Query<(
        &FogGated,
        &mut Visibility,
        Option<&mut MeshMaterial3d<StandardMaterial>>,
    )>,
) {
    if crate::screenshot_hides_fog() {
        return;
    }
    for (gated, vis, mat) in &mut views {
        apply_tile_fog(
            tile_fog(&fog, (gated.pos.x, gated.pos.y)),
            gated.dims,
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
