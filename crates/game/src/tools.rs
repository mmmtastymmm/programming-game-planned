//! The build-bar tool catalog and the systems that arm and apply tools.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim::map::{Direction, OverlayKind};
use sim::sim::Command;
use sim::world::BlueprintKind;

use crate::editor::EditorState;
use crate::GameSim;
use crate::palette::*;
use crate::camera::*;
use crate::scene::*;

/// The translucent placement ghost (slab + one-way chevron children).
#[derive(Component)]
pub(crate) struct PreviewSlab;
#[derive(Component)]
pub(crate) struct PreviewStrip;
#[derive(Component)]
pub(crate) struct PreviewTip;

/// What an armed build-bar item does on click.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ToolKind {
    /// Blueprint construction (bots do the labor).
    Building(BlueprintKind),
    /// Instant traffic signage on any tile; None = eraser.
    Overlay(Option<OverlayKind>),
    /// Instant cosmetic tile paint (drag to paint); None = eraser.
    Paint(Option<u8>),
    /// Emergency stop: click a bot to wreck it (logs kept, cargo spills).
    Kill,
}

pub(crate) struct BuildItem {
    pub(crate) name: &'static str,
    pub(crate) kind: ToolKind,
}

/// Paint palette (index -> display color).
pub(crate) const PAINT_COLORS: [(u8, u8, u8); 4] =
    [(220, 60, 50), (70, 200, 80), (70, 120, 230), (235, 200, 60)];

pub(crate) const BUILD_CATEGORIES: &[(&str, &[BuildItem])] = &[
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
pub(crate) fn same_item(a: ToolKind, b: ToolKind) -> bool {
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
pub(crate) fn build_icon(name: &str) -> egui::ColorImage {
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

/// Build mode: LMB picks a tile via the cursor ray onto the ground plane;
/// the sim validates (water only, funds, no duplicate) — the UI just aims.
pub(crate) fn place_blueprint(
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
            let _ = game.0.apply(&Command::PlaceBlueprint { pos, kind: blueprint, faction: 0 });
        }
        ToolKind::Overlay(overlay) => {
            let _ = game.0.apply(&Command::PlaceOverlay { pos, overlay, faction: 0 });
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

/// The translucent ghost: follows the hovered tile while armed, tinted by
/// placement validity; the one-way chevron shows which way traffic will
/// flow (R rotates it live).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn build_preview(
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
            let cost = game.0.tuning.bridge_cost_stone;
            let ok = world.grid.get(pos) == Some(sim::TileKind::Water)
                && !world.blueprints.values().any(|b| b.pos == pos)
                && world.stock_get(0, sim::resources::Resource::Stone) >= cost;
            (ok, None)
        }
        ToolKind::Overlay(Some(_)) => {
            (world.stock_get(0, sim::resources::Resource::Stone)
                >= game.0.tuning.overlay_cost_stone, None)
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
