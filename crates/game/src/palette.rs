//! Colors, materials, and procedural meshes: the game's entire "look".

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;
use sim::{TileKind, TilePos};
use std::collections::HashMap;


// ---------------------------------------------------------------- palette
// Lifted from the original prototype (see docs of that repo).
pub(crate) const CLEAR: Color = Color::srgb(0.05, 0.06, 0.09);
pub(crate) const ORE_GOLD: Color = Color::srgb(1.0, 0.85, 0.15);
pub(crate) const PRINT_GLOW: Color = Color::srgb(0.25, 0.55, 0.95);
pub(crate) const EXPLODE_ORANGE: Color = Color::srgb(1.0, 0.45, 0.1);

#[derive(Resource)]
pub(crate) struct Palette {
    pub(crate) unit_cube: Handle<Mesh>,
    pub(crate) nose_cube: Handle<Mesh>,
    pub(crate) gem: Handle<Mesh>,
    pub(crate) explode_cube: Handle<Mesh>,
    pub(crate) ore_mat: Handle<StandardMaterial>,
    pub(crate) black_mat: Handle<StandardMaterial>,
    pub(crate) explode_mat: Handle<StandardMaterial>,
    pub(crate) print_glow_mat: Handle<StandardMaterial>,
    pub(crate) nose_mat: Handle<StandardMaterial>,
    pub(crate) tile_slab: Handle<Mesh>,
    /// Bot body: cube whose faces sample cells of the team's 3x2 atlas.
    pub(crate) bot_cube: Handle<Mesh>,
    pub(crate) bot_tex_mats: HashMap<u8, Handle<StandardMaterial>>,
    /// Printer body: same atlas treatment, squat box.
    pub(crate) printer_box: Handle<Mesh>,
    pub(crate) printer_tex_mats: HashMap<u8, Handle<StandardMaterial>>,
    pub(crate) printer_ruined_mat: Handle<StandardMaterial>,
    /// The sheet rising from a working printer's top slot.
    pub(crate) paper_sheet: Handle<Mesh>,
    pub(crate) paper_mat: Handle<StandardMaterial>,
    /// Depot: a low wooden crate (bots stand on the depot tile, so it
    /// stays pallet-height).
    pub(crate) crate_box: Handle<Mesh>,
    pub(crate) crate_mat: Handle<StandardMaterial>,
    /// Sub-tile textured slab for wrecks.
    pub(crate) pad_slab: Handle<Mesh>,
    pub(crate) wreck_tex_mat: Handle<StandardMaterial>,
    /// Terrain slab: full tile texture on top, dark trim on the sides.
    pub(crate) tex_slab: Handle<Mesh>,
    /// The "tech" tile — terraformed ground (unused by natural terrain).
    pub(crate) ground_tex_mat: Handle<StandardMaterial>,
    pub(crate) bridge_tex_mat: Handle<StandardMaterial>,
    pub(crate) oneway_tex_mat: Handle<StandardMaterial>,
    pub(crate) grass_tex_mat: Handle<StandardMaterial>,
    pub(crate) water_tex_mat: Handle<StandardMaterial>,
    pub(crate) mountain_tex_mat: Handle<StandardMaterial>,
    /// Full-height block for mountain (Rubble) tiles.
    pub(crate) mountain_block: Handle<Mesh>,
    pub(crate) preview_valid_mat: Handle<StandardMaterial>,
    pub(crate) preview_invalid_mat: Handle<StandardMaterial>,
    pub(crate) preview_chevron_mat: Handle<StandardMaterial>,
    pub(crate) paint_mats: [Handle<StandardMaterial>; 4],
    pub(crate) bar_mesh: Handle<Mesh>,
    pub(crate) bar_bg_mat: Handle<StandardMaterial>,
    pub(crate) bar_fill_mat: Handle<StandardMaterial>,
    /// Health-fill gradient bins: index 0 = empty/red .. last = full/green.
    pub(crate) bar_health_grad: Vec<Handle<StandardMaterial>>,
    pub(crate) bar_trail_mat: Handle<StandardMaterial>,
    pub(crate) scribble_quad: Handle<Mesh>,
    /// Mood-cloud frames, keyed "angry" / "error" / "hurt" / "death".
    pub(crate) scribble_mats: HashMap<&'static str, Vec<Handle<StandardMaterial>>>,
    pub(crate) sel_ring: Handle<Mesh>,
    pub(crate) sel_mat: Handle<StandardMaterial>,
}

/// LMB click-vs-drag disambiguation while a tool is armed: a press is the
/// tool's click only if the cursor stays inside the dead zone; traveling
/// past it turns the gesture into a camera pan instead.

// -------------------------------------------------------- textured meshes

/// Axis-aligned box with an explicit UV rectangle `[u0, v0, u1, v1]` per
/// face, ordered [front(-Z), right(+X), back(+Z), left(-X), top(+Y),
/// bottom(-Y)]. Front is -Z so it matches the nose child and the facing
/// math in `update_poses`; image-up on the top face is the bot's forward.
pub(crate) fn box_with_face_uvs(half: Vec3, face_uvs: [[f32; 4]; 6]) -> Mesh {
    // (outward normal, texture-right, texture-up), chosen so r x u = n.
    const AXES: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for ((n, r, u), [u0, v0, u1, v1]) in AXES.into_iter().zip(face_uvs) {
        let center = n * n.abs().dot(half);
        let rv = r * r.abs().dot(half);
        let uv = u * u.abs().dot(half);
        let base = positions.len() as u32;
        for (p, tex) in [
            (center - rv - uv, [u0, v1]), // bottom-left
            (center + rv - uv, [u1, v1]), // bottom-right
            (center + rv + uv, [u1, v0]), // top-right
            (center - rv + uv, [u0, v0]), // top-left
        ] {
            positions.push(p.to_array());
            normals.push(n.to_array());
            uvs.push(tex);
        }
        indices.extend([base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// A body box (bot, printer, ...): each face samples its cell of a 3x2
/// atlas (front/right/back over left/top/bottom — the layout the build.rs
/// bake emits).
pub(crate) fn atlas_box_mesh(half: Vec3) -> Mesh {
    let cell =
        |c: f32, r: f32| [c / 3.0, r / 2.0, (c + 1.0) / 3.0, (r + 1.0) / 2.0];
    box_with_face_uvs(
        half,
        [
            cell(0.0, 0.0),
            cell(1.0, 0.0),
            cell(2.0, 0.0),
            cell(0.0, 1.0),
            cell(1.0, 1.0),
            cell(2.0, 1.0),
        ],
    )
}

/// Textured slab (terrain tile, depot pad, wreck): the full texture on top
/// (image-up = north, so directional art points east until the transform
/// spins it); sides and bottom sample a sliver of the texture's border so
/// they read as dark trim.
pub(crate) fn textured_slab_mesh(half: Vec3) -> Mesh {
    const EDGE: [f32; 4] = [0.005, 0.45, 0.02, 0.55];
    box_with_face_uvs(half, [EDGE, EDGE, EDGE, EDGE, [0.0, 0.0, 1.0, 1.0], EDGE])
}

/// Render height of a mountain (Rubble) block's top face; other terrain
/// tops sit at 0.0 (water slightly below). Bots, overlays, and paint all
/// ride the terrain they're on.
pub(crate) const MOUNTAIN_TOP: f32 = 0.25;

/// Top surface of the tile at `pos` in render space.
pub(crate) fn terrain_top(world: &sim::World, pos: TilePos) -> f32 {
    match world.grid.get(pos) {
        Some(TileKind::Rubble) => MOUNTAIN_TOP,
        Some(TileKind::Water) => -0.05,
        _ => 0.0,
    }
}

/// Mountain block, mapped into the baked `mountain_atlas` (peaks in the
/// left cell, rock-face strata in the right): summit art on top, strata on
/// every side (the bottom face is never seen).
pub(crate) fn mountain_block_mesh() -> Mesh {
    const TOP: [f32; 4] = [0.0, 0.0, 0.5, 1.0];
    const SIDE: [f32; 4] = [0.5, 0.0, 1.0, 1.0];
    const EDGE: [f32; 4] = [0.51, 0.45, 0.53, 0.55];
    box_with_face_uvs(
        Vec3::new(0.48, MOUNTAIN_TOP / 2.0 + 0.025, 0.48),
        [SIDE, SIDE, SIDE, SIDE, TOP, EDGE],
    )
}
