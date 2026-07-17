//! Fog of war (M7, docs/05): the viewer sees the world as FACTION 0
//! perceives it. Pure view layer — no sim state, no replay exposure:
//! currently-seen tiles render live, previously-seen tiles render as a
//! greyed snapshot (ambient animations frozen elsewhere), never-seen
//! tiles are dark. Heard-only enemy contacts render as pulsing blips —
//! a position, not a picture — and a searching bot shows its expanding
//! survey ring.

use bevy::prelude::*;
use sim::perception::los_clear;
use sim::TilePos;
use std::collections::{HashMap, HashSet};

use crate::palette::Palette;
use crate::scene::tile_xyz;
use crate::GameSim;

/// The viewer faction whose perception the fog renders.
const VIEWER: u8 = 0;

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

/// One fog overlay quad (per tile).
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
    known: Handle<StandardMaterial>,
    blip: Handle<StandardMaterial>,
    ring: Handle<StandardMaterial>,
    tiles: HashMap<(i32, i32), Entity>,
    blips: HashMap<u64, Entity>,
    rings: HashMap<u32, Entity>,
}

pub(crate) fn setup_fog(
    mut assets: ResMut<FogAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    assets.quad = meshes.add(Cuboid::new(1.0, 0.02, 1.0));
    let mut dark = StandardMaterial::from(Color::srgba(0.02, 0.02, 0.05, 0.93));
    dark.alpha_mode = AlphaMode::Blend;
    dark.unlit = true;
    assets.unknown = materials.add(dark);
    let mut grey = StandardMaterial::from(Color::srgba(0.05, 0.05, 0.09, 0.55));
    grey.alpha_mode = AlphaMode::Blend;
    grey.unlit = true;
    assets.known = materials.add(grey);
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
            + sim::perception::high_ground_bonus(&world.grid, bot.data.pos);
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
    for d in world.depots.values() {
        eyes.push((d.pos, s, false)); // factionless: viewer's eyes (see sim note)
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

/// Spawn-or-update the overlay quads, blips, and rings to match FogState.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_fog(
    mut commands: Commands,
    game: NonSend<GameSim>,
    fog: Res<FogState>,
    mut assets: ResMut<FogAssets>,
    palette: Res<Palette>,
    mut tiles: Query<(&FogTile, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>)>,
    mut rings: Query<(&SurveyRing, &mut Transform), Without<Blip>>,
) {
    let world = &game.0.world;
    // Lazily cover the whole grid once.
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
                            1.35, // above terrain, below the camera
                        )),
                    ))
                    .id();
                assets.tiles.insert((x, y), entity);
            }
        }
    }
    for (tile, mut vis, mut mat) in &mut tiles {
        let key = (tile.0, tile.1);
        if fog.visible.contains(&key) {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Visible;
            let want = if fog.known.contains(&key) {
                &assets.known
            } else {
                &assets.unknown
            };
            if mat.0 != *want {
                mat.0 = want.clone();
            }
        }
    }

    // Blips: one pulsing marker per heard-only contact.
    let mut live: HashSet<u64> = HashSet::new();
    for (id, pos) in &fog.heard {
        live.insert(*id);
        let at = tile_xyz(world, *pos, 0.8);
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
        let at = tile_xyz(world, bot.data.pos, 0.1);
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

/// The heard-contact pulse: a position, not a picture — it breathes.
pub(crate) fn pulse_blips(time: Res<Time>, mut blips: Query<&mut Transform, With<Blip>>) {
    let s = 0.4 + 0.15 * (time.elapsed_secs() * 5.0).sin().abs();
    for mut tf in &mut blips {
        tf.scale = Vec3::splat(s);
    }
}
