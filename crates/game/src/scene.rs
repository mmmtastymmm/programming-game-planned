//! Colony construction and initial scene setup.

use bevy::prelude::*;
use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::{BlueprintKind, Color as BotColor};
use sim::{TileKind, TilePos};
use std::collections::HashMap;

use crate::GameSim;
use crate::palette::*;
use crate::camera::*;
use crate::view::*;
use crate::tools::*;

pub(crate) fn build_colony() -> Sim {
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
    // Working but idle (dial 0): exists so the showcase scrap recall below
    // has a nearby, boot-free home to walk to.
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 9),
        faction: 0,
        color: 1,
        ruined: false,
        desired_max: 0,
    });
    spec.starting_ore = 30;

    // Signal-cloud showcase (docs/01 table): a sealed south strip where one
    // bot per state holds its cloud for inspection. Water walls keep colony
    // traffic out — a stray bump on a parked handler would double-handle it.
    for x in 4..=15 {
        spec.water.push(TilePos::new(x, 10));
        spec.water.push(TilePos::new(x, 12));
    }
    spec.water.push(TilePos::new(3, 11));
    spec.water.push(TilePos::new(9, 11));

    let mut game = Sim::new(&spec);
    // Slow boots so the power-on cloud is watchable; capacity one below the
    // steady-state population so exactly one scrap recall fires (the purple
    // walk) and the chain stops there.
    game.tuning.boot_ticks = 40;
    game.tuning.capacity = 9;
    game.apply(&Command::DeployProgram {
        faction: 0,
        color: BotColor::GREEN,
        source: crate::editor::DEFAULT_PROGRAM.into(),
    })
    .expect("miner program parses");
    // Bridge blueprints across the wall: the default program services
    // blueprints first, so the opening minutes are the colony building its
    // own crossings — progress bars and all — before mining east. (No
    // blueprint at y=11: its approach tile sits inside the showcase strip.)
    for y in [2, 5, 8] {
        game.apply(&Command::PlaceBlueprint {
            pos: TilePos::new(16, y),
            kind: BlueprintKind::Bridge,
        })
        .expect("blueprint placement");
    }

    // The showcase cast, one signal cloud each. `on signal(s): wait(...)`
    // parks a bot inside whichever handler fires, holding its cloud.
    // Spawn order matters: the scrap recall picks the lowest (XP, id) bot,
    // and ties on the attackers' closest(enemy) break by id.
    const IDLE: &str = "wait(100000)\n";
    const PARK: &str = "on signal(s):\n    wait(100000)\n\nwait(100000)\n";
    let spawn = |game: &mut Sim, pos, faction, color, hp, source: &str| {
        game.apply(&Command::SpawnBot {
            pos,
            source: source.into(),
            cpu: 4,
            cargo_cap: 1,
            faction,
            hp,
            color,
        })
        .expect("showcase bot parses");
    };
    let blue = BotColor(2);
    // recall (purple): first-spawned = lowest id = the scrap victim once
    // the 4th print pushes faction population past capacity. Placed nearer
    // the idle south printer than the busy north one, so the walk home
    // avoids the print landing zone (bumping a booting bot would abort it).
    spawn(&mut game, TilePos::new(15, 13), 0, blue, 100, IDLE);
    // bump (angry): chases the bait but the corridor is blocked — one ram,
    // then parked in its bump handler.
    spawn(
        &mut game,
        TilePos::new(4, 11),
        0,
        blue,
        100,
        "on signal(s):\n    wait(100000)\n\nmove_to(closest(enemy).expect())\n",
    );
    // bumped (dizzy): the blocker, parked in its bumped handler.
    spawn(&mut game, TilePos::new(6, 11), 0, blue, 100, PARK);
    // The bait: an enemy idler behind the blocker, out of reach.
    spawn(&mut game, TilePos::new(8, 11), 1, BotColor::RED, 100, IDLE);
    // hurt (amber): hp tuned so one 10-damage hit crosses the 50% line.
    spawn(&mut game, TilePos::new(10, 11), 0, blue, 18, PARK);
    // Its attacker: exactly one swing, then sleep.
    spawn(
        &mut game,
        TilePos::new(11, 11),
        1,
        BotColor::RED,
        100,
        "wait(50)\nattack(closest(enemy).expect())\nwait(100000)\n",
    );
    // error (red ?!): mine() with no ore in range faults into the handler.
    spawn(
        &mut game,
        TilePos::new(13, 11),
        0,
        blue,
        100,
        "on signal(s):\n    wait(100000)\n\nmine()\n",
    );
    // death (skull, then the wreck race): one hit from its neighbor kills.
    spawn(&mut game, TilePos::new(14, 11), 0, blue, 10, IDLE);
    // The executioner: waits half a minute so you can watch it happen.
    spawn(
        &mut game,
        TilePos::new(15, 11),
        1,
        BotColor::RED,
        100,
        "wait(300)\nattack(closest(enemy).expect())\nwait(100000)\n",
    );
    game
}

/// Tile -> world: XZ plane, one unit per tile, map centered at the origin.
pub(crate) fn tile_xyz(world: &sim::World, pos: TilePos, y: f32) -> Vec3 {
    Vec3::new(
        pos.x as f32 - world.grid.width as f32 / 2.0,
        y,
        pos.y as f32 - world.grid.height as f32 / 2.0,
    )
}

pub(crate) fn setup_scene(
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
    let load_frames = |prefix: &str| -> Vec<Vec<Handle<Image>>> {
        (0..3)
            .map(|f| {
                (0..16)
                    .map(|mask| asset_server.load(format!("textures/{prefix}_{mask}_f{f}.png")))
                    .collect()
            })
            .collect()
    };
    let grass_frames = load_frames("tile_grass");
    let water_frames = load_frames("tile_water");
    let mut mats_from = |frame0: &[Handle<Image>], roughness: f32| -> Vec<_> {
        frame0.iter().map(|img| tile_tex_mat(&mut materials, img.clone(), roughness)).collect()
    };
    let grass_tex_mats = mats_from(&grass_frames[0], 0.95);
    let water_tex_mats = mats_from(&water_frames[0], 0.35);
    let mountain_tex_mats = (0..16)
        .map(|mask| {
            tile_tex_mat(
                &mut materials,
                asset_server.load(format!("textures/mountain_atlas_{mask}.png")),
                0.95,
            )
        })
        .collect();
    let mut overlay_mats = |prefix: &str| -> Vec<_> {
        (0..16)
            .map(|mask| {
                materials.add(StandardMaterial {
                    base_color_texture: Some(
                        asset_server.load(format!("textures/{prefix}_{mask}.png")),
                    ),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 0.95,
                    ..default()
                })
            })
            .collect()
    };
    let scree_mats = overlay_mats("tile_scree");
    let water_corner_mats = overlay_mats("tile_water_corner");
    let grass_corner_mats = overlay_mats("tile_grass_corner");
    let scree_corner_mats = overlay_mats("tile_scree_corner");
    let mountain_corner_mats = overlay_mats("tile_mountain_corner");
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
        grass_tex_mats,
        water_tex_mats,
        mountain_tex_mats,
        grass_frames,
        water_frames,
        scree_mats,
        water_corner_mats,
        grass_corner_mats,
        scree_corner_mats,
        mountain_corner_mats,
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
        // Sized so the icon inside the thought bubble keeps its old
        // on-screen size (icons render at ~0.58 of the texture now).
        scribble_quad: meshes.add(Rectangle::new(1.05, 0.85)),
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
                ("bumped", "scribble_bumped"),
                ("boot", "scribble_boot"),
                ("recall", "scribble_recall"),
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
    //
    // Grass, water, and mountain autotile: a NESW same-neighbor bitmask
    // (bit 0 = N … bit 3 = W) picks 1 of 16 baked variants with edge art
    // on the "different" sides. Off-map counts as same, so terrain runs
    // off the board clean. Every predicate is stable under the one runtime
    // grid change (Water -> Bridge), so tiles never need re-syncing.
    for y in 0..world.grid.height {
        for x in 0..world.grid.width {
            let pos = TilePos::new(x, y);
            let kind = world.grid.get(pos).expect("in bounds");
            let mask_of = |same: fn(TileKind) -> bool| -> usize {
                let mut mask = 0usize;
                for (bit, (dx, dy)) in
                    [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter().enumerate()
                {
                    if world.grid.get(TilePos::new(x + dx, y + dy)).is_none_or(same) {
                        mask |= 1 << bit;
                    }
                }
                mask
            };
            let (mesh, mat, y_off) = match kind {
                // Sand fringes where the meadow meets the river — Bridge
                // counts as water (same beach before and after the planks
                // land, keeping the no-resync invariant). Mountains count
                // as grass; the block would hide a fringe anyway.
                TileKind::Plains => {
                    let mask =
                        mask_of(|t| !matches!(t, TileKind::Water | TileKind::Bridge));
                    (palette.tex_slab.clone(), palette.grass_tex_mats[mask].clone(), 0.0)
                }
                // Mountains rise a full block: crossing costs double, and
                // the silhouette should say so. The summit grows a cliff
                // rim wherever the range ends.
                TileKind::Rubble => {
                    let mask = mask_of(|t| matches!(t, TileKind::Rubble));
                    (
                        palette.mountain_block.clone(),
                        palette.mountain_tex_mats[mask].clone(),
                        MOUNTAIN_TOP - 0.10,
                    )
                }
                // Banks on the sides that border land. Bridges count as
                // water: the river visibly flows under the planks.
                TileKind::Water => {
                    let mask =
                        mask_of(|t| matches!(t, TileKind::Water | TileKind::Bridge));
                    (palette.tex_slab.clone(), palette.water_tex_mats[mask].clone(), -0.05)
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
            // Inner corners: both flanking neighbors match but the
            // diagonal doesn't — a nub caps that corner (bit unset = nub;
            // 15 = no nubs). Same bit walk as the edge masks: NW/NE/SE/SW.
            let corner_mask_of = |same: fn(TileKind) -> bool| -> usize {
                const CORNERS: [((i32, i32), (i32, i32), (i32, i32)); 4] = [
                    ((0, -1), (-1, 0), (-1, -1)), // NW: flanks N+W
                    ((0, -1), (1, 0), (1, -1)),   // NE
                    ((0, 1), (1, 0), (1, 1)),     // SE
                    ((0, 1), (-1, 0), (-1, 1)),   // SW
                ];
                let is_same = |(dx, dy): (i32, i32)| {
                    world.grid.get(TilePos::new(x + dx, y + dy)).is_none_or(same)
                };
                let mut mask = 15usize;
                for (bit, (a, b, diag)) in CORNERS.into_iter().enumerate() {
                    if is_same(a) && is_same(b) && !is_same(diag) {
                        mask &= !(1 << bit);
                    }
                }
                mask
            };
            // Overlay quads float just above the tile surface; stacked
            // overlays on one tile get distinct epsilons to avoid z-fights.
            let overlay = |commands: &mut Commands,
                               mats: &Vec<Handle<StandardMaterial>>,
                               mask: usize,
                               eps: f32| {
                if mask != 15 {
                    let top = terrain_top(world, pos);
                    commands.spawn((
                        Mesh3d(palette.tex_slab.clone()),
                        MeshMaterial3d(mats[mask].clone()),
                        Transform::from_translation(tile_xyz(world, pos, top - 0.05 + eps)),
                    ));
                }
            };
            match kind {
                TileKind::Plains => {
                    // Scree at a mountain's base: contact shadow + stones
                    // on the looming sides, plus corner clusters.
                    let not_mountain = |t: TileKind| !matches!(t, TileKind::Rubble);
                    overlay(&mut commands, &palette.scree_mats, mask_of(not_mountain), 0.012);
                    overlay(
                        &mut commands,
                        &palette.scree_corner_mats,
                        corner_mask_of(not_mountain),
                        0.015,
                    );
                    overlay(
                        &mut commands,
                        &palette.grass_corner_mats,
                        corner_mask_of(|t| !matches!(t, TileKind::Water | TileKind::Bridge)),
                        0.0135,
                    );
                }
                TileKind::Water => overlay(
                    &mut commands,
                    &palette.water_corner_mats,
                    corner_mask_of(|t| matches!(t, TileKind::Water | TileKind::Bridge)),
                    0.012,
                ),
                TileKind::Rubble => overlay(
                    &mut commands,
                    &palette.mountain_corner_mats,
                    corner_mask_of(|t| matches!(t, TileKind::Rubble)),
                    0.012,
                ),
                TileKind::Bridge => {}
            }
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
