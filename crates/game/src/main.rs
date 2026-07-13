//! Colony viewer & editor, modeled on the original prototype's look:
//! bright emissive primitives on a dark void, orbit camera, and a live
//! Pyrite editor panel on the left. No 3D models — bots are cubes whose six
//! faces sample a baked SVG atlas, and terrain slabs sample baked SVG tiles
//! (`assets/art/*.svg` -> build.rs bake -> `assets/textures/*.png`). Team
//! identity is a palette swap done at bake time.
//!
//! The sim steps on `FixedUpdate` at 10 Hz (docs/07 tick rate); rendering
//! free-runs with per-frame interpolation between ticks. All game state
//! lives in `sim::Sim` (NonSend — the VMs hold `Rc`s); the UI only ever
//! mutates it through `Command`s, like any other lockstep peer.
//!
//! Camera: LMB (no tool armed) / MMB / Shift+RMB drag = pan · RMB drag =
//! orbit · scroll = zoom.
//! Run: `cargo run -p game`

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::MouseWheel;
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

mod editor;
use editor::EditorState;


struct GameSim(Sim);

// ---------------------------------------------------------------- palette
// Lifted from the original prototype (see docs of that repo).
const CLEAR: Color = Color::srgb(0.05, 0.06, 0.09);
const ORE_GOLD: Color = Color::srgb(1.0, 0.85, 0.15);
const PRINT_GLOW: Color = Color::srgb(0.25, 0.55, 0.95);
const EXPLODE_ORANGE: Color = Color::srgb(1.0, 0.45, 0.1);


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
    /// Was the bot in a handler last tick (hop on entry)?
    was_in_handler: bool,
    /// Last fault_count seen; a rise triggers the fault hop.
    fault_seen: u64,
    /// Fixed ticks since the last fault.
    fault_age: u32,
    /// Last hp seen; a change shows the health bar for a few seconds.
    hp_seen: i64,
    hp_age: u32,
    /// Last bump_frozen seen; a rise triggers the recoil hop.
    freeze_seen: u32,
    freeze_age: u32,
}

/// World-space progress bar over anything being built: root billboards
/// toward the camera; the fill scales with progress (left-anchored).
#[derive(Component)]
struct BillboardBar;
#[derive(Component)]
struct ProgressFill;
/// The in-world ring under the inspected bot.
#[derive(Component)]
struct SelMarker;

/// Angry scribble cloud over a bump-frozen bot (baked SVG frames).
#[derive(Component)]
struct ScribbleCloud;

/// Red health fill on a bot's billboarded bar.
#[derive(Component)]
struct HealthFill;
/// Pale "damage ghost" behind the red fill: holds the pre-hit fraction and
/// drains toward it, so each hit reads as a shrinking chunk.
#[derive(Component)]
struct HealthTrail {
    frac: f32,
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
    /// Blueprint id -> its progress-bar fill entity.
    blueprint_fills: HashMap<u64, Entity>,
    /// Printer id -> (bar root, fill) for print-job progress.
    printer_fills: HashMap<u64, (Entity, Entity)>,
    /// Bot id -> (bar root, red fill, damage-ghost trail).
    bot_health: HashMap<u32, (Entity, Entity, Entity)>,
    /// Bot id -> its scribble-cloud entity.
    bot_scribbles: HashMap<u32, Entity>,
    paint: HashMap<(i32, i32), (Entity, u8)>,
}

#[derive(Resource)]
struct Palette {
    unit_cube: Handle<Mesh>,
    nose_cube: Handle<Mesh>,
    gem: Handle<Mesh>,
    explode_cube: Handle<Mesh>,
    ore_mat: Handle<StandardMaterial>,
    black_mat: Handle<StandardMaterial>,
    explode_mat: Handle<StandardMaterial>,
    print_glow_mat: Handle<StandardMaterial>,
    nose_mat: Handle<StandardMaterial>,
    tile_slab: Handle<Mesh>,
    /// Bot body: cube whose faces sample cells of the team's 3x2 atlas.
    bot_cube: Handle<Mesh>,
    bot_tex_mats: HashMap<u8, Handle<StandardMaterial>>,
    /// Printer body: same atlas treatment, squat box.
    printer_box: Handle<Mesh>,
    printer_tex_mats: HashMap<u8, Handle<StandardMaterial>>,
    printer_ruined_mat: Handle<StandardMaterial>,
    /// The sheet rising from a working printer's top slot.
    paper_sheet: Handle<Mesh>,
    paper_mat: Handle<StandardMaterial>,
    /// Depot: a low wooden crate (bots stand on the depot tile, so it
    /// stays pallet-height).
    crate_box: Handle<Mesh>,
    crate_mat: Handle<StandardMaterial>,
    /// Sub-tile textured slab for wrecks.
    pad_slab: Handle<Mesh>,
    wreck_tex_mat: Handle<StandardMaterial>,
    /// Terrain slab: full tile texture on top, dark trim on the sides.
    tex_slab: Handle<Mesh>,
    /// The "tech" tile — terraformed ground (unused by natural terrain).
    ground_tex_mat: Handle<StandardMaterial>,
    bridge_tex_mat: Handle<StandardMaterial>,
    oneway_tex_mat: Handle<StandardMaterial>,
    grass_tex_mat: Handle<StandardMaterial>,
    water_tex_mat: Handle<StandardMaterial>,
    mountain_tex_mat: Handle<StandardMaterial>,
    /// Full-height block for mountain (Rubble) tiles.
    mountain_block: Handle<Mesh>,
    preview_valid_mat: Handle<StandardMaterial>,
    preview_invalid_mat: Handle<StandardMaterial>,
    preview_chevron_mat: Handle<StandardMaterial>,
    paint_mats: [Handle<StandardMaterial>; 4],
    bar_mesh: Handle<Mesh>,
    bar_bg_mat: Handle<StandardMaterial>,
    bar_fill_mat: Handle<StandardMaterial>,
    /// Health-fill gradient bins: index 0 = empty/red .. last = full/green.
    bar_health_grad: Vec<Handle<StandardMaterial>>,
    bar_trail_mat: Handle<StandardMaterial>,
    scribble_quad: Handle<Mesh>,
    /// Mood-cloud frames, keyed "angry" / "error" / "hurt" / "death".
    scribble_mats: HashMap<&'static str, Vec<Handle<StandardMaterial>>>,
    sel_ring: Handle<Mesh>,
    sel_mat: Handle<StandardMaterial>,
}

/// LMB click-vs-drag disambiguation while a tool is armed: a press is the
/// tool's click only if the cursor stays inside the dead zone; traveling
/// past it turns the gesture into a camera pan instead.
#[derive(Resource, Default)]
struct LmbGesture {
    /// Accumulated cursor travel (px) since LMB went down over the world;
    /// None while released or when the press began over the UI.
    travel: Option<f32>,
    /// The press outgrew the dead zone and owns the rest of the drag.
    panning: bool,
    /// Set for exactly the frame LMB was released inside the dead zone —
    /// the armed tool's "click" (consumed by place_blueprint).
    clicked: bool,
}

/// Cursor travel (px) that separates a click from a pan.
const LMB_DRAG_THRESHOLD: f32 = 6.0;

/// What an armed build-bar item does on click.
#[derive(Clone, Copy, PartialEq)]
enum ToolKind {
    /// Blueprint construction (bots do the labor).
    Building(BlueprintKind),
    /// Instant traffic signage on any tile; None = eraser.
    Overlay(Option<OverlayKind>),
    /// Instant cosmetic tile paint (drag to paint); None = eraser.
    Paint(Option<u8>),
    /// Emergency stop: click a bot to wreck it (logs kept, cargo spills).
    Kill,
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
    ("Command", &[BuildItem { name: "Kill Bot", kind: ToolKind::Kill }]),
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
    if name == "Kill Bot" {
        let bg = egui::Color32::from_rgb(40, 20, 22);
        for x in 0..s {
            for y in 0..s {
                img[(x, y)] = bg;
            }
        }
        let red = egui::Color32::from_rgb(235, 60, 45);
        for i in 6..42usize {
            for w in 0..4usize {
                img[(i, (i + w).min(47))] = red;
                img[(i, (47usize.saturating_sub(i) + w).min(47))] = red;
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
        source: editor::DEFAULT_PROGRAM.into(),
    })
    .expect("miner program parses");
    // Four bridge blueprints across the wall: the default program services
    // blueprints first, so the opening minutes are the colony building its
    // own crossings — progress bars and all — before mining east.
    for y in [2, 5, 8, 11] {
        game.apply(&Command::PlaceBlueprint {
            pos: TilePos::new(16, y),
            kind: BlueprintKind::Bridge,
        })
        .expect("blueprint placement");
    }
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
        .insert_resource(LmbGesture::default())
        .add_systems(Startup, (setup_sim, setup_scene).chain())
        .add_systems(FixedUpdate, (step_sim, update_poses).chain())
        .add_systems(
            Update,
            (
                editor::editor_ui,
                inspector_ui,
                time_controls,
                orbit_camera,
                place_blueprint,
                select_bot,
                update_sel_marker,
                build_preview,
                update_progress_bars,
                update_health_bars,
                update_scribbles,
                billboard_bars,
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

/// A body box (bot, printer, ...): each face samples its cell of a 3x2
/// atlas (front/right/back over left/top/bottom — the layout the build.rs
/// bake emits).
fn atlas_box_mesh(half: Vec3) -> Mesh {
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
fn textured_slab_mesh(half: Vec3) -> Mesh {
    const EDGE: [f32; 4] = [0.005, 0.45, 0.02, 0.55];
    box_with_face_uvs(half, [EDGE, EDGE, EDGE, EDGE, [0.0, 0.0, 1.0, 1.0], EDGE])
}

/// Render height of a mountain (Rubble) block's top face; other terrain
/// tops sit at 0.0 (water slightly below). Bots, overlays, and paint all
/// ride the terrain they're on.
const MOUNTAIN_TOP: f32 = 0.25;

/// Top surface of the tile at `pos` in render space.
fn terrain_top(world: &sim::World, pos: TilePos) -> f32 {
    match world.grid.get(pos) {
        Some(TileKind::Rubble) => MOUNTAIN_TOP,
        Some(TileKind::Water) => -0.05,
        _ => 0.0,
    }
}

/// Mountain block, mapped into the baked `mountain_atlas` (peaks in the
/// left cell, rock-face strata in the right): summit art on top, strata on
/// every side (the bottom face is never seen).
fn mountain_block_mesh() -> Mesh {
    const TOP: [f32; 4] = [0.0, 0.0, 0.5, 1.0];
    const SIDE: [f32; 4] = [0.5, 0.0, 1.0, 1.0];
    const EDGE: [f32; 4] = [0.51, 0.45, 0.53, 0.55];
    box_with_face_uvs(
        Vec3::new(0.48, MOUNTAIN_TOP / 2.0 + 0.025, 0.48),
        [SIDE, SIDE, SIDE, SIDE, TOP, EDGE],
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

    // Baked-atlas bodies; the emissive texture keeps the "glowing
    // primitives" identity without washing out the face art.
    let atlas_mat = |materials: &mut Assets<StandardMaterial>, tex: Handle<Image>| {
        materials.add(StandardMaterial {
            base_color_texture: Some(tex.clone()),
            emissive: LinearRgba::new(0.35, 0.35, 0.35, 1.0),
            emissive_texture: Some(tex),
            metallic: 0.1,
            perceptual_roughness: 0.5,
            ..default()
        })
    };
    let mut bot_tex_mats = HashMap::new();
    let mut printer_tex_mats = HashMap::new();
    for (c, team) in [(0u8, "green"), (1, "red"), (2, "blue")] {
        let bot: Handle<Image> = asset_server.load(format!("textures/bot_atlas_{team}.png"));
        bot_tex_mats.insert(c, atlas_mat(&mut materials, bot));
        let printer: Handle<Image> =
            asset_server.load(format!("textures/printer_atlas_{team}.png"));
        printer_tex_mats.insert(c, atlas_mat(&mut materials, printer));
    }
    // Ruined printers: the gray palette swap, no glow — the machine is dead.
    let printer_ruined_mat = materials.add(StandardMaterial {
        base_color_texture: Some(asset_server.load("textures/printer_atlas_ruined.png")),
        perceptual_roughness: 0.95,
        ..default()
    });
    let tile_tex_mat =
        |materials: &mut Assets<StandardMaterial>, tex: Handle<Image>, rough: f32| {
            materials.add(StandardMaterial {
                base_color_texture: Some(tex),
                perceptual_roughness: rough,
                ..default()
            })
        };
    let ground_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_ground.png"), 0.85);
    let bridge_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_bridge.png"), 0.85);
    let oneway_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_oneway.png"), 0.85);
    let grass_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_grass.png"), 0.95);
    let water_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_water.png"), 0.35);
    let mountain_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/mountain_atlas.png"), 0.95);
    let wreck_tex_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/tile_wreck.png"), 0.95);
    let crate_mat =
        tile_tex_mat(&mut materials, asset_server.load("textures/crate.png"), 0.9);
    // Paper stays readable in shadow: its own texture doubles as a faint
    // emissive map.
    let paper_tex: Handle<Image> = asset_server.load("textures/paper.png");
    let paper_mat = materials.add(StandardMaterial {
        base_color_texture: Some(paper_tex.clone()),
        emissive: LinearRgba::new(0.25, 0.25, 0.25, 1.0),
        emissive_texture: Some(paper_tex),
        perceptual_roughness: 0.9,
        ..default()
    });
    let palette = Palette {
        unit_cube: meshes.add(Cuboid::new(0.7, 0.7, 0.7)),
        nose_cube: meshes.add(Cuboid::new(0.22, 0.22, 0.22)),
        gem: meshes.add(Cuboid::new(0.32, 0.32, 0.32)),
        explode_cube: meshes.add(Cuboid::new(0.9, 0.9, 0.9)),
        ore_mat: materials.add(StandardMaterial {
            base_color: ORE_GOLD,
            emissive: LinearRgba::new(0.9, 0.65, 0.1, 1.0),
            metallic: 0.2,
            perceptual_roughness: 0.3,
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
        tile_slab: meshes.add(Cuboid::new(0.96, 0.12, 0.96)),
        bot_cube: meshes.add(atlas_box_mesh(Vec3::splat(0.35))),
        bot_tex_mats,
        printer_box: meshes.add(atlas_box_mesh(Vec3::new(0.45, 0.25, 0.45))),
        printer_tex_mats,
        printer_ruined_mat,
        paper_sheet: meshes.add(Cuboid::new(0.36, 0.26, 0.02)),
        paper_mat,
        crate_box: meshes.add(Cuboid::new(0.78, 0.3, 0.78)),
        crate_mat,
        pad_slab: meshes.add(textured_slab_mesh(Vec3::new(0.425, 0.07, 0.425))),
        wreck_tex_mat,
        tex_slab: meshes.add(textured_slab_mesh(Vec3::new(0.48, 0.05, 0.48))),
        mountain_block: meshes.add(mountain_block_mesh()),
        ground_tex_mat,
        bridge_tex_mat,
        oneway_tex_mat,
        grass_tex_mat,
        water_tex_mat,
        mountain_tex_mat,
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
        bar_mesh: meshes.add(Cuboid::new(0.9, 0.12, 0.02)),
        bar_bg_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.07, 0.07, 0.10),
            unlit: true,
            ..default()
        }),
        bar_fill_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.95, 0.35),
            emissive: LinearRgba::new(0.1, 0.7, 0.12, 1.0),
            unlit: true,
            ..default()
        }),
        bar_health_grad: (0..12)
            .map(|i| {
                let p = i as f32 / 11.0;
                // red (0) -> yellow (0.5) -> green (1).
                let (r, gr, b) = if p >= 0.5 {
                    let t = (p - 0.5) * 2.0;
                    (0.9 - 0.7 * t, 0.85, 0.2 + 0.05 * t)
                } else {
                    let t = p * 2.0;
                    (0.95 - 0.05 * t, 0.25 + 0.6 * t, 0.2)
                };
                materials.add(StandardMaterial {
                    base_color: Color::srgb(r, gr, b),
                    emissive: LinearRgba::new(r * 0.6, gr * 0.6, b * 0.3, 1.0),
                    unlit: true,
                    ..default()
                })
            })
            .collect(),
        bar_trail_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.12, 0.08),
            emissive: LinearRgba::new(0.55, 0.05, 0.03, 1.0),
            unlit: true,
            ..default()
        }),
        scribble_quad: meshes.add(Rectangle::new(0.75, 0.6)),
        sel_ring: meshes.add(Cylinder::new(0.55, 0.05)),
        sel_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.95, 1.0, 0.65),
            emissive: LinearRgba::new(0.2, 0.6, 0.8, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        scribble_mats: {
            let mut sets = HashMap::new();
            for (mood, prefix) in [
                ("angry", "scribble"),
                ("error", "scribble_error"),
                ("hurt", "scribble_hurt"),
                ("death", "scribble_death"),
            ] {
                let frames = (0..3)
                    .map(|i| {
                        materials.add(StandardMaterial {
                            base_color_texture: Some(
                                asset_server.load(format!("textures/{prefix}_{i}.png")),
                            ),
                            alpha_mode: AlphaMode::Blend,
                            unlit: true,
                            ..default()
                        })
                    })
                    .collect();
                sets.insert(mood, frames);
            }
            sets
        },
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

    // Terrain slabs (0.96 with grout lines, prototype-style). The default
    // world is natural — grass, water, mountains; the circuit "tech" tile
    // is what terraforming turns ground into.
    for y in 0..world.grid.height {
        for x in 0..world.grid.width {
            let pos = TilePos::new(x, y);
            let kind = world.grid.get(pos).expect("in bounds");
            let (mesh, mat, y_off) = match kind {
                TileKind::Plains => {
                    (palette.tex_slab.clone(), palette.grass_tex_mat.clone(), 0.0)
                }
                // Mountains rise a full block: crossing costs double, and
                // the silhouette should say so.
                TileKind::Rubble => (
                    palette.mountain_block.clone(),
                    palette.mountain_tex_mat.clone(),
                    MOUNTAIN_TOP - 0.10,
                ),
                TileKind::Water => {
                    (palette.tex_slab.clone(), palette.water_tex_mat.clone(), -0.05)
                }
                // Bridges only exist after terraforming; at startup none do
                // (sync_view overlays planks when they appear).
                TileKind::Bridge => {
                    (palette.tex_slab.clone(), palette.ground_tex_mat.clone(), 0.0)
                }
            };
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(tile_xyz(world, pos, y_off - 0.05)),
            ));
        }
    }

    // Depots: low wooden crates.
    for depot in world.depots.values() {
        commands.spawn((
            Mesh3d(palette.crate_box.clone()),
            MeshMaterial3d(palette.crate_mat.clone()),
            Transform::from_translation(tile_xyz(world, depot.pos, 0.15)),
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

    commands.spawn((
        SelMarker,
        Mesh3d(palette.sel_ring.clone()),
        MeshMaterial3d(palette.sel_mat.clone()),
        Transform::from_xyz(0.0, 0.02, 0.0),
        Visibility::Hidden,
    ));

    commands.insert_resource(palette);
}

fn step_sim(mut game: NonSendMut<GameSim>, editor: Res<EditorState>) {
    if editor.paused {
        return;
    }
    game.0.step();
}

/// Space toggles pause (unless the code editor has keyboard focus);
/// the chosen speed drives the fixed timestep.
fn time_controls(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: ResMut<EditorState>,
    mut fixed: ResMut<Time<Fixed>>,
) {
    let typing = contexts.try_ctx_mut().is_some_and(|ctx| ctx.wants_keyboard_input());
    if !typing && keys.just_pressed(KeyCode::Space) {
        editor.paused = !editor.paused;
    }
    fixed.set_timestep_hz((10.0 * editor.speed as f64).max(0.01));
}

// ------------------------------------------------------------------ camera

fn orbit_transform(cam: &OrbitCam) -> Transform {
    let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
    Transform::from_translation(cam.focus + rot * Vec3::new(0.0, 0.0, cam.distance))
        .looking_at(cam.focus, Vec3::Y)
}

fn orbit_camera(
    mut contexts: EguiContexts,
    editor: Res<EditorState>,
    mut gesture: ResMut<LmbGesture>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut last_cursor: Local<Option<Vec2>>,
    mut wheel: EventReader<MouseWheel>,
    mut cams: Query<(&mut OrbitCam, &mut Transform)>,
) {
    // try_ctx_mut: the context is gone during shutdown / not yet there on
    // the first frame — never panic for a camera nicety.
    let over_ui = contexts.try_ctx_mut().is_some_and(|ctx| ctx.wants_pointer_input());
    let Ok((mut cam, mut transform)) = cams.single_mut() else { return };

    // Cursor-position deltas rather than raw MouseMotion: identical for a
    // mouse, but also correct for tablets/synthetic input, and it is the
    // cursor the pan is anchored to anyway.
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    let delta = match (cursor, *last_cursor) {
        (Some(now), Some(before)) => now - before,
        _ => Vec2::ZERO,
    };
    *last_cursor = cursor;
    let scroll: f32 = wheel.read().map(|w| w.y).sum();

    // LMB click-vs-drag: releasing inside the dead zone is the armed
    // tool's click (place_blueprint runs after us and consumes it);
    // outgrowing the dead zone hands the drag to the camera as a pan.
    gesture.clicked = false;
    if buttons.just_released(MouseButton::Left) {
        gesture.clicked = gesture.travel.is_some() && !gesture.panning;
        gesture.travel = None;
        gesture.panning = false;
    }
    if over_ui {
        return;
    }
    if buttons.just_pressed(MouseButton::Left) {
        gesture.travel = Some(0.0);
    }
    if buttons.pressed(MouseButton::Left)
        && let Some(travel) = &mut gesture.travel
    {
        *travel += delta.length();
        if *travel > LMB_DRAG_THRESHOLD {
            gesture.panning = true;
        }
    }

    // Paint keeps its LMB drag (drag = paint an area); with any other tool
    // — or none — a clear drag pans. With no tool armed there is no click
    // to protect, so the pan starts immediately.
    let paint_armed = matches!(editor.selected_build, Some(ToolKind::Paint(_)));
    let lmb_pan = buttons.pressed(MouseButton::Left)
        && !paint_armed
        && (editor.selected_build.is_none() || gesture.panning);
    let panning = buttons.pressed(MouseButton::Middle)
        || (buttons.pressed(MouseButton::Right) && keys.pressed(KeyCode::ShiftLeft))
        || lmb_pan;
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
    gesture: Res<LmbGesture>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut game: NonSendMut<GameSim>,
) {
    if keys.just_pressed(KeyCode::Escape)
        || (editor.selected_build.is_some() && buttons.just_pressed(MouseButton::Right))
    {
        editor.selected_build = None;
        editor.last_paint_tile = None;
        return;
    }
    if keys.just_pressed(KeyCode::KeyR)
        && let Some(ToolKind::Overlay(Some(OverlayKind::Arrow(d)))) = editor.selected_build
    {
        editor.selected_build = Some(ToolKind::Overlay(Some(OverlayKind::Arrow(d.clockwise()))));
    }
    let Some(kind) = editor.selected_build else { return };
    // Paint drags; everything else places on a dead-zone click (a longer
    // LMB drag belongs to the camera pan — see LmbGesture).
    let painting = matches!(kind, ToolKind::Paint(_));
    if painting && !buttons.pressed(MouseButton::Left) {
        editor.last_paint_tile = None;
        return;
    }
    if !painting && !gesture.clicked {
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
        ToolKind::Kill => {
            // Lowest-id bot standing on the clicked tile.
            let victim = game
                .0
                .world
                .bots
                .values()
                .filter(|b| b.data.pos == pos && !b.data.dying)
                .map(|b| b.data.id)
                .min();
            if let Some(bot) = victim {
                let _ = game.0.apply(&Command::KillBot { bot });
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
        ToolKind::Kill => {
            (world.bots.values().any(|b| b.data.pos == pos && !b.data.dying), None)
        }
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
    for (id, bot) in &world.bots {
        let Some(&entity) = index.bots.get(&id.0) else { continue };
        let Ok((mut pose, mut transform)) = poses.get_mut(entity) else { continue };
        let mut y = if bot.data.booting.is_some() {
            0.1 // rising out of the printer
        } else {
            0.45
        };
        // Ride the terrain: mountains lift the bot, water (bridges) don't.
        y += terrain_top(world, bot.data.pos);
        // The problem hop: entering ANY handler (fault, bump, bumped,
        // hurt) makes the bot jump — the handler_init ritual is when it
        // happens. One rule, every problem.
        let in_handler = bot.in_signal_handler();
        if in_handler && !pose.was_in_handler {
            pose.freeze_age = 0;
        } else {
            pose.freeze_age = pose.freeze_age.saturating_add(1);
        }
        pose.was_in_handler = in_handler;
        pose.freeze_seen = bot.data.bump_frozen; // legacy field, unused
        if pose.freeze_age < 5 {
            y += 0.3 * (std::f32::consts::PI * (pose.freeze_age as f32 + 1.0) / 6.0).sin();
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
        // Health-bar recency clock.
        if bot.data.hp != pose.hp_seen {
            pose.hp_seen = bot.data.hp;
            pose.hp_age = 0;
        } else {
            pose.hp_age = pose.hp_age.saturating_add(1);
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
                    palette.printer_tex_mats[&printer.color.0.min(2)].clone(),
                    Vec3::ONE,
                ),
                PrinterState::Ruined => {
                    (palette.printer_ruined_mat.clone(), Vec3::new(1.0, 0.45, 1.0))
                }
            };
            let entity = commands
                .spawn((
                    Mesh3d(palette.printer_box.clone()),
                    MeshMaterial3d(mat),
                    // Face the default camera (the atlas front is -Z).
                    Transform::from_translation(tile_xyz(world, printer.pos, 0.25))
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                        .with_scale(scale),
                ))
                .with_children(|parent| {
                    // The sheet rising out of the top slot; dead machines
                    // don't print.
                    if printer.state == PrinterState::Working {
                        parent.spawn((
                            Mesh3d(palette.paper_sheet.clone()),
                            MeshMaterial3d(palette.paper_mat.clone()),
                            Transform::from_xyz(0.0, 0.33, 0.0)
                                .with_rotation(Quat::from_rotation_z(0.06)),
                        ));
                    }
                    parent.spawn((
                        JobCube,
                        Mesh3d(palette.unit_cube.clone()),
                        MeshMaterial3d(palette.print_glow_mat.clone()),
                        Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::splat(0.1)),
                        Visibility::Hidden,
                    ));
                })
                .id();
            // Print-job progress bar, shown only while a job runs.
            let mut fill_entity = Entity::PLACEHOLDER;
            let mut bar_root = Entity::PLACEHOLDER;
            commands.entity(entity).with_children(|parent| {
                bar_root = parent
                    .spawn((
                        BillboardBar,
                        Transform::from_xyz(0.0, 1.8, 0.0),
                        Visibility::Hidden,
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            Mesh3d(palette.bar_mesh.clone()),
                            MeshMaterial3d(palette.bar_bg_mat.clone()),
                            Transform::default(),
                        ));
                        fill_entity = bar
                            .spawn((
                                ProgressFill,
                                Mesh3d(palette.bar_mesh.clone()),
                                MeshMaterial3d(palette.bar_fill_mat.clone()),
                                Transform::from_xyz(0.0, 0.0, 0.011)
                                    .with_scale(Vec3::new(0.02, 0.8, 1.0)),
                            ))
                            .id();
                    })
                    .id();
            });
            index.printers.insert(id.0, (entity, printer.state));
            index.printer_fills.insert(id.0, (bar_root, fill_entity));
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
        let mut bar_root = Entity::PLACEHOLDER;
        let mut health_fill = Entity::PLACEHOLDER;
        let mut health_trail = Entity::PLACEHOLDER;
        let entity = commands
            .spawn((
                Mesh3d(palette.bot_cube.clone()),
                MeshMaterial3d(palette.bot_tex_mats[&bot.data.color.0.min(2)].clone()),
                Transform::from_translation(start),
                Pose {
                    prev: start,
                    curr: start,
                    grid: bot.data.pos,
                    was_in_handler: false,
                    fault_seen: bot.vm.as_ref().map(|v| v.fault_count()).unwrap_or(0),
                    fault_age: u32::MAX,
                    hp_seen: bot.data.hp,
                    hp_age: u32::MAX,
                    freeze_seen: 0,
                    freeze_age: u32::MAX,
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(palette.nose_cube.clone()),
                    MeshMaterial3d(palette.nose_mat.clone()),
                    Transform::from_xyz(0.0, 0.05, -0.45),
                ));
                // Health bar: shown for a few seconds after any hp change.
                bar_root = parent
                    .spawn((
                        BillboardBar,
                        Transform::from_xyz(0.0, 1.2, 0.0),
                        Visibility::Hidden,
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            Mesh3d(palette.bar_mesh.clone()),
                            MeshMaterial3d(palette.bar_bg_mat.clone()),
                            Transform::default().with_scale(Vec3::new(0.8, 0.7, 1.0)),
                        ));
                        health_trail = bar
                            .spawn((
                                HealthTrail { frac: 1.0 },
                                Mesh3d(palette.bar_mesh.clone()),
                                MeshMaterial3d(palette.bar_trail_mat.clone()),
                                Transform::from_xyz(0.0, 0.0, 0.0105)
                                    .with_scale(Vec3::new(0.8, 0.55, 1.0)),
                            ))
                            .id();
                        health_fill = bar
                            .spawn((
                                HealthFill,
                                Mesh3d(palette.bar_mesh.clone()),
                                MeshMaterial3d(
                                    palette.bar_health_grad.last().expect("bins").clone(),
                                ),
                                Transform::from_xyz(0.0, 0.0, 0.011)
                                    .with_scale(Vec3::new(0.02, 0.55, 1.0)),
                            ))
                            .id();
                    })
                    .id();
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
        index.bot_health.insert(id.0, (bar_root, health_fill, health_trail));
        let scribble = commands
            .spawn((
                ScribbleCloud,
                BillboardBar, // camera-facing, parent-rotation compensated
                Mesh3d(palette.scribble_quad.clone()),
                MeshMaterial3d(palette.scribble_mats["angry"][0].clone()),
                Transform::from_xyz(0.0, 1.75, 0.0),
                Visibility::Hidden,
            ))
            .id();
        commands.entity(entity).add_child(scribble);
        index.bot_scribbles.insert(id.0, scribble);
    }
    index.bots.retain(|id, entity| {
        if seen.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
    index.bot_health.retain(|id, _| seen.contains(id));
    index.bot_scribbles.retain(|id, _| seen.contains(id));

    // Blueprints: glowing ghost slabs with a billboarded progress bar.
    for (id, bp) in &world.blueprints {
        if index.blueprints.contains_key(&id.0) {
            continue;
        }
        let mut fill_entity = Entity::PLACEHOLDER;
        let entity = commands
            .spawn((
                Mesh3d(palette.tile_slab.clone()),
                MeshMaterial3d(palette.print_glow_mat.clone()),
                Transform::from_translation(tile_xyz(world, bp.pos, 0.05)),
            ))
            .with_children(|parent| {
                parent
                    .spawn((
                        BillboardBar,
                        Transform::from_xyz(0.0, 0.9, 0.0),
                        Visibility::default(),
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            Mesh3d(palette.bar_mesh.clone()),
                            MeshMaterial3d(palette.bar_bg_mat.clone()),
                            Transform::default(),
                        ));
                        fill_entity = bar
                            .spawn((
                                ProgressFill,
                                Mesh3d(palette.bar_mesh.clone()),
                                MeshMaterial3d(palette.bar_fill_mat.clone()),
                                Transform::from_xyz(0.0, 0.0, 0.011)
                                    .with_scale(Vec3::new(0.02, 0.8, 1.0)),
                            ))
                            .id();
                    });
            })
            .id();
        index.blueprints.insert(id.0, entity);
        index.blueprint_fills.insert(id.0, fill_entity);
    }
    index.blueprints.retain(|id, entity| {
        if world.blueprints.contains_key(&sim::EntityId(*id)) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
    index
        .blueprint_fills
        .retain(|id, _| world.blueprints.contains_key(&sim::EntityId(*id)));

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
                Transform::from_translation(tile_xyz(
                    world,
                    *pos,
                    terrain_top(world, *pos) + 0.08,
                ))
                .with_rotation(rot),
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
                Transform::from_translation(tile_xyz(
                    world,
                    *pos,
                    terrain_top(world, *pos) + 0.02,
                ))
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

    // Wrecks: charred dead-bot slabs.
    for (id, wreck) in &world.wrecks {
        if let std::collections::hash_map::Entry::Vacant(e) = index.wrecks.entry(id.0) {
            let entity = commands
                .spawn((
                    Mesh3d(palette.pad_slab.clone()),
                    MeshMaterial3d(palette.wreck_tex_mat.clone()),
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

/// Grow each progress fill (left-anchored): blueprints always show their
/// bar; printers show one only while a print job runs.
fn update_progress_bars(
    game: NonSend<GameSim>,
    index: Res<ViewIndex>,
    mut fills: Query<&mut Transform, With<ProgressFill>>,
    mut roots: Query<&mut Visibility, With<BillboardBar>>,
) {
    let set_fill = |transform: &mut Transform, p: f32| {
        let p = p.clamp(0.02, 1.0);
        transform.scale = Vec3::new(p, 0.8, 1.0);
        transform.translation.x = -(0.9 * (1.0 - p)) / 2.0;
    };
    for (id, bp) in &game.0.world.blueprints {
        let Some(&fill) = index.blueprint_fills.get(&id.0) else { continue };
        let Ok(mut transform) = fills.get_mut(fill) else { continue };
        set_fill(&mut transform, bp.progress as f32 / bp.needed as f32);
    }
    let total = game.0.tuning.print_ticks as f32;
    for (id, printer) in &game.0.world.printers {
        let Some(&(root, fill)) = index.printer_fills.get(&id.0) else { continue };
        let Ok(mut visibility) = roots.get_mut(root) else { continue };
        match printer.job {
            Some(ticks_left) => {
                *visibility = Visibility::Visible;
                if let Ok(mut transform) = fills.get_mut(fill) {
                    set_fill(&mut transform, 1.0 - ticks_left as f32 / total);
                }
            }
            None => *visibility = Visibility::Hidden,
        }
    }
}

/// Health bars: visible while the hp change is recent, red fill = hp
/// fraction (left-anchored, slightly narrower than build bars).
fn update_health_bars(
    time: Res<Time>,
    game: NonSend<GameSim>,
    index: Res<ViewIndex>,
    palette: Res<Palette>,
    poses: Query<&Pose>,
    mut fills: Query<
        (&mut Transform, &mut MeshMaterial3d<StandardMaterial>),
        (With<HealthFill>, Without<HealthTrail>),
    >,
    mut trails: Query<(&mut Transform, &mut HealthTrail), Without<HealthFill>>,
    mut roots: Query<&mut Visibility, With<BillboardBar>>,
) {
    // Left-anchored bar segment within the 0.9-wide mesh scaled by 0.8.
    let place = |transform: &mut Transform, frac: f32, height: f32| {
        let frac = frac.clamp(0.02, 1.0);
        transform.scale = Vec3::new(frac * 0.8, height, 1.0);
        transform.translation.x = -(0.9 * 0.8 * (1.0 - frac)) / 2.0;
    };
    for (id, bot) in &game.0.world.bots {
        let Some(&(root, fill, trail)) = index.bot_health.get(&id.0) else { continue };
        let Some(&view) = index.bots.get(&id.0) else { continue };
        let Ok(pose) = poses.get(view) else { continue };
        let Ok(mut visibility) = roots.get_mut(root) else { continue };
        let p = (bot.data.hp as f32 / bot.data.max_hp as f32).clamp(0.0, 1.0);
        // ~3 s at 10 Hz; permanent while below half (Damaged).
        let recent = pose.hp_age < 30 || bot.data.hp * 2 < bot.data.max_hp;
        *visibility = if recent { Visibility::Visible } else { Visibility::Hidden };
        if let Ok((mut transform, mut ghost)) = trails.get_mut(trail) {
            if recent {
                // Ghost drains toward the real fraction; heals snap it up.
                ghost.frac = ghost.frac.max(p);
                ghost.frac = (ghost.frac - 0.35 * time.delta_secs()).max(p);
                place(&mut transform, ghost.frac, 0.55);
            } else {
                ghost.frac = p; // no stale chunk on the next reveal
            }
        }
        if recent && let Ok((mut transform, mut material)) = fills.get_mut(fill) {
            place(&mut transform, p, 0.55);
            // Green -> yellow -> red as health falls.
            let bins = &palette.bar_health_grad;
            let bin = ((p * (bins.len() - 1) as f32).round() as usize).min(bins.len() - 1);
            if material.0 != bins[bin] {
                material.0 = bins[bin].clone();
            }
        }
    }
}

/// Frustration clouds: visible while bump-frozen, cycling scribble frames
/// with a little scale pulse — thinking, angrily.
fn update_scribbles(
    time: Res<Time>,
    game: NonSend<GameSim>,
    index: Res<ViewIndex>,
    palette: Res<Palette>,
    mut clouds: Query<
        (&mut Visibility, &mut MeshMaterial3d<StandardMaterial>, &mut Transform),
        With<ScribbleCloud>,
    >,
) {
    let t = time.elapsed_secs();
    for (id, bot) in &game.0.world.bots {
        let Some(&cloud) = index.bot_scribbles.get(&id.0) else { continue };
        let Ok((mut visibility, mut material, mut transform)) = clouds.get_mut(cloud) else {
            continue;
        };
        // The cloud reads *why* the bot is upset: angry squiggle for bump
        // stun and collision handlers, ?! for error, starburst for hurt,
        // skull for the death handler.
        let mood = if bot.data.bump_frozen > 0 {
            Some("angry")
        } else {
            bot.handler_name().map(|name| match name {
                "error" | "hurt" | "death" => name,
                _ => "angry", // bump / bumped share the collision squiggle
            })
        };
        if let Some(mood) = mood {
            let frames = &palette.scribble_mats[mood];
            let frame = ((t * 8.0) as usize) % frames.len();
            *visibility = Visibility::Visible;
            if material.0 != frames[frame] {
                material.0 = frames[frame].clone();
            }
            transform.scale = Vec3::splat(1.0 + 0.08 * (t * 9.0).sin());
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Progress/health bars always face the camera, level and consistent —
/// compensating for the parent's rotation (bots turn; their bars must not).
fn billboard_bars(
    cams: Query<&Transform, (With<Camera3d>, Without<BillboardBar>)>,
    parents: Query<&Transform, (Without<BillboardBar>, Without<Camera3d>)>,
    mut bars: Query<(&mut Transform, Option<&ChildOf>), With<BillboardBar>>,
) {
    let Ok(cam) = cams.single() else { return };
    for (mut bar, child_of) in &mut bars {
        let parent_rotation = child_of
            .and_then(|c| parents.get(c.parent()).ok())
            .map(|t| t.rotation)
            .unwrap_or(Quat::IDENTITY);
        // World rotation = parent * local; we want world == camera.
        bar.rotation = parent_rotation.inverse() * cam.rotation;
    }
}

/// Click a bot (no tool armed, click-not-drag) to inspect it; click empty
/// ground or press Esc to deselect.
fn select_bot(
    mut contexts: EguiContexts,
    mut editor: ResMut<EditorState>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    game: NonSend<GameSim>,
) {
    let typing = contexts.try_ctx_mut().is_some_and(|ctx| ctx.wants_keyboard_input());
    if !typing && keys.just_pressed(KeyCode::Escape) && editor.selected_build.is_none() {
        editor.selected_bot = None;
    }
    if editor.selected_build.is_some() {
        return; // armed tools own the mouse
    }
    let over_ui = contexts.try_ctx_mut().is_some_and(|ctx| ctx.wants_pointer_input());
    let cursor = windows.single().ok().and_then(|w| w.cursor_position());
    if buttons.just_pressed(MouseButton::Left) && !over_ui {
        editor.press_pos = cursor;
    }
    if buttons.just_released(MouseButton::Left) {
        let Some(press) = editor.press_pos.take() else { return };
        if over_ui {
            return;
        }
        let Some(now) = cursor else { return };
        if press.distance(now) > 6.0 {
            return; // that was a pan, not a click
        }
        let world = &game.0.world;
        let Some(tile) = cursor_tile(&windows, &cams, world.grid.width, world.grid.height)
        else {
            return;
        };
        editor.selected_bot = world
            .bots
            .values()
            .find(|b| b.data.pos == tile && !b.data.dying)
            .map(|b| b.data.id.0);
    }
}

/// The cyan ring tracks the inspected bot (interpolated view position).
fn update_sel_marker(
    editor: Res<EditorState>,
    index: Res<ViewIndex>,
    time: Res<Time>,
    views: Query<&Transform, Without<SelMarker>>,
    mut marker: Query<(&mut Transform, &mut Visibility), With<SelMarker>>,
) {
    let Ok((mut transform, mut visibility)) = marker.single_mut() else { return };
    let target = editor
        .selected_bot
        .and_then(|id| index.bots.get(&id))
        .and_then(|&e| views.get(e).ok());
    match target {
        Some(view) => {
            transform.translation = Vec3::new(view.translation.x, 0.06, view.translation.z);
            transform.rotate_y(1.5 * time.delta_secs());
            *visibility = Visibility::Visible;
        }
        None => *visibility = Visibility::Hidden,
    }
}

/// The inspector: live program with the executing line highlighted, VM
/// state, vitals, XP, and logs — the transparency pillar as UI.
fn inspector_ui(
    mut contexts: EguiContexts,
    mut editor: ResMut<EditorState>,
    game: NonSend<GameSim>,
) {
    let Some(bot_id) = editor.selected_bot else { return };
    let Some(ctx) = contexts.try_ctx_mut() else { return };
    let world = &game.0.world;
    let mut open = true;

    egui::SidePanel::right("inspector").exact_width(320.0).show(ctx, |ui| {
        let Some(bot) = world.bots.get(&sim::world::BotId(bot_id)) else {
            // The bot is gone: wreck or total destruction.
            if let Some(wreck) = world.wrecks.get(&sim::world::BotId(bot_id)) {
                ui.heading(format!("Bot {bot_id} — WRECKED"));
                ui.label(format!("at ({}, {})", wreck.pos.x, wreck.pos.y));
                ui.separator();
                ui.strong("recovered logs");
                for line in &wreck.logs {
                    ui.monospace(line);
                }
            } else {
                ui.heading(format!("Bot {bot_id} — DESTROYED"));
                ui.small("No wreck. Check the black boxes and the cloud.");
            }
            if ui.button("close").clicked() {
                open = false;
            }
            return;
        };
        let data = &bot.data;
        let color_name = match data.color {
            BotColor::GREEN => "Green",
            BotColor::RED => "Red",
            _ => "Blue",
        };
        ui.horizontal(|ui| {
            ui.heading(format!("Bot {bot_id} — {color_name}"));
            if ui.button("✕").clicked() {
                open = false;
            }
        });

        // Status line.
        let status = if data.booting.is_some() {
            "booting".to_string()
        } else if data.recall.is_some() {
            "recalled (engine)".to_string()
        } else if data.bump_frozen > 0 {
            format!("bump-frozen ({} ticks)", data.bump_frozen)
        } else if bot.in_handler_init() {
            let ticks = match &bot.data.action {
                Some(sim::world::Action::Wait { ticks_left }) => format!(" — {ticks_left} ticks left"),
                _ => String::new(),
            };
            let signal = bot.handler_name().unwrap_or("?");
            format!("flinching: handler_init() for `{signal}`{ticks}")
        } else if let Some(signal) = bot.handler_name() {
            if bot.in_default_handler() {
                format!("handling: on {signal}: (engine default)")
            } else {
                format!("handling: on {signal}:")
            }
        } else if bot.vm.as_ref().is_some_and(|vm| vm.is_blocked()) {
            match &data.action {
                Some(sim::world::Action::Move { path, .. }) => {
                    format!("moving ({} tiles left)", path.len())
                }
                Some(sim::world::Action::Mine { .. }) => "mining".into(),
                Some(sim::world::Action::Deposit { .. }) => "depositing".into(),
                Some(sim::world::Action::Attack { .. }) => "attacking".into(),
                Some(sim::world::Action::Wait { ticks_left }) => {
                    format!("waiting ({ticks_left} ticks)")
                }
                Some(sim::world::Action::Build { .. }) => "building".into(),
                None => "blocked".into(),
            }
        } else {
            "thinking".to_string()
        };
        ui.label(status);
        ui.monospace(format!(
            "hp {}/{}   cargo {}/{}   at ({}, {})",
            data.hp, data.max_hp, data.cargo, data.cargo_cap, data.pos.x, data.pos.y
        ));
        ui.monospace(format!(
            "xp  mine {}  haul {}  fight {}  build {}",
            data.xp_mining, data.xp_hauling, data.xp_combat, data.xp_building
        ));
        ui.separator();

        // VM state.
        if let Some(vm) = &bot.vm {
            ui.monospace(format!(
                "line {}   budget {}   faults {} ({} crashes)",
                vm.current_line(),
                vm.budget(),
                vm.fault_count(),
                vm.crash_count()
            ));
            if let Some(fault) = vm.last_fault() {
                ui.colored_label(egui::Color32::from_rgb(240, 120, 100), format!("last: {fault}"));
            }
            ui.separator();

            // While an engine default handler runs, show ITS code with the
            // executing line highlighted — the engine's response is real
            // Pyrite, debuggable like anything else.
            let in_handler = bot.in_signal_handler();
            let in_init = bot.in_handler_init();
            let default_running = bot.in_default_handler();
            // While ANY handler runs, show the full causal chain: the
            // forced entry ritual first, then the handler code.
            if in_handler && let Some(signal) = bot.handler_name() {
                if default_running {
                    ui.strong(format!("engine default: on {signal}:"));
                } else {
                    ui.strong(format!("handler: on {signal}:"));
                }
                // The unskippable entry ritual, as its own visible step.
                if signal != "death" {
                    let ritual = "  ⚙ handler_init()   # forced entry ritual — the flinch";
                    if in_init {
                        ui.label(
                            egui::RichText::new(ritual)
                                .monospace()
                                .background_color(egui::Color32::from_rgb(85, 55, 20))
                                .color(egui::Color32::from_rgb(255, 220, 160)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(ritual)
                                .monospace()
                                .color(egui::Color32::from_rgb(140, 130, 110)),
                        );
                    }
                }
                // Default handlers: show their own source with the line
                // highlight (suppressed during init — the counter still
                // points at the pre-fault line).
                if default_running
                    && let Some(src) = bot.default_handler_source(if signal == "death" {
                        "death"
                    } else {
                        "signal"
                    })
                {
                    let current = if in_init { 0 } else { vm.current_line() as usize };
                    for (i, line) in src.lines().enumerate() {
                        let n = i + 1;
                        let text = format!("{n:>3} {line}");
                        if n == current {
                            ui.label(
                                egui::RichText::new(text)
                                    .monospace()
                                    .background_color(egui::Color32::from_rgb(70, 45, 25))
                                    .color(egui::Color32::from_rgb(255, 230, 200)),
                            );
                        } else {
                            ui.monospace(text);
                        }
                    }
                }
                ui.separator();
            }

            // The program, current line highlighted (only meaningful while
            // the main program is executing).
            ui.strong("program");
            let source = world
                .color_programs
                .get(&(data.faction, data.color.0))
                .map(|cp| cp.source.clone());
            match source {
                Some(source) => {
                    let current = if default_running || in_init {
                        0
                    } else {
                        vm.current_line() as usize
                    };
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (i, line) in source.lines().enumerate() {
                            let n = i + 1;
                            let text = format!("{n:>3} {line}");
                            if n == current {
                                ui.label(
                                    egui::RichText::new(text)
                                        .monospace()
                                        .background_color(egui::Color32::from_rgb(60, 70, 30))
                                        .color(egui::Color32::from_rgb(230, 255, 200)),
                                );
                            } else {
                                ui.monospace(text);
                            }
                        }
                    });
                }
                None => {
                    ui.small("(no deployed source for this color)");
                }
            }
        }
        ui.separator();

        // Handler coverage: player-installed ones point at their line;
        // the rest show the engine default in plain words.
        ui.strong("handlers");
        ui.small(format!(
            "every entry begins: handler_init() — wait {} ticks (engine, unskippable)",
            game.0.tuning.handler_init_ticks
        ));
        let tuning = &game.0.tuning;
        for (signal, line) in bot.handler_summary() {
            match line {
                Some(n) => {
                    ui.monospace(format!("on {signal}: — line {n}"));
                }
                None => match bot.default_handler_source(signal) {
                    // The engine default IS code — show it.
                    Some(src) => {
                        let code = src.trim_end();
                        let note = match signal {
                            "error" => format!("  (+crash: -{} hp)", tuning.fault_damage),
                            "bump" | "bumped" => format!("  (-{} hp)", tuning.bump_damage),
                            _ => String::new(),
                        };
                        ui.monospace(format!("on {signal}: (engine) {code}{note}"));
                    }
                    None => {
                        ui.monospace(format!("on {signal}: (engine) nothing"));
                    }
                },
            }
        }
        ui.separator();

        ui.strong("local logs");
        if data.log_buf.is_empty() {
            ui.small("(empty)");
        }
        for line in &data.log_buf {
            ui.monospace(line);
        }
    });

    if !open {
        editor.selected_bot = None;
    }
}

