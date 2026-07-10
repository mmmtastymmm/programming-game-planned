//! Colony viewer & editor, modeled on the original prototype's look:
//! bright emissive primitives on a dark void, orbit camera, and a live
//! Pyrite editor panel on the left. No models — color IS the identity.
//!
//! The sim steps on `FixedUpdate` at 10 Hz (docs/07 tick rate); rendering
//! free-runs with per-frame interpolation between ticks. All game state
//! lives in `sim::Sim` (NonSend — the VMs hold `Rc`s); the UI only ever
//! mutates it through `Command`s, like any other lockstep peer.
//!
//! Camera: RMB drag = orbit · Shift+RMB / MMB = pan · scroll = zoom.
//! Run: `cargo run -p game`

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::{Color as BotColor, PrinterState};
use sim::{TileKind, TilePos};
use std::collections::HashMap;

/// The doc's Tier-0 starter program, verbatim.
const MINER: &str = "\
move_to(nearest_ore())
mine()
move_to(nearest_depot())
deposit()
";

struct GameSim(Sim);

// ---------------------------------------------------------------- palette
// Lifted from the original prototype (see docs of that repo).
const CLEAR: Color = Color::srgb(0.05, 0.06, 0.09);
const GROUND: Color = Color::srgb(0.20, 0.36, 0.22);
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
}

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
}

#[derive(Resource)]
struct EditorState {
    code: String,
    status: String,
    status_ok: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self { code: MINER.to_string(), status: "ready".into(), status_ok: true }
    }
}

// ------------------------------------------------------------------ world

fn build_colony() -> Sim {
    let mut spec = MapSpec::empty(24, 14);
    for y in 2..9 {
        spec.rubble.push(TilePos::new(12, y));
    }
    for x in 15..19 {
        for y in 9..12 {
            spec.water.push(TilePos::new(x, y));
        }
    }
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
        source: MINER.into(),
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

fn setup_scene(
    mut commands: Commands,
    game: NonSend<GameSim>,
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
    };

    // Terrain slabs (0.96 with grout lines, prototype-style).
    let slab_plains = meshes.add(Cuboid::new(0.96, 0.1, 0.96));
    let plains_mat = materials.add(StandardMaterial {
        base_color: GROUND,
        perceptual_roughness: 0.9,
        ..default()
    });
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
                TileKind::Plains => (plains_mat.clone(), 0.0),
                TileKind::Rubble => (rubble_mat.clone(), 0.04),
                TileKind::Water => (water_mat.clone(), -0.05),
            };
            commands.spawn((
                Mesh3d(slab_plains.clone()),
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
    let over_ui = contexts.ctx_mut().wants_pointer_input();
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
                Mesh3d(palette.unit_cube.clone()),
                MeshMaterial3d(palette.bot_mats[&bot.data.color.0.min(2)].clone()),
                Transform::from_translation(start),
                Pose { prev: start, curr: start, grid: bot.data.pos },
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

fn editor_ui(
    mut contexts: EguiContexts,
    mut game: NonSendMut<GameSim>,
    mut editor: ResMut<EditorState>,
) {
    let ctx = contexts.ctx_mut();
    egui::SidePanel::left("editor").exact_width(300.0).show(ctx, |ui| {
        ui.heading("Pyrite");
        ui.add(
            egui::TextEdit::multiline(&mut editor.code)
                .font(egui::TextStyle::Monospace)
                .desired_rows(14)
                .desired_width(f32::INFINITY),
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
