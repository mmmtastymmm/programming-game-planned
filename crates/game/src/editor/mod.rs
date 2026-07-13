//! The in-game Pyrite editor: side-panel UI, live syntax highlighting,
//! parse-error squiggles, kind-argument completion, and hover docs.

mod complete;
mod highlight;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim::sim::Command;
use sim::world::{BlueprintKind, Color as BotColor, PrinterState};
use sim::map::OverlayKind;
use sim::TilePos;
use std::collections::HashMap;

use crate::tools::{build_icon, same_item, ToolKind, BUILD_CATEGORIES};
use crate::GameSim;
use complete::{completion_at, insert_kind, word_at};
use highlight::{error_byte_range, highlight_pyrite, HL_ERROR, HL_FUNCTION, HL_VARIABLE};

/// The default colony program: service any blueprint first, then mine.
/// (Uses `if` — the dev sandbox runs with all constructs unlocked; the
/// doc's true Tier-0 starter is the four mining lines alone.)
pub(crate) const DEFAULT_PROGRAM: &str = "\
if exists(blueprint):
    move_to(closest(blueprint).expect())
    build()
move_to(closest(ore).expect())
mine()
move_to(closest(depot).expect())
deposit()
";

#[derive(Resource)]
pub(crate) struct EditorState {
    pub(crate) code: String,
    pub(crate) status: String,
    pub(crate) status_ok: bool,
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
    /// Caret position (char index) for kind-argument completion. Cached
    /// across frames: on the frame a popup entry is clicked, the TextEdit
    /// has already lost focus (no live cursor), so insertion needs this.
    pub(crate) completion_cursor: Option<usize>,
    /// Row highlighted in the completion popup (↑↓ moves, Enter accepts).
    pub(crate) completion_selected: usize,
    /// Context dismissed with Esc — (partial_start, partial). The popup
    /// stays closed until typing changes the partial word.
    pub(crate) completion_muted: Option<(usize, String)>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            code: DEFAULT_PROGRAM.to_string(),
            status: "ready".into(),
            status_ok: true,
            selected_build: None,
            last_paint_tile: None,
            selected_bot: None,
            press_pos: None,
            paused: false,
            speed: 1.0,
            build_category: 0,
            icons: HashMap::new(),
            completion_cursor: None,
            completion_selected: 0,
            completion_muted: None,
        }
    }
}

pub(crate) fn editor_ui(
    mut contexts: EguiContexts,
    mut game: NonSendMut<GameSim>,
    mut editor: ResMut<EditorState>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else { return };

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
                    ToolKind::Building(BlueprintKind::Bridge) => game.0.tuning.bridge_cost_ore,
                    ToolKind::Overlay(Some(_)) => game.0.tuning.overlay_cost_ore,
                    ToolKind::Overlay(None) | ToolKind::Paint(_) | ToolKind::Kill => 0,
                };
                let affordable = game.0.world.stockpile_ore >= cost;
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
        ui.heading("Pyrite");
        let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
            // Live parse of the in-progress text: the error location gets a
            // red squiggle. (Programs are tiny; parsing per relayout is fine.)
            let squiggle = pyrite::parse(text, &pyrite::UnlockSet::all())
                .err()
                .map(|e| error_byte_range(text, e.line, e.col));
            let mut job = highlight_pyrite(
                text,
                egui::TextStyle::Monospace.resolve(ui.style()),
                squiggle,
            );
            job.wrap.max_width = wrap_width;
            ui.fonts(|fonts| fonts.layout_job(job))
        };
        let editor_id = egui::Id::new("pyrite_editor");

        // Completion keys must be taken BEFORE the TextEdit runs — it would
        // otherwise eat ArrowUp/Down (caret moves) and Enter (newline).
        let mut completion = editor
            .completion_cursor
            .and_then(|cursor| completion_at(&editor.code, cursor))
            .filter(|c| editor.completion_muted != Some((c.partial_start, c.partial.clone())));
        if let Some(c) = &completion
            && ui.ctx().memory(|m| m.has_focus(editor_id))
        {
            let n = c.suggestions.len();
            editor.completion_selected %= n;
            let (mut down, mut up, mut accept, mut dismiss) = (false, false, false, false);
            ui.input_mut(|i| {
                down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                accept = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                dismiss = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
            if down {
                editor.completion_selected = (editor.completion_selected + 1) % n;
            }
            if up {
                editor.completion_selected = (editor.completion_selected + n - 1) % n;
            }
            if dismiss {
                // Mute this exact context so the popup doesn't reopen until
                // the user types (which changes the partial word).
                editor.completion_muted = Some((c.partial_start, c.partial.clone()));
                editor.completion_cursor = None;
                completion = None;
            } else if accept {
                let kind = c.suggestions[editor.completion_selected];
                let after =
                    insert_kind(&mut editor.code, ui.ctx(), editor_id, c.partial_start, c.cursor, kind);
                editor.completion_cursor = Some(after);
                completion = None;
            }
        }

        let output = egui::TextEdit::multiline(&mut editor.code)
            .id(editor_id)
            .font(egui::TextStyle::Monospace)
            .desired_rows(14)
            .desired_width(f32::INFINITY)
            .layouter(&mut layouter)
            .show(ui);

        // Kind-argument completion popup: with the caret in the argument
        // slot of `closest(` / `exists(`, list the kinds — ↑↓ + Enter or a
        // click inserts one. The caret is cached in EditorState because on
        // the frame a popup entry is clicked the TextEdit has lost focus and
        // reports no cursor — the popup must persist through that frame for
        // the click to land.
        if output.response.has_focus() {
            editor.completion_cursor = output.cursor_range.map(|c| c.primary.ccursor.index);
            // Re-derive from this frame's text so the popup tracks typing.
            completion = editor
                .completion_cursor
                .and_then(|cursor| completion_at(&editor.code, cursor))
                .filter(|c| editor.completion_muted != Some((c.partial_start, c.partial.clone())));
        }
        if let Some(c) = completion {
            editor.completion_selected %= c.suggestions.len();
            let caret = output
                .galley
                .pos_from_cursor(&output.galley.from_ccursor(egui::text::CCursor::new(c.cursor)));
            let pos = output.galley_pos + caret.left_bottom().to_vec2() + egui::vec2(0.0, 4.0);
            let area = egui::Area::new(editor_id.with("kind_complete"))
                .fixed_pos(pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        // Plain words: egui's default font has no ↑/↓ glyphs.
                        ui.small(format!("{} takes a kind — arrows + Enter", c.func));
                        for (i, kind) in c.suggestions.iter().enumerate() {
                            let label = egui::RichText::new(*kind).monospace().color(HL_VARIABLE);
                            if ui
                                .selectable_label(i == editor.completion_selected, label)
                                .clicked()
                            {
                                let after = insert_kind(
                                    &mut editor.code,
                                    ui.ctx(),
                                    editor_id,
                                    c.partial_start,
                                    c.cursor,
                                    kind,
                                );
                                editor.completion_cursor = Some(after);
                            }
                        }
                    });
                });
            // Editor unfocused and pointer not on the popup: dismiss, so it
            // doesn't linger after clicking elsewhere.
            if !output.response.has_focus() && !area.response.contains_pointer() {
                editor.completion_cursor = None;
            }
        }

        // Hover docs: an identifier under the pointer that names a builtin
        // (or a kind constant) gets a tooltip with signature, summary, and
        // its cost from the live cost table.
        if let Some(pointer) = output.response.hover_pos() {
            let rel = pointer - output.galley_pos;
            let cursor = output.galley.cursor_from_pos(rel);
            let caret = output.galley.pos_from_cursor(&cursor);
            // cursor_from_pos snaps to the nearest column — require the
            // pointer to actually be on the glyph, not in the blank space
            // right of the line.
            let on_text = caret
                .expand2(egui::vec2(8.0, 2.0))
                .contains(egui::pos2(rel.x, rel.y));
            if on_text && let Some(word) = word_at(&editor.code, cursor.ccursor.index) {
                let costs = &game.0.costs;
                if let Some(doc) = sim::host::builtin_doc(&word) {
                    // upload_crash_dump is costed by its dedicated field.
                    let cost = if doc.name == "upload_crash_dump" {
                        costs.crash_dump
                    } else {
                        costs.builtin_cost(doc.name)
                    };
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        ui.layer_id(),
                        editor_id.with("hover_doc"),
                        |ui| {
                            ui.monospace(egui::RichText::new(doc.signature).color(HL_FUNCTION));
                            ui.label(doc.summary);
                            ui.small(format!("cost: {cost} cycles{}", doc.cost_note));
                        },
                    );
                } else if sim::host::KINDS.contains(&word.as_str()) {
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        ui.layer_id(),
                        editor_id.with("hover_doc"),
                        |ui| {
                            ui.monospace(
                                egui::RichText::new(format!("{word} — entity kind")).color(HL_VARIABLE),
                            );
                            ui.label("A kind constant: pass it to closest() or exists().");
                        },
                    );
                }
            }
        }

        // Live parse status, so squiggles come with words.
        if let Err(e) = pyrite::parse(&editor.code, &pyrite::UnlockSet::all()) {
            ui.colored_label(HL_ERROR, format!("parse error: {e}"));
        }
        ui.horizontal(|ui| {
            for (label, color) in [("Deploy Green", BotColor::GREEN), ("Deploy Red", BotColor::RED)] {
                if ui.button(label).clicked() {
                    let cmd = Command::DeployProgram {
                        faction: 0,
                        color,
                        source: editor.code.clone(),
                    };
                    match game.0.apply(&cmd) {
                        Ok(_) => {
                            editor.status = format!("deployed to {label:?}");
                            editor.status_ok = true;
                        }
                        Err(e) => {
                            editor.status = e.to_string();
                            editor.status_ok = false;
                        }
                    }
                }
            }
        });
        let status_color = if editor.status_ok {
            egui::Color32::from_rgb(120, 220, 120)
        } else {
            egui::Color32::from_rgb(240, 120, 100)
        };
        ui.colored_label(status_color, &editor.status);
        ui.separator();

        ui.heading("Printers");
        let printer_ids: Vec<_> = game.0.world.printers.keys().copied().collect();
        let repair_cost = game.0.tuning.repair_cost_ore;
        for pid in printer_ids {
            let (color, state, mut desired) = {
                let p = &game.0.world.printers[&pid];
                (p.color, p.state, p.desired_max)
            };
            let name = match color {
                BotColor::GREEN => "Green",
                BotColor::RED => "Red",
                _ => "Other",
            };
            ui.horizontal(|ui| {
                ui.label(name);
                match state {
                    PrinterState::Ruined => {
                        let affordable = game.0.world.stockpile_ore >= repair_cost;
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

    // Bar across the top of the world view (added after the editor panel,
    // so it spans only the world). Colony counters on the left; time
    // controls in the top right. The cloud stream stays in the side panel.
    egui::TopBottomPanel::top("world_bar").exact_height(28.0).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            let (tick, ore, bots, wrecks, cloud) = {
                let w = &game.0.world;
                (w.tick, w.stockpile_ore, w.bots.len(), w.wrecks.len(), w.archive.len())
            };
            for text in [
                format!("tick {tick}"),
                format!("ore {ore}"),
                format!("bots {bots}"),
                format!("wrecks {wrecks}"),
                format!("cloud {cloud}"),
            ] {
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
