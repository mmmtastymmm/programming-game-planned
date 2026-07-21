//! Orbit camera, pan/zoom gestures, and cursor-to-tile picking.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use sim::TilePos;

use crate::editor::EditorState;
use crate::tools::*;

#[derive(Component)]
pub(crate) struct OrbitCam {
    pub(crate) focus: Vec3,
    pub(crate) distance: f32,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

#[derive(Resource, Default)]
pub(crate) struct LmbGesture {
    /// Accumulated cursor travel (px) since LMB went down over the world;
    /// None while released or when the press began over the UI.
    pub(crate) travel: Option<f32>,
    /// The press outgrew the dead zone and owns the rest of the drag.
    pub(crate) panning: bool,
    /// Set for exactly the frame LMB was released inside the dead zone —
    /// the armed tool's "click" (consumed by place_blueprint).
    pub(crate) clicked: bool,
}

/// Cursor travel (px) that separates a click from a pan.
pub(crate) const LMB_DRAG_THRESHOLD: f32 = 6.0;

// ------------------------------------------------------------------ camera

pub(crate) fn orbit_transform(cam: &OrbitCam) -> Transform {
    let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
    Transform::from_translation(cam.focus + rot * Vec3::new(0.0, 0.0, cam.distance))
        .looking_at(cam.focus, Vec3::Y)
}

pub(crate) fn orbit_camera(
    mut contexts: EguiContexts,
    editor: Res<EditorState>,
    mut gesture: ResMut<LmbGesture>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut last_cursor: Local<Option<Vec2>>,
    // 0.17 renamed buffered events to "messages"; EventReader is a
    // deprecated alias for this.
    mut wheel: MessageReader<MouseWheel>,
    mut cams: Query<(&mut OrbitCam, &mut Transform)>,
) {
    // Tolerate a missing context (gone during shutdown / not yet there on
    // the first frame) — never panic for a camera nicety.
    let over_ui = contexts.ctx_mut().is_ok_and(|ctx| ctx.egui_wants_pointer_input());
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

/// Cursor ray onto the ground plane -> tile coordinates.
pub(crate) fn cursor_tile(
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

