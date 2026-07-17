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
    let mut spec = MapSpec::empty(34, 20);
    for y in 2..9 {
        spec.rubble.push(TilePos::new(12, y));
    }
    // A water wall fully splits the map: the ONLY way east is bridges the
    // player builds — one-way pairs make deadlock-free crossings.
    for y in 0..20 {
        spec.water.push(TilePos::new(16, y));
    }
    // Modest west-side ore keeps the colony alive pre-bridge.
    spec.ore_nodes.push((TilePos::new(8, 3), 25));

    spec.ore_nodes.push((TilePos::new(20, 3), 60));
    spec.ore_nodes.push((TilePos::new(19, 11), 40));
    // Ore nodes sit in vein outcrops (docs/05): the gem marks the node,
    // the vein marks the ground it came out of. Same move cost as plains,
    // so miner routes are unchanged.
    for (x, y) in [(8, 3), (9, 3), (8, 4), (20, 3), (21, 3), (20, 2), (19, 3)] {
        spec.ore_veins.push(TilePos::new(x, y));
    }
    for (x, y) in [(19, 11), (20, 11), (19, 12), (18, 11)] {
        spec.ore_veins.push(TilePos::new(x, y));
    }

    // --- East-side terrain showcase (docs/05). Everything lives behind
    // the water wall (or is cost-neutral, like the veins above), so the
    // tuned west-side choreography below never feels it. ---
    // High-ground mesa running off the NE corner: cliff rim + scree where
    // it meets the meadow. Impassable until ramps exist.
    for (x, depth) in [(29, 2), (30, 3), (31, 4), (32, 4), (33, 3)] {
        for y in 0..depth {
            spec.high_ground.push(TilePos::new(x, y));
        }
    }
    // A snowfield drapes the mesa's west flank (altitude reads cold);
    // plains-cost for now, so no route changes (docs/QUESTIONS Q67).
    for (x, depth) in [(25, 2), (26, 3), (27, 4), (28, 3)] {
        for y in 0..depth {
            spec.snow.push(TilePos::new(x, y));
        }
    }
    // Mud bog with a standing pool at its heart — haulers would route
    // around it (3x entry), which is the tile's whole lesson.
    for (x, y0, y1) in [(21, 6, 9), (22, 5, 10), (23, 5, 10), (24, 6, 10), (25, 7, 9)] {
        for y in y0..y1 {
            // The pool tile stays water (mud is painted after water in
            // from_spec, so it must be left out here, not just overdrawn).
            if (x, y) != (23, 7) {
                spec.mud.push(TilePos::new(x, y));
            }
        }
    }
    spec.water.push(TilePos::new(23, 7));
    // Geothermal vent: a lone glowing crater on the east plain.
    spec.vents.push(TilePos::new(24, 2));
    // Corruption creeps in from the SE corner; the crystal field sits on
    // its doorstep (docs/05: Crystal spawns near Corruption).
    for (x, y0) in [(26, 16), (27, 16), (28, 15), (29, 15), (30, 14), (31, 14), (32, 14), (33, 13)] {
        for y in y0..20 {
            spec.corruption.push(TilePos::new(x, y));
        }
    }
    for (x, y) in
        [(26, 14), (26, 15), (27, 14), (27, 15), (28, 13), (28, 14), (29, 13), (29, 14), (30, 13)]
    {
        spec.crystal.push(TilePos::new(x, y));
    }
    // --- Raw-resource sampler (docs/03): a 2x2 swatch of each of the
    // nine resource grounds in the open south field, spaced so every
    // fringe and corner nub shows against the meadow. Ground kinds only,
    // plains-cost — nodes and recipes are Q69, so no bot behavior
    // changes. Kept south of y=14: clear of the sealed signal strip and
    // the scrap-recall walk along y=13.
    let swatches: [(i32, i32, TileKind); 9] = [
        (2, 15, TileKind::Sand),
        (5, 15, TileKind::StoneOutcrop),
        (8, 15, TileKind::Grove),
        (11, 15, TileKind::CoalSeam),
        (14, 15, TileKind::IronVein),
        (2, 18, TileKind::CopperVein),
        (5, 18, TileKind::TinVein),
        (8, 18, TileKind::SilverVein),
        (11, 18, TileKind::GoldVein),
    ];
    for (x, y, kind) in swatches {
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            spec.resource_tiles.push((TilePos::new(x + dx, y + dy), kind));
        }
    }
    spec.depots.push(TilePos::new(3, 7));
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 5),
        faction: 0,
        color: 0, // Green: first-born — the remainder bucket (M9)
        ruined: false,
    });
    // A second, dialed printer: target 0 until the player edits its rules
    // in the Printers panel (turn it up to see red bots run the red file).
    spec.printers.push(PrinterSpec {
        pos: TilePos::new(2, 9),
        faction: 0,
        color: 1,
        ruined: false,
    });
    // M9: the remainder prints to the fleet cap — pin the demo colony at
    // four miners (2 per printer). The signal-cloud strip below spawns
    // BLUE bots: blue has no printer, so they are ghost machines (Q65) —
    // outside the allocation and scrap by rule, which is exactly what
    // set pieces want.
    spec.fleet_cap_override = Some(2);
    spec.starting_ore = 30;
    spec.starting_stock.push((0, sim::resources::Resource::Stone, 50));

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
    // Slow boots so the power-on cloud is watchable. (The old capacity-
    // driven scrap-walk choreography died with the desired-max dial: the
    // strip bots are ghosts now, exempt from scrap — dial a printer's
    // rules to watch recalls instead.)
    game.tuning.boot_ticks = 40;
    // Both starter programs deploy at boot: green's `from hauling import
    // haul_home` and red's `import hauling` show the two import forms (the
    // red printer idles at dial 0 — turn it up to see red bots run it).
    game.apply(&Command::DeployProgram {
        faction: 0,
        color: BotColor::GREEN,
        source: crate::editor::starter_deploy_source(BotColor::GREEN.0),
    })
    .expect("miner program parses");
    game.apply(&Command::DeployProgram {
        faction: 0,
        color: BotColor::RED,
        source: crate::editor::starter_deploy_source(BotColor::RED.0),
    })
    .expect("red starter program parses");
    // Bridge blueprints across the wall: the default program services
    // blueprints first, so the opening minutes are the colony building its
    // own crossings — progress bars and all — before mining east. (No
    // blueprint at y=11: its approach tile sits inside the showcase strip.)
    for y in [2, 5, 8] {
        game.apply(&Command::PlaceBlueprint {
            pos: TilePos::new(16, y),
            kind: BlueprintKind::Bridge,
            faction: 0,
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

/// One entity of the terrain slab layer — despawned wholesale and
/// rebuilt by [`resync_terrain`] when the map changes (M8).
#[derive(Component)]
pub(crate) struct TerrainTile(i32, i32);

/// Spawn the full terrain slab layer (base slabs + edge/corner overlay
/// art). Called at startup and again on every terrain change.
pub(crate) fn spawn_terrain(commands: &mut Commands, palette: &Palette, world: &sim::World) {
    // Terrain slabs (0.96 with grout lines, prototype-style). The default
    // world is natural — grass, water, mountains; the circuit "tech" tile
    // is what terraforming turns ground into.
    //
    // Grass, water, and mountain autotile: a NESW same-neighbor bitmask
    // (bit 0 = N … bit 3 = W) picks 1 of 16 baked variants with edge art
    // on the "different" sides. Off-map counts as same, so terrain runs
    // off the board clean. Terrain MUTATES now (M8: scree collapse,
    // corruption spread, terraform works), so every slab carries the
    // TerrainTile marker and resync_terrain rebuilds the layer whenever
    // the sim's cached terrain hash moves.
    for y in 0..world.grid.height {
        for x in 0..world.grid.width {
            spawn_tile(commands, palette, world, x, y);
        }
    }
}

/// Spawn one tile's slab + edge/corner overlay art. The autotile masks
/// read the four neighbors (and corners the diagonals), so a rebuild of
/// tile T must also rebuild T's 3×3 neighborhood.
fn spawn_tile(commands: &mut Commands, palette: &Palette, world: &sim::World, x: i32, y: i32) {
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
    // Raw-resource grounds all autotile against "not myself"; one
    // fn-pointer table keeps the per-kind predicates non-capturing
    // (mask_of takes `fn`, not a closure).
    let same_resource: Option<fn(TileKind) -> bool> = match kind {
        TileKind::Sand => Some(|t| matches!(t, TileKind::Sand)),
        TileKind::StoneOutcrop => Some(|t| matches!(t, TileKind::StoneOutcrop)),
        TileKind::Grove => Some(|t| matches!(t, TileKind::Grove)),
        TileKind::CoalSeam => Some(|t| matches!(t, TileKind::CoalSeam)),
        TileKind::IronVein => Some(|t| matches!(t, TileKind::IronVein)),
        TileKind::CopperVein => Some(|t| matches!(t, TileKind::CopperVein)),
        TileKind::TinVein => Some(|t| matches!(t, TileKind::TinVein)),
        TileKind::SilverVein => Some(|t| matches!(t, TileKind::SilverVein)),
        TileKind::GoldVein => Some(|t| matches!(t, TileKind::GoldVein)),
        _ => None,
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
        // M8 moved the full block from Rubble to Mountain; Rubble
        // is LOW DEBRIS now — flat broken rock you drive over
        // (and what worn Scree collapses into).
        TileKind::Mountain => {
            let mask = mask_of(|t| matches!(t, TileKind::Mountain));
            (
                palette.mountain_block.clone(),
                palette.mountain_tex_mats[mask].clone(),
                MOUNTAIN_TOP - 0.10,
            )
        }
        TileKind::Rubble => {
            (palette.tex_slab.clone(), palette.mountain_tex_mats[15].clone(), 0.0)
        }
        // Scree sits slightly sunken; the stone-strewn overlay
        // below distinguishes it from settled Rubble.
        TileKind::Scree => {
            (palette.tex_slab.clone(), palette.mountain_tex_mats[15].clone(), -0.02)
        }
        // The mesa doorstep: tan plateau art at ground level reads
        // as the cut in the cliff (a true sloped mesh can come
        // with real art passes).
        TileKind::Ramp => {
            (palette.tex_slab.clone(), palette.highground_tex_mats[15].clone(), 0.0)
        }
        // A frozen sheet: open-water art, flat and grounded —
        // distinct from the sunken, banked river.
        TileKind::Ice => {
            (palette.tex_slab.clone(), palette.water_tex_mats[15].clone(), 0.0)
        }
        // Shallows: water art raised toward the banks.
        TileKind::Ford => {
            let mask = mask_of(|t| {
                matches!(t, TileKind::Water | TileKind::Bridge | TileKind::Ford)
            });
            (palette.tex_slab.clone(), palette.water_tex_mats[mask].clone(), -0.03)
        }
        // Terraformed artery: the circuit "tech" tile IS the road.
        TileKind::Road => {
            (palette.tex_slab.clone(), palette.ground_tex_mat.clone(), 0.0)
        }
        // Built mass at wall height, teched-over.
        TileKind::Barricade => {
            (
                palette.mountain_block.clone(),
                palette.ground_tex_mat.clone(),
                MOUNTAIN_TOP - 0.10,
            )
        }
        // Dunes wear Sand's art (autotiling against both).
        TileKind::Dunes => {
            let mask = mask_of(|t| matches!(t, TileKind::Dunes | TileKind::Sand));
            (
                palette.tex_slab.clone(),
                palette.resource_tex_mats[&TileKind::Sand.as_u8()][mask].clone(),
                0.0,
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
        // Each remaining terrain owns its boundary art: a dried
        // crust, a creep frontier, a broken-rock lip, a frost band
        // — all against anything that isn't itself.
        TileKind::Mud => {
            let mask = mask_of(|t| matches!(t, TileKind::Mud));
            (palette.tex_slab.clone(), palette.mud_tex_mats[mask].clone(), -0.02)
        }
        TileKind::Corruption => {
            let mask = mask_of(|t| matches!(t, TileKind::Corruption));
            (palette.tex_slab.clone(), palette.corruption_tex_mats[mask].clone(), 0.0)
        }
        TileKind::OreVein => {
            let mask = mask_of(|t| matches!(t, TileKind::OreVein));
            (palette.tex_slab.clone(), palette.ore_tex_mats[mask].clone(), 0.0)
        }
        TileKind::CrystalField => {
            let mask = mask_of(|t| matches!(t, TileKind::CrystalField));
            (palette.tex_slab.clone(), palette.crystal_tex_mats[mask].clone(), 0.0)
        }
        TileKind::Snow => {
            let mask = mask_of(|t| matches!(t, TileKind::Snow));
            (palette.tex_slab.clone(), palette.snow_tex_mats[mask].clone(), 0.0)
        }
        // High ground is a mesa: same block as the mountain, tan
        // plateau top, cliff rim wherever the mesa ends.
        TileKind::HighGround => {
            let mask = mask_of(|t| matches!(t, TileKind::HighGround));
            (
                palette.mountain_block.clone(),
                palette.highground_tex_mats[mask].clone(),
                MOUNTAIN_TOP - 0.10,
            )
        }
        // Vents are point features: one crater, no autotiling.
        TileKind::Vent => {
            (palette.tex_slab.clone(), palette.vent_tex_mat.clone(), 0.0)
        }
        TileKind::Sand
        | TileKind::StoneOutcrop
        | TileKind::Grove
        | TileKind::CoalSeam
        | TileKind::IronVein
        | TileKind::CopperVein
        | TileKind::TinVein
        | TileKind::SilverVein
        | TileKind::GoldVein => {
            let mask = mask_of(same_resource.expect("resource kind has predicate"));
            (
                palette.tex_slab.clone(),
                palette.resource_tex_mats[&kind.as_u8()][mask].clone(),
                0.0,
            )
        }
    };
    commands.spawn((
        TerrainTile(x, y),
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
        TerrainTile(x, y),
                Mesh3d(palette.tex_slab.clone()),
                MeshMaterial3d(mats[mask].clone()),
                Transform::from_translation(tile_xyz(world, pos, top - 0.05 + eps)),
            ));
        }
    };
    match kind {
        TileKind::Plains => {
            // Scree at a cliff's base (mountain or mesa): contact
            // shadow + stones on the looming sides, plus corner
            // clusters.
            let not_cliff = |t: TileKind| {
                !matches!(
                    t,
                    TileKind::Mountain | TileKind::HighGround | TileKind::Barricade
                )
            };
            overlay(commands, &palette.scree_mats, mask_of(not_cliff), 0.012);
            overlay(
                commands,
                &palette.scree_corner_mats,
                corner_mask_of(not_cliff),
                0.015,
            );
            overlay(
                commands,
                &palette.grass_corner_mats,
                corner_mask_of(|t| !matches!(t, TileKind::Water | TileKind::Bridge)),
                0.0135,
            );
        }
        TileKind::Water => overlay(
            commands,
            &palette.water_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::Water | TileKind::Bridge)),
            0.012,
        ),
        TileKind::Mountain => overlay(
            commands,
            &palette.mountain_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::Mountain)),
            0.012,
        ),
        // Scree reads as stone-strewn ground: the cliff-base
        // stones drawn unconditionally (mask 0 = all sides).
        TileKind::Scree => overlay(commands, &palette.scree_mats, 0, 0.012),
        TileKind::Mud => overlay(
            commands,
            &palette.mud_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::Mud)),
            0.012,
        ),
        TileKind::Corruption => overlay(
            commands,
            &palette.corruption_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::Corruption)),
            0.012,
        ),
        TileKind::OreVein => overlay(
            commands,
            &palette.ore_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::OreVein)),
            0.012,
        ),
        TileKind::CrystalField => overlay(
            commands,
            &palette.crystal_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::CrystalField)),
            0.012,
        ),
        TileKind::Snow => overlay(
            commands,
            &palette.snow_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::Snow)),
            0.012,
        ),
        TileKind::HighGround => overlay(
            commands,
            &palette.highground_corner_mats,
            corner_mask_of(|t| matches!(t, TileKind::HighGround)),
            0.012,
        ),
        TileKind::Sand
        | TileKind::StoneOutcrop
        | TileKind::Grove
        | TileKind::CoalSeam
        | TileKind::IronVein
        | TileKind::CopperVein
        | TileKind::TinVein
        | TileKind::SilverVein
        | TileKind::GoldVein => overlay(
            commands,
            &palette.resource_corner_mats[&kind.as_u8()],
            corner_mask_of(same_resource.expect("resource kind has predicate")),
            0.012,
        ),
        TileKind::Bridge
        | TileKind::Vent
        | TileKind::Rubble
        | TileKind::Ramp
        | TileKind::Dunes
        | TileKind::Ice
        | TileKind::Ford
        | TileKind::Road
        | TileKind::Barricade => {}
    }
}

/// Terrain mutates routinely now (M8): scree collapses, corruption
/// spreads, terraform works land. Rebuild ONLY the changed tiles plus
/// their 3×3 neighborhoods (autotile edge/corner masks read neighbors
/// and diagonals) — a living Blight Core moves the hash every spread
/// interval, and a full-layer rebuild per one-tile change re-spawned
/// thousands of entities each time (review 2026-07-16). The diff runs
/// against a grid snapshot; the terrain hash is the cheap fast-path.
pub(crate) fn resync_terrain(
    mut commands: Commands,
    game: NonSend<GameSim>,
    palette: Res<Palette>,
    mut last: Local<Option<(u64, sim::map::Grid)>>,
    tiles: Query<(Entity, &TerrainTile)>,
) {
    let world = &game.0.world;
    let hash = world.terrain_hash;
    let Some((last_hash, prev)) = last.as_mut() else {
        // setup_scene already spawned the layer; just take the baseline.
        *last = Some((hash, world.grid.clone()));
        return;
    };
    if *last_hash == hash {
        return;
    }
    *last_hash = hash;

    // Changed tiles, dilated one step (masks look at neighbors).
    let mut rebuild: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for y in 0..world.grid.height {
        for x in 0..world.grid.width {
            let pos = TilePos::new(x, y);
            if prev.get(pos) == world.grid.get(pos) {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && ny >= 0 && nx < world.grid.width && ny < world.grid.height {
                        rebuild.insert((nx, ny));
                    }
                }
            }
        }
    }
    *prev = world.grid.clone();
    if rebuild.is_empty() {
        return; // hash moved without a kind change (can't happen today)
    }
    for (entity, tile) in &tiles {
        if rebuild.contains(&(tile.0, tile.1)) {
            commands.entity(entity).despawn();
        }
    }
    for &(x, y) in &rebuild {
        spawn_tile(&mut commands, &palette, world, x, y);
    }
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
    let mut lens_glass_mats = HashMap::new();
    for (c, team, accent) in [
        (0u8, "green", Color::srgb(0.22, 0.85, 0.54)),
        (1, "red", Color::srgb(0.95, 0.30, 0.25)),
        (2, "blue", Color::srgb(0.31, 0.64, 0.95)),
        (3, "yellow", Color::srgb(0.95, 0.79, 0.30)),
        (4, "cyan", Color::srgb(0.27, 0.83, 0.83)),
        (5, "magenta", Color::srgb(0.89, 0.33, 0.78)),
        (6, "orange", Color::srgb(0.95, 0.57, 0.25)),
        (7, "purple", Color::srgb(0.61, 0.35, 0.95)),
        (8, "white", Color::srgb(0.91, 0.91, 0.94)),
    ] {
        let bot: Handle<Image> = asset_server.load(format!("textures/bot_atlas_{team}.png"));
        bot_tex_mats.insert(c, atlas_mat(&mut materials, bot));
        let printer: Handle<Image> =
            asset_server.load(format!("textures/printer_atlas_{team}.png"));
        printer_tex_mats.insert(c, atlas_mat(&mut materials, printer));
        // The lens glass is the team's accent — a glossy, faintly lit eye.
        lens_glass_mats.insert(
            c,
            materials.add(StandardMaterial {
                base_color: accent,
                metallic: 0.6,
                perceptual_roughness: 0.2,
                emissive: LinearRgba::from(accent) * 0.25,
                ..default()
            }),
        );
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
    let mud_frames = load_frames("tile_mud");
    let corruption_frames = load_frames("tile_corruption");
    let ore_frames = load_frames("tile_ore");
    let crystal_frames = load_frames("tile_crystal");
    let snow_frames = load_frames("tile_snow");
    let mut mats_from = |frame0: &[Handle<Image>], roughness: f32| -> Vec<_> {
        frame0.iter().map(|img| tile_tex_mat(&mut materials, img.clone(), roughness)).collect()
    };
    let grass_tex_mats = mats_from(&grass_frames[0], 0.95);
    let water_tex_mats = mats_from(&water_frames[0], 0.35);
    let mud_tex_mats = mats_from(&mud_frames[0], 0.75); // wet = a little glossy
    let corruption_tex_mats = mats_from(&corruption_frames[0], 0.9);
    let ore_tex_mats = mats_from(&ore_frames[0], 0.95);
    let crystal_tex_mats = mats_from(&crystal_frames[0], 0.7); // glassy shards
    let snow_tex_mats = mats_from(&snow_frames[0], 0.85);
    let mut autotile_mats = |prefix: &str, roughness: f32| -> Vec<_> {
        (0..16)
            .map(|mask| {
                tile_tex_mat(
                    &mut materials,
                    asset_server.load(format!("textures/{prefix}_{mask}.png")),
                    roughness,
                )
            })
            .collect()
    };
    let mountain_tex_mats = autotile_mats("mountain_atlas", 0.95);
    let highground_tex_mats = autotile_mats("highground_atlas", 0.95);
    // The nine raw-resource grounds (docs/03), keyed by kind. Metals get
    // a touch of gloss; sand, stone, and the grove stay matte.
    const RESOURCE_KINDS: [(TileKind, &str, f32); 9] = [
        (TileKind::Sand, "tile_sand", 0.95),
        (TileKind::StoneOutcrop, "tile_stone", 0.95),
        (TileKind::Grove, "tile_wood", 0.95),
        (TileKind::CoalSeam, "tile_coal", 0.9),
        (TileKind::IronVein, "tile_iron", 0.95),
        (TileKind::CopperVein, "tile_copper", 0.85),
        (TileKind::TinVein, "tile_tin", 0.9),
        (TileKind::SilverVein, "tile_silver", 0.8),
        (TileKind::GoldVein, "tile_gold", 0.8),
    ];
    let mut resource_tex_mats = HashMap::new();
    for (kind, prefix, rough) in RESOURCE_KINDS {
        resource_tex_mats.insert(kind.as_u8(), autotile_mats(prefix, rough));
    }
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
    let mud_corner_mats = overlay_mats("tile_mud_corner");
    let corruption_corner_mats = overlay_mats("tile_corruption_corner");
    let ore_corner_mats = overlay_mats("tile_ore_corner");
    let crystal_corner_mats = overlay_mats("tile_crystal_corner");
    let snow_corner_mats = overlay_mats("tile_snow_corner");
    let highground_corner_mats = overlay_mats("tile_highground_corner");
    let mut resource_corner_mats = HashMap::new();
    for (kind, prefix, _) in RESOURCE_KINDS {
        resource_corner_mats.insert(kind.as_u8(), overlay_mats(&format!("{prefix}_corner")));
    }
    // Vent crater stays lit in shadow: its texture doubles as the emissive
    // map, so only the glow pixels glow (same trick as paper). Three pulse
    // frames; animate_terrain retargets base + emissive together.
    let vent_frames: Vec<Handle<Image>> =
        (0..3).map(|f| asset_server.load(format!("textures/tile_vent_f{f}.png"))).collect();
    let vent_tex_mat = materials.add(StandardMaterial {
        base_color_texture: Some(vent_frames[0].clone()),
        emissive: LinearRgba::new(0.7, 0.35, 0.12, 1.0),
        emissive_texture: Some(vent_frames[0].clone()),
        perceptual_roughness: 0.9,
        ..default()
    });
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
        lens_barrel: meshes.add(Cylinder::new(LENS_BARREL_RADIUS, LENS_BARREL_LEN)),
        lens_glass: meshes.add(Cylinder::new(LENS_GLASS_RADIUS, LENS_GLASS_LEN)),
        lens_barrel_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.11, 0.14, 0.18),
            metallic: 0.7,
            perceptual_roughness: 0.35,
            ..default()
        }),
        lens_glass_mats,
        tile_slab: meshes.add(Cuboid::new(0.96, 0.12, 0.96)),
        bot_cube: meshes.add(atlas_box_mesh(Vec3::splat(BOT_HALF))),
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
        mud_tex_mats,
        corruption_tex_mats,
        ore_tex_mats,
        crystal_tex_mats,
        snow_tex_mats,
        highground_tex_mats,
        resource_tex_mats,
        resource_corner_mats,
        vent_tex_mat,
        grass_frames,
        water_frames,
        mud_frames,
        corruption_frames,
        ore_frames,
        crystal_frames,
        snow_frames,
        vent_frames,
        scree_mats,
        water_corner_mats,
        grass_corner_mats,
        scree_corner_mats,
        mountain_corner_mats,
        mud_corner_mats,
        corruption_corner_mats,
        ore_corner_mats,
        crystal_corner_mats,
        snow_corner_mats,
        highground_corner_mats,
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

    spawn_terrain(&mut commands, &palette, world);

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
    let cam = OrbitCam { focus: Vec3::ZERO, distance: 28.0, yaw: 0.0, pitch: 0.85 };
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
