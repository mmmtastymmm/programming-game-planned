//! The bot inspector panel.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim::world::Color as BotColor;

use crate::editor::EditorState;
use crate::GameSim;

/// The inspector: live program with the executing line highlighted, VM
/// state, vitals, XP, and logs — the transparency pillar as UI.
pub(crate) fn inspector_ui(
    mut contexts: EguiContexts,
    mut editor: ResMut<EditorState>,
    game: NonSend<GameSim>,
) {
    let Some(bot_id) = editor.selected_bot else { return };
    let Some(ctx) = contexts.try_ctx_mut() else { return };
    let world = &game.0.world;
    let mut open = true;

    egui::SidePanel::right("inspector").exact_width(320.0).show(ctx, |ui| {
        let Some(bot) = world.bots.get(&sim::world::BotId(bot_id)) else {
            // The bot is gone: wreck or total destruction.
            if let Some(wreck) = world.wrecks.get(&sim::world::BotId(bot_id)) {
                ui.heading(format!("Bot {bot_id} — WRECKED"));
                ui.label(format!("at ({}, {})", wreck.pos.x, wreck.pos.y));
                ui.separator();
                ui.strong("recovered logs");
                for line in &wreck.logs {
                    ui.monospace(line);
                }
            } else {
                ui.heading(format!("Bot {bot_id} — DESTROYED"));
                ui.small("No wreck. Check the black boxes and the cloud.");
            }
            if ui.button("close").clicked() {
                open = false;
            }
            return;
        };
        let data = &bot.data;
        let color_name = match data.color {
            BotColor::GREEN => "Green",
            BotColor::RED => "Red",
            _ => "Blue",
        };
        ui.horizontal(|ui| {
            ui.heading(format!("Bot {bot_id} — {color_name}"));
            if ui.button("✕").clicked() {
                open = false;
            }
        });

        // Status line.
        let status = if data.booting.is_some() {
            "booting".to_string()
        } else if data.recall.is_some() {
            "recalled (engine)".to_string()
        } else if data.bump_frozen > 0 {
            format!("bump-frozen ({} ticks)", data.bump_frozen)
        } else if bot.in_handler_init() {
            let ticks = match &bot.data.action {
                Some(sim::world::Action::Wait { ticks_left }) => format!(" — {ticks_left} ticks left"),
                _ => String::new(),
            };
            let signal = bot.handler_name().unwrap_or("?");
            format!("flinching: handler_init() for `{signal}`{ticks}")
        } else if let Some(signal) = bot.handler_name() {
            if bot.in_default_handler() {
                format!("handling: on {signal}: (engine default)")
            } else {
                format!("handling: on {signal}:")
            }
        } else if bot.vm.as_ref().is_some_and(|vm| vm.is_blocked()) {
            match &data.action {
                Some(sim::world::Action::Move { path, .. }) => {
                    format!("moving ({} tiles left)", path.len())
                }
                Some(sim::world::Action::Mine { .. }) => "mining".into(),
                Some(sim::world::Action::Deposit { .. }) => "depositing".into(),
                Some(sim::world::Action::Attack { .. }) => "attacking".into(),
                Some(sim::world::Action::Wait { ticks_left }) => {
                    format!("waiting ({ticks_left} ticks)")
                }
                Some(sim::world::Action::Build { .. }) => "building".into(),
                None => "blocked".into(),
            }
        } else {
            "thinking".to_string()
        };
        ui.label(status);
        ui.monospace(format!(
            "hp {}/{}   cargo {}/{}   at ({}, {})",
            data.hp, data.max_hp, data.cargo, data.cargo_cap, data.pos.x, data.pos.y
        ));
        ui.monospace(format!(
            "xp  mine {}  haul {}  fight {}  build {}",
            data.xp_mining, data.xp_hauling, data.xp_combat, data.xp_building
        ));
        ui.separator();

        // VM state.
        if let Some(vm) = &bot.vm {
            // Budget is stored in centicycles (Q56) — display whole cycles.
            ui.monospace(format!(
                "line {}   budget {}.{:02}   faults {} ({} crashes)",
                vm.current_line(),
                vm.budget() / 100,
                (vm.budget() % 100).abs(),
                vm.fault_count(),
                vm.crash_count()
            ));
            if let Some(fault) = vm.last_fault() {
                ui.colored_label(egui::Color32::from_rgb(240, 120, 100), format!("last: {fault}"));
            }
            ui.separator();

            // While an engine default handler runs, show ITS code with the
            // executing line highlighted — the engine's response is real
            // Pyrite, debuggable like anything else.
            let in_handler = bot.in_signal_handler();
            let in_init = bot.in_handler_init();
            let default_running = bot.in_default_handler();
            // While ANY handler runs, show the full causal chain: the
            // forced entry ritual first, then the handler code.
            if in_handler && let Some(signal) = bot.handler_name() {
                if default_running {
                    ui.strong(format!("engine default: on {signal}:"));
                } else {
                    ui.strong(format!("handler: on {signal}:"));
                }
                // The unskippable entry ritual, as its own visible step.
                if signal != "death" {
                    let ritual = "  ⚙ handler_init()   # forced entry ritual — the flinch";
                    if in_init {
                        ui.label(
                            egui::RichText::new(ritual)
                                .monospace()
                                .background_color(egui::Color32::from_rgb(85, 55, 20))
                                .color(egui::Color32::from_rgb(255, 220, 160)),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(ritual)
                                .monospace()
                                .color(egui::Color32::from_rgb(140, 130, 110)),
                        );
                    }
                }
                // Default handlers: show their own source with the line
                // highlight (suppressed during init — the counter still
                // points at the pre-fault line).
                if default_running
                    && let Some(src) = bot.default_handler_source(if signal == "death" {
                        "death"
                    } else {
                        "signal"
                    })
                {
                    let current = if in_init { 0 } else { vm.current_line() as usize };
                    for (i, line) in src.lines().enumerate() {
                        let n = i + 1;
                        let text = format!("{n:>3} {line}");
                        if n == current {
                            ui.label(
                                egui::RichText::new(text)
                                    .monospace()
                                    .background_color(egui::Color32::from_rgb(70, 45, 25))
                                    .color(egui::Color32::from_rgb(255, 230, 200)),
                            );
                        } else {
                            ui.monospace(text);
                        }
                    }
                }
                ui.separator();
            }

            // The program, current line highlighted (only meaningful while
            // the main program is executing).
            ui.strong("program");
            let source = world
                .color_programs
                .get(&(data.faction, data.color.0))
                .map(|cp| cp.source.clone());
            match source {
                Some(source) => {
                    let current = if default_running || in_init {
                        0
                    } else {
                        vm.current_line() as usize
                    };
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (i, line) in source.lines().enumerate() {
                            let n = i + 1;
                            let text = format!("{n:>3} {line}");
                            if n == current {
                                ui.label(
                                    egui::RichText::new(text)
                                        .monospace()
                                        .background_color(egui::Color32::from_rgb(60, 70, 30))
                                        .color(egui::Color32::from_rgb(230, 255, 200)),
                                );
                            } else {
                                ui.monospace(text);
                            }
                        }
                    });
                }
                None => {
                    ui.small("(no deployed source for this color)");
                }
            }
        }
        ui.separator();

        // Handler coverage: player-installed ones point at their line;
        // the rest show the engine default in plain words.
        ui.strong("handlers");
        ui.small(format!(
            "every entry begins: handler_init() — wait {} ticks (engine, unskippable)",
            game.0.tuning.handler_init_ticks
        ));
        let tuning = &game.0.tuning;
        for (signal, line) in bot.handler_summary() {
            match line {
                Some(n) => {
                    ui.monospace(format!("on {signal}: — line {n}"));
                }
                None => match bot.default_handler_source(signal) {
                    // The engine default IS code — show it.
                    Some(src) => {
                        let code = src.trim_end();
                        let note = match signal {
                            "error" => format!("  (+crash: -{} hp)", tuning.fault_damage),
                            "bump" | "bumped" => format!("  (-{} hp)", tuning.bump_damage),
                            _ => String::new(),
                        };
                        ui.monospace(format!("on {signal}: (engine) {code}{note}"));
                    }
                    None => {
                        ui.monospace(format!("on {signal}: (engine) nothing"));
                    }
                },
            }
        }
        ui.separator();

        ui.strong("local logs");
        if data.log_buf.is_empty() {
            ui.small("(empty)");
        }
        for line in &data.log_buf {
            ui.monospace(line);
        }
    });

    if !open {
        editor.selected_bot = None;
    }
}
