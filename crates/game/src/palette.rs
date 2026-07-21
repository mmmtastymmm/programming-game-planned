//! Colors, materials, and procedural meshes: the game's entire "look".

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;
use sim::{TileKind, TilePos};
use std::collections::HashMap;


// ---------------------------------------------------------------- palette
// Lifted from the original prototype (see docs of that repo).
pub(crate) const CLEAR: Color = Color::srgb(0.05, 0.06, 0.09);
pub(crate) const ORE_GOLD: Color = Color::srgb(1.0, 0.85, 0.15);
pub(crate) const PRINT_GLOW: Color = Color::srgb(0.25, 0.55, 0.95);
pub(crate) const EXPLODE_ORANGE: Color = Color::srgb(1.0, 0.45, 0.1);

/// Bot cube half-extent (tiles are 1.0 apart; two adjacent bots leave a
/// `1.0 - 2*BOT_HALF` gap between faces).
pub(crate) const BOT_HALF: f32 = 0.35;
/// Camera-lens nose geometry, shared by the meshes (scene.rs) and the
/// spawn transforms (view.rs) so the clip test below checks the real
/// numbers. The barrel sits half-sunk: centered on the face plane.
pub(crate) const LENS_Y: f32 = 0.044;
pub(crate) const LENS_BARREL_RADIUS: f32 = 0.115;
pub(crate) const LENS_BARREL_LEN: f32 = 0.24;
pub(crate) const LENS_BARREL_Z: f32 = -BOT_HALF;
pub(crate) const LENS_GLASS_RADIUS: f32 = 0.08;
pub(crate) const LENS_GLASS_LEN: f32 = 0.02;
/// Glass cap: seated on the barrel tip, overlapping it by a quarter of
/// its own thickness so there's no gap.
pub(crate) const LENS_GLASS_Z: f32 =
    LENS_BARREL_Z - LENS_BARREL_LEN / 2.0 - LENS_GLASS_LEN / 4.0;

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
    /// Camera-lens nose: gunmetal barrel + team-accent glass front,
    /// aligned with the lens flange drawn on the front-face atlas.
    pub(crate) lens_barrel: Handle<Mesh>,
    pub(crate) lens_glass: Handle<Mesh>,
    pub(crate) lens_barrel_mat: Handle<StandardMaterial>,
    /// Keyed like `bot_tex_mats` (color 0 = green, 1 = red, 2+ = blue).
    pub(crate) lens_glass_mats: HashMap<u8, Handle<StandardMaterial>>,
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
    /// Autotiled terrain, indexed by the NESW same-neighbor bitmask
    /// (bit 0 = N … bit 3 = W; set = that neighbor is the same terrain):
    /// grass grows a sandy fringe, water grows banks, mountain summits
    /// grow a cliff rim on their "different" sides.
    pub(crate) grass_tex_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) water_tex_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) mountain_tex_mats: Vec<Handle<StandardMaterial>>,
    /// Static autotiled terrains: mud grows a dried crust, corruption a
    /// creep frontier, ore a broken-rock lip, crystal a frost band.
    pub(crate) mud_tex_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) corruption_tex_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) ore_tex_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) crystal_tex_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) snow_tex_mats: Vec<Handle<StandardMaterial>>,
    /// High-ground plateau: atlas pairs like the mountain (top + rock
    /// face), rendered on the same block mesh.
    pub(crate) highground_tex_mats: Vec<Handle<StandardMaterial>>,
    /// Raw-resource terrains (docs/03), keyed by `TileKind::as_u8`:
    /// autotile sets and corner-nub overlays. Ground art only until the
    /// Q69 sim migration wires nodes and recipes.
    pub(crate) resource_tex_mats: HashMap<u8, Vec<Handle<StandardMaterial>>>,
    pub(crate) resource_corner_mats: HashMap<u8, Vec<Handle<StandardMaterial>>>,
    /// Geothermal vent: a point feature, no autotiling; the crater glows.
    pub(crate) vent_tex_mat: Handle<StandardMaterial>,
    /// Ambient animation frames, indexed `[frame][mask]`. Every tile of a
    /// (terrain, mask) shares one material, so retargeting one material
    /// set's textures animates the whole map (see animate_terrain).
    pub(crate) grass_frames: Vec<Vec<Handle<Image>>>,
    pub(crate) water_frames: Vec<Vec<Handle<Image>>>,
    pub(crate) mud_frames: Vec<Vec<Handle<Image>>>,
    pub(crate) corruption_frames: Vec<Vec<Handle<Image>>>,
    pub(crate) ore_frames: Vec<Vec<Handle<Image>>>,
    pub(crate) crystal_frames: Vec<Vec<Handle<Image>>>,
    pub(crate) snow_frames: Vec<Vec<Handle<Image>>>,
    /// Vent pulse frames (one tile, no autotile). The animator retargets
    /// base AND emissive so the crater actually glows brighter.
    pub(crate) vent_frames: Vec<Handle<Image>>,
    /// Transparent scree overlays for grass at a mountain's base, indexed
    /// by the same NESW mask (bit unset = mountain on that side).
    pub(crate) scree_mats: Vec<Handle<StandardMaterial>>,
    /// Inner-corner nub overlays, indexed by a NW/NE/SE/SW corner mask
    /// (bit unset = both flanks match but the diagonal doesn't — nub
    /// there). One set per transition kind.
    pub(crate) water_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) grass_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) scree_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) mountain_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) mud_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) corruption_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) ore_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) crystal_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) snow_corner_mats: Vec<Handle<StandardMaterial>>,
    pub(crate) highground_corner_mats: Vec<Handle<StandardMaterial>>,
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
    /// Cycle-bar gradient bins: index 0 = just started saving (dark blue)
    /// .. last = about to execute (bright cyan-white).
    pub(crate) bar_cycle_grad: Vec<Handle<StandardMaterial>>,
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

/// Render height of a mountain (Rubble) or high-ground block's top face;
/// other terrain tops sit at 0.0 (water and mud slightly below). Bots,
/// overlays, and paint all ride the terrain they're on.
pub(crate) const MOUNTAIN_TOP: f32 = 0.25;

/// Top surface of the tile at `pos` in render space.
pub(crate) fn terrain_top(world: &sim::World, pos: TilePos) -> f32 {
    match world.grid.get(pos) {
        // Mountain took the full block from Rubble in M8 (Rubble is low
        // debris now); Barricades are wall-height built mass.
        Some(TileKind::Mountain | TileKind::HighGround | TileKind::Barricade) => MOUNTAIN_TOP,
        Some(TileKind::Water) => -0.05,
        Some(TileKind::Ford) => -0.03,
        Some(TileKind::Mud | TileKind::Scree) => -0.02,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Two bots on adjacent tiles staring at each other must not touch:
    /// everything past the face plane has to fit in half the inter-face
    /// gap, with a visible sliver of daylight to spare. This is pure
    /// arithmetic on the same constants the meshes and spawn transforms
    /// use, so any future lens resizing re-runs the collision math.
    #[test]
    fn facing_lenses_never_clip() {
        const TILE_PITCH: f32 = 1.0; // tile_xyz: one world unit per tile
        const DAYLIGHT: f32 = 0.02; // minimum visible gap between lens tips

        let gap = TILE_PITCH - 2.0 * BOT_HALF;
        let tip = LENS_GLASS_Z - LENS_GLASS_LEN / 2.0; // most negative point
        let protrusion = -tip - BOT_HALF;

        assert!(protrusion > 0.0, "the lens should protrude past the face at all");
        assert!(
            2.0 * protrusion + DAYLIGHT <= gap,
            "facing bots clip (or touch): 2 x {protrusion} + {DAYLIGHT} > gap {gap}"
        );
        // The glass must cap the barrel, not float in front of it.
        let barrel_tip = LENS_BARREL_Z - LENS_BARREL_LEN / 2.0;
        assert!(
            LENS_GLASS_Z + LENS_GLASS_LEN / 2.0 > barrel_tip,
            "glass is detached from the barrel"
        );
    }
}
