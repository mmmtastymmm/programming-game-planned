//! Minimal viewer: watch the deterministic sim run a living colony.
//!
//! The sim steps on `FixedUpdate` at 10 Hz (docs/07 tick rate); rendering
//! free-runs. This is a *window into* the sim, never a mutator — all game
//! state lives in `sim::Sim`, held as a NonSend resource (the VM holds
//! `Rc`s, and the sim must stay on one thread anyway).
//!
//! Run: `cargo run -p game`

use bevy::prelude::*;
use sim::map::{MapSpec, PrinterSpec};
use sim::sim::{Command, Sim};
use sim::world::{Color as BotColor, PrinterState};
use sim::{TileKind, TilePos};

const TILE: f32 = 32.0;

/// The doc's Tier-0 starter program, verbatim.
const MINER: &str = "\
move_to(nearest_ore())
mine()
move_to(nearest_depot())
deposit()
";

/// Marker for entities redrawn every frame.
#[derive(Component)]
struct Dyn;

struct GameSim(Sim);

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
    // Resources & drop-off.
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
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, step_sim)
        .add_systems(Update, redraw)
        .run();
}

fn world_xy(pos: TilePos, spec_h: i32) -> Vec3 {
    // Flip y so the map reads top-down like the tile grid.
    Vec3::new(pos.x as f32 * TILE, (spec_h - pos.y) as f32 * TILE, 0.0)
}

fn setup(world: &mut World) {
    let game = build_colony();
    let grid_w = game.world.grid.width;
    let grid_h = game.world.grid.height;

    // Static terrain sprites, spawned once.
    let mut tiles = Vec::new();
    for y in 0..grid_h {
        for x in 0..grid_w {
            let pos = TilePos::new(x, y);
            let kind = game.world.grid.get(pos).expect("in bounds");
            let color = match kind {
                TileKind::Plains => Color::srgb(0.16, 0.14, 0.12),
                TileKind::Rubble => Color::srgb(0.32, 0.28, 0.22),
                TileKind::Water => Color::srgb(0.10, 0.16, 0.32),
            };
            tiles.push((
                Sprite { color, custom_size: Some(Vec2::splat(TILE - 1.0)), ..default() },
                Transform::from_translation(world_xy(pos, grid_h)),
            ));
        }
    }
    world.spawn_batch(tiles);

    // Camera centered on the map.
    let center = Vec3::new(grid_w as f32 * TILE / 2.0, grid_h as f32 * TILE / 2.0, 999.0);
    world.spawn((Camera2d, Transform::from_translation(center)));

    world.insert_non_send_resource(GameSim(game));
}

fn step_sim(mut game: NonSendMut<GameSim>) {
    game.0.step();

    // Story beat: once the colony has banked enough ore, fix the ruined Red
    // printer and give it a dial (stands in for the player's first
    // milestone until there's UI).
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

fn redraw(
    mut commands: Commands,
    game: NonSend<GameSim>,
    old: Query<Entity, With<Dyn>>,
    mut windows: Query<&mut Window>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let world = &game.0.world;
    let h = world.grid.height;

    let bot_tint = |c: BotColor| match c {
        BotColor::GREEN => Color::srgb(0.3, 0.9, 0.3),
        BotColor::RED => Color::srgb(0.95, 0.3, 0.25),
        BotColor(n) => Color::srgb(0.4 + (0.13 * n as f32) % 0.6, 0.5, 0.9),
    };

    // Printers (big squares; ruined ones dimmed).
    for printer in world.printers.values() {
        let base = bot_tint(printer.color);
        let color = match printer.state {
            PrinterState::Working => base,
            PrinterState::Ruined => base.with_alpha(0.25),
        };
        commands.spawn((
            Dyn,
            Sprite { color, custom_size: Some(Vec2::splat(TILE * 0.9)), ..default() },
            Transform::from_translation(world_xy(printer.pos, h) + Vec3::Z),
        ));
    }
    // Depots.
    for depot in world.depots.values() {
        commands.spawn((
            Dyn,
            Sprite {
                color: Color::srgb(0.25, 0.55, 0.95),
                custom_size: Some(Vec2::splat(TILE * 0.8)),
                ..default()
            },
            Transform::from_translation(world_xy(depot.pos, h) + Vec3::Z),
        ));
    }
    // Ore nodes, sized by remaining amount.
    for node in world.ore_nodes.values() {
        if node.amount == 0 {
            continue;
        }
        let size = TILE * (0.3 + 0.5 * (node.amount as f32 / 60.0).min(1.0));
        commands.spawn((
            Dyn,
            Sprite {
                color: Color::srgb(0.95, 0.8, 0.2),
                custom_size: Some(Vec2::splat(size)),
                ..default()
            },
            Transform::from_translation(world_xy(node.pos, h) + Vec3::Z),
        ));
    }
    // Wrecks.
    for wreck in world.wrecks.values() {
        commands.spawn((
            Dyn,
            Sprite {
                color: Color::srgb(0.35, 0.35, 0.35),
                custom_size: Some(Vec2::splat(TILE * 0.5)),
                ..default()
            },
            Transform::from_translation(world_xy(wreck.pos, h) + Vec3::Z * 2.0),
        ));
    }
    // Black boxes.
    for bb in &world.black_boxes {
        commands.spawn((
            Dyn,
            Sprite {
                color: Color::srgb(0.05, 0.05, 0.05),
                custom_size: Some(Vec2::splat(TILE * 0.25)),
                ..default()
            },
            Transform::from_translation(world_xy(bb.pos, h) + Vec3::Z * 2.0),
        ));
    }
    // Bots on top: tinted by their color, dimmed while booting, shrunk
    // while recalling.
    for bot in world.bots.values() {
        let mut color = bot_tint(bot.data.color);
        if bot.data.booting.is_some() {
            color = color.with_alpha(0.4);
        }
        let size = if bot.data.recall.is_some() { TILE * 0.4 } else { TILE * 0.6 };
        commands.spawn((
            Dyn,
            Sprite { color, custom_size: Some(Vec2::splat(size)), ..default() },
            Transform::from_translation(world_xy(bot.data.pos, h) + Vec3::Z * 3.0),
        ));
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
