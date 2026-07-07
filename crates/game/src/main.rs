//! 3D colony viewer: watch the deterministic sim run a living colony.
//!
//! The sim steps on `FixedUpdate` at 10 Hz (docs/07 tick rate); rendering
//! free-runs. This is a *window into* the sim, never a mutator — all game
//! state lives in `sim::Sim`, held as a NonSend resource (the VM holds
//! `Rc`s, and the sim must stay on one thread anyway).
//!
//! Models: Kenney "Space Kit" (CC0) — see assets/models/kenney/LICENSE.txt.
//!
//! Run: `cargo run -p game`

use bevy::prelude::*;
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

/// Render-side bookkeeping: sim id -> spawned view entity.
/// (HashMap is fine here — this is the render layer, not the sim.)
#[derive(Resource, Default)]
struct ViewIndex {
    bots: HashMap<u32, Entity>,
    ore: HashMap<u64, Entity>,
    wrecks: HashMap<u32, Entity>,
    black_boxes: usize,
    printers: HashMap<u64, (Entity, PrinterState)>,
}

#[derive(Resource)]
struct Models {
    rover: Handle<Scene>,
    hangar: Handle<Scene>,
    depot: Handle<Scene>,
    crystals: Handle<Scene>,
    crystals_large: Handle<Scene>,
    rocks_small: Handle<Scene>,
    crater: Handle<Scene>,
    ring_mesh: Handle<Mesh>,
    box_mesh: Handle<Mesh>,
    green_mat: Handle<StandardMaterial>,
    red_mat: Handle<StandardMaterial>,
    other_mat: Handle<StandardMaterial>,
    black_mat: Handle<StandardMaterial>,
}

impl Models {
    fn tint(&self, color: BotColor) -> Handle<StandardMaterial> {
        match color {
            BotColor::GREEN => self.green_mat.clone(),
            BotColor::RED => self.red_mat.clone(),
            _ => self.other_mat.clone(),
        }
    }
}

/// Remembers the previous grid position to derive facing.
#[derive(Component)]
struct LastPos(TilePos);

fn build_colony() -> Sim {
    let mut spec = MapSpec::empty(24, 14);
    // Terrain: a rubble ridge and a small lake.
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
    // The doc's starting state: working Green printer, ruined Red one.
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
                title: "programming game — colony viewer".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Time::<Fixed>::from_hz(10.0))
        .insert_resource(ViewIndex::default())
        .add_systems(Startup, (setup_sim, setup_scene).chain())
        .add_systems(FixedUpdate, step_sim)
        .add_systems(Update, sync_view)
        .run();
}

/// Tile -> world: the map lies on the XZ plane, one unit per tile.
fn tile_xyz(pos: TilePos, y: f32) -> Vec3 {
    Vec3::new(pos.x as f32, y, pos.y as f32)
}

/// Exclusive startup: the sim is a NonSend resource (Rc inside the VMs).
fn setup_sim(world: &mut World) {
    world.insert_non_send_resource(GameSim(build_colony()));
}

fn setup_scene(
    mut commands: Commands,
    game: NonSend<GameSim>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let game = &game.0;
    let (w, h) = (game.world.grid.width, game.world.grid.height);

    let models = Models {
        rover: asset_server.load("models/kenney/rover.glb#Scene0"),
        hangar: asset_server.load("models/kenney/hangar_smallA.glb#Scene0"),
        depot: asset_server.load("models/kenney/machine_barrelLarge.glb#Scene0"),
        crystals: asset_server.load("models/kenney/rock_crystals.glb#Scene0"),
        crystals_large: asset_server.load("models/kenney/rock_crystalsLargeA.glb#Scene0"),
        rocks_small: asset_server.load("models/kenney/rocks_smallA.glb#Scene0"),
        crater: asset_server.load("models/kenney/crater.glb#Scene0"),
        ring_mesh: meshes.add(Cylinder::new(0.42, 0.06)),
        box_mesh: meshes.add(Cuboid::new(0.22, 0.22, 0.22)),
        green_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.9, 0.3),
            emissive: LinearRgba::new(0.02, 0.4, 0.05, 1.0),
            ..default()
        }),
        red_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.25, 0.2),
            emissive: LinearRgba::new(0.4, 0.03, 0.02, 1.0),
            ..default()
        }),
        other_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.4, 0.5, 0.95),
            ..default()
        }),
        black_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.05, 0.05, 0.06),
            ..default()
        }),
    };

    // Ground tiles: shared meshes, one entity per tile (water sits lower
    // and translucent; rubble tiles get a rock model on top).
    let tile_mesh = meshes.add(Cuboid::new(1.0, 0.1, 1.0));
    let plains_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.36, 0.28),
        perceptual_roughness: 1.0,
        ..default()
    });
    let rubble_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.28, 0.24),
        perceptual_roughness: 1.0,
        ..default()
    });
    let water_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.15, 0.35, 0.7, 0.75),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.2,
        ..default()
    });
    for y in 0..h {
        for x in 0..w {
            let pos = TilePos::new(x, y);
            let kind = game.world.grid.get(pos).expect("in bounds");
            let (mat, y_off) = match kind {
                TileKind::Plains => (plains_mat.clone(), 0.0),
                TileKind::Rubble => (rubble_mat.clone(), 0.02),
                TileKind::Water => (water_mat.clone(), -0.06),
            };
            commands.spawn((
                Mesh3d(tile_mesh.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(tile_xyz(pos, y_off - 0.05)),
            ));
            if kind == TileKind::Rubble {
                commands.spawn((
                    SceneRoot(models.rocks_small.clone()),
                    Transform::from_translation(tile_xyz(pos, 0.0))
                        .with_scale(Vec3::splat(0.8)),
                ));
            }
        }
    }

    // Depots (static).
    for depot in game.world.depots.values() {
        commands.spawn((
            SceneRoot(models.depot.clone()),
            Transform::from_translation(tile_xyz(depot.pos, 0.0)),
        ));
    }

    // Light + camera: angled overview of the whole map.
    commands.spawn((
        DirectionalLight { illuminance: 9_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(8.0, 16.0, 6.0).looking_at(Vec3::new(w as f32 / 2.0, 0.0, h as f32 / 2.0), Vec3::Y),
    ));
    commands.insert_resource(AmbientLight { brightness: 250.0, ..default() });
    let center = Vec3::new(w as f32 / 2.0, 0.0, h as f32 / 2.0);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(center + Vec3::new(0.0, 14.0, 11.0)).looking_at(center, Vec3::Y),
    ));

    commands.insert_resource(models);
}

fn step_sim(mut game: NonSendMut<GameSim>) {
    game.0.step();

    // Story beat: once the colony banks enough ore, fix the ruined Red
    // printer and staff it (stands in for the player until real UI).
    let sim = &mut game.0;
    if sim.world.stockpile_ore >= 40 {
        let ruined: Vec<_> = sim
            .world
            .printers
            .iter()
            .filter(|(_, p)| p.state == PrinterState::Ruined)
            .map(|(id, _)| *id)
            .collect();
        for id in ruined {
            let _ = sim.apply(&Command::DeployProgram {
                faction: 0,
                color: BotColor::RED,
                source: MINER.into(),
            });
            let _ = sim.apply(&Command::RepairPrinter { printer: id });
            let _ = sim.apply(&Command::SetDesiredMax { printer: id, value: 3 });
        }
    }
}

/// Diff the sim state into persistent view entities.
#[allow(clippy::too_many_arguments)]
fn sync_view(
    mut commands: Commands,
    game: NonSend<GameSim>,
    models: Res<Models>,
    mut index: ResMut<ViewIndex>,
    mut transforms: Query<&mut Transform>,
    mut last_pos: Query<&mut LastPos>,
    mut windows: Query<&mut Window>,
) {
    let world = &game.0.world;

    // --- printers: respawn the view when state flips (ruined -> working) ---
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
            let (scale, y) = match printer.state {
                PrinterState::Working => (Vec3::ONE, 0.0),
                PrinterState::Ruined => (Vec3::new(1.0, 0.45, 1.0), -0.02),
            };
            let entity = commands
                .spawn((
                    SceneRoot(models.hangar.clone()),
                    Transform::from_translation(tile_xyz(printer.pos, y)).with_scale(scale),
                ))
                .with_children(|parent| {
                    if printer.state == PrinterState::Working {
                        parent.spawn((
                            Mesh3d(models.ring_mesh.clone()),
                            MeshMaterial3d(models.tint(printer.color)),
                            Transform::from_xyz(0.0, 0.04, 0.85),
                        ));
                    }
                })
                .id();
            index.printers.insert(id.0, (entity, printer.state));
        }
    }

    // --- ore: scale with remaining amount, despawn when gone ---
    for (id, node) in &world.ore_nodes {
        if node.amount == 0 {
            if let Some(entity) = index.ore.remove(&id.0) {
                commands.entity(entity).despawn();
            }
            continue;
        }
        let scale = Vec3::splat(0.5 + 0.7 * (node.amount as f32 / 60.0).min(1.0));
        match index.ore.get(&id.0) {
            Some(&entity) => {
                if let Ok(mut transform) = transforms.get_mut(entity) {
                    transform.scale = scale;
                }
            }
            None => {
                let model = if node.amount > 40 {
                    models.crystals_large.clone()
                } else {
                    models.crystals.clone()
                };
                let entity = commands
                    .spawn((
                        SceneRoot(model),
                        Transform::from_translation(tile_xyz(node.pos, 0.0)).with_scale(scale),
                    ))
                    .id();
                index.ore.insert(id.0, entity);
            }
        }
    }

    // --- bots: spawn rover + color ring, move & face travel direction ---
    let mut seen: Vec<u32> = Vec::new();
    for (id, bot) in &world.bots {
        seen.push(id.0);
        // Booting bots rise out of the printer; recalled bots sink slightly.
        let y = if bot.data.booting.is_some() {
            -0.25
        } else if bot.data.recall.is_some() {
            -0.08
        } else {
            0.0
        };
        let target = tile_xyz(bot.data.pos, y);
        match index.bots.get(&id.0) {
            Some(&entity) => {
                if let Ok(mut transform) = transforms.get_mut(entity) {
                    transform.translation = target;
                    if let Ok(mut last) = last_pos.get_mut(entity) {
                        let prev = last.0;
                        if prev != bot.data.pos {
                            let dx = (bot.data.pos.x - prev.x) as f32;
                            let dz = (bot.data.pos.y - prev.y) as f32;
                            transform.rotation = Quat::from_rotation_y(dx.atan2(dz));
                            last.0 = bot.data.pos;
                        }
                    }
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        SceneRoot(models.rover.clone()),
                        Transform::from_translation(target).with_scale(Vec3::splat(0.7)),
                        LastPos(bot.data.pos),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            Mesh3d(models.ring_mesh.clone()),
                            MeshMaterial3d(models.tint(bot.data.color)),
                            Transform::from_xyz(0.0, 0.03, 0.0).with_scale(Vec3::splat(1.2)),
                        ));
                    })
                    .id();
                index.bots.insert(id.0, entity);
            }
        }
    }
    index.bots.retain(|id, entity| {
        if seen.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    // --- wrecks ---
    for (id, wreck) in &world.wrecks {
        if !index.wrecks.contains_key(&id.0) {
            let entity = commands
                .spawn((
                    SceneRoot(models.crater.clone()),
                    Transform::from_translation(tile_xyz(wreck.pos, 0.0))
                        .with_scale(Vec3::splat(0.7)),
                ))
                .id();
            index.wrecks.insert(id.0, entity);
        }
    }

    // --- black boxes (append-only) ---
    while index.black_boxes < world.black_boxes.len() {
        let bb = &world.black_boxes[index.black_boxes];
        commands.spawn((
            Mesh3d(models.box_mesh.clone()),
            MeshMaterial3d(models.black_mat.clone()),
            Transform::from_translation(tile_xyz(bb.pos, 0.12)),
        ));
        index.black_boxes += 1;
    }

    // HUD in the title bar until real UI exists.
    if let Ok(mut window) = windows.single_mut() {
        window.title = format!(
            "programming game — tick {} | ore {} | bots {} | wrecks {} | cloud entries {}",
            world.tick,
            world.stockpile_ore,
            world.bots.len(),
            world.wrecks.len(),
            world.archive.len(),
        );
    }
}
