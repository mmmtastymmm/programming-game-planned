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

const TILES: &[&str] = &["tile_ground", "tile_bridge", "tile_oneway"];

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
/// `sim::world::Color` (0 = green, 1 = red, 2+ = blue).
const TEAMS: &[(&str, [&str; 3])] = &[
    ("green", ["#39d98a", "#2aa86b", "#c8ffe6"]),
    ("red", ["#f24c40", "#c03227", "#ffd8d3"]),
    ("blue", ["#4fa3f2", "#2f7fd1", "#d9ecff"]),
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

    for tile in TILES {
        let svg = fs::read_to_string(art.join(format!("{tile}.svg"))).expect("tile svg");
        render(&svg, SIZE)
            .save_png(out.join(format!("{tile}.png")))
            .expect("save tile png");
    }

    for (team, colors) in TEAMS {
        let mut atlas = tiny_skia::Pixmap::new(SIZE * 3, SIZE * 2).expect("atlas alloc");
        for (face, col, row) in FACES {
            let mut svg = fs::read_to_string(art.join(format!("bot_face_{face}.svg")))
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
            .save_png(out.join(format!("bot_atlas_{team}.png")))
            .expect("save atlas png");
    }
}
