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
    mut game: NonSendMut<GameSim>,
) {
    let Some(bot_id) = editor.selected_bot else { return };
    let Some(ctx) = contexts.try_ctx_mut() else { return };
    // Station orders queued from the catalog UI, applied after the panel
    // borrow ends (commands are the only mutation path).
    let mut queued: Vec<sim::sim::Command> = Vec::new();
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
                for (level, line) in &wreck.logs {
                    ui.monospace(leveled_line(*level, line));
                }
                if !wreck.env.is_empty() {
                    ui.separator();
                    ui.strong("env snapshot");
                    for (key, value) in &wreck.env {
                        ui.monospace(format!("{key} = {value}"));
                    }
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
            "hp {}/{}   cargo {:.1}/{:.1}   at ({}, {})",
            data.hp,
            data.max_hp,
            data.cargo_total() as f32 / 10.0,
            data.cargo_cap as f32 / 10.0,
            data.pos.x,
            data.pos.y
        ));
        ui.monospace(format!(
            "xp  mine {}  haul {}  fight {}  build {}",
            data.xp_mining, data.xp_hauling, data.xp_combat, data.xp_building
        ));
        ui.separator();

        // Hardware + the Upgrade Station catalog (M5): queue an order
        // here; the PROGRAM must bring the bot to a pad (docs/03).
        let stats = &game.0.stats;
        egui::CollapsingHeader::new("hardware & upgrades").show(ui, |ui| {
            if data.pad_sit {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 200, 240),
                    "on the pad — sitting an upgrade",
                );
            }
            let installed: Vec<String> = data
                .upgrades
                .iter()
                .filter_map(|&u| stats.upgrades.get(u as usize).map(|s| s.name.clone()))
                .collect();
            let slotted: Vec<String> = data
                .modules
                .iter()
                .filter_map(|&m| stats.modules.get(m as usize).map(|s| s.name.clone()))
                .collect();
            ui.monospace(format!(
                "compute: {}",
                if installed.is_empty() { "stock".into() } else { installed.join(", ") }
            ));
            ui.monospace(format!(
                "slots {}/{}: {}",
                slotted.len(),
                data.module_slots,
                if slotted.is_empty() { "empty".into() } else { slotted.join(", ") }
            ));
            if !data.upgrade_queue.is_empty() {
                let names: Vec<&str> = data
                    .upgrade_queue
                    .iter()
                    .map(|o| match o {
                        sim::world::UpgradeOrder::Compute(idx) => {
                            stats.upgrades[*idx as usize].name.as_str()
                        }
                        sim::world::UpgradeOrder::Module { idx, .. } => {
                            stats.modules[*idx as usize].name.as_str()
                        }
                    })
                    .collect();
                ui.monospace(format!("queued: {}", names.join(", ")));
            }
            ui.separator();
            let price = |cost: &[(sim::resources::Resource, u32)]| -> String {
                cost.iter()
                    .map(|(k, units)| format!("{units} {}", k.name()))
                    .collect::<Vec<_>>()
                    .join(" + ")
            };
            // The coolant figure comes from stats.ron — the UI must never
            // misprice what the sim charges.
            ui.small(format!(
                "compute (coolant: {} Water at the pad)",
                stats.coolant_water_deci as f32 / 10.0
            ));
            for spec in &stats.upgrades {
                ui.horizontal(|ui| {
                    if ui.small_button(&spec.name).clicked() {
                        queued.push(sim::sim::Command::QueueUpgrade {
                            bot: sim::world::BotId(bot_id),
                            order: spec.name.clone(),
                            replace: None,
                        });
                    }
                    ui.small(price(&spec.cost));
                });
            }
            ui.small("modules (a swap destroys the old part — no refund)");
            let slots_full = data.modules.len() >= data.module_slots as usize;
            for spec in &stats.modules {
                ui.horizontal(|ui| {
                    let label = if slots_full {
                        format!("{} (swap slot 1)", spec.name)
                    } else {
                        spec.name.clone()
                    };
                    if ui.small_button(label).clicked() {
                        queued.push(sim::sim::Command::QueueUpgrade {
                            bot: sim::world::BotId(bot_id),
                            order: spec.name.clone(),
                            replace: slots_full.then_some(0),
                        });
                    }
                    ui.small(price(&spec.cost));
                });
            }
        });
        ui.separator();

        // VM state.
        if let Some(vm) = &bot.vm {
            // The budget meter scales to the DERIVED bank_cap (M5, Q75/Q82)
            // — the most expensive effective op this bot could pay here.
            // Budget is stored in centicycles (Q56); display whole cycles.
            let bank_cap_centi = game.0.costs.bank_cap as f32 * 100.0;
            let fraction = (vm.budget().max(0) as f32 / bank_cap_centi).min(1.0);
            ui.add(
                egui::ProgressBar::new(fraction)
                    .text(format!(
                        "budget {}.{:02} / bank cap {}",
                        vm.budget() / 100,
                        (vm.budget() % 100).abs(),
                        game.0.costs.bank_cap
                    ))
                    .desired_height(14.0),
            );
            ui.monospace(format!(
                "line {}   faults {} ({} crashes)",
                vm.current_line(),
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
                // The forced prologue, as its own visible locked line
                // (boot's prologue is the forced upload, not the flinch).
                if signal != "boot" {
                    let ritual = "  ⚙ handler_init()   # forced prologue — the flinch";
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
                // Factory windows: show their own source with the line
                // highlight (suppressed during init — the counter still
                // points at the pre-fault line).
                if default_running
                    && let Some(src) = bot.default_handler_source(signal)
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
        for (level, line) in &data.log_buf {
            ui.monospace(leveled_line(*level, line));
        }
    });

    // Catalog clicks become ordinary lockstep commands.
    for command in queued {
        let _ = game.0.apply(&command);
    }

    if !open {
        editor.selected_bot = None;
    }
}

/// One log line prefixed by its severity name — the names come from the
/// sim's canonical ladder (the same list bound as VM constants), so the
/// two can't drift.
fn leveled_line(level: u8, line: &str) -> String {
    let name = sim::world::LEVEL_NAMES
        .get(level as usize)
        .copied()
        .unwrap_or("error");
    format!("[{name}] {line}")
}
