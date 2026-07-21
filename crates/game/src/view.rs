//! World-to-render sync: view components, per-frame interpolation,
//! bars, scribbles, explosions, and bot selection.

use bevy::prelude::*;
use bevy_egui::EguiContexts;
use sim::map::OverlayKind;
use sim::world::PrinterState;
use sim::TilePos;
use std::collections::{HashMap, HashSet};

use crate::editor::EditorState;
use crate::GameSim;
use crate::palette::*;
use crate::camera::*;
use crate::scene::*;

/// Fixed-tick pose targets; per-frame lerp between them.
#[derive(Component)]
pub(crate) struct Pose {
    pub(crate) prev: Vec3,
    pub(crate) curr: Vec3,
    /// Facing across the last tick boundary; a turn takes exactly one
    /// tick, slerped per frame like the position lerp.
    pub(crate) yaw_prev: Quat,
    pub(crate) yaw: Quat,
    pub(crate) grid: TilePos,
    /// Was the bot in a handler last tick (hop on entry)?
    pub(crate) was_in_handler: bool,
    /// Last fault_count seen; a rise triggers the fault hop.
    pub(crate) fault_seen: u64,
    /// Fixed ticks since the last fault.
    pub(crate) fault_age: u32,
    /// Last hp seen; a change shows the health bar for a few seconds.
    pub(crate) hp_seen: i64,
    pub(crate) hp_age: u32,
    /// Last bump_frozen seen; a rise triggers the recoil hop.
    pub(crate) freeze_seen: u32,
    pub(crate) freeze_age: u32,
}

/// World-space progress bar over anything being built: root billboards
/// toward the camera; the fill scales with progress (left-anchored).
#[derive(Component)]
pub(crate) struct BillboardBar;
#[derive(Component)]
pub(crate) struct ProgressFill;
/// The in-world ring under the inspected bot.
#[derive(Component)]
pub(crate) struct SelMarker;

/// Angry scribble cloud over a bump-frozen bot (baked SVG frames).
#[derive(Component)]
pub(crate) struct ScribbleCloud;

/// Red health fill on a bot's billboarded bar.
#[derive(Component)]
pub(crate) struct HealthFill;
/// Pale "damage ghost" behind the red fill: holds the pre-hit fraction and
/// drains toward it, so each hit reads as a shrinking chunk.
#[derive(Component)]
pub(crate) struct HealthTrail {
    pub(crate) frac: f32,
}

/// Blue "saving up cycles" fill: how close a cycle-starved bot is to
/// affording its next op. Sibling of `HealthFill` on its own bar root.
#[derive(Component)]
pub(crate) struct CycleFill;

/// Marks a bot view's carry-indicator child (slot index).
#[derive(Component)]
pub(crate) struct CarrySlot(pub(crate) u32);

/// Marks a printer view's floating print-job cube.
#[derive(Component)]
pub(crate) struct JobCube;

#[derive(Component)]
pub(crate) struct Spinner(pub(crate) f32);

#[derive(Component)]
pub(crate) struct Explosion {
    pub(crate) age: f32,
}

/// A scrapped bot being taken apart at the printer: spin, shrink, sink.
#[derive(Component)]
pub(crate) struct Disassembling {
    pub(crate) age: f32,
}

#[derive(Resource, Default)]
pub(crate) struct ViewIndex {
    pub(crate) bots: HashMap<u32, Entity>,
    pub(crate) ore: HashMap<u64, Entity>,
    pub(crate) wrecks: HashMap<u32, Entity>,
    pub(crate) black_boxes: HashMap<u64, Entity>,
    pub(crate) printers: HashMap<u64, (Entity, PrinterState)>,
    pub(crate) blueprints: HashMap<u64, Entity>,
    pub(crate) bridges: HashMap<(i32, i32), Entity>,
    pub(crate) overlays: HashMap<(i32, i32), (Entity, OverlayKind)>,
    /// Blueprint id -> its progress-bar fill entity.
    pub(crate) blueprint_fills: HashMap<u64, Entity>,
    /// Printer id -> (bar root, fill) for print-job progress.
    pub(crate) printer_fills: HashMap<u64, (Entity, Entity)>,
    /// Bot id -> (bar root, red fill, damage-ghost trail).
    pub(crate) bot_health: HashMap<u32, (Entity, Entity, Entity)>,
    /// Bot id -> (bar root, blue fill) for the cycle-saving meter.
    pub(crate) bot_cycles: HashMap<u32, (Entity, Entity)>,
    /// Bot id -> its scribble-cloud entity.
    pub(crate) bot_scribbles: HashMap<u32, Entity>,
    /// Bots currently on a recall walk: a vanish while recalling is the
    /// printer disassembling them (scrap), not a death.
    pub(crate) bot_recalling: HashSet<u32>,
    pub(crate) paint: HashMap<(i32, i32), (Entity, u8)>,
}

// ------------------------------------------------------------------- view

/// FixedUpdate, after the sim step: shift pose targets (prev <- curr) and
/// point noses along the travel direction.
pub(crate) fn update_poses(
    game: NonSend<GameSim>,
    index: Res<ViewIndex>,
    mut poses: Query<&mut Pose>,
) {
    let world = &game.0.world;
    for (id, bot) in &world.bots {
        let Some(&entity) = index.bots.get(&id.0) else { continue };
        let Ok(mut pose) = poses.get_mut(entity) else { continue };
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
        pose.yaw_prev = pose.yaw;
        // Face the tile currently being attempted (so a bumped bot stares
        // at whatever it walked into for the whole freeze), else the thing
        // being worked on (miners face their node — diagonally if that's
        // where it is), else the tile just entered.
        use sim::world::Action;
        let next_tile = match (&bot.data.action, &bot.data.recall) {
            (Some(Action::Move { path, .. }), _) if !path.is_empty() => Some(path[0]),
            (_, Some(recall)) if !recall.path.is_empty() => Some(recall.path[0]),
            (Some(Action::Mine { node, .. }), _) => world.entity_pos(*node),
            (Some(Action::Deposit { depot, .. }), _) => world.entity_pos(*depot),
            (Some(Action::Attack { target, .. }), _) => world.entity_pos(*target),
            (Some(Action::Build { blueprint }), _) => world.entity_pos(*blueprint),
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
            pose.yaw = Quat::from_rotation_y((-dx).atan2(-dz));
        }
        pose.grid = bot.data.pos;
    }
}

/// Per-frame smoothing between fixed ticks: positions lerp and noses
/// slerp between the tick-boundary targets, so a turn takes exactly one
/// tick (shortest way around) and scales with game speed like movement.
pub(crate) fn interpolate(fixed: Res<Time<Fixed>>, mut q: Query<(&Pose, &mut Transform)>) {
    let a = fixed.overstep_fraction();
    for (pose, mut transform) in &mut q {
        transform.translation = pose.prev.lerp(pose.curr, a);
        transform.rotation = pose.yaw_prev.slerp(pose.yaw, a);
    }
}

pub(crate) fn spin(time: Res<Time>, mut q: Query<(&Spinner, &mut Transform)>) {
    for (spinner, mut transform) in &mut q {
        transform.rotate_y(spinner.0 * time.delta_secs());
    }
}

pub(crate) fn animate_job_cubes(
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

pub(crate) fn animate_explosions(
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

/// The printer taking a scrapped bot apart: an accelerating spin as the
/// fasteners come out, shrinking while being drawn down into the machine.
pub(crate) fn animate_disassembly(
    time: Res<Time>,
    game: NonSend<GameSim>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Disassembling, &mut Transform)>,
) {
    let world = &game.0.world;
    for (entity, mut d, mut transform) in &mut q {
        d.age += time.delta_secs();
        let t = d.age / 1.4;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.scale = Vec3::splat(1.0 - t * t);
        // Drawn into the nearest printer (the bot was scrapped standing
        // beside it), sinking into the bed as it goes.
        let target = world
            .printers
            .values()
            .map(|p| tile_top_xyz(world, p.pos, 0.15))
            .min_by(|a, b| {
                let (da, db) = (
                    a.distance_squared(transform.translation),
                    b.distance_squared(transform.translation),
                );
                da.partial_cmp(&db).expect("distances are finite")
            });
        if let Some(target) = target {
            let a = 1.0 - (-time.delta_secs() * 1.6).exp();
            transform.translation = transform.translation.lerp(target, a);
        } else {
            transform.translation.y -= 0.3 * time.delta_secs();
        }
        transform.rotate_y((2.0 + 14.0 * t) * time.delta_secs());
    }
}

/// Ambient terrain animation: water's surface drifts on a forward 3-frame
/// cycle, grass sways on a ping-pong. All tiles of a (terrain, mask) share
/// one material, so retargeting 32 materials animates the whole map; the
/// two clocks are deliberately different so the world doesn't tick in
/// lockstep. Materials are only touched when a frame index changes.
pub(crate) fn animate_terrain(
    time: Res<Time>,
    palette: Res<Palette>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shown: Local<[usize; 8]>,
) {
    let t = time.elapsed_secs();
    // Each terrain ticks on its own period so the map never pulses in
    // lockstep. SWAY ping-pongs through the frames; GLITCH runs them out
    // of order — corruption should stutter, not breathe.
    const SWAY: [usize; 4] = [0, 1, 0, 2];
    const GLITCH: [usize; 4] = [0, 2, 0, 1];
    let water = (t / 0.55) as usize % 3;
    let grass = SWAY[(t / 0.8) as usize % 4];
    let mud = (t / 1.0) as usize % 3;
    let corruption = GLITCH[(t / 0.35) as usize % 4];
    let ore = (t / 0.7) as usize % 3;
    let crystal = SWAY[(t / 1.0) as usize % 4];
    // Snow runs forward-only: flakes fall, land, and respawn at the top —
    // a ping-pong would send them back up.
    let snow = (t / 0.5) as usize % 3;
    let vent = SWAY[(t / 0.6) as usize % 4];
    let mut retarget = |mats: &[Handle<StandardMaterial>], frame: &[Handle<Image>]| {
        for (mat, img) in mats.iter().zip(frame) {
            if let Some(mut m) = materials.get_mut(mat) {
                m.base_color_texture = Some(img.clone());
            }
        }
    };
    let sets: [(usize, &[Handle<StandardMaterial>], &[Vec<Handle<Image>>]); 7] = [
        (water, &palette.water_tex_mats, &palette.water_frames),
        (grass, &palette.grass_tex_mats, &palette.grass_frames),
        (mud, &palette.mud_tex_mats, &palette.mud_frames),
        (corruption, &palette.corruption_tex_mats, &palette.corruption_frames),
        (ore, &palette.ore_tex_mats, &palette.ore_frames),
        (crystal, &palette.crystal_tex_mats, &palette.crystal_frames),
        (snow, &palette.snow_tex_mats, &palette.snow_frames),
    ];
    for (slot, (frame, mats, frames)) in sets.into_iter().enumerate() {
        if shown[slot] != frame {
            shown[slot] = frame;
            retarget(mats, &frames[frame]);
        }
    }
    // The vent pulses base + emissive together, so the glow itself beats.
    if shown[7] != vent {
        shown[7] = vent;
        if let Some(mut m) = materials.get_mut(&palette.vent_tex_mat) {
            m.base_color_texture = Some(palette.vent_frames[vent].clone());
            m.emissive_texture = Some(palette.vent_frames[vent].clone());
        }
    }
}

/// Diff sim state into persistent view entities.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_view(
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
                    palette.printer_tex_mats[&printer.color.0.min(8)].clone(),
                    Vec3::ONE,
                ),
                // Ruined and Dormant are both dead machines — greyed, no
                // paper. Dormant (Q87) reuses the ruined look.
                PrinterState::Ruined | PrinterState::Dormant => {
                    (palette.printer_ruined_mat.clone(), Vec3::new(1.0, 0.45, 1.0))
                }
            };
            let entity = commands
                .spawn((
                    Mesh3d(palette.printer_box.clone()),
                    MeshMaterial3d(mat),
                    // Face the default camera (the atlas front is -Z).
                    Transform::from_translation(tile_top_xyz(world, printer.pos, 0.25))
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

    // Nodes: spinning gems, scaled by remaining amount (typed tints
    // land with the M4 game pass; gold gem stands in for all kinds).
    for (id, node) in &world.nodes {
        if node.amount == 0 {
            if let Some(entity) = index.ore.remove(&id.0) {
                commands.entity(entity).despawn();
            }
            continue;
        }
        // Fog of war (M7, docs/05): a node exists to the viewer only once
        // faction 0 has DISCOVERED it — the greyed snapshot then keeps it.
        let discovered =
            world.known_nodes.get(&0).is_some_and(|known| known.contains_key(id));
        if !discovered {
            if let Some(&entity) = index.ore.get(&id.0) {
                commands.entity(entity).insert(Visibility::Hidden);
            }
            continue;
        }
        let scale = Vec3::splat(0.6 + 0.8 * (node.amount as f32 / 60.0).min(1.0));
        match index.ore.get(&id.0) {
            Some(&entity) => {
                commands.entity(entity).insert(Visibility::Inherited);
                if let Ok(mut transform) = transforms.get_mut(entity) {
                    transform.scale = scale;
                }
            }
            None => {
                let entity = commands
                    .spawn((
                        Mesh3d(palette.gem.clone()),
                        MeshMaterial3d(palette.ore_mat.clone()),
                        Transform::from_translation(tile_top_xyz(world, node.pos, 0.35))
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
        if bot.data.recall.is_some() {
            index.bot_recalling.insert(id.0);
        } else {
            index.bot_recalling.remove(&id.0);
        }
        // Fog of war (M7): enemy bots render only while faction 0 SEES
        // them — heard-only contacts get a blip (fog.rs), not a picture.
        let viewer_sees = bot.data.faction == 0
            || world
                .perception
                .get(&0)
                .is_some_and(|p| p.seen.contains(&bot.data.entity));
        if let Some(&entity) = index.bots.get(&id.0) {
            commands.entity(entity).insert(if viewer_sees {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            // Carry indicators track cargo.
            if let Ok(kids) = children.get(entity) {
                for kid in kids {
                    if let Ok((slot, mut vis)) = slots.get_mut(*kid) {
                        *vis = if bot.data.cargo_total() > slot.0 * sim::resources::DECI {
                            Visibility::Visible
                        } else {
                            Visibility::Hidden
                        };
                    }
                }
            }
            continue;
        }
        let start = tile_top_xyz(world, bot.data.pos, 0.45);
        let mut bar_root = Entity::PLACEHOLDER;
        let mut health_fill = Entity::PLACEHOLDER;
        let mut health_trail = Entity::PLACEHOLDER;
        let mut cycle_root = Entity::PLACEHOLDER;
        let mut cycle_fill = Entity::PLACEHOLDER;
        let entity = commands
            .spawn((
                Mesh3d(palette.bot_cube.clone()),
                MeshMaterial3d(palette.bot_tex_mats[&bot.data.color.0.min(8)].clone()),
                Transform::from_translation(start),
                Pose {
                    prev: start,
                    curr: start,
                    yaw_prev: Quat::IDENTITY,
                    yaw: Quat::IDENTITY,
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
                // Camera-lens nose on the -Z face: barrel half-sunk into
                // the face (its drawn flange rings it), team-accent glass
                // at the tip. Cylinders are Y-up; tip them to point along
                // Z. Geometry constants live in palette.rs, where the
                // facing_lenses_never_clip test guards the protrusion
                // budget (facing bots on adjacent tiles must not touch).
                let tip = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                parent.spawn((
                    Mesh3d(palette.lens_barrel.clone()),
                    MeshMaterial3d(palette.lens_barrel_mat.clone()),
                    Transform::from_xyz(0.0, LENS_Y, LENS_BARREL_Z).with_rotation(tip),
                ));
                parent.spawn((
                    Mesh3d(palette.lens_glass.clone()),
                    MeshMaterial3d(palette.lens_glass_mats[&bot.data.color.0.min(8)].clone()),
                    Transform::from_xyz(0.0, LENS_Y, LENS_GLASS_Z).with_rotation(tip),
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
                // Cycle bar: sits just under the health bar, deliberately
                // slimmer — it is a "what is this bot doing" tell, not a
                // survival stat, and should never out-shout the hp bar.
                cycle_root = parent
                    .spawn((
                        BillboardBar,
                        Transform::from_xyz(0.0, 1.04, 0.0),
                        Visibility::Hidden,
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            Mesh3d(palette.bar_mesh.clone()),
                            MeshMaterial3d(palette.bar_bg_mat.clone()),
                            Transform::default().with_scale(Vec3::new(0.7, 0.42, 1.0)),
                        ));
                        cycle_fill = bar
                            .spawn((
                                CycleFill,
                                Mesh3d(palette.bar_mesh.clone()),
                                MeshMaterial3d(
                                    palette.bar_cycle_grad.first().expect("bins").clone(),
                                ),
                                Transform::from_xyz(0.0, 0.0, 0.011)
                                    .with_scale(Vec3::new(0.02, 0.30, 1.0)),
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
        index.bot_cycles.insert(id.0, (cycle_root, cycle_fill));
        let scribble = commands
            .spawn((
                ScribbleCloud,
                BillboardBar, // camera-facing, parent-rotation compensated
                bevy::light::NotShadowCaster, // a thought casts no shadow
                Mesh3d(palette.scribble_quad.clone()),
                MeshMaterial3d(palette.scribble_mats["angry"][0].clone()),
                Transform::from_xyz(0.0, 1.75, 0.0),
                Visibility::Hidden,
            ))
            .id();
        commands.entity(entity).add_child(scribble);
        index.bot_scribbles.insert(id.0, scribble);
    }
    let ViewIndex { bots, bot_recalling, .. } = &mut *index;
    bots.retain(|id, entity| {
        if seen.contains(id) {
            true
        } else {
            if bot_recalling.contains(id) {
                // Scrapped at the printer: play the take-apart instead of
                // blinking out. Pose goes too, or interpolate would keep
                // re-planting the transform every frame.
                commands
                    .entity(*entity)
                    .remove::<Pose>()
                    .insert(Disassembling { age: 0.0 });
            } else {
                commands.entity(*entity).despawn();
            }
            false
        }
    });
    index.bot_health.retain(|id, _| seen.contains(id));
    index.bot_cycles.retain(|id, _| seen.contains(id));
    index.bot_scribbles.retain(|id, _| seen.contains(id));
    index.bot_recalling.retain(|id| seen.contains(id));

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
                Transform::from_translation(tile_top_xyz(world, bp.pos, 0.05)),
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
    // arrows are an overlay layer now — see below.) Demolish (M8) can
    // return a bridge to water, so planks are tracked and despawned.
    index.bridges.retain(|&(x, y), entity| {
        if world.grid.get(TilePos::new(x, y)) == Some(sim::TileKind::Bridge) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
    for y in 0..world.grid.height {
        for x in 0..world.grid.width {
            let pos = TilePos::new(x, y);
            if world.grid.get(pos) != Some(sim::TileKind::Bridge) {
                continue;
            }
            if index.bridges.contains_key(&(x, y)) {
                continue;
            }
            let entity = commands
                .spawn((
                    Mesh3d(palette.tex_slab.clone()),
                    MeshMaterial3d(palette.bridge_tex_mat.clone()),
                    Transform::from_translation(tile_xyz(world, pos, 0.0)),
                ))
                .id();
            index.bridges.insert((x, y), entity);
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
                Transform::from_translation(tile_top_xyz(world, *pos, 0.08))
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
                Transform::from_translation(tile_top_xyz(world, *pos, 0.02))
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

    // Wrecks: charred dead-bot slabs. M10 made wreck removal routine
    // (salvage/analyze/hijack/rescue/attack/blast), so stale slabs are
    // despawned and a re-wrecked bot re-renders at its new tile.
    for (id, wreck) in &world.wrecks {
        if let std::collections::hash_map::Entry::Vacant(e) = index.wrecks.entry(id.0) {
            let entity = commands
                .spawn((
                    Mesh3d(palette.pad_slab.clone()),
                    MeshMaterial3d(palette.wreck_tex_mat.clone()),
                    Transform::from_translation(tile_top_xyz(world, wreck.pos(), 0.07)),
                ))
                .id();
            e.insert(entity);
        }
    }
    index.wrecks.retain(|id, entity| {
        if world.wrecks.contains_key(&sim::BotId(*id)) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    // Black boxes: an explosion flash on first sight, then the small dark
    // cube remains until the box is recovered (keyed by entity id —
    // recover_black_box removes mid-Vec, so a spawn cursor goes stale).
    for bb in &world.black_boxes {
        if let std::collections::hash_map::Entry::Vacant(e) = index.black_boxes.entry(bb.entity.0)
        {
            commands.spawn((
                Explosion { age: 0.0 },
                Mesh3d(palette.explode_cube.clone()),
                MeshMaterial3d(palette.explode_mat.clone()),
                Transform::from_translation(tile_top_xyz(world, bb.pos, 0.5)),
            ));
            let cube = commands
                .spawn((
                    Mesh3d(palette.nose_cube.clone()),
                    MeshMaterial3d(palette.black_mat.clone()),
                    Transform::from_translation(tile_top_xyz(world, bb.pos, 0.12)),
                ))
                .id();
            e.insert(cube);
        }
    }
    index.black_boxes.retain(|id, entity| {
        if world.black_boxes.iter().any(|bb| bb.entity.0 == *id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
}

/// Grow each progress fill (left-anchored): blueprints always show their
/// bar; printers show one only while a print job runs.
pub(crate) fn update_progress_bars(
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
pub(crate) fn update_health_bars(
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

/// Cycle bars: visible only while a bot is cycle-starved, blue fill =
/// banked budget as a fraction of the op it is saving up for. Dark blue
/// just after spending, bright cyan the tick before it executes.
///
/// Deliberately NOT shown for a bot that is merely idle-blocked (waiting
/// on an action or a channel): that bot banks nothing, so a creeping bar
/// would promise an execution that isn't coming. The VM reports a stall
/// only when it actually stopped short on price.
pub(crate) fn update_cycle_bars(
    game: NonSend<GameSim>,
    index: Res<ViewIndex>,
    palette: Res<Palette>,
    mut fills: Query<(&mut Transform, &mut MeshMaterial3d<StandardMaterial>), With<CycleFill>>,
    mut roots: Query<&mut Visibility, With<BillboardBar>>,
) {
    for (id, bot) in &game.0.world.bots {
        let Some(&(root, fill)) = index.bot_cycles.get(&id.0) else { continue };
        let Ok(mut visibility) = roots.get_mut(root) else { continue };
        // Engine interrupt contexts (boot, recall, pad-sit, bump stun) and
        // death suspend the program entirely — the sim skips the VM, so any
        // stall left on it is stale. Shared predicate with phase 2 so the bar
        // never advertises an op the sim won't run.
        let saving = if bot.data.vm_suspended() {
            None
        } else {
            bot.vm.as_ref().filter(|vm| !vm.is_blocked()).and_then(|vm| {
                // Debt (a forced charge drove the budget negative) reads as
                // an empty bar, not a negative one.
                vm.stall_cost().map(|cost| (vm.budget().max(0), cost))
            })
        };
        let Some((budget, cost)) = saving else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;
        let p = if cost > 0 { (budget as f32 / cost as f32).clamp(0.0, 1.0) } else { 0.0 };
        if let Ok((mut transform, mut material)) = fills.get_mut(fill) {
            // Left-anchored within the 0.9-wide mesh scaled by 0.7, matching
            // the health bar's geometry one row up.
            let frac = p.clamp(0.02, 1.0);
            transform.scale = Vec3::new(frac * 0.7, 0.30, 1.0);
            transform.translation.x = -(0.9 * 0.7 * (1.0 - frac)) / 2.0;
            let bins = &palette.bar_cycle_grad;
            let bin = ((p * (bins.len() - 1) as f32).round() as usize).min(bins.len() - 1);
            if material.0 != bins[bin] {
                material.0 = bins[bin].clone();
            }
        }
    }
}

/// Frustration clouds: visible while bump-frozen, cycling scribble frames
/// with a little scale pulse — thinking, angrily.
pub(crate) fn update_scribbles(
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
        // The cloud reads the VM's RUN STATE (docs/01's per-signal palette;
        // docs/07's enum): angry squiggle for bump, dizzy stars for bumped,
        // ?! for error, starburst for hurt, power-on for boot, home arrow
        // for the recall walk — and the distinct black skull for ABORT
        // (a bot mid-forced-sequence / freshly disabled), never confused
        // with an ordinary handler tint.
        use pyrite::{ast::SignalKind, RunState};
        let mood = if bot.aborted() {
            Some("death") // the skull frames — abort's cloud
        } else if bot.data.bump_frozen > 0 {
            Some("angry")
        } else {
            match bot.vm.as_ref().map(|vm| vm.run_state()) {
                Some(RunState::Recall) => Some("recall"),
                Some(RunState::Boot) => Some("boot"),
                Some(RunState::Faulted) => Some("error"),
                Some(RunState::Template { signal, .. }) => Some(match signal {
                    SignalKind::Hurt => "hurt",
                    SignalKind::Bumped => "bumped",
                    SignalKind::Boot => "boot",
                    SignalKind::Error => "error",
                    SignalKind::Bump => "angry",
                }),
                Some(RunState::Disabled) => Some("death"),
                _ => None,
            }
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
pub(crate) fn billboard_bars(
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
pub(crate) fn select_bot(
    mut contexts: EguiContexts,
    mut editor: ResMut<EditorState>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    game: NonSend<GameSim>,
) {
    let typing = contexts.ctx_mut().is_ok_and(|ctx| ctx.egui_wants_keyboard_input());
    if !typing && keys.just_pressed(KeyCode::Escape) && editor.selected_build.is_none() {
        editor.selected_bot = None;
    }
    if editor.selected_build.is_some() {
        return; // armed tools own the mouse
    }
    let over_ui = contexts.ctx_mut().is_ok_and(|ctx| ctx.egui_wants_pointer_input());
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
        let Some(tile) = cursor_tile(&windows, &cams, world)
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
pub(crate) fn update_sel_marker(
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
