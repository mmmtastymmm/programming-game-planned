//! The in-game Pyrite editor: a colony file viewer in the side panel and
//! floating, movable code windows — one per color program, one per library
//! module — each with live syntax highlighting, parse-error squiggles,
//! kind-argument completion, and hover docs (docs/01 "Editor & Player
//! Experience"). Program windows render the engine's implicit forever-loop
//! as locked phantom lines tinted the program's color.

mod complete;
mod highlight;
mod window;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim::sim::Command;
use sim::world::{BlueprintKind, Color as BotColor, PrinterState};
use sim::map::OverlayKind;
use sim::TilePos;
use std::collections::{BTreeMap, HashMap};

use crate::tools::{build_icon, same_item, ToolKind, BUILD_CATEGORIES};
use crate::GameSim;
use window::{code_editor, line_start_char, Doc, DocKey, HandlerSlot};

/// Green's default program: the `from module import name` side of the
/// import showcase — `haul_home` bound bare. (Uses `if` and imports — the
/// dev sandbox runs with all constructs unlocked; the doc's true Tier-0
/// starter is the four mining lines alone.)
pub(crate) const GREEN_PROGRAM: &str = "\
from hauling import haul_home

if exists(blueprint):
    move_to(closest(blueprint).expect())
    build()
move_to(closest(ore).expect())
mine()
haul_home()
";

/// Red's default program: the plain `import module` side — same behavior
/// as green, called qualified.
pub(crate) const RED_PROGRAM: &str = "\
import hauling

if exists(blueprint):
    move_to(closest(blueprint).expect())
    build()
move_to(closest(ore).expect())
mine()
hauling.haul_home()
";

/// Which import style a fresh color slot starts with (green and red
/// together demo both forms).
fn default_program(color: u8) -> &'static str {
    match BotColor(color) {
        BotColor::RED => RED_PROGRAM,
        _ => GREEN_PROGRAM,
    }
}

/// The pre-seeded `hauling` module the sandbox boots with, so the file
/// viewer opens on a working program+module example (both default
/// programs import it). The docstring is deliberate teaching material —
/// it shows in the file viewer's hover for `def haul_home()`.
const STARTER_MODULE: &str = "\
def haul_home():
    \"\"\"Take the cargo home: nearest depot, then deposit.\"\"\"
    move_to(closest(depot).expect())
    deposit()
";

/// Starter contents for a fresh "+ new" module: tells the player this is
/// where shared functions go, how to import them, and that docstrings
/// exist. Comment-only — a module only rides a deploy once a program
/// imports it.
const MODULE_TEMPLATE: &str = "\
# A library module: def new functions here, then import them from a
# program window — Green and Red show the two forms:
#   from hauling import haul_home  ->  haul_home()
#   import hauling                 ->  hauling.haul_home()
# A \"\"\"docstring\"\"\" first line documents the def — it shows when
# you hover the function in the file viewer, and costs nothing.
# def patrol():
#     \"\"\"Chase whatever enemy is closest.\"\"\"
#     move_to(closest(enemy).expect())
";

/// What the showcase scene deploys for a color at boot: the starter
/// module's block plus that color's default program. Must match the
/// editor's first `assembled_source` byte-for-byte, so colors boot
/// showing "v1" rather than "modified".
pub(crate) fn starter_deploy_source(color: u8) -> String {
    let program = default_program(color);
    format!("{}{}", module_prelude(&EditorState::default(), program), program)
}

#[derive(Resource)]
pub(crate) struct EditorState {
    /// Every file in the colony file viewer, programs and modules alike.
    pub(crate) docs: BTreeMap<DocKey, Doc>,
    /// Stable id source for created modules (names are editable).
    next_module_id: u32,
    /// Armed build-bar tool (Esc/RMB cancels).
    pub(crate) selected_build: Option<ToolKind>,
    /// Last tile painted during a drag (avoids re-sending every frame).
    pub(crate) last_paint_tile: Option<TilePos>,
    /// Inspected bot (click a bot with no tool armed).
    pub(crate) selected_bot: Option<u32>,
    /// LMB press position, for click-vs-drag discrimination.
    pub(crate) press_pos: Option<Vec2>,
    /// Sim time controls (viewer-local; multiplayer will vote — docs/08).
    pub(crate) paused: bool,
    pub(crate) speed: f32,
    /// Selected category tab in the build bar.
    pub(crate) build_category: usize,
    /// Procedurally-drawn item icons, keyed by item name.
    pub(crate) icons: HashMap<&'static str, egui::TextureHandle>,
}

impl Default for EditorState {
    fn default() -> Self {
        // The starter `hauling` module ships open next to the green program
        // window: the boot screen is a working call-into-module example.
        let mut docs = BTreeMap::new();
        docs.insert(DocKey::Module(0), Doc::new("hauling", STARTER_MODULE, true));
        Self {
            docs,
            next_module_id: 1,
            selected_build: None,
            last_paint_tile: None,
            selected_bot: None,
            press_pos: None,
            paused: false,
            speed: 1.0,
            build_category: 0,
            icons: HashMap::new(),
        }
    }
}

fn color_name(color: u8) -> String {
    match BotColor(color) {
        BotColor::GREEN => "Green".into(),
        BotColor::RED => "Red".into(),
        _ => format!("Color {color}"),
    }
}

/// The egui tint for a program color — window titles, phantom loop lines,
/// and file-viewer entries all agree (docs/01: the bot is visibly tinted).
fn color_tint(color: u8) -> egui::Color32 {
    match BotColor(color) {
        BotColor::GREEN => egui::Color32::from_rgb(120, 220, 120),
        BotColor::RED => egui::Color32::from_rgb(240, 120, 100),
        _ => egui::Color32::from_rgb(180, 180, 200),
    }
}

/// Every printer color gets a program doc; the buffer seeds from the
/// deployed source when there is one (so restarts don't lose the program).
/// Handler blocks are split out into their own documents — the file
/// viewer's Handlers folder — and reassembled with the body at deploy.
fn ensure_program_docs(editor: &mut EditorState, game: &GameSim) {
    for color in program_colors(game) {
        if editor.docs.contains_key(&DocKey::Program(color)) {
            continue;
        }
        let source = game
            .0
            .world
            .color_programs
            .get(&(0, color))
            .map(|cp| cp.source.clone())
            .unwrap_or_else(|| default_program(color).to_string());
        // Deploys carry the imported module blocks up front; the program
        // document holds only what the player wrote, so peel them back
        // off. (The scan reads the artifact's own import lines, which
        // name exactly the modules whose blocks were prepended.)
        let prelude = module_prelude(editor, &source);
        let source = source.strip_prefix(&prelude).map(str::to_string).unwrap_or(source);
        let (body, files) = window::split_source(&source);
        // The starting color's window opens on boot so the app never
        // starts windowless; the rest open from the file viewer.
        editor.docs.insert(
            DocKey::Program(color),
            Doc::new(color_name(color), body, BotColor(color) == BotColor::GREEN),
        );
        // Handlers are colony-wide: the first deployed block for a slot
        // seeds the shared document; other colors assemble against it.
        for (slot, text) in files {
            editor
                .docs
                .entry(DocKey::Handler(slot))
                .or_insert_with(|| Doc::new(slot.name(), text, false));
        }
    }
}

/// The colony's program colors, sorted (one per printer).
fn program_colors(game: &GameSim) -> Vec<u8> {
    let mut colors: Vec<u8> = game.0.world.printers.values().map(|p| p.color.0).collect();
    colors.sort_unstable();
    colors.dedup();
    colors
}

/// The module names a program body pulls in — its `import x` / `from x
/// import ...` lines, read textually (the real parse validates them; this
/// only picks which module blocks ride the artifact). Module-granularity
/// shipping is the stand-in for Q61's function-level tree-shaking.
fn imported_modules(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in body.lines() {
        let t = line.trim_start();
        let rest = if let Some(r) = t.strip_prefix("import ") {
            r
        } else if let Some(r) = t.strip_prefix("from ") {
            r
        } else {
            continue;
        };
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// True if a module doc has any live code (the fresh-module template is
/// all comments — an empty `module` block wouldn't parse, so comment-only
/// modules never ride a deploy).
fn module_has_code(code: &str) -> bool {
    code.lines().any(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#')
    })
}

/// The library section of one program's deploy: a `module <name>:` block
/// for each module the body imports, in first-import order. The program's
/// own import lines then resolve against these at parse — unknown modules
/// simply get no block, so the parser reports them at deploy.
fn module_prelude(editor: &EditorState, body: &str) -> String {
    let mut out = String::new();
    for name in imported_modules(body) {
        let module = editor.docs.iter().find(|(key, doc)| {
            matches!(key, DocKey::Module(_)) && doc.name == name && module_has_code(&doc.code)
        });
        let Some((_, doc)) = module else { continue };
        out.push_str(&format!("module {}:\n", doc.name));
        out.push_str(&window::indent_lines(&doc.code, "    "));
        out.push('\n');
    }
    out
}

/// What a deploy of `color` would ship: the imported module blocks, the
/// program body, then the colony-wide handler documents in slot order
/// ([01-language.md]: the editor shows windows, the deployed program is
/// one assembled source; imports resolve at deploy).
fn assembled_source(editor: &EditorState, color: u8) -> String {
    let body =
        editor.docs.get(&DocKey::Program(color)).map(|d| d.code.as_str()).unwrap_or_default();
    let files: BTreeMap<HandlerSlot, String> = HandlerSlot::ALL
        .iter()
        .filter_map(|slot| {
            editor.docs.get(&DocKey::Handler(*slot)).map(|d| (*slot, d.code.clone()))
        })
        .collect();
    let body = format!("{}{}", module_prelude(editor, body), body);
    window::assemble_color(&body, &files)
}

/// The colony file viewer: programs (with their handlers and functions as
/// live-parsed children) and library modules. Clicking a file toggles its
/// window; clicking a child opens the window with the caret on that line.
fn file_viewer(ui: &mut egui::Ui, editor: &mut EditorState, game: &GameSim) {
    let colors = program_colors(game);
    let assembled: BTreeMap<u8, String> =
        colors.iter().map(|&c| (c, assembled_source(editor, c))).collect();

    let keys: Vec<DocKey> = editor.docs.keys().copied().collect();
    egui::CollapsingHeader::new(egui::RichText::new("Programs").strong())
        .default_open(true)
        .show(ui, |ui| {
            for key in &keys {
                let DocKey::Program(color) = *key else { continue };
                let deployed = game.0.world.color_programs.get(&(0, color));
                let printer_ruined = game
                    .0
                    .world
                    .printers
                    .values()
                    .any(|p| p.color.0 == color && p.state == PrinterState::Ruined);
                // The outline parses the doc as it deploys — with its
                // imported module blocks prepended — then maps lines back
                // and drops the library's own defs from the children.
                let prelude = module_prelude(editor, &editor.docs[key].code);
                let prelude_lines = prelude.lines().count() as u32;
                let doc = editor.docs.get_mut(key).unwrap();

                // Children are parsed up front (label, line) so the row closures
                // below can mutate the doc freely.
                let children: Vec<(String, u32, Option<String>)> =
                    window::doc_outline(&format!("{prelude}{}", doc.code))
                        .into_iter()
                        .filter(|(_, line, _)| *line > prelude_lines)
                        .map(|(label, line, fdoc)| (label, line - prelude_lines, fdoc))
                        .collect();

                ui.horizontal(|ui| {
                    let title = egui::RichText::new(format!("● {}", doc.name))
                        .color(color_tint(color))
                        .strong();
                    if ui
                        .selectable_label(doc.open, title)
                        .on_hover_text("open/close this program's window")
                        .clicked()
                    {
                        doc.open = !doc.open;
                    }
                    match deployed {
                        Some(cp) if Some(&cp.source) != assembled.get(&color) => {
                            ui.small(format!("v{:08x} · modified", cp.hash as u32));
                        }
                        Some(cp) => {
                            ui.small(format!("v{:08x}", cp.hash as u32));
                        }
                        None => {
                            ui.small("undeployed");
                        }
                    }
                    if printer_ruined {
                        ui.small(egui::RichText::new("printer ruined").weak());
                    }
                });
                for (label, line, fdoc) in children {
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        // Docstrings surface here: the hover reads the
                        // function's own documentation.
                        let hover = match fdoc {
                            Some(d) => format!("{d}\n\nline {line} — click to jump"),
                            None => format!("line {line} — click to jump"),
                        };
                        if ui
                            .small_button(egui::RichText::new(label).monospace())
                            .on_hover_text(hover)
                            .clicked()
                        {
                            doc.open = true;
                            doc.pending_caret = Some(line_start_char(&doc.code, line));
                        }
                    });
                }
            }
        });

    // Handlers folder: the colony-wide signal-handler documents — every
    // bot of every color runs these. Existing handlers open like any file;
    // missing slots offer a one-click stub. Deploys assemble whatever
    // exists here after each color's program body.
    ui.add_space(6.0);
    egui::CollapsingHeader::new(egui::RichText::new("Handlers").strong())
        .default_open(true)
        .show(ui, |ui| {
            ui.small("shared by every bot, all colors");
            for slot in HandlerSlot::ALL {
                let key = DocKey::Handler(slot);
                if let Some(doc) = editor.docs.get_mut(&key) {
                    let mut delete = false;
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        let label = egui::RichText::new(format!("● {}", doc.name)).strong();
                        if ui
                            .selectable_label(doc.open, label)
                            .on_hover_text("open/close this handler's window")
                            .clicked()
                        {
                            doc.open = !doc.open;
                        }
                        delete = ui
                            .small_button("🗑")
                            .on_hover_text(
                                "remove this handler — bots fall back to the engine default \
                                 (takes effect on next deploy)",
                            )
                            .clicked();
                    });
                    if delete {
                        editor.docs.remove(&key);
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        if ui
                            .small_button(egui::RichText::new(format!("+ {}", slot.name())).weak())
                            .on_hover_text("add this handler (until then the engine default runs)")
                            .clicked()
                        {
                            editor.docs.insert(key, Doc::new(slot.name(), slot.stub(), true));
                        }
                    });
                }
            }
            // The engine-owned interrupts, visible but locked (docs/01: you always
            // see the whole sandwich — these two have no editable middle yet).
            for (label, tip) in [
                (
                    "boot — engine",
                    "prologue only today: upload_log() if the local buffer is non-empty, \
                     then the main program from line 1. A player boot window isn't in \
                     the language yet.",
                ),
                (
                    "recall — engine",
                    "fully reserved, never writable: suspend the program, walk home, \
                     re-color (XP kept) or scrap.",
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.add_enabled(
                        false,
                        egui::Button::new(egui::RichText::new(label).weak()).small(),
                    )
                    .on_disabled_hover_text(tip);
                });
            }
        });

    ui.add_space(6.0);
    egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id("file_viewer_modules"),
        true,
    )
    .show_header(ui, |ui| {
        ui.strong("Modules");
        if ui.small_button("+ new").clicked() {
            let id = editor.next_module_id;
            editor.next_module_id += 1;
            editor
                .docs
                .insert(DocKey::Module(id), Doc::new(format!("module_{id}"), MODULE_TEMPLATE, true));
        }
    })
    .body(|ui| {
        let mut delete: Option<DocKey> = None;
        for key in &keys {
            let DocKey::Module(_) = *key else { continue };
            let doc = editor.docs.get_mut(key).unwrap();
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(doc.open, egui::RichText::new(&doc.name).monospace())
                    .on_hover_text("open/close this module's window")
                    .clicked()
                {
                    doc.open = !doc.open;
                }
                if ui.small_button("🗑").on_hover_text("delete module").clicked() {
                    delete = Some(*key);
                }
            });
        }
        if let Some(key) = delete {
            editor.docs.remove(&key);
        }
        ui.small("import a module and it ships with the deploy — Green and Red show the two forms");
    });
}

/// One floating code window. Programs get the color-tinted phantom
/// forever-loop lines; the colony-wide handlers get the engine's locked
/// sandwich (forced prologue/epilogue) drawn around the editable block;
/// both deploy the assembled sources in `deploy`. Modules get a rename
/// field and no deploy.
fn doc_window(
    ctx: &egui::Context,
    key: DocKey,
    doc: &mut Doc,
    game: &mut GameSim,
    deploy: Vec<(u8, String)>,
    prelude: &str,
) {
    let (title, tint) = match key {
        DocKey::Program(c) => (format!("{} program", doc.name), color_tint(c)),
        DocKey::Handler(_) => {
            (format!("{} — all bots", doc.name), egui::Color32::from_rgb(235, 235, 245))
        }
        DocKey::Module(_) => (format!("{} (module)", doc.name), egui::Color32::from_rgb(180, 180, 200)),
    };
    let mut open = doc.open;
    egui::Window::new(egui::RichText::new(title).color(tint).strong())
        .id(egui::Id::new(("doc_window", key)))
        .open(&mut open)
        .default_size([470.0, 420.0])
        .resizable(true)
        .show(ctx, |ui| {
            // Claim the whole window height up front: the content always
            // exactly fills the dragged size, so the auto-size pass never
            // sees "smaller content" and ratchets the window down on
            // interaction (the code rows quantize to just under the
            // available space).
            ui.set_min_height(ui.available_height());
            let editor_id = egui::Id::new(("pyrite_doc", key));
            if let DocKey::Module(_) = key {
                ui.horizontal(|ui| {
                    ui.label("name:");
                    ui.text_edit_singleline(&mut doc.name)
                        .on_hover_text("programs import this name — renaming breaks their imports until they catch up");
                    // The name is an identifier in import lines; keep it one.
                    doc.name.retain(|c| c.is_ascii_alphanumeric() || c == '_');
                });
            }
            if let DocKey::Program(_) = key {
                // The engine's implicit forever-loop, drawn as locked
                // phantom lines in the program's color (docs/01): the code
                // visibly sits inside its loop.
                ui.horizontal(|ui| {
                    ui.monospace(egui::RichText::new("while True:").color(tint).strong());
                    ui.small(egui::RichText::new("engine loop — not in your source").weak());
                });
            }
            if let DocKey::Handler(slot) = key {
                // The template sandwich: the engine's forced prologue,
                // drawn as locked lines the player can see but never edit
                // (docs/01: you always see the whole sandwich, you can
                // only type in the middle).
                ui.horizontal(|ui| {
                    ui.monospace(
                        egui::RichText::new(format!("on {}:", slot.signal()))
                            .color(tint)
                            .strong(),
                    );
                    ui.small(egui::RichText::new("this window's reserved template").weak());
                });
                if slot == window::HandlerSlot::Boot {
                    ui.horizontal(|ui| {
                        ui.monospace(
                            egui::RichText::new("    upload_log()").color(tint).strong(),
                        );
                        ui.small(
                            egui::RichText::new(
                                "forced prologue — the incident report, if the buffer is non-empty",
                            )
                            .weak(),
                        );
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.monospace(
                            egui::RichText::new("    handler_init()").color(tint).strong(),
                        );
                        ui.small(
                            egui::RichText::new(format!(
                                "forced prologue — the flinch, {} ticks, unskippable",
                                game.0.tuning.handler_init_ticks
                            ))
                            .weak(),
                        );
                    });
                }
                // The cap meter: worst-case instructions vs this signal's
                // window cap, live as you type. Measured against the FULL
                // assembled deploy source (module prelude + program body +
                // every window), so helpers defined in the body or an
                // imported module resolve exactly as the deploy check will
                // see them.
                let kind = match slot {
                    window::HandlerSlot::Error => pyrite::ast::SignalKind::Error,
                    window::HandlerSlot::Hurt => pyrite::ast::SignalKind::Hurt,
                    window::HandlerSlot::Bump => pyrite::ast::SignalKind::Bump,
                    window::HandlerSlot::Bumped => pyrite::ast::SignalKind::Bumped,
                    window::HandlerSlot::Boot => pyrite::ast::SignalKind::Boot,
                };
                let cap = pyrite::analysis::window_cap(&game.0.costs, kind);
                let usage = deploy.first().and_then(|(_, source)| {
                    let program = pyrite::parse(source, &pyrite::UnlockSet::all()).ok()?;
                    pyrite::analysis::window_usage(&program, &game.0.costs, kind)
                });
                match usage {
                    Some((worst, cap)) => {
                        let over = worst > cap;
                        let color = if over {
                            egui::Color32::from_rgb(240, 120, 100)
                        } else {
                            egui::Color32::from_rgb(140, 170, 140)
                        };
                        ui.small(egui::RichText::new(format!(
                            "cap meter: {worst}/{cap} worst-case instructions{}",
                            if over { " — OVER CAP, deploy will reject" } else { "" }
                        )).color(color));
                    }
                    None => {
                        ui.small(
                            egui::RichText::new(format!(
                                "cap meter: —/{cap} (window empty or not yet parseable)"
                            ))
                            .weak(),
                        );
                    }
                }
            }
            // The editor fills whatever height the user drags the window
            // to: rows are derived from the space left after reserving
            // room for the chrome below the code (loop footer, deploy row,
            // status). Fixed rows would pin the window's height.
            let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
            let reserved = match key {
                DocKey::Program(_) | DocKey::Handler(..) => 96.0,
                DocKey::Module(_) => 44.0,
            };
            let rows =
                (((ui.available_height() - reserved) / row_h) as usize).clamp(8, 200);
            match key {
                DocKey::Program(_) => {
                    // The program sits visibly inside its loop: indented,
                    // with a tinted gutter bar tying it to the phantom
                    // `while True:` above — the handler-sandwich idiom.
                    let body = ui.indent(editor_id.with("loop_body"), |ui| {
                        code_editor(ui, editor_id, doc, &game.0, rows, prelude, None);
                    });
                    let rect = body.response.rect;
                    ui.painter().line_segment(
                        [
                            rect.left_top() + egui::vec2(4.0, 2.0),
                            rect.left_bottom() + egui::vec2(4.0, -2.0),
                        ],
                        egui::Stroke::new(2.0, tint.gamma_multiply(0.55)),
                    );
                    ui.monospace(
                        egui::RichText::new("# end of loop -> line 1")
                            .color(tint.gamma_multiply(0.8)),
                    );
                }
                DocKey::Handler(slot) => {
                    // Same sandwich idiom as the program loop: the block
                    // is indented behind a gutter, and the engine's forced
                    // exit renders as a locked line below.
                    let body = ui.indent(editor_id.with("handler_body"), |ui| {
                        // The window's live parse wraps the text in its
                        // `on <signal>:` block, so loop/safety/cap errors
                        // squiggle here instead of first failing at deploy.
                        code_editor(ui, editor_id, doc, &game.0, rows, prelude, Some(slot.signal()));
                    });
                    let rect = body.response.rect;
                    ui.painter().line_segment(
                        [
                            rect.left_top() + egui::vec2(5.0, 2.0),
                            rect.left_bottom() + egui::vec2(5.0, -2.0),
                        ],
                        egui::Stroke::new(2.0, tint.gamma_multiply(0.55)),
                    );
                    let _ = slot;
                    ui.monospace(
                        egui::RichText::new(
                            "# forced epilogue: restart main program at line 1",
                        )
                        .color(tint.gamma_multiply(0.8)),
                    );
                }
                DocKey::Module(_) => {
                    code_editor(ui, editor_id, doc, &game.0, rows, prelude, None);
                }
            }

            if !deploy.is_empty() {
                // Program and handler windows deploy the same assembled
                // artifacts (body + colony handlers); a handler window
                // pushes every color at once — the handler is shared.
                ui.separator();
                let button_label = if let [(color, _)] = deploy.as_slice() {
                    format!("Deploy {}", color_name(*color))
                } else {
                    "Deploy all colors".to_string()
                };
                ui.horizontal(|ui| {
                    if ui.button(button_label).clicked() {
                        let mut shipped: Vec<String> = Vec::new();
                        let mut failed: Option<String> = None;
                        for (color, source) in &deploy {
                            let cmd = Command::DeployProgram {
                                faction: 0,
                                color: BotColor(*color),
                                source: source.clone(),
                            };
                            match game.0.apply(&cmd) {
                                Ok(_) => shipped.push(color_name(*color)),
                                Err(e) => {
                                    failed = Some(e.to_string());
                                    break;
                                }
                            }
                        }
                        match failed {
                            None => {
                                doc.status = format!("deployed {}", shipped.join(", "));
                                doc.status_ok = true;
                            }
                            Some(e) => {
                                doc.status = e;
                                doc.status_ok = false;
                            }
                        }
                    }
                    if let [(color, source)] = deploy.as_slice() {
                        match game.0.world.color_programs.get(&(0, *color)) {
                            Some(cp) if cp.source == *source => {
                                ui.small(format!("deployed v{:08x}", cp.hash as u32));
                            }
                            Some(cp) => {
                                ui.small(format!("v{:08x} deployed · buffer modified", cp.hash as u32));
                            }
                            None => {
                                ui.small("never deployed");
                            }
                        }
                    } else {
                        let stale: Vec<String> = deploy
                            .iter()
                            .filter(|(c, s)| {
                                game.0
                                    .world
                                    .color_programs
                                    .get(&(0, *c))
                                    .is_none_or(|cp| cp.source != *s)
                            })
                            .map(|(c, _)| color_name(*c))
                            .collect();
                        if stale.is_empty() {
                            ui.small("all colors up to date");
                        } else {
                            ui.small(format!("modified vs deployed: {}", stale.join(", ")));
                        }
                    }
                });
                let status_color = if doc.status_ok {
                    egui::Color32::from_rgb(120, 220, 120)
                } else {
                    egui::Color32::from_rgb(240, 120, 100)
                };
                ui.colored_label(status_color, &doc.status);
            } else if let DocKey::Module(_) = key {
                ui.small(
                    "library module — ships with any program that imports it \
                     (import name / from name import fn)",
                );
            }
        });
    doc.open = open;
}

pub(crate) fn editor_ui(
    mut contexts: EguiContexts,
    mut game: NonSendMut<GameSim>,
    mut editor: ResMut<EditorState>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else { return };
    ensure_program_docs(&mut editor, &game);

    egui::TopBottomPanel::bottom("build_bar").exact_height(96.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Category tabs.
            ui.vertical(|ui| {
                ui.strong("Build");
                for (i, (name, _)) in BUILD_CATEGORIES.iter().enumerate() {
                    if ui.selectable_label(editor.build_category == i, *name).clicked() {
                        editor.build_category = i;
                    }
                }
            });
            ui.separator();

            // Items of the selected category.
            let (_, items) = BUILD_CATEGORIES[editor.build_category.min(BUILD_CATEGORIES.len() - 1)];
            for item in items {
                let cost = match item.kind {
                    ToolKind::Building(BlueprintKind::Bridge) => game.0.tuning.bridge_cost_stone,
                    ToolKind::Overlay(Some(_)) => game.0.tuning.overlay_cost_stone,
                    ToolKind::Overlay(None) | ToolKind::Paint(_) | ToolKind::Kill => 0,
                };
                let affordable = game.0.world.stock_get(0, sim::resources::Resource::Stone) >= cost;
                if !editor.icons.contains_key(item.name) {
                    let tex = ctx.load_texture(
                        item.name,
                        build_icon(item.name),
                        egui::TextureOptions::NEAREST,
                    );
                    editor.icons.insert(item.name, tex);
                }
                let tex_id = editor.icons[item.name].id();
                let selected = editor.selected_build.is_some_and(|k| same_item(k, item.kind));
                ui.vertical(|ui| {
                    let button = egui::ImageButton::new(egui::load::SizedTexture::new(
                        tex_id,
                        egui::vec2(48.0, 48.0),
                    ))
                    .selected(selected);
                    let hover = if cost > 0 {
                        format!("{} — {cost} ore", item.name)
                    } else {
                        format!("{} — free", item.name)
                    };
                    let response = ui.add_enabled(affordable, button).on_hover_text(hover);
                    if response.clicked() {
                        editor.selected_build = if selected { None } else { Some(item.kind) };
                    }
                    let cost_line = if cost > 0 { format!("{cost} ore") } else { "free".into() };
                    ui.small(format!("{}
{cost_line}", item.name));
                });
            }

            // Status / hints on the right.
            ui.separator();
            ui.vertical(|ui| {
                if let Some(kind) = editor.selected_build {
                    match kind {
                        ToolKind::Building(BlueprintKind::Bridge) => {
                            ui.label("Click a water tile to place — Esc/RMB cancels");
                        }
                        ToolKind::Overlay(Some(OverlayKind::Arrow(d))) => {
                            ui.label(format!(
                                "Click any tile to set {} — R rotates, Esc/RMB cancels",
                                d.arrow()
                            ));
                        }
                        ToolKind::Overlay(None) => {
                            ui.label("Click a tile to clear its overlay — Esc/RMB cancels");
                        }
                        ToolKind::Paint(Some(_)) => {
                            ui.label("Click or drag to paint tiles — Esc/RMB cancels");
                        }
                        ToolKind::Paint(None) => {
                            ui.label("Click or drag to erase paint — Esc/RMB cancels");
                        }
                        ToolKind::Kill => {
                            ui.label("Click a bot to shut it down — Esc/RMB cancels");
                        }
                    }
                } else {
                    ui.small("Select a tool, then click the map.");
                }
                let pending = game.0.world.blueprints.len();
                if pending > 0 {
                    ui.small(format!(
                        "{pending} blueprint(s) waiting for builders (closest(blueprint) / build)"
                    ));
                }
            });
        });
    });

    egui::SidePanel::left("editor").exact_width(300.0).show(ctx, |ui| {
        ui.heading("Files");
        file_viewer(ui, &mut editor, &game);
        ui.separator();

        ui.heading("Printers");
        let printer_ids: Vec<_> = game.0.world.printers.keys().copied().collect();
        let repair_cost = game.0.tuning.repair_cost_data;
        for pid in printer_ids {
            let (color, state, mut desired) = {
                let p = &game.0.world.printers[&pid];
                (p.color, p.state, p.desired_max)
            };
            let name = color_name(color.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(name).color(color_tint(color.0)));
                match state {
                    PrinterState::Ruined => {
                        let affordable = game.0.world.data.get(&0).copied().unwrap_or(0) >= repair_cost;
                        if ui
                            .add_enabled(
                                affordable,
                                egui::Button::new(format!("Repair ({repair_cost} ore)")),
                            )
                            .clicked()
                        {
                            let _ = game.0.apply(&Command::RepairPrinter { printer: pid });
                        }
                    }
                    PrinterState::Working => {
                        if ui
                            .add(egui::Slider::new(&mut desired, 0..=8).text("bots"))
                            .changed()
                        {
                            let _ = game
                                .0
                                .apply(&Command::SetDesiredMax { printer: pid, value: desired });
                        }
                        // Print progress lives on the world-space bar
                        // above the printer now — no duplicate here.
                    }
                }
            });
        }
        ui.separator();

        ui.heading("Cloud");
        let archive = &game.0.world.archive;
        for entry in archive.iter().rev().take(8).rev() {
            ui.small(format!("[{}] bot{}: {}", entry.tick, entry.bot.0, entry.text));
        }
        ui.separator();
        ui.small("LMB / MMB drag: pan · RMB drag: orbit · scroll: zoom");
    });

    // Floating code windows — one per open document, movable and
    // resizable anywhere over the world view. Assembled sources (body +
    // handlers) are computed first: they are what a deploy ships.
    let assembled: BTreeMap<u8, String> = program_colors(&game)
        .into_iter()
        .map(|c| (c, assembled_source(&editor, c)))
        .collect();
    let keys: Vec<DocKey> = editor.docs.keys().copied().collect();
    for key in keys {
        // A program window deploys its color; a handler window deploys
        // EVERY color (the handler is colony-wide, so all of them change).
        let deploy: Vec<(u8, String)> = match key {
            DocKey::Program(c) => {
                assembled.get(&c).map(|s| vec![(c, s.clone())]).unwrap_or_default()
            }
            DocKey::Handler(_) => assembled.iter().map(|(c, s)| (*c, s.clone())).collect(),
            DocKey::Module(_) => Vec::new(),
        };
        // Program windows live-parse with their imported module blocks in
        // front, so import errors show exactly as they would at deploy.
        let prelude = match key {
            DocKey::Program(_) => module_prelude(&editor, &editor.docs[&key].code),
            _ => String::new(),
        };
        let doc = editor.docs.get_mut(&key).unwrap();
        if doc.open {
            doc_window(ctx, key, doc, &mut game, deploy, &prelude);
        }
    }

    // Bar across the top of the world view (added after the editor panel,
    // so it spans only the world). Colony counters on the left; time
    // controls in the top right. The cloud stream stays in the side panel.
    egui::TopBottomPanel::top("world_bar").exact_height(28.0).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            let (tick, stock, data, bots, wrecks, cloud) = {
                let w = &game.0.world;
                // Typed stock (docs/03): every nonzero kind, deci → units.
                let stock: Vec<String> = w
                    .stock
                    .iter()
                    .filter(|((f, _), deci)| *f == 0 && **deci > 0)
                    .map(|((_, kind), deci)| {
                        format!("{} {}", kind.name(), *deci / sim::resources::DECI as u64)
                    })
                    .collect();
                let data = w.data.get(&0).copied().unwrap_or(0);
                (w.tick, stock, data, w.bots.len(), w.wrecks.len(), w.archive.len())
            };
            let mut texts = vec![format!("tick {tick}")];
            if stock.is_empty() {
                texts.push("stock —".into());
            } else {
                texts.extend(stock);
            }
            texts.push(format!("data {data}"));
            texts.push(format!("bots {bots}"));
            texts.push(format!("wrecks {wrecks}"));
            texts.push(format!("cloud {cloud}"));
            for text in texts {
                ui.monospace(text);
                ui.separator();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // right_to_left: first added is rightmost.
                ui.small("Space pauses");
                ui.separator();
                for (label, mult) in
                    [("4×", 4.0f32), ("2×", 2.0), ("1×", 1.0), ("½×", 0.5), ("¼×", 0.25)]
                {
                    if ui.selectable_label((editor.speed - mult).abs() < 0.01, label).clicked() {
                        editor.speed = mult;
                    }
                }
                ui.separator();
                let pause_label = if editor.paused { "▶ resume" } else { "⏸ pause" };
                if ui.selectable_label(editor.paused, pause_label).clicked() {
                    editor.paused = !editor.paused;
                }
                // Debug stepping, paused only (right_to_left: these land
                // left of the pause button).
                if editor.paused {
                    if ui.button("⏭ tick").on_hover_text("advance one sim tick").clicked() {
                        game.0.step();
                    }
                    let target = editor.selected_bot;
                    let step_line = ui
                        .add_enabled(target.is_some(), egui::Button::new("⏭ line"))
                        .on_hover_text(
                            "run until the inspected bot's line changes or an \
                             interrupt fires (handler entry/exit, init ritual, \
                             boot, recall)",
                        );
                    if step_line.clicked()
                        && let Some(id) = target
                    {
                        let bot_id = sim::world::BotId(id);
                        // Break on ANY observable execution-state change:
                        // line, fault, handler entry/exit, the init ritual
                        // starting or finishing, boot, recall. Interrupts
                        // are breakpoints — stepping never skips a flinch.
                        let probe = |game: &GameSim| {
                            game.0.world.bots.get(&bot_id).map(|b| {
                                (
                                    b.vm.as_ref().map(|vm| (vm.current_line(), vm.fault_count())),
                                    b.handler_name(),
                                    b.in_handler_init(),
                                    b.data.booting.is_some(),
                                    b.data.recall.is_some(),
                                )
                            })
                        };
                        let before = probe(&game);
                        for _ in 0..300 {
                            game.0.step();
                            let now = probe(&game);
                            if now != before || now.is_none() {
                                break;
                            }
                        }
                    }
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full handler-folder flow, minus the button: add stub handler
    /// docs to a program, and the assembled artifact must parse — i.e. the
    /// one-click stubs can never produce an undeployable program.
    #[test]
    fn stub_handlers_assemble_into_a_parseable_deploy() {
        let mut editor = EditorState::default();
        editor
            .docs
            .insert(DocKey::Program(0), Doc::new("Green", GREEN_PROGRAM, true));
        for slot in HandlerSlot::ALL {
            editor.docs.insert(DocKey::Handler(slot), Doc::new(slot.name(), slot.stub(), true));
        }
        let assembled = assembled_source(&editor, 0);
        assert!(assembled.contains("on error:"));
        assert!(assembled.contains("on boot:"));
        let program = pyrite::parse(&assembled, &pyrite::UnlockSet::all())
            .expect("assembled body + stub windows must parse");
        assert_eq!(program.handlers.len(), 5);
    }

    /// Handlers are colony-wide: every color's assembled artifact carries
    /// the same shared blocks. (Starter module removed — this test pins
    /// the handler blocks' exact placement around the body.)
    #[test]
    fn shared_handlers_ride_every_colors_deploy() {
        let mut editor = EditorState::default();
        editor.docs.remove(&DocKey::Module(0));
        editor.docs.insert(DocKey::Program(0), Doc::new("Green", "mine()\n", true));
        editor.docs.insert(DocKey::Program(1), Doc::new("Red", "deposit()\n", true));
        editor.docs.insert(
            DocKey::Handler(HandlerSlot::Error),
            Doc::new("on error", HandlerSlot::Error.stub(), true),
        );
        let green = assembled_source(&editor, 0);
        let red = assembled_source(&editor, 1);
        assert!(green.starts_with("mine()\n") && green.contains("on error:"));
        assert!(red.starts_with("deposit()\n") && red.contains("on error:"));
    }

    /// Removing a handler doc removes its block from the next deploy.
    #[test]
    fn deleting_a_handler_doc_removes_it_from_assembly() {
        let mut editor = EditorState::default();
        editor.docs.remove(&DocKey::Module(0));
        editor.docs.insert(DocKey::Program(0), Doc::new("Green", "mine()\n", true));
        editor.docs.insert(
            DocKey::Handler(HandlerSlot::Boot),
            Doc::new("on boot", HandlerSlot::Boot.stub(), true),
        );
        assert!(assembled_source(&editor, 0).contains("on boot:"));
        editor.docs.remove(&DocKey::Handler(HandlerSlot::Boot));
        assert_eq!(assembled_source(&editor, 0), "mine()\n");
    }

    /// The boot examples are working program+module pairs, one per import
    /// form: each scene deploy matches the editor's first assembled source
    /// byte-for-byte (colors boot "up to date", not "modified"), each
    /// artifact parses with the module's function registered, and the
    /// module blocks peel back off cleanly when seeding program docs from
    /// a deployed source.
    #[test]
    fn starter_module_and_default_programs_round_trip() {
        let mut editor = EditorState::default();
        editor.docs.insert(DocKey::Program(0), Doc::new("Green", GREEN_PROGRAM, true));
        editor.docs.insert(DocKey::Program(1), Doc::new("Red", RED_PROGRAM, true));

        for (color, body) in [(0u8, GREEN_PROGRAM), (1u8, RED_PROGRAM)] {
            let assembled = assembled_source(&editor, color);
            assert_eq!(assembled, starter_deploy_source(color));
            let program = pyrite::parse(&assembled, &pyrite::UnlockSet::all())
                .expect("starter module + default program must parse");
            assert!(program.functions.contains_key("hauling.haul_home"));

            let stripped = assembled
                .strip_prefix(&module_prelude(&editor, body))
                .expect("deployed source starts with the module blocks");
            assert_eq!(stripped, body);
        }
    }

    /// A typo'd import is a deploy error with a pointed message, not a
    /// silent no-op.
    #[test]
    fn importing_a_missing_module_fails_the_deploy() {
        let mut editor = EditorState::default();
        editor
            .docs
            .insert(DocKey::Program(0), Doc::new("Green", "import nosuch\n\nmine()\n", true));
        let err = pyrite::parse(&assembled_source(&editor, 0), &pyrite::UnlockSet::all())
            .expect_err("unknown module must fail the artifact parse");
        assert_eq!(err.kind, pyrite::PyriteErrorKind::UnknownModule("nosuch".into()));
    }

    /// Fresh "+ new" modules are comment-only and unimported: they never
    /// ride a deploy, so any number of untouched templates can't break one.
    #[test]
    fn fresh_module_templates_never_break_deploys() {
        let mut editor = EditorState::default();
        editor.docs.insert(DocKey::Program(0), Doc::new("Green", GREEN_PROGRAM, true));
        editor.docs.insert(DocKey::Module(1), Doc::new("module_1", MODULE_TEMPLATE, true));
        editor.docs.insert(DocKey::Module(2), Doc::new("module_2", MODULE_TEMPLATE, true));
        let program = pyrite::parse(&assembled_source(&editor, 0), &pyrite::UnlockSet::all())
            .expect("two untouched fresh modules must still deploy");
        assert!(program.functions.contains_key("hauling.haul_home"));
    }
}
