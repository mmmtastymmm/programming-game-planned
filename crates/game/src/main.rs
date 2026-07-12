//! Colony viewer & editor, modeled on the original prototype's look:
//! bright emissive primitives on a dark void, orbit camera, and a live
//! Pyrite editor panel on the left. No 3D models — bots are cubes whose six
//! faces sample a baked SVG atlas, and terrain slabs sample baked SVG tiles
//! (`assets/art/*.svg` -> `cargo bake` -> `assets/textures/*.png`). Team
//! identity is a palette swap done at bake time.
//!
//! The sim steps on `FixedUpdate` at 10 Hz (docs/07 tick rate); rendering
//! free-runs with per-frame interpolation between ticks. All game state
//! lives in `sim::Sim` (NonSend — the VMs hold `Rc`s); the UI only ever
//! mutates it through `Command`s, like any other lockstep peer.
//!
//! Camera: RMB drag = orbit · Shift+RMB / MMB = pan · scroll = zoom.
//! Run: `cargo run -p game`

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::map::{Direction, OverlayKind};
use sim::world::{BlueprintKind, Color as BotColor, PrinterState};
use sim::{TileKind, TilePos};
use std::collections::{HashMap, HashSet};

/// The default colony program: service any blueprint first, then mine.
/// (Uses `if` — the dev sandbox runs with all constructs unlocked; the
/// doc's true Tier-0 starter is the four mining lines alone.)
const DEFAULT_PROGRAM: &str = "\
if blueprint_exists():
    move_to(nearest_blueprint())
    build()
move_to(nearest_ore())
mine()
move_to(nearest_depot())
deposit()
";

struct GameSim(Sim);

// ---------------------------------------------------------------- palette
// Lifted from the original prototype (see docs of that repo).
const CLEAR: Color = Color::srgb(0.05, 0.06, 0.09);
const RUBBLE: Color = Color::srgb(0.45, 0.46, 0.50);
const WATER: Color = Color::srgb(0.10, 0.28, 0.55);
const ORE_GOLD: Color = Color::srgb(1.0, 0.85, 0.15);
const DEPOT_BLUE: Color = Color::srgb(0.30, 0.55, 0.75);
const PRINT_GLOW: Color = Color::srgb(0.25, 0.55, 0.95);
const EXPLODE_ORANGE: Color = Color::srgb(1.0, 0.45, 0.1);
const WRECK_GRAY: Color = Color::srgb(0.28, 0.28, 0.30);

fn bot_body_color(color: BotColor) -> (Color, LinearRgba) {
    match color {
        BotColor::GREEN => (Color::srgb(0.30, 0.85, 0.30), LinearRgba::new(0.03, 0.25, 0.03, 1.0)),
        BotColor::RED => (Color::srgb(0.95, 0.30, 0.25), LinearRgba::new(0.30, 0.04, 0.03, 1.0)),
        _ => (Color::srgb(0.40, 0.50, 0.95), LinearRgba::new(0.05, 0.08, 0.30, 1.0)),
    }
}

// ------------------------------------------------------------- components

#[derive(Component)]
struct OrbitCam {
    focus: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

/// Fixed-tick pose targets; per-frame lerp between them.
#[derive(Component)]
struct Pose {
    prev: Vec3,
    curr: Vec3,
    grid: TilePos,
    /// Last fault_count seen; a rise triggers the fault hop.
    fault_seen: u64,
    /// Fixed ticks since the last fault.
    fault_age: u32,
}

/// The translucent placement ghost (slab + one-way chevron children).
#[derive(Component)]
struct PreviewSlab;
#[derive(Component)]
struct PreviewStrip;
#[derive(Component)]
struct PreviewTip;

/// Marks a bot view's carry-indicator child (slot index).
#[derive(Component)]
struct CarrySlot(u32);

/// Marks a printer view's floating print-job cube.
#[derive(Component)]
struct JobCube;

#[derive(Component)]
struct Spinner(f32);

#[derive(Component)]
struct Explosion {
    age: f32,
}

// -------------------------------------------------------------- resources

#[derive(Resource, Default)]
struct ViewIndex {
    bots: HashMap<u32, Entity>,
    ore: HashMap<u64, Entity>,
    wrecks: HashMap<u32, Entity>,
    black_boxes: usize,
    printers: HashMap<u64, (Entity, PrinterState)>,
    blueprints: HashMap<u64, Entity>,
    bridges: HashSet<(i32, i32)>,
    overlays: HashMap<(i32, i32), (Entity, OverlayKind)>,
    paint: HashMap<(i32, i32), (Entity, u8)>,
}

#[derive(Resource)]
struct Palette {
    unit_cube: Handle<Mesh>,
    nose_cube: Handle<Mesh>,
    gem: Handle<Mesh>,
    slab: Handle<Mesh>,
    printer_body: Handle<Mesh>,
    explode_cube: Handle<Mesh>,
    ore_mat: Handle<StandardMaterial>,
    wreck_mat: Handle<StandardMaterial>,
    black_mat: Handle<StandardMaterial>,
    explode_mat: Handle<StandardMaterial>,
    print_glow_mat: Handle<StandardMaterial>,
    nose_mat: Handle<StandardMaterial>,
    bot_mats: HashMap<u8, Handle<StandardMaterial>>,
    ruined_mat: Handle<StandardMaterial>,
    tile_slab: Handle<Mesh>,
    /// Bot body: cube whose faces sample cells of the team's 3x2 atlas.
    bot_cube: Handle<Mesh>,
    bot_tex_mats: HashMap<u8, Handle<StandardMaterial>>,
    /// Terrain slab: full tile texture on top, dark trim on the sides.
    tex_slab: Handle<Mesh>,
    ground_tex_mat: Handle<StandardMaterial>,
    bridge_tex_mat: Handle<StandardMaterial>,
    oneway_tex_mat: Handle<StandardMaterial>,
    preview_valid_mat: Handle<StandardMaterial>,
    preview_invalid_mat: Handle<StandardMaterial>,
    preview_chevron_mat: Handle<StandardMaterial>,
    paint_mats: [Handle<StandardMaterial>; 4],
}

#[derive(Resource)]
struct EditorState {
    code: String,
    status: String,
    status_ok: bool,
    /// Armed build-bar tool (Esc cancels).
    selected_build: Option<ToolKind>,
    /// Last tile painted during a drag (avoids re-sending every frame).
    last_paint_tile: Option<TilePos>,
    /// Selected category tab in the build bar.
    build_category: usize,
    /// Procedurally-drawn item icons, keyed by item name.
    icons: HashMap<&'static str, egui::TextureHandle>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            code: DEFAULT_PROGRAM.to_string(),
            status: "ready".into(),
            status_ok: true,
            selected_build: None,
            last_paint_tile: None,
            build_category: 0,
            icons: HashMap::new(),
        }
    }
}

/// What an armed build-bar item does on click.
#[derive(Clone, Copy, PartialEq)]
enum ToolKind {
    /// Blueprint construction (bots do the labor).
    Building(BlueprintKind),
    /// Instant traffic signage on any tile; None = eraser.
    Overlay(Option<OverlayKind>),
    /// Instant cosmetic tile paint (drag to paint); None = eraser.
    Paint(Option<u8>),
}

struct BuildItem {
    name: &'static str,
    kind: ToolKind,
}

/// Paint palette (index -> display color).
const PAINT_COLORS: [(u8, u8, u8); 4] =
    [(220, 60, 50), (70, 200, 80), (70, 120, 230), (235, 200, 60)];

const BUILD_CATEGORIES: &[(&str, &[BuildItem])] = &[
    (
        "Buildings",
        &[BuildItem { name: "Bridge", kind: ToolKind::Building(BlueprintKind::Bridge) }],
    ),
    (
        "Overlay",
        &[
            BuildItem {
                name: "Arrow",
                kind: ToolKind::Overlay(Some(OverlayKind::Arrow(Direction::East))),
            },
            BuildItem { name: "Clear Overlay", kind: ToolKind::Overlay(None) },
        ],
    ),
    (
        "Paint",
        &[
            BuildItem { name: "Red Paint", kind: ToolKind::Paint(Some(0)) },
            BuildItem { name: "Green Paint", kind: ToolKind::Paint(Some(1)) },
            BuildItem { name: "Blue Paint", kind: ToolKind::Paint(Some(2)) },
            BuildItem { name: "Yellow Paint", kind: ToolKind::Paint(Some(3)) },
            BuildItem { name: "Clear Paint", kind: ToolKind::Paint(None) },
        ],
    ),
];

/// Same catalog item, ignoring per-placement state (arrow rotation).
fn same_item(a: ToolKind, b: ToolKind) -> bool {
    match (a, b) {
        (
            ToolKind::Overlay(Some(OverlayKind::Arrow(_))),
            ToolKind::Overlay(Some(OverlayKind::Arrow(_))),
        ) => true,
        _ => a == b,
    }
}

/// 48x48 pixel-art icon for a build item, drawn in code (no asset files;
/// matches the primitive look).
fn build_icon(name: &str) -> egui::ColorImage {
    let s = 48usize;
    let water = egui::Color32::from_rgb(26, 72, 140);
    let mut img = egui::ColorImage::new([s, s], water);
    if name == "Bridge" {
        let plank_light = egui::Color32::from_rgb(150, 108, 60);
        let plank_dark = egui::Color32::from_rgb(122, 86, 46);
        let rail = egui::Color32::from_rgb(92, 64, 34);
        for y in 15..33 {
            for x in 2..46 {
                let c = if (x / 7) % 2 == 0 { plank_light } else { plank_dark };
                img[(x, y)] = c;
            }
        }
        for x in 2..46 {
            img[(x, 13)] = rail;
            img[(x, 14)] = rail;
            img[(x, 33)] = rail;
            img[(x, 34)] = rail;
        }
        // Pylons into the water.
        for y in 35..44 {
            for x in [6usize, 7, 23, 24, 40, 41] {
                img[(x, y)] = rail;
            }
        }
    }
    if name == "Arrow" {
        // Neutral ground under the arrow glyph.
        let ground = egui::Color32::from_rgb(70, 92, 66);
        for x in 0..s {
            for y in 0..s {
                img[(x, y)] = ground;
            }
        }
    }
    if let Some(stripped) = name.strip_suffix(" Paint") {
        let rgb = match stripped {
            "Red" => PAINT_COLORS[0],
            "Green" => PAINT_COLORS[1],
            "Blue" => PAINT_COLORS[2],
            "Yellow" => PAINT_COLORS[3],
            _ => (230, 230, 230), // "Clear" handled below
        };
        let c = egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2);
        for x in 4..44 {
            for y in 4..44 {
                img[(x, y)] = c;
            }
        }
    }
    if name.starts_with("Clear") {
        // Checkerboard + red X = eraser.
        for x in 0..s {
            for y in 0..s {
                let light = ((x / 8) + (y / 8)) % 2 == 0;
                img[(x, y)] = if light {
                    egui::Color32::from_rgb(200, 200, 205)
                } else {
                    egui::Color32::from_rgb(150, 150, 158)
                };
            }
        }
        let red = egui::Color32::from_rgb(210, 50, 40);
        for i in 6..42usize {
            for w in 0..3usize {
                img[(i, (i + w).min(47))] = red;
                img[(i, (47 - i + w).min(47))] = red;
            }
        }
    }
    if name == "Arrow" || name == "One-way Bridge" {
        // Bold arrow across the planks.
        let glow = egui::Color32::from_rgb(255, 235, 130);
        for x in 8..32 {
            for y in 21..27 {
                img[(x, y)] = glow;
            }
        }
        for i in 0..9usize {
            for y in (15 + i)..(33 - i) {
                img[(31 + i, y)] = glow;
            }
        }
    }
    img
}

// ------------------------------------------------------------------ world

fn build_colony() -> Sim {
    let mut spec = MapSpec::empty(24, 14);
    for y in 2..9 {
        spec.rubble.push(TilePos::new(12, y));
    }
    // A water wall fully splits the map: the ONLY way east is bridges the
    // player builds — one-way pairs make deadlock-free crossings.
    for y in 0..14 {
        spec.water.push(TilePos::new(16, y));
    }
    // Modest west-side ore keeps the colony alive pre-bridge.
    spec.ore_nodes.push((TilePos::new(8, 3), 25));
    spec.ore_nodes.push((TilePos::new(20, 3), 60));
    spec.ore_nodes.push((TilePos::new(19, 11), 40));
    spec.depots.push(TilePos::new(3, 7));
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 5),
        faction: 0,
        color: 0,
        ruined: false,
        desired_max: 4,
    });
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 9),
        faction: 0,
        color: 1,
        ruined: true,
        desired_max: 0,
    });
    spec.starting_ore = 30;

    let mut game = Sim::new(&spec);
    game.apply(&Command::DeployProgram {
        faction: 0,
        color: BotColor::GREEN,
        source: DEFAULT_PROGRAM.into(),
    })
    .expect("miner program parses");
    game
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "programming game".into(),
                resolution: (1280.0, 800.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin { enable_multipass_for_primary_context: false })
        .insert_resource(ClearColor(CLEAR))
        .insert_resource(Time::<Fixed>::from_hz(10.0))
        .insert_resource(ViewIndex::default())
        .insert_resource(EditorState::default())
        .add_systems(Startup, (setup_sim, setup_scene).chain())
        .add_systems(FixedUpdate, (step_sim, update_poses).chain())
        .add_systems(
            Update,
            (
                editor_ui,
                orbit_camera,
                place_blueprint,
                build_preview,
                sync_view,
                interpolate,
                spin,
                animate_job_cubes,
                animate_explosions,
            )
                .chain(),
        )
        .run();
}

/// Tile -> world: XZ plane, one unit per tile, map centered at the origin.
fn tile_xyz(world: &sim::World, pos: TilePos, y: f32) -> Vec3 {
    Vec3::new(
        pos.x as f32 - world.grid.width as f32 / 2.0,
        y,
        pos.y as f32 - world.grid.height as f32 / 2.0,
    )
}

fn setup_sim(world: &mut World) {
    world.insert_non_send_resource(GameSim(build_colony()));
}

// -------------------------------------------------------- textured meshes

/// Axis-aligned box with an explicit UV rectangle `[u0, v0, u1, v1]` per
/// face, ordered [front(-Z), right(+X), back(+Z), left(-X), top(+Y),
/// bottom(-Y)]. Front is -Z so it matches the nose child and the facing
/// math in `update_poses`; image-up on the top face is the bot's forward.
fn box_with_face_uvs(half: Vec3, face_uvs: [[f32; 4]; 6]) -> Mesh {
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

/// The bot body: each face samples its cell of the 3x2 team atlas
/// (front/right/back over left/top/bottom — the layout `cargo bake` emits).
fn bot_cube_mesh() -> Mesh {
    let cell =
        |c: f32, r: f32| [c / 3.0, r / 2.0, (c + 1.0) / 3.0, (r + 1.0) / 2.0];
    box_with_face_uvs(
        Vec3::splat(0.35),
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

/// Terrain slab: the full tile texture on top (image-up = north, so the
/// one-way chevrons point east until the transform spins them); sides and
/// bottom sample a sliver of the tile's border so they read as dark trim.
fn tex_slab_mesh() -> Mesh {
    const EDGE: [f32; 4] = [0.005, 0.45, 0.02, 0.55];
    box_with_face_uvs(
        Vec3::new(0.48, 0.05, 0.48),
        [EDGE, EDGE, EDGE, EDGE, [0.0, 0.0, 1.0, 1.0], EDGE],
    )
}

fn setup_scene(
    mut commands: Commands,
    game: NonSend<GameSim>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let world = &game.0.world;

    let mut bot_mats = HashMap::new();
    for c in [0u8, 1, 2] {
        let (base, emissive) = bot_body_color(BotColor(c));
        bot_mats.insert(
            c,
            materials.add(StandardMaterial {
                base_color: base,
                emissive,
                metallic: 0.1,
                perceptual_roughness: 0.4,
                ..default()
            }),
        );
    }
    // Baked-atlas bot bodies; the emissive texture keeps the "glowing
    // primitives" identity without washing out the face art.
    let mut bot_tex_mats = HashMap::new();
    for (c, team) in [(0u8, "green"), (1, "red"), (2, "blue")] {
        let atlas: Handle<Image> =
            asset_server.load(format!("textures/bot_atlas_{team}.png"));
        bot_tex_mats.insert(
            c,
            materials.add(StandardMaterial {
                base_color_texture: Some(atlas.clone()),
                emissive: LinearRgba::new(0.35, 0.35, 0.35, 1.0),
                emissive_texture: Some(atlas),
                metallic: 0.1,
                perceptual_roughness: 0.5,
                ..default()
            }),
        );
    }
    let tile_tex_mat = |materials: &mut Assets<StandardMaterial>, tex: Handle<Image>| {
        materials.add(StandardMaterial {
            base_color_texture: Some(tex),
            perceptual_roughness: 0.85,
            ..default()
        })
    };
    let ground_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_ground.png"));
    let bridge_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_bridge.png"));
    let oneway_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_oneway.png"));
    let palette = Palette {
        unit_cube: meshes.add(Cuboid::new(0.7, 0.7, 0.7)),
        nose_cube: meshes.add(Cuboid::new(0.22, 0.22, 0.22)),
        gem: meshes.add(Cuboid::new(0.32, 0.32, 0.32)),
        slab: meshes.add(Cuboid::new(0.85, 0.14, 0.85)),
        printer_body: meshes.add(Cuboid::new(0.9, 0.5, 0.9)),
        explode_cube: meshes.add(Cuboid::new(0.9, 0.9, 0.9)),
        ore_mat: materials.add(StandardMaterial {
            base_color: ORE_GOLD,
            emissive: LinearRgba::new(0.9, 0.65, 0.1, 1.0),
            metallic: 0.2,
            perceptual_roughness: 0.3,
            ..default()
        }),
        wreck_mat: materials.add(StandardMaterial {
            base_color: WRECK_GRAY,
            perceptual_roughness: 0.9,
            ..default()
        }),
        black_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.04, 0.04, 0.05),
            perceptual_roughness: 0.6,
            ..default()
        }),
        explode_mat: materials.add(StandardMaterial {
            base_color: EXPLODE_ORANGE,
            emissive: LinearRgba::new(2.0, 0.9, 0.2, 1.0),
            perceptual_roughness: 0.4,
            ..default()
        }),
        print_glow_mat: materials.add(StandardMaterial {
            base_color: PRINT_GLOW,
            emissive: LinearRgba::new(0.2, 0.6, 1.8, 1.0),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        nose_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.10, 0.05),
            perceptual_roughness: 0.6,
            ..default()
        }),
        bot_mats,
        ruined_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.14, 0.12),
            perceptual_roughness: 0.95,
            ..default()
        }),
        tile_slab: meshes.add(Cuboid::new(0.96, 0.12, 0.96)),
        bot_cube: meshes.add(bot_cube_mesh()),
        bot_tex_mats,
        tex_slab: meshes.add(tex_slab_mesh()),
        ground_tex_mat,
        bridge_tex_mat,
        oneway_tex_mat,
        preview_valid_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(0.85, 0.95, 1.0, 0.45),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        preview_invalid_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.25, 0.2, 0.45),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        preview_chevron_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.85, 0.2, 0.7),
            emissive: LinearRgba::new(0.5, 0.35, 0.05, 1.0),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        paint_mats: PAINT_COLORS.map(|(r, gc, b)| {
            materials.add(StandardMaterial {
                base_color: Color::srgba(
                    r as f32 / 255.0,
                    gc as f32 / 255.0,
                    b as f32 / 255.0,
                    0.55,
                ),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.9,
                ..default()
            })
        }),
    };

    // Terrain slabs (0.96 with grout lines, prototype-style). Plains get
    // the baked circuit tile; rubble/water stay flat colors.
    let rubble_mat = materials.add(StandardMaterial {
        base_color: RUBBLE,
        metallic: 0.05,
        perceptual_roughness: 0.9,
        ..default()
    });
    let water_mat = materials.add(StandardMaterial {
        base_color: WATER,
        perceptual_roughness: 0.3,
        ..default()
    });
    for y in 0..world.grid.height {
        for x in 0..world.grid.width {
            let pos = TilePos::new(x, y);
            let kind = world.grid.get(pos).expect("in bounds");
            let (mat, y_off) = match kind {
                TileKind::Plains => (palette.ground_tex_mat.clone(), 0.0),
                TileKind::Rubble => (rubble_mat.clone(), 0.04),
                TileKind::Water => (water_mat.clone(), -0.05),
                // Bridges only exist after terraforming; at startup none do
                // (sync_view overlays planks when they appear).
                TileKind::Bridge => {
                    (palette.ground_tex_mat.clone(), 0.0)
                }
            };
            commands.spawn((
                Mesh3d(palette.tex_slab.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(tile_xyz(world, pos, y_off - 0.05)),
            ));
        }
    }

    // Depots: flat glowing blue slabs (the prototype's "base").
    let depot_mat = materials.add(StandardMaterial {
        base_color: DEPOT_BLUE,
        emissive: LinearRgba::new(0.05, 0.18, 0.30, 1.0),
        metallic: 0.4,
        perceptual_roughness: 0.35,
        ..default()
    });
    for depot in world.depots.values() {
        commands.spawn((
            Mesh3d(palette.slab.clone()),
            MeshMaterial3d(depot_mat.clone()),
            Transform::from_translation(tile_xyz(world, depot.pos, 0.07)),
        ));
    }

    // Lighting: bright ambient + warm sun with shadows.
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.75, 0.78, 0.92),
        brightness: 250.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight { illuminance: 10_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(6.0, 14.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Orbit camera.
    let cam = OrbitCam { focus: Vec3::ZERO, distance: 22.0, yaw: 0.0, pitch: 0.85 };
    let transform = orbit_transform(&cam);
    commands.spawn((Camera3d::default(), transform, cam));

    // Placement ghost: follows the cursor while a build item is armed.
    commands
        .spawn((
            PreviewSlab,
            Mesh3d(palette.tile_slab.clone()),
            MeshMaterial3d(palette.preview_valid_mat.clone()),
            Transform::from_xyz(0.0, 0.08, 0.0),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                PreviewStrip,
                Mesh3d(palette.nose_cube.clone()),
                MeshMaterial3d(palette.preview_chevron_mat.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0),
                Visibility::Hidden,
            ));
            parent.spawn((
                PreviewTip,
                Mesh3d(palette.nose_cube.clone()),
                MeshMaterial3d(palette.preview_chevron_mat.clone()),
                Transform::from_xyz(0.0, 0.12, 0.0).with_scale(Vec3::new(1.4, 1.2, 1.4)),
                Visibility::Hidden,
            ));
        });

    commands.insert_resource(palette);
}

fn step_sim(mut game: NonSendMut<GameSim>) {
    game.0.step();
}

// ------------------------------------------------------------------ camera

fn orbit_transform(cam: &OrbitCam) -> Transform {
    let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
    Transform::from_translation(cam.focus + rot * Vec3::new(0.0, 0.0, cam.distance))
        .looking_at(cam.focus, Vec3::Y)
}

fn orbit_camera(
    mut contexts: EguiContexts,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
    mut cams: Query<(&mut OrbitCam, &mut Transform)>,
) {
    // try_ctx_mut: the context is gone during shutdown / not yet there on
    // the first frame — never panic for a camera nicety.
    let over_ui = contexts.try_ctx_mut().is_some_and(|ctx| ctx.wants_pointer_input());
    let Ok((mut cam, mut transform)) = cams.single_mut() else { return };

    let delta: Vec2 = motion.read().map(|m| m.delta).sum();
    let scroll: f32 = wheel.read().map(|w| w.y).sum();
    if over_ui {
        return;
    }

    let panning = buttons.pressed(MouseButton::Middle)
        || (buttons.pressed(MouseButton::Right) && keys.pressed(KeyCode::ShiftLeft));
    if panning && delta != Vec2::ZERO {
        let right = transform.right();
        let up = transform.up();
        let pan_scale = 0.0015 * cam.distance;
        cam.focus += (-right * delta.x + up * delta.y) * pan_scale;
    } else if buttons.pressed(MouseButton::Right) && delta != Vec2::ZERO {
        cam.yaw -= delta.x * 0.005;
        cam.pitch = (cam.pitch + delta.y * 0.005).clamp(0.1, 1.5);
    }
    if scroll != 0.0 {
        cam.distance = (cam.distance * (1.0 - scroll * 0.1)).clamp(3.0, 80.0);
    }
    *transform = orbit_transform(&cam);
}

/// Build mode: LMB picks a tile via the cursor ray onto the ground plane;
/// the sim validates (water only, funds, no duplicate) — the UI just aims.
fn place_blueprint(
    mut contexts: EguiContexts,
    mut editor: ResMut<EditorState>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut game: NonSendMut<GameSim>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        editor.selected_build = None;
    }
    if keys.just_pressed(KeyCode::KeyR)
        && let Some(ToolKind::Overlay(Some(OverlayKind::Arrow(d)))) = editor.selected_build
    {
        editor.selected_build = Some(ToolKind::Overlay(Some(OverlayKind::Arrow(d.clockwise()))));
    }
    let Some(kind) = editor.selected_build else { return };
    // Paint drags; everything else places on click.
    let painting = matches!(kind, ToolKind::Paint(_));
    if painting && !buttons.pressed(MouseButton::Left) {
        editor.last_paint_tile = None;
        return;
    }
    if !painting && !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    if contexts.try_ctx_mut().is_some_and(|ctx| ctx.wants_pointer_input()) {
        return;
    }
    let world = &game.0.world;
    let Some(pos) = cursor_tile(&windows, &cams, world.grid.width, world.grid.height) else {
        return;
    };
    if !world.grid.in_bounds(pos) {
        return;
    }
    match kind {
        ToolKind::Building(blueprint) => {
            let _ = game.0.apply(&Command::PlaceBlueprint { pos, kind: blueprint });
        }
        ToolKind::Overlay(overlay) => {
            let _ = game.0.apply(&Command::PlaceOverlay { pos, overlay });
        }
        ToolKind::Paint(color) => {
            if editor.last_paint_tile != Some(pos) {
                editor.last_paint_tile = Some(pos);
                let _ = game.0.apply(&Command::PlacePaint { pos, color });
            }
        }
    }
}

/// Cursor ray onto the ground plane -> tile coordinates.
fn cursor_tile(
    windows: &Query<&Window>,
    cams: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    grid_w: i32,
    grid_h: i32,
) -> Option<TilePos> {
    let window = windows.single().ok()?;
    let cursor = window.cursor_position()?;
    let (camera, cam_transform) = cams.single().ok()?;
    let ray = camera.viewport_to_world(cam_transform, cursor).ok()?;
    if ray.direction.y.abs() < 1e-4 {
        return None;
    }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return None;
    }
    let hit = ray.origin + *ray.direction * t;
    Some(TilePos::new(
        (hit.x + grid_w as f32 / 2.0).round() as i32,
        (hit.z + grid_h as f32 / 2.0).round() as i32,
    ))
}

/// The translucent ghost: follows the hovered tile while armed, tinted by
/// placement validity; the one-way chevron shows which way traffic will
/// flow (R rotates it live).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn build_preview(
    mut contexts: EguiContexts,
    editor: Res<EditorState>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    game: NonSend<GameSim>,
    palette: Res<Palette>,
    mut slab: Query<
        (&mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>),
        With<PreviewSlab>,
    >,
    mut strip: Query<
        (&mut Transform, &mut Visibility),
        (With<PreviewStrip>, Without<PreviewSlab>),
    >,
    mut tip: Query<
        (&mut Transform, &mut Visibility),
        (With<PreviewTip>, Without<PreviewSlab>, Without<PreviewStrip>),
    >,
) {
    let Ok((mut slab_tf, mut slab_vis, mut slab_mat)) = slab.single_mut() else { return };
    let Ok((mut strip_tf, mut strip_vis)) = strip.single_mut() else { return };
    let Ok((mut tip_tf, mut tip_vis)) = tip.single_mut() else { return };
    let hide = |a: &mut Visibility, b: &mut Visibility, c: &mut Visibility| {
        (*a, *b, *c) = (Visibility::Hidden, Visibility::Hidden, Visibility::Hidden);
    };

    let over_ui = contexts.try_ctx_mut().is_some_and(|ctx| ctx.wants_pointer_input());
    let world = &game.0.world;
    let (Some(kind), false) = (editor.selected_build, over_ui) else {
        hide(&mut slab_vis, &mut strip_vis, &mut tip_vis);
        return;
    };
    let Some(pos) = cursor_tile(&windows, &cams, world.grid.width, world.grid.height) else {
        hide(&mut slab_vis, &mut strip_vis, &mut tip_vis);
        return;
    };
    if !world.grid.in_bounds(pos) {
        hide(&mut slab_vis, &mut strip_vis, &mut tip_vis);
        return;
    }

    let (valid, paint_ghost) = match kind {
        ToolKind::Building(BlueprintKind::Bridge) => {
            let cost = game.0.tuning.bridge_cost_ore;
            let ok = world.grid.get(pos) == Some(sim::TileKind::Water)
                && !world.blueprints.values().any(|b| b.pos == pos)
                && world.stockpile_ore >= cost;
            (ok, None)
        }
        ToolKind::Overlay(Some(_)) => {
            (world.stockpile_ore >= game.0.tuning.overlay_cost_ore, None)
        }
        ToolKind::Overlay(None) | ToolKind::Paint(None) => (true, None),
        ToolKind::Paint(Some(c)) => (true, Some(palette.paint_mats[c as usize % 4].clone())),
    };

    slab_tf.translation = tile_xyz(world, pos, 0.08);
    *slab_vis = Visibility::Visible;
    slab_mat.0 = paint_ghost.unwrap_or_else(|| {
        if valid {
            palette.preview_valid_mat.clone()
        } else {
            palette.preview_invalid_mat.clone()
        }
    });

    match kind {
        ToolKind::Overlay(Some(OverlayKind::Arrow(d))) => {
            let (dx, dz) = d.delta();
            let along = Vec3::new(dx as f32, 0.0, dz as f32);
            let strip_size = if dx != 0 {
                Vec3::new(0.6, 0.06, 0.16)
            } else {
                Vec3::new(0.16, 0.06, 0.6)
            };
            strip_tf.scale = strip_size / 0.22;
            strip_tf.translation = Vec3::Y * 0.12;
            tip_tf.translation = along * 0.34 + Vec3::Y * 0.12;
            *strip_vis = Visibility::Visible;
            *tip_vis = Visibility::Visible;
        }
        _ => {
            *strip_vis = Visibility::Hidden;
            *tip_vis = Visibility::Hidden;
        }
    }
}

// ------------------------------------------------------------------- view

/// FixedUpdate, after the sim step: shift pose targets (prev <- curr) and
/// point noses along the travel direction.
fn update_poses(
    game: NonSend<GameSim>,
    index: Res<ViewIndex>,
    mut poses: Query<(&mut Pose, &mut Transform)>,
) {
    let world = &game.0.world;
    let freeze_total = game.0.tuning.bump_freeze_ticks;
    for (id, bot) in &world.bots {
        let Some(&entity) = index.bots.get(&id.0) else { continue };
        let Ok((mut pose, mut transform)) = poses.get_mut(entity) else { continue };
        let mut y = if bot.data.booting.is_some() {
            0.1 // rising out of the printer
        } else {
            0.45
        };
        // Bump recoil: a little hop over the first few frozen ticks.
        if bot.data.bump_frozen > 0 {
            let age = freeze_total.saturating_sub(bot.data.bump_frozen) as f32;
            if age < 5.0 {
                y += 0.3 * (std::f32::consts::PI * (age + 1.0) / 6.0).sin();
            }
        }
        // Fault jump: any entry into error handling (crash dump or
        // on error: trap) makes the bot visibly startle.
        let faults = bot.vm.as_ref().map(|v| v.fault_count()).unwrap_or(pose.fault_seen);
        if faults > pose.fault_seen {
            pose.fault_seen = faults;
            pose.fault_age = 0;
        } else {
            pose.fault_age = pose.fault_age.saturating_add(1);
        }
        if pose.fault_age < 5 {
            y += 0.3 * (std::f32::consts::PI * (pose.fault_age as f32 + 1.0) / 6.0).sin();
        }
        let target = tile_xyz(world, bot.data.pos, y);
        pose.prev = pose.curr;
        pose.curr = target;
        // Face the tile currently being attempted (so a bumped bot stares
        // at whatever it walked into for the whole freeze), else the tile
        // just entered.
        let next_tile = match (&bot.data.action, &bot.data.recall) {
            (Some(sim::world::Action::Move { path, .. }), _) if !path.is_empty() => Some(path[0]),
            (_, Some(recall)) if !recall.path.is_empty() => Some(recall.path[0]),
            _ => None,
        };
        let face_from_to = match next_tile {
            Some(next) if next != bot.data.pos => Some((bot.data.pos, next)),
            _ if pose.grid != bot.data.pos => Some((pose.grid, bot.data.pos)),
            _ => None,
        };
        if let Some((from, to)) = face_from_to {
            let dx = (to.x - from.x) as f32;
            let dz = (to.y - from.y) as f32;
            // Nose is on the local -Z face; lead with it.
            transform.rotation = Quat::from_rotation_y((-dx).atan2(-dz));
        }
        pose.grid = bot.data.pos;
    }
}

/// Per-frame smoothing between fixed ticks.
fn interpolate(fixed: Res<Time<Fixed>>, mut q: Query<(&Pose, &mut Transform)>) {
    let a = fixed.overstep_fraction();
    for (pose, mut transform) in &mut q {
        transform.translation = pose.prev.lerp(pose.curr, a);
    }
}

fn spin(time: Res<Time>, mut q: Query<(&Spinner, &mut Transform)>) {
    for (spinner, mut transform) in &mut q {
        transform.rotate_y(spinner.0 * time.delta_secs());
    }
}

fn animate_job_cubes(
    time: Res<Time>,
    game: NonSend<GameSim>,
    index: Res<ViewIndex>,
    children: Query<&Children>,
    mut cubes: Query<(&mut Transform, &mut Visibility), With<JobCube>>,
) {
    let world = &game.0.world;
    let total = game.0.tuning.print_ticks as f32;
    for (id, printer) in &world.printers {
        let Some(&(entity, _)) = index.printers.get(&id.0) else { continue };
        let Ok(kids) = children.get(entity) else { continue };
        for kid in kids {
            let Ok((mut transform, mut vis)) = cubes.get_mut(*kid) else { continue };
            match printer.job {
                Some(ticks_left) => {
                    *vis = Visibility::Visible;
                    let grown = 1.0 - ticks_left as f32 / total;
                    transform.scale = Vec3::splat(0.1 + 0.9 * grown);
                    transform.translation.y =
                        1.1 + (time.elapsed_secs() * 2.0).sin() * 0.1;
                    transform.rotate_y(0.8 * time.delta_secs());
                }
                None => *vis = Visibility::Hidden,
            }
        }
    }
}

fn animate_explosions(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Explosion, &mut Transform)>,
) {
    for (entity, mut explosion, mut transform) in &mut q {
        explosion.age += time.delta_secs();
        let t = explosion.age / 0.8;
        if t >= 1.0 {
            commands.entity(entity).despawn();
        } else {
            transform.scale = Vec3::splat(1.0 - t);
            transform.rotate_y(6.0 * time.delta_secs());
        }
    }
}

/// Diff sim state into persistent view entities.
#[allow(clippy::too_many_arguments)]
fn sync_view(
    mut commands: Commands,
    game: NonSend<GameSim>,
    palette: Res<Palette>,
    mut index: ResMut<ViewIndex>,
    mut transforms: Query<&mut Transform>,
    children: Query<&Children>,
    mut slots: Query<(&CarrySlot, &mut Visibility)>,
) {
    let world = &game.0.world;

    // Printers: respawn view on state flips (repair!).
    for (id, printer) in &world.printers {
        let needs_spawn = match index.printers.get(&id.0) {
            Some((entity, state)) if *state != printer.state => {
                commands.entity(*entity).despawn();
                true
            }
            None => true,
            _ => false,
        };
        if needs_spawn {
            let (mat, scale) = match printer.state {
                PrinterState::Working => (
                    palette.bot_mats[&printer.color.0.min(2)].clone(),
                    Vec3::ONE,
                ),
                PrinterState::Ruined => (palette.ruined_mat.clone(), Vec3::new(1.0, 0.45, 1.0)),
            };
            let entity = commands
                .spawn((
                    Mesh3d(palette.printer_body.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_translation(tile_xyz(world, printer.pos, 0.25))
                        .with_scale(scale),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        JobCube,
                        Mesh3d(palette.unit_cube.clone()),
                        MeshMaterial3d(palette.print_glow_mat.clone()),
                        Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::splat(0.1)),
                        Visibility::Hidden,
                    ));
                })
                .id();
            index.printers.insert(id.0, (entity, printer.state));
        }
    }

    // Ore: spinning gold gems, scaled by remaining amount.
    for (id, node) in &world.ore_nodes {
        if node.amount == 0 {
            if let Some(entity) = index.ore.remove(&id.0) {
                commands.entity(entity).despawn();
            }
            continue;
        }
        let scale = Vec3::splat(0.6 + 0.8 * (node.amount as f32 / 60.0).min(1.0));
        match index.ore.get(&id.0) {
            Some(&entity) => {
                if let Ok(mut transform) = transforms.get_mut(entity) {
                    transform.scale = scale;
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(palette.gem.clone()),
                        MeshMaterial3d(palette.ore_mat.clone()),
                        Transform::from_translation(tile_xyz(world, node.pos, 0.35))
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4))
                            .with_scale(scale),
                        Spinner(1.5),
                    ))
                    .id();
                index.ore.insert(id.0, entity);
            }
        }
    }

    // Bots: colored cube + dark nose + carry slots.
    let mut seen: Vec<u32> = Vec::new();
    for (id, bot) in &world.bots {
        seen.push(id.0);
        if let Some(&entity) = index.bots.get(&id.0) {
            // Carry indicators track cargo.
            if let Ok(kids) = children.get(entity) {
                for kid in kids {
                    if let Ok((slot, mut vis)) = slots.get_mut(*kid) {
                        *vis = if bot.data.cargo > slot.0 {
                            Visibility::Visible
                        } else {
                            Visibility::Hidden
                        };
                    }
                }
            }
            continue;
        }
        let start = tile_xyz(world, bot.data.pos, 0.45);
        let entity = commands
            .spawn((
                Mesh3d(palette.bot_cube.clone()),
                MeshMaterial3d(palette.bot_tex_mats[&bot.data.color.0.min(2)].clone()),
                Transform::from_translation(start),
                Pose {
                    prev: start,
                    curr: start,
                    grid: bot.data.pos,
                    fault_seen: bot.vm.as_ref().map(|v| v.fault_count()).unwrap_or(0),
                    fault_age: u32::MAX,
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(palette.nose_cube.clone()),
                    MeshMaterial3d(palette.nose_mat.clone()),
                    Transform::from_xyz(0.0, 0.05, -0.45),
                ));
                for (slot, y) in [(0u32, 0.55), (1u32, 0.85)] {
                    parent.spawn((
                        CarrySlot(slot),
                        Mesh3d(palette.nose_cube.clone()),
                        MeshMaterial3d(palette.ore_mat.clone()),
                        Transform::from_xyz(0.0, y, 0.0),
                        Visibility::Hidden,
                    ));
                }
            })
            .id();
        index.bots.insert(id.0, entity);
    }
    index.bots.retain(|id, entity| {
        if seen.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    // Blueprints: glowing ghost slabs that rise with build progress.
    for (id, bp) in &world.blueprints {
        let grown = 0.15 + 0.85 * (bp.progress as f32 / bp.needed as f32);
        match index.blueprints.get(&id.0) {
            Some(&entity) => {
                if let Ok(mut transform) = transforms.get_mut(entity) {
                    transform.scale = Vec3::new(1.0, grown, 1.0);
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(palette.tile_slab.clone()),
                        MeshMaterial3d(palette.print_glow_mat.clone()),
                        Transform::from_translation(tile_xyz(world, bp.pos, 0.05))
                            .with_scale(Vec3::new(1.0, grown, 1.0)),
                    ))
                    .id();
                index.blueprints.insert(id.0, entity);
            }
        }
    }
    index.blueprints.retain(|id, entity| {
        if world.blueprints.contains_key(&sim::EntityId(*id)) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    // Finished bridges: baked plank tiles over the water. (Direction
    // arrows are an overlay layer now — see below.)
    for y in 0..world.grid.height {
        for x in 0..world.grid.width {
            let pos = TilePos::new(x, y);
            if world.grid.get(pos) != Some(sim::TileKind::Bridge) {
                continue;
            }
            if !index.bridges.insert((x, y)) {
                continue;
            }
            commands.spawn((
                Mesh3d(palette.tex_slab.clone()),
                MeshMaterial3d(palette.bridge_tex_mat.clone()),
                Transform::from_translation(tile_xyz(world, pos, 0.0)),
            ));
        }
    }

    // Overlay layer: the baked arrow tile (east-pointing art), spun to the
    // arrow's direction, floated just above whatever terrain is beneath.
    for (pos, overlay) in &world.overlays {
        let key = (pos.x, pos.y);
        if let Some((entity, kind)) = index.overlays.get(&key) {
            if kind == overlay {
                continue;
            }
            commands.entity(*entity).despawn();
            index.overlays.remove(&key);
        }
        let OverlayKind::Arrow(d) = overlay;
        let (dx, dz) = d.delta();
        let rot = Quat::from_rotation_y(-(dz as f32).atan2(dx as f32));
        let entity = commands
            .spawn((
                Mesh3d(palette.tex_slab.clone()),
                MeshMaterial3d(palette.oneway_tex_mat.clone()),
                Transform::from_translation(tile_xyz(world, *pos, 0.08)).with_rotation(rot),
            ))
            .id();
        index.overlays.insert(key, (entity, *overlay));
    }
    index.overlays.retain(|key, (entity, _)| {
        if world.overlays.contains_key(&TilePos::new(key.0, key.1)) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    // Paint layer: thin translucent color washes over tiles.
    for (pos, color) in &world.paint {
        let key = (pos.x, pos.y);
        if let Some((entity, c)) = index.paint.get(&key) {
            if c == color {
                continue;
            }
            commands.entity(*entity).despawn();
            index.paint.remove(&key);
        }
        let entity = commands
            .spawn((
                Mesh3d(palette.tile_slab.clone()),
                MeshMaterial3d(palette.paint_mats[*color as usize % 4].clone()),
                Transform::from_translation(tile_xyz(world, *pos, 0.02))
                    .with_scale(Vec3::new(1.0, 0.25, 1.0)),
            ))
            .id();
        index.paint.insert(key, (entity, *color));
    }
    index.paint.retain(|key, (entity, _)| {
        if world.paint.contains_key(&TilePos::new(key.0, key.1)) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    // Wrecks: low dark slabs.
    for (id, wreck) in &world.wrecks {
        if let std::collections::hash_map::Entry::Vacant(e) = index.wrecks.entry(id.0) {
            let entity = commands
                .spawn((
                    Mesh3d(palette.slab.clone()),
                    MeshMaterial3d(palette.wreck_mat.clone()),
                    Transform::from_translation(tile_xyz(world, wreck.pos, 0.07)),
                ))
                .id();
            e.insert(entity);
        }
    }

    // Black boxes: an explosion flash, then the small dark cube remains.
    while index.black_boxes < world.black_boxes.len() {
        let bb = &world.black_boxes[index.black_boxes];
        let at = tile_xyz(world, bb.pos, 0.5);
        commands.spawn((
            Explosion { age: 0.0 },
            Mesh3d(palette.explode_cube.clone()),
            MeshMaterial3d(palette.explode_mat.clone()),
            Transform::from_translation(at),
        ));
        commands.spawn((
            Mesh3d(palette.nose_cube.clone()),
            MeshMaterial3d(palette.black_mat.clone()),
            Transform::from_translation(tile_xyz(world, bb.pos, 0.12)),
        ));
        index.black_boxes += 1;
    }
}

// --------------------------------------------------------------------- ui

// ------------------------------------------------------ syntax highlighting

// Editor colors, tuned for egui's dark theme.
const HL_KEYWORD: egui::Color32 = egui::Color32::from_rgb(197, 134, 192);
const HL_FUNCTION: egui::Color32 = egui::Color32::from_rgb(220, 220, 130);
const HL_VARIABLE: egui::Color32 = egui::Color32::from_rgb(156, 220, 254);
const HL_NUMBER: egui::Color32 = egui::Color32::from_rgb(181, 206, 168);
const HL_STRING: egui::Color32 = egui::Color32::from_rgb(206, 145, 120);
const HL_COMMENT: egui::Color32 = egui::Color32::from_rgb(106, 153, 85);
const HL_PLAIN: egui::Color32 = egui::Color32::from_rgb(212, 212, 212);

/// Best-effort Pyrite highlighting for the editor. Unlike `pyrite::lexer`
/// this never fails, so half-typed programs still get colored. Keywords come
/// from the lexer's own table (`pyrite::token::keyword`) so the two can't
/// drift.
fn highlight_pyrite(text: &str, font_id: egui::FontId) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};

    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let byte_at =
        |i: usize| chars.get(i).map_or(text.len(), |&(b, _)| b);

    let mut job = LayoutJob::default();
    let fmt = |color: egui::Color32| TextFormat {
        font_id: font_id.clone(),
        color,
        ..Default::default()
    };

    let n = chars.len();
    let mut plain_start = 0; // byte offset of pending uncolored text
    let mut i = 0;
    while i < n {
        let (start, c) = chars[i];
        let (end, color) = if c == '#' {
            while i < n && chars[i].1 != '\n' {
                i += 1;
            }
            (byte_at(i), HL_COMMENT)
        } else if c == '"' {
            i += 1;
            while i < n && chars[i].1 != '"' && chars[i].1 != '\n' {
                i += if chars[i].1 == '\\' { 2 } else { 1 };
            }
            if i < n && chars[i].1 == '"' {
                i += 1;
            }
            (byte_at(i), HL_STRING)
        } else if c.is_ascii_digit() {
            while i < n && chars[i].1.is_ascii_digit() {
                i += 1;
            }
            (byte_at(i), HL_NUMBER)
        } else if c.is_ascii_alphabetic() || c == '_' {
            while i < n && (chars[i].1.is_ascii_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            let end = byte_at(i);
            let color = if pyrite::token::keyword(&text[start..end]).is_some() {
                HL_KEYWORD
            } else {
                // A call (or `def` header) if the next non-space char is `(`.
                let mut j = i;
                while j < n && chars[j].1 == ' ' {
                    j += 1;
                }
                if j < n && chars[j].1 == '(' { HL_FUNCTION } else { HL_VARIABLE }
            };
            (end, color)
        } else {
            i += 1;
            continue;
        };
        if plain_start < start {
            job.append(&text[plain_start..start], 0.0, fmt(HL_PLAIN));
        }
        job.append(&text[start..end], 0.0, fmt(color));
        plain_start = end;
    }
    if plain_start < text.len() {
        job.append(&text[plain_start..], 0.0, fmt(HL_PLAIN));
    }
    job
}

fn editor_ui(
    mut contexts: EguiContexts,
    mut game: NonSendMut<GameSim>,
    mut editor: ResMut<EditorState>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else { return };

    egui::TopBottomPanel::bottom("build_bar").exact_height(96.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Category tabs.
            ui.vertical(|ui| {
                ui.strong("Build");
                for (i, (name, _)) in BUILD_CATEGORIES.iter().enumerate() {
                    if ui.selectable_label(editor.build_category == i, *name).clicked() {
                        editor.build_category = i;
                    }
                }
            });
            ui.separator();

            // Items of the selected category.
            let (_, items) = BUILD_CATEGORIES[editor.build_category.min(BUILD_CATEGORIES.len() - 1)];
            for item in items {
                let cost = match item.kind {
                    ToolKind::Building(BlueprintKind::Bridge) => game.0.tuning.bridge_cost_ore,
                    ToolKind::Overlay(Some(_)) => game.0.tuning.overlay_cost_ore,
                    ToolKind::Overlay(None) | ToolKind::Paint(_) => 0,
                };
                let affordable = game.0.world.stockpile_ore >= cost;
                if !editor.icons.contains_key(item.name) {
                    let tex = ctx.load_texture(
                        item.name,
                        build_icon(item.name),
                        egui::TextureOptions::NEAREST,
                    );
                    editor.icons.insert(item.name, tex);
                }
                let tex_id = editor.icons[item.name].id();
                let selected = editor.selected_build.is_some_and(|k| same_item(k, item.kind));
                ui.vertical(|ui| {
                    let button = egui::ImageButton::new(egui::load::SizedTexture::new(
                        tex_id,
                        egui::vec2(48.0, 48.0),
                    ))
                    .selected(selected);
                    let hover = if cost > 0 {
                        format!("{} — {cost} ore", item.name)
                    } else {
                        format!("{} — free", item.name)
                    };
                    let response = ui.add_enabled(affordable, button).on_hover_text(hover);
                    if response.clicked() {
                        editor.selected_build = if selected { None } else { Some(item.kind) };
                    }
                    let cost_line = if cost > 0 { format!("{cost} ore") } else { "free".into() };
                    ui.small(format!("{}
{cost_line}", item.name));
                });
            }

            // Status / hints on the right.
            ui.separator();
            ui.vertical(|ui| {
                if let Some(kind) = editor.selected_build {
                    match kind {
                        ToolKind::Building(BlueprintKind::Bridge) => {
                            ui.label("Click a water tile to place — Esc to cancel");
                        }
                        ToolKind::Overlay(Some(OverlayKind::Arrow(d))) => {
                            ui.label(format!(
                                "Click any tile to set {} — R rotates, Esc cancels",
                                d.arrow()
                            ));
                        }
                        ToolKind::Overlay(None) => {
                            ui.label("Click a tile to clear its overlay — Esc cancels");
                        }
                        ToolKind::Paint(Some(_)) => {
                            ui.label("Click or drag to paint tiles — Esc cancels");
                        }
                        ToolKind::Paint(None) => {
                            ui.label("Click or drag to erase paint — Esc cancels");
                        }
                    }
                } else {
                    ui.small("Select a tool, then click the map.");
                }
                let pending = game.0.world.blueprints.len();
                if pending > 0 {
                    ui.small(format!(
                        "{pending} blueprint(s) waiting for builders (nearest_blueprint / build)"
                    ));
                }
            });
        });
    });

    egui::SidePanel::left("editor").exact_width(300.0).show(ctx, |ui| {
        ui.heading("Pyrite");
        let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
            let mut job =
                highlight_pyrite(text, egui::TextStyle::Monospace.resolve(ui.style()));
            job.wrap.max_width = wrap_width;
            ui.fonts(|fonts| fonts.layout_job(job))
        };
        ui.add(
            egui::TextEdit::multiline(&mut editor.code)
                .font(egui::TextStyle::Monospace)
                .desired_rows(14)
                .desired_width(f32::INFINITY)
                .layouter(&mut layouter),
        );
        ui.horizontal(|ui| {
            for (label, color) in [("Deploy Green", BotColor::GREEN), ("Deploy Red", BotColor::RED)] {
                if ui.button(label).clicked() {
                    let cmd = Command::DeployProgram {
                        faction: 0,
                        color,
                        source: editor.code.clone(),
                    };
                    match game.0.apply(&cmd) {
                        Ok(_) => {
                            editor.status = format!("deployed to {label:?}");
                            editor.status_ok = true;
                        }
                        Err(e) => {
                            editor.status = e.to_string();
                            editor.status_ok = false;
                        }
                    }
                }
            }
        });
        let status_color = if editor.status_ok {
            egui::Color32::from_rgb(120, 220, 120)
        } else {
            egui::Color32::from_rgb(240, 120, 100)
        };
        ui.colored_label(status_color, &editor.status);
        ui.separator();

        let (tick, ore, bots, wrecks, cloud) = {
            let w = &game.0.world;
            (w.tick, w.stockpile_ore, w.bots.len(), w.wrecks.len(), w.archive.len())
        };
        ui.heading("Colony");
        ui.monospace(format!("tick   {tick}"));
        ui.monospace(format!("ore    {ore}"));
        ui.monospace(format!("bots   {bots}"));
        ui.monospace(format!("wrecks {wrecks}"));
        ui.monospace(format!("cloud  {cloud}"));
        ui.separator();

        ui.heading("Printers");
        let printer_ids: Vec<_> = game.0.world.printers.keys().copied().collect();
        let repair_cost = game.0.tuning.repair_cost_ore;
        for pid in printer_ids {
            let (color, state, mut desired, job) = {
                let p = &game.0.world.printers[&pid];
                (p.color, p.state, p.desired_max, p.job)
            };
            let name = match color {
                BotColor::GREEN => "Green",
                BotColor::RED => "Red",
                _ => "Other",
            };
            ui.horizontal(|ui| {
                ui.label(name);
                match state {
                    PrinterState::Ruined => {
                        let affordable = game.0.world.stockpile_ore >= repair_cost;
                        if ui
                            .add_enabled(
                                affordable,
                                egui::Button::new(format!("Repair ({repair_cost} ore)")),
                            )
                            .clicked()
                        {
                            let _ = game.0.apply(&Command::RepairPrinter { printer: pid });
                        }
                    }
                    PrinterState::Working => {
                        if ui
                            .add(egui::Slider::new(&mut desired, 0..=8).text("bots"))
                            .changed()
                        {
                            let _ = game
                                .0
                                .apply(&Command::SetDesiredMax { printer: pid, value: desired });
                        }
                        if let Some(ticks) = job {
                            let total = game.0.tuning.print_ticks as f32;
                            ui.add(
                                egui::ProgressBar::new(1.0 - ticks as f32 / total)
                                    .desired_width(60.0),
                            );
                        }
                    }
                }
            });
        }
        ui.separator();

        ui.heading("Cloud");
        let archive = &game.0.world.archive;
        for entry in archive.iter().rev().take(8).rev() {
            ui.small(format!("[{}] bot{}: {}", entry.tick, entry.bot.0, entry.text));
        }
        ui.separator();
        ui.small("RMB drag: orbit · Shift+RMB / MMB: pan · scroll: zoom");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<(String, egui::Color32)> {
        highlight_pyrite(text, egui::FontId::monospace(12.0))
            .sections
            .iter()
            .map(|s| (text[s.byte_range.clone()].to_string(), s.format.color))
            .collect()
    }

    #[test]
    fn highlights_keywords_functions_variables_and_literals() {
        let got = spans("if x > 1:\n    move_to(target, \"hi\") # go\n");
        for expected in [
            ("if", HL_KEYWORD),
            ("x", HL_VARIABLE),
            ("1", HL_NUMBER),
            ("move_to", HL_FUNCTION),
            ("target", HL_VARIABLE),
            ("\"hi\"", HL_STRING),
            ("# go", HL_COMMENT),
        ] {
            assert!(
                got.contains(&(expected.0.to_string(), expected.1)),
                "missing span {expected:?} in {got:?}"
            );
        }
    }

    #[test]
    fn def_name_is_a_function_and_unterminated_string_stops_at_eol() {
        let got = spans("def go(n):\n    s = \"oops\nreturn n\n");
        assert!(got.contains(&("def".into(), HL_KEYWORD)));
        assert!(got.contains(&("go".into(), HL_FUNCTION)));
        assert!(got.contains(&("n".into(), HL_VARIABLE)));
        assert!(got.contains(&("\"oops".into(), HL_STRING)));
        assert!(got.contains(&("return".into(), HL_KEYWORD)));
    }

    #[test]
    fn highlight_covers_every_byte_exactly_once() {
        let text = "move_to(nearest_ore())\n# comment\nwhile True:\n    x = x + 1\n";
        let job = highlight_pyrite(text, egui::FontId::monospace(12.0));
        let mut pos = 0;
        for s in &job.sections {
            assert_eq!(s.byte_range.start, pos, "gap or overlap at byte {pos}");
            pos = s.byte_range.end;
        }
        assert_eq!(pos, text.len());
    }
}
