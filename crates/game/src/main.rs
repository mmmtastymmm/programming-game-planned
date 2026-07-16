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
use bevy_egui::{EguiContexts, EguiPlugin};
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
                resolution: (1280.0, 800.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin { enable_multipass_for_primary_context: false })
        .insert_resource(ClearColor(palette::CLEAR))
        .insert_resource(Time::<Fixed>::from_hz(10.0))
        .insert_resource(view::ViewIndex::default())
        .insert_resource(EditorState::default())
        .insert_resource(camera::LmbGesture::default())
        .insert_resource(fog::FogState::default())
        .init_resource::<fog::FogAssets>()
        .add_systems(Startup, (setup_sim, scene::setup_scene, fog::setup_fog).chain())
        .add_systems(FixedUpdate, (step_sim, view::update_poses).chain())
        .add_systems(
            Update,
            (
                editor::editor_ui,
                hud::inspector_ui,
                time_controls,
                camera::orbit_camera,
                tools::place_blueprint,
                view::select_bot,
                view::update_sel_marker,
                tools::build_preview,
                view::update_progress_bars,
                view::update_health_bars,
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
        .run();
}

fn setup_sim(world: &mut World) {
    world.insert_non_send_resource(GameSim(scene::build_colony()));
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

