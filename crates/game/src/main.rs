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

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use sim::sim::Sim;

mod camera;
mod editor;
mod fog;
mod hud;
mod palette;
mod scene;
mod tools;
mod view;

use editor::EditorState;


struct GameSim(Sim);

// ------------------------------------------------------------- components

// -------------------------------------------------------------- resources

// ------------------------------------------------------------------ world

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "programming game".into(),
                // 0.17: WindowResolution is physical pixels (u32), not f32.
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .insert_resource(ClearColor(palette::CLEAR))
        .insert_resource(Time::<Fixed>::from_hz(10.0))
        .insert_resource(view::ViewIndex::default())
        .insert_resource(EditorState::default())
        .insert_resource(camera::LmbGesture::default())
        .insert_resource(fog::FogState::default())
        .init_resource::<fog::FogAssets>()
        .add_systems(Startup, (setup_sim, scene::setup_scene, fog::setup_fog).chain())
        .add_systems(FixedUpdate, (step_sim, view::update_poses).chain())
        // egui multi-pass mode (the bevy_egui default since 0.34) requires
        // every system that DRAWS ui to live in this schedule; systems that
        // merely ask the context whether it wants input stay in Update.
        .add_systems(EguiPrimaryContextPass, ui_root)
        .add_systems(
            Update,
            (
                time_controls,
                camera::orbit_camera,
                tools::place_blueprint,
                view::select_bot,
                view::update_sel_marker,
                tools::build_preview,
                view::update_progress_bars,
                view::update_health_bars,
                view::update_cycle_bars,
                view::update_scribbles,
                view::interpolate,
                view::billboard_bars,
                view::sync_view,
                view::spin,
                view::animate_job_cubes,
                view::animate_explosions,
                view::animate_disassembly,
                view::animate_terrain,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (scene::resync_terrain, fog::recompute_fog, fog::apply_fog, fog::pulse_blips)
                .chain()
                .after(view::animate_terrain),
        )
        .add_systems(Update, screenshot_and_exit)
        .run();
}

/// Dev tool: if `SCREENSHOT_PATH` is set, capture the primary window after the
/// scene settles and exit — a headless way to eyeball the render (the editor
/// panels and fog also step aside; see `ui_root` / `fog::apply_fog`). No-op on
/// normal runs.
fn screenshot_and_exit(
    mut commands: Commands,
    mut count: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(path) = std::env::var("SCREENSHOT_PATH") else { return };
    *count += 1;
    if *count == 60 {
        use bevy::render::view::screenshot::{save_to_disk, Screenshot};
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if *count >= 90 {
        exit.write(AppExit::Success);
    }
}

/// The app's single egui entry point.
///
/// egui 0.35 made panels claim space from an enclosing `Ui` rather than from
/// the context, so all panels must descend from ONE root or each would lay
/// out against the full viewport and overlap. The editor's panels are added
/// first (outermost, as before); the inspector's right panel nests inside
/// what they leave — the order here IS the layout.
fn ui_root(
    mut contexts: EguiContexts,
    mut game: NonSendMut<GameSim>,
    mut editor: ResMut<EditorState>,
) {
    // Dev tool: hide the panels for a clean SCREENSHOT_PATH capture.
    if std::env::var("SCREENSHOT_PATH").is_ok() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let ctx = ctx.clone();
    let mut root = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    editor::editor_ui(&mut root, &mut game, &mut editor);
    hud::inspector_ui(&mut root, &mut game, &mut editor);
}

fn setup_sim(world: &mut World) {
    // `MAPGEN_SEED=<n>` (optionally `MAPGEN_PLAYERS=<n>`, default 1) launches
    // a procedurally generated colony (M14, docs/05 Map Generation); with no
    // seed set, the hand-authored showcase demo runs as before. A malformed
    // value is reported loudly rather than silently swallowed — otherwise the
    // operator tests the wrong map without knowing.
    let sim = match std::env::var("MAPGEN_SEED") {
        Ok(raw) => match raw.parse::<u64>() {
            Ok(seed) => {
                let players = match std::env::var("MAPGEN_PLAYERS") {
                    Ok(p) => match p.parse::<u32>() {
                        Ok(n) => n,
                        Err(_) => {
                            warn!("mapgen: MAPGEN_PLAYERS='{p}' is not a number — using 1");
                            1
                        }
                    },
                    Err(_) => 1,
                };
                info!("mapgen: generating colony (seed {seed}, {players} players)");
                scene::build_generated_colony(seed, players)
            }
            Err(_) => {
                warn!("mapgen: MAPGEN_SEED='{raw}' is not a u64 — falling back to the showcase demo");
                scene::build_colony()
            }
        },
        Err(_) => scene::build_colony(),
    };
    world.insert_non_send(GameSim(sim));
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
    let typing = contexts.ctx_mut().is_ok_and(|ctx| ctx.egui_wants_keyboard_input());
    if !typing && keys.just_pressed(KeyCode::Space) {
        editor.paused = !editor.paused;
    }
    fixed.set_timestep_hz((10.0 * editor.speed as f64).max(0.01));
}

