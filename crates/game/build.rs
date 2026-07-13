//! Asset bake: rasterize the SVG sources in `assets/art` into the PNG
//! textures the game loads from `assets/textures`. Runs automatically when
//! anything under `assets/art` changes (`cargo bake` is an alias for a
//! plain build of this crate).
//!
//! Bot faces are authored once in the green master palette; team variants
//! are string-level palette swaps of the accent hexes, baked into one 3x2
//! atlas per team (front/right/back over left/top/bottom — the same layout
//! `bot_cube_mesh` in main.rs maps UVs to).

use resvg::{tiny_skia, usvg};
use std::fs;
use std::path::Path;

/// Pixels per face / per tile texture.
const SIZE: u32 = 256;

const TILES: &[&str] = &[
    "tile_ground",
    "tile_bridge",
    "tile_oneway",
    "tile_grass",
    "tile_water",
    "tile_mountain",
    "tile_terraform",
    "tile_wreck",
    "crate",
    "paper",
    "scribble_0",
    "scribble_1",
    "scribble_2",
    "scribble_error_0",
    "scribble_error_1",
    "scribble_error_2",
    "scribble_hurt_0",
    "scribble_hurt_1",
    "scribble_hurt_2",
    "scribble_death_0",
    "scribble_death_1",
    "scribble_death_2",
    "scribble_bumped_0",
    "scribble_bumped_1",
    "scribble_bumped_2",
    "scribble_boot_0",
    "scribble_boot_1",
    "scribble_boot_2",
    "scribble_recall_0",
    "scribble_recall_1",
    "scribble_recall_2",
];

/// Atlased 6-face bodies: (svg prefix, output prefix).
const ATLASES: &[(&str, &str)] = &[("bot_face", "bot_atlas"), ("printer_face", "printer_atlas")];

/// (face, atlas column, atlas row)
const FACES: &[(&str, u32, u32)] = &[
    ("front", 0, 0),
    ("right", 1, 0),
    ("back", 2, 0),
    ("left", 0, 1),
    ("top", 1, 1),
    ("bottom", 2, 1),
];

/// Master accent palette as authored (green team).
const MASTER: [&str; 3] = ["#39d98a", "#2aa86b", "#c8ffe6"];

/// (atlas name, [accent, accent-dark, highlight]) — indices match
/// `sim::world::Color` (0 = green, 1 = red, 2+ = blue). "ruined" is the
/// dead-gray swap used for wrecked structures.
const TEAMS: &[(&str, [&str; 3])] = &[
    ("green", ["#39d98a", "#2aa86b", "#c8ffe6"]),
    ("red", ["#f24c40", "#c03227", "#ffd8d3"]),
    ("blue", ["#4fa3f2", "#2f7fd1", "#d9ecff"]),
    ("ruined", ["#5a5f6a", "#4a4e57", "#8a8e99"]),
];

fn render(svg: &str, px: u32) -> tiny_skia::Pixmap {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("valid SVG");
    let mut pixmap = tiny_skia::Pixmap::new(px, px).expect("pixmap alloc");
    let scale = px as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap
}

fn main() {
    println!("cargo:rerun-if-changed=assets/art");

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let art = root.join("assets/art");
    let out = root.join("assets/textures");
    fs::create_dir_all(&out).expect("create textures dir");

    // Scribble icons composite onto the shared white thought bubble; the
    // icon shrinks into the bubble's body (the tail hangs bottom-left).
    let bubble_svg =
        fs::read_to_string(art.join("scribble_bubble.svg")).expect("bubble svg");
    for tile in TILES {
        let svg = fs::read_to_string(art.join(format!("{tile}.svg"))).expect("tile svg");
        let icon = render(&svg, SIZE);
        let px = if tile.starts_with("scribble") {
            let mut base = render(&bubble_svg, SIZE);
            let scale = 0.58;
            let tx = SIZE as f32 * (1.0 - scale) / 2.0;
            let ty = tx - SIZE as f32 * 0.075; // bias up, away from the bottom tail
            base.draw_pixmap(
                0,
                0,
                icon.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, tx, ty),
                None,
            );
            base
        } else {
            icon
        };
        px.save_png(out.join(format!("{tile}.png"))).expect("save tile png");
    }

    for (svg_prefix, out_prefix) in ATLASES {
        for (team, colors) in TEAMS {
            let mut atlas = tiny_skia::Pixmap::new(SIZE * 3, SIZE * 2).expect("atlas alloc");
            for (face, col, row) in FACES {
                let mut svg = fs::read_to_string(art.join(format!("{svg_prefix}_{face}.svg")))
                    .expect("face svg");
                for (master, team_color) in MASTER.iter().zip(colors) {
                    svg = svg.replace(master, team_color);
                }
                let face_px = render(&svg, SIZE);
                atlas.draw_pixmap(
                    (col * SIZE) as i32,
                    (row * SIZE) as i32,
                    face_px.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
            atlas
                .save_png(out.join(format!("{out_prefix}_{team}.png")))
                .expect("save atlas png");
        }
    }

    // Side-by-side pairs: (left svg, right svg, output). The mountain
    // block samples its top from the left cell and its sides from the
    // right (see `mountain_block_mesh` in main.rs).
    for (left, right, name) in [("tile_mountain", "rock_face", "mountain_atlas")] {
        let mut pair = tiny_skia::Pixmap::new(SIZE * 2, SIZE).expect("pair alloc");
        for (i, src) in [left, right].into_iter().enumerate() {
            let svg = fs::read_to_string(art.join(format!("{src}.svg"))).expect("pair svg");
            pair.draw_pixmap(
                (i as u32 * SIZE) as i32,
                0,
                render(&svg, SIZE).as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                None,
            );
        }
        pair.save_png(out.join(format!("{name}.png"))).expect("save pair png");
    }
}
