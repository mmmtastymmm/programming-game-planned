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
///
/// LMB click-vs-drag disambiguation while a tool is armed: a press is the
/// tool's click only if the cursor stays inside this dead zone; traveling
/// past it turns the gesture into a camera pan instead.
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

/// World XZ -> tile indices — the grid is centered on the origin, so tile
/// `(i, j)` spans `[i-0.5, i+0.5) x [j-0.5, j+0.5)`. One spelling of the
/// convention, shared by the raycast start cell and the plane fallback so they
/// can't drift or disagree at half-integer coordinates.
fn tile_at(x: f32, z: f32, width: i32, height: i32) -> (i32, i32) {
    (
        (x + width as f32 / 2.0 + 0.5).floor() as i32,
        (z + height as f32 / 2.0 + 0.5).floor() as i32,
    )
}

/// Cursor ray -> tile coordinates.
///
/// The elevation pass turned terrain into a heightfield of raised blocks, so a
/// flat `y=0` pick lands on the tile *behind* a raised one and a click on a
/// cliff *face* misses the block entirely. Instead we march the ray through the
/// heightfield front-to-back ([`raycast_tile`]) and take the first tile it
/// strikes — top face or cliff face both resolve to the tile they belong to.
/// A near-horizontal / upward ray, or an eye that has been panned below the
/// ground plane (no valid top-down pick — this is the old `t < 0` guard), falls
/// back to the ground-plane solve.
pub(crate) fn cursor_tile(
    windows: &Query<&Window>,
    cams: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    world: &sim::World,
) -> Option<TilePos> {
    let window = windows.single().ok()?;
    let cursor = window.cursor_position()?;
    let (camera, cam_transform) = cams.single().ok()?;
    let ray = camera.viewport_to_world(cam_transform, cursor).ok()?;
    let (grid_w, grid_h) = (world.grid.width, world.grid.height);
    // Only cast when the ray genuinely descends onto terrain from above the
    // ground — a below-ground eye would spuriously "hit" the first edge cell.
    if ray.direction.y < -1e-4 && ray.origin.y > 0.0 {
        if let Some(tile) = raycast_tile(grid_w, grid_h, ray.origin, *ray.direction, |i, j| {
            crate::palette::terrain_top(world, TilePos::new(i, j))
        }) {
            return Some(tile);
        }
    }
    // Fallback: intersect the ground plane (degenerate ray, or the ray cleared
    // the whole heightfield without touching a block).
    if ray.direction.y.abs() < 1e-4 {
        return None;
    }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return None;
    }
    let hit = ray.origin + *ray.direction * t;
    let (i, j) = tile_at(hit.x, hit.z, grid_w, grid_h);
    Some(TilePos::new(i, j))
}

/// March a descending ray through the terrain heightfield and return the first
/// tile it strikes, front-to-back — so a click on a cliff face resolves to the
/// raised tile that owns it, not the ground behind it.
///
/// Each tile `(i, j)` is a column whose rendered top is `top(i, j)` (the tile's
/// [`crate::palette::terrain_top`]); the terrain is a gap-free heightfield on a
/// shared floor, so only the top matters for a descending ray. Uses a 2-D grid
/// DDA (Amanatides–Woo) over the XZ plane; pure over its inputs so it's
/// unit-testable without a `World`. `None` if the ray leaves the grid without
/// meeting any block (the caller falls back to the ground plane).
///
/// Precondition: `dir` descends (`dir.y < 0`) and `origin` is above the terrain
/// — the caller gates on both. A ray from below would report the first cell.
fn raycast_tile(
    width: i32,
    height: i32,
    origin: Vec3,
    dir: Vec3,
    top: impl Fn(i32, i32) -> f32,
) -> Option<TilePos> {
    // Grid space: tile (i, j) is centered on integer (i, j) and spans
    // [i-0.5, i+0.5) — p = worldx + width/2.
    let p0 = origin.x + width as f32 / 2.0;
    let q0 = origin.z + height as f32 / 2.0;
    let (mut i, mut j) = tile_at(origin.x, origin.z, width, height);

    let step_i = if dir.x > 0.0 { 1 } else { -1 };
    let step_j = if dir.z > 0.0 { 1 } else { -1 };
    let t_delta_i = if dir.x != 0.0 { (1.0 / dir.x).abs() } else { f32::INFINITY };
    let t_delta_j = if dir.z != 0.0 { (1.0 / dir.z).abs() } else { f32::INFINITY };
    // t to the first cell boundary on each axis (boundaries at half-integers).
    let bound_i = if step_i > 0 { i as f32 + 0.5 } else { i as f32 - 0.5 };
    let bound_j = if step_j > 0 { j as f32 + 0.5 } else { j as f32 - 0.5 };
    let mut t_max_i = if dir.x != 0.0 { (bound_i - p0) / dir.x } else { f32::INFINITY };
    let mut t_max_j = if dir.z != 0.0 { (bound_j - q0) / dir.z } else { f32::INFINITY };

    // Budget = cells to reach the grid from the (possibly far, zoomed-out)
    // origin + cells to cross it. Counting only the perimeter would let a
    // distant camera run out before entering, silently reverting to the flat
    // pick. `out_of` is 0 inside the grid, else the gap to the near edge.
    let out_of = |c: i32, dim: i32| -> i64 {
        if c < 0 {
            (-c) as i64
        } else if c >= dim {
            (c - dim + 1) as i64
        } else {
            0
        }
    };
    let max_steps = out_of(i, width) + out_of(j, height) + 2 * (width + height) as i64 + 8;

    let mut entered = false;
    for _ in 0..max_steps {
        let t_exit = t_max_i.min(t_max_j);
        if (0..width).contains(&i) && (0..height).contains(&j) {
            entered = true;
            // Descending ray: it strikes this column if it is at or below the
            // tile top by the time it leaves the cell (lowest y over the cell —
            // covers both entering through the cliff face and crossing the top
            // face within the cell).
            let y_lo = origin.y + t_exit.min(1e9) * dir.y;
            if y_lo <= top(i, j) {
                return Some(TilePos::new(i, j));
            }
        } else if entered {
            break; // crossed the grid without a hit
        }
        if t_exit.is_infinite() {
            break; // no further boundary crossings (axis-aligned ray)
        }
        if t_max_i <= t_max_j {
            i += step_i;
            t_max_i += t_delta_i;
        } else {
            j += step_j;
            t_max_j += t_delta_j;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_ray_hits_the_tile_below() {
        // Camera over tile (1,1) of a flat 3x3 grid, looking straight down.
        let origin = Vec3::new(1.0 - 1.5, 5.0, 1.0 - 1.5); // tile (1,1) center
        let dir = Vec3::new(0.0, -1.0, 0.0);
        assert_eq!(raycast_tile(3, 3, origin, dir, |_, _| 0.0), Some(TilePos::new(1, 1)));
    }

    #[test]
    fn shallow_ray_hits_the_cliff_face_not_the_ground_behind_it() {
        // A wall runs down column i=2 (top 0.55); everything else is flat. A
        // shallow ray from the east never reaches y=0 inside the grid, so a
        // plane solve would miss the wall — the DDA must catch its east face.
        let wall = |i: i32, _j: i32| if i == 2 { 0.55 } else { 0.0 };
        let origin = Vec3::new(5.0, 1.0, -0.5); // east of a 5x5 grid, row j=2
        let dir = Vec3::new(-1.0, -0.1, 0.0).normalize();
        assert_eq!(raycast_tile(5, 5, origin, dir, wall), Some(TilePos::new(2, 2)));
    }

    #[test]
    fn far_zoomed_out_origin_still_reaches_the_grid() {
        // The step budget must count the long march from a distant origin, not
        // just the grid perimeter — else a zoomed-out camera runs out before
        // entering and reverts to the flat pick. Origin 55 cells east of a 5x5
        // grid; a shallow ray still clears the flat east tiles and strikes the
        // i=2 wall's face. (Under the old 2*(w+h)+8=28 budget this returned
        // None.)
        let wall = |i: i32, _j: i32| if i == 2 { 0.55 } else { 0.0 };
        let origin = Vec3::new(60.0, 1.0, -0.5);
        let dir = Vec3::new(-1.0, -0.015, 0.0).normalize();
        assert_eq!(raycast_tile(5, 5, origin, dir, wall), Some(TilePos::new(2, 2)));
    }

    #[test]
    fn ray_over_a_flat_tile_passes_until_it_meets_the_ground() {
        // Same shallow ray, but flat terrain: it should sail over the near
        // tiles and land where it finally descends to y=0 (tile i=0, worldx
        // -2.5 -> y = 1 - 0.1*7.5 = 0.25 still >0; it exits west above ground),
        // so with no block anywhere the heightfield reports no hit.
        let origin = Vec3::new(5.0, 1.0, -0.5);
        let dir = Vec3::new(-1.0, -0.1, 0.0).normalize();
        assert_eq!(raycast_tile(5, 5, origin, dir, |_, _| 0.0), None);
    }
}

