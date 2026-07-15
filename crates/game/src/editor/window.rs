//! Editor documents and the code-editing widget shared by every floating
//! window: per-color program files and library modules (docs/01 "Modules &
//! the Program Library"), each with live highlighting, parse squiggles,
//! kind-argument completion, and hover docs.

use bevy_egui::egui;

use super::complete::{completion_at, insert_kind, word_at};
use super::highlight::{error_byte_range, highlight_pyrite, HL_ERROR, HL_FUNCTION, HL_VARIABLE};

/// A file in the colony file viewer. Programs are keyed by their color
/// (faction 0 — the viewer's colony); modules are library files that ride
/// a deploy as generated `module <name>:` blocks when a program imports
/// them (docs/01 "imports resolve at deploy").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DocKey {
    /// A color slot's main-loop program body, keyed by `Color.0`.
    Program(u8),
    /// One COLONY-WIDE signal handler block: every bot, every color, runs
    /// the same handler. Stored as its own document (the file viewer's
    /// Handlers folder) and assembled after each color's body at deploy.
    Handler(HandlerSlot),
    /// A library module, keyed by a stable creation id.
    Module(u32),
}

/// The per-signal WINDOW files (the file viewer's Handlers folder) —
/// docs/01's five editable windows, one document per signal. Each file is
/// exactly that window's contents; the assembler emits it as its own
/// `on <signal>:` block (the forced prologue/epilogue never exist in
/// source — they're engine-owned, rendered as locked phantom lines).
/// Abort and recall have no file: fully engine-reserved, zero-size
/// windows. Ord = assembly order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum HandlerSlot {
    Error,
    Hurt,
    Bump,
    Bumped,
    Boot,
}

impl HandlerSlot {
    pub(crate) const ALL: [HandlerSlot; 5] = [
        HandlerSlot::Error,
        HandlerSlot::Hurt,
        HandlerSlot::Bump,
        HandlerSlot::Bumped,
        HandlerSlot::Boot,
    ];

    /// The bare signal name (block header = `on <signal>:`).
    pub(crate) fn signal(self) -> &'static str {
        match self {
            HandlerSlot::Error => "error",
            HandlerSlot::Hurt => "hurt",
            HandlerSlot::Bump => "bump",
            HandlerSlot::Bumped => "bumped",
            HandlerSlot::Boot => "boot",
        }
    }

    /// Display name (also the doc name in the file viewer).
    pub(crate) fn name(self) -> &'static str {
        match self {
            HandlerSlot::Error => "on error",
            HandlerSlot::Hurt => "on hurt",
            HandlerSlot::Bump => "on bump",
            HandlerSlot::Bumped => "on bumped",
            HandlerSlot::Boot => "on boot",
        }
    }

    /// Starter body for a freshly added window file (written at column 0;
    /// the assembler indents it into its block) — the factory contents
    /// where they exist, an idiomatic line otherwise.
    pub(crate) fn stub(self) -> &'static str {
        match self {
            HandlerSlot::Error => "upload_crash_dump()\n",
            HandlerSlot::Hurt => "drop_cargo()\n",
            HandlerSlot::Bump => "wait(35)\n",
            HandlerSlot::Bumped => "wait(15)\n",
            HandlerSlot::Boot => "setenv(hurt_line, 50)\n",
        }
    }

    fn from_signal(name: &str) -> Option<HandlerSlot> {
        Self::ALL.into_iter().find(|s| s.signal() == name)
    }
}

/// Prefix every non-blank line with `indent` (blank lines stay bare —
/// matching what the splitter reproduces).
pub(crate) fn indent_lines(body: &str, indent: &str) -> String {
    let mut out = String::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            out.push('\n');
        } else {
            out.push_str(indent);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Strip up to `indent` leading spaces from every line.
fn dedent_lines(text: &str, indent: usize) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            out.push('\n');
        } else if line.len() >= indent && line[..indent].bytes().all(|b| b == b' ') {
            out.push_str(&line[indent..]);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The deployable source for one color: the body, then each written
/// window as its own `on <signal>:` block, in slot order.
/// Empty/whitespace-only files are treated as absent (the reserved
/// template still runs with factory contents — nothing is unhandled,
/// just uncustomized, docs/06).
pub(crate) fn assemble_color(body: &str, files: &std::collections::BTreeMap<HandlerSlot, String>) -> String {
    let mut out = body.to_string();
    for slot in HandlerSlot::ALL {
        let Some(text) = files.get(&slot).filter(|t| !t.trim().is_empty()) else { continue };
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("on {}:\n", slot.signal()));
        out.push_str(&indent_lines(text, "    "));
    }
    out
}

/// Split a deployed source into (body, per-signal window files) at the
/// text level: a top-level `on <signal>:` line starts a block owning
/// every following blank/indented line; the block body (dedented) becomes
/// that signal's file. Anything else stays in the body. Sources the
/// editor itself assembled round-trip byte-exactly.
pub(crate) fn split_source(source: &str) -> (String, std::collections::BTreeMap<HandlerSlot, String>) {
    let header_slot = |line: &str| -> Option<HandlerSlot> {
        let rest = line.strip_prefix("on ").or_else(|| line.strip_prefix("on\t"))?;
        let name = rest.trim_start().trim_end().strip_suffix(':')?;
        HandlerSlot::from_signal(name.trim_end())
    };

    let mut body = String::new();
    let mut files = std::collections::BTreeMap::new();
    let mut current: Option<(HandlerSlot, String)> = None;
    let flush = |current: &mut Option<(HandlerSlot, String)>,
                     files: &mut std::collections::BTreeMap<HandlerSlot, String>| {
        let Some((slot, text)) = current.take() else { return };
        files.entry(slot).or_insert_with(|| dedent_lines(&text, 4));
    };
    for line in source.split_inclusive('\n') {
        let top_level = !line.starts_with(' ') && !line.starts_with('\t') && !line.trim().is_empty();
        if top_level {
            flush(&mut current, &mut files);
            if let Some(slot) = header_slot(line) {
                current = Some((slot, String::new()));
                continue;
            }
            body.push_str(line);
        } else {
            match &mut current {
                Some((_, text)) => text.push_str(line),
                None => body.push_str(line),
            }
        }
    }
    flush(&mut current, &mut files);
    (body, files)
}

/// One open-able document: buffer, deploy status, and the per-window
/// completion state (each window owns its own popup).
pub(crate) struct Doc {
    pub(crate) name: String,
    pub(crate) code: String,
    /// Whether this doc's floating window is open.
    pub(crate) open: bool,
    pub(crate) status: String,
    pub(crate) status_ok: bool,
    /// One-shot caret placement (char index) requested by the file viewer.
    pub(crate) pending_caret: Option<usize>,
    /// Caret position (char index) for kind-argument completion. Cached
    /// across frames: on the frame a popup entry is clicked, the TextEdit
    /// has already lost focus (no live cursor), so insertion needs this.
    completion_cursor: Option<usize>,
    /// Row highlighted in the completion popup (↑↓ moves, Enter accepts).
    completion_selected: usize,
    /// Context dismissed with Esc — (partial_start, partial). The popup
    /// stays closed until typing changes the partial word.
    completion_muted: Option<(usize, String)>,
}

impl Doc {
    pub(crate) fn new(name: impl Into<String>, code: impl Into<String>, open: bool) -> Self {
        Self {
            name: name.into(),
            code: code.into(),
            open,
            status: "ready".into(),
            status_ok: true,
            pending_caret: None,
            completion_cursor: None,
            completion_selected: 0,
            completion_muted: None,
        }
    }
}

/// The file viewer's children for one document: every handler and function
/// in the buffer as (label, 1-based line, docstring), in source order.
/// Unparseable buffers get no outline (the window shows the squiggle).
pub(crate) fn doc_outline(code: &str) -> Vec<(String, u32, Option<String>)> {
    let Ok(prog) = pyrite::parse(code, &pyrite::UnlockSet::all()) else {
        return Vec::new();
    };
    let mut items: Vec<(String, u32, Option<String>)> = prog
        .handlers
        .values()
        .map(|h| {
            // The cap meter: worst-case instructions vs the signal's cap.
            let usage = pyrite::analysis::window_usage(&prog, editor_costs(), h.kind)
                .map(|(worst, cap)| format!("  [{worst}/{cap} instr]"))
                .unwrap_or_default();
            (format!("on {}:{usage}", h.kind.name()), h.line, None)
        })
        .collect();
    items.extend(
        prog.functions.values().map(|f| (format!("def {}()", f.name), f.line, f.doc.clone())),
    );
    items.sort_by_key(|(_, line, _)| *line);
    items
}

/// Char index of the start of 1-based `line` — for caret jumps from the
/// file viewer's handler/function entries.
pub(crate) fn line_start_char(code: &str, line: u32) -> usize {
    let mut chars = 0usize;
    for (i, l) in code.lines().enumerate() {
        if i + 1 == line as usize {
            return chars;
        }
        chars += l.chars().count() + 1; // + the newline
    }
    code.chars().count()
}

/// A live parse of a document as it will deploy: `prefix` (the module
/// library section for program docs, empty otherwise) is prepended before
/// parsing, and error positions are mapped back into the doc's own lines.
enum LiveParse {
    Ok,
    /// Error inside the doc's text — line/col are doc coordinates.
    Local(pyrite::PyriteError),
    /// Error inside the prepended module library (broken library code).
    Library(pyrite::PyriteError),
}

/// The live cost table for editor-side checks (parsed once — same data
/// the sim prices with, so the editor can't drift).
pub(crate) fn editor_costs() -> &'static pyrite::CostTable {
    static COSTS: std::sync::OnceLock<pyrite::CostTable> = std::sync::OnceLock::new();
    COSTS.get_or_init(pyrite::CostTable::default)
}

/// Live-parse a doc as it will deploy. `window` is the signal name when
/// the doc IS a handler-window file: its text is then wrapped in the same
/// `on <signal>:` block the assembler emits, so the window rules (loop
/// ban, signal-safe gate, caps) squiggle live instead of only failing at
/// deploy — and error positions map back into the bare window text.
fn live_parse(prefix: &str, window: Option<&'static str>, text: &str) -> LiveParse {
    let (assembled, skip_lines, indent) = match window {
        Some(signal) => {
            (format!("{prefix}on {signal}:\n{}", indent_lines(text, "    ")), 1u32, 4u32)
        }
        None => (format!("{prefix}{text}"), 0, 0),
    };
    let result = pyrite::parse(&assembled, &pyrite::UnlockSet::all())
        // A clean parse still has to clear the deploy-time window analysis
        // (caps, signal safety, loops/recursion) — same rejection, live.
        .and_then(|program| pyrite::check_windows(&program, editor_costs()));
    match result {
        Ok(()) => LiveParse::Ok,
        Err(mut e) => {
            let prefix_lines = prefix.lines().count() as u32 + skip_lines;
            if e.line > prefix_lines {
                e.line -= prefix_lines;
                e.col = e.col.saturating_sub(indent).max(1);
                LiveParse::Local(e)
            } else {
                LiveParse::Library(e)
            }
        }
    }
}

/// Resolve a hovered identifier to a user `def` in this doc's deploy
/// context (`prefix` + buffer): a local def, a `from`-import alias, or —
/// for the bare tail of a qualified `module.fn(...)` call — a function of
/// any module the source carries. Unparseable buffers resolve nothing.
fn user_function_at(prefix: &str, text: &str, word: &str) -> Option<pyrite::ast::Function> {
    let program = pyrite::parse(&format!("{prefix}{text}"), &pyrite::UnlockSet::all()).ok()?;
    if let Some(f) = program.functions.get(word) {
        return Some(f.clone());
    }
    if let Some(qualified) = program.aliases.get(word) {
        return program.functions.get(qualified).cloned();
    }
    program
        .modules
        .iter()
        .find_map(|m| program.functions.get(&format!("{m}.{word}")).cloned())
}

/// The full-featured Pyrite text editor: syntax highlight, error squiggle,
/// kind completion popup, hover docs, and live parse status. Extracted so
/// every floating window gets the identical experience. `prefix` is
/// invisible parse context prepended to the buffer — program windows pass
/// their module-library section so imports resolve exactly as they will
/// at deploy.
pub(crate) fn code_editor(
    ui: &mut egui::Ui,
    editor_id: egui::Id,
    doc: &mut Doc,
    sim: &sim::sim::Sim,
    rows: usize,
    prefix: &str,
    // The signal name when this doc is a handler-window file — live
    // parsing then wraps the text in its `on <signal>:` block so the
    // window rules squiggle here, not first at deploy.
    window: Option<&'static str>,
) {
    // A caret jump requested by the file viewer lands before the TextEdit
    // runs, using the same state-poke pattern as completion insertion.
    if let Some(caret) = doc.pending_caret.take() {
        let mut state =
            egui::text_edit::TextEditState::load(ui.ctx(), editor_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(caret))));
        state.store(ui.ctx(), editor_id);
        ui.ctx().memory_mut(|m| m.request_focus(editor_id));
    }

    let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
        // Live parse of the in-progress text: the error location gets a
        // red squiggle. (Programs are tiny; parsing per relayout is fine.)
        // Library errors have no position in this buffer — status only.
        let squiggle = match live_parse(prefix, window, text) {
            LiveParse::Local(e) => Some(error_byte_range(text, e.line, e.col)),
            LiveParse::Ok | LiveParse::Library(_) => None,
        };
        let mut job = highlight_pyrite(
            text,
            egui::TextStyle::Monospace.resolve(ui.style()),
            squiggle,
        );
        job.wrap.max_width = wrap_width;
        ui.fonts(|fonts| fonts.layout_job(job))
    };

    // Completion keys must be taken BEFORE the TextEdit runs — it would
    // otherwise eat ArrowUp/Down (caret moves) and Enter (newline).
    let mut completion = doc
        .completion_cursor
        .and_then(|cursor| completion_at(&doc.code, cursor))
        .filter(|c| doc.completion_muted != Some((c.partial_start, c.partial.clone())));
    if let Some(c) = &completion
        && ui.ctx().memory(|m| m.has_focus(editor_id))
    {
        let n = c.suggestions.len();
        doc.completion_selected %= n;
        let (mut down, mut up, mut accept, mut dismiss) = (false, false, false, false);
        ui.input_mut(|i| {
            down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
            up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
            accept = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
            dismiss = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        });
        if down {
            doc.completion_selected = (doc.completion_selected + 1) % n;
        }
        if up {
            doc.completion_selected = (doc.completion_selected + n - 1) % n;
        }
        if dismiss {
            // Mute this exact context so the popup doesn't reopen until
            // the user types (which changes the partial word).
            doc.completion_muted = Some((c.partial_start, c.partial.clone()));
            doc.completion_cursor = None;
            completion = None;
        } else if accept {
            let kind = c.suggestions[doc.completion_selected];
            let after =
                insert_kind(&mut doc.code, ui.ctx(), editor_id, c.partial_start, c.cursor, kind);
            doc.completion_cursor = Some(after);
            completion = None;
        }
    }

    // No focus ring around the code box — the bright selection stroke on
    // the whole TextEdit reads as "something is selected" when it's just
    // focus. Restored right after, so buttons keep their outlines.
    let ring = ui.visuals().selection.stroke;
    ui.visuals_mut().selection.stroke = egui::Stroke::NONE;
    let output = egui::TextEdit::multiline(&mut doc.code)
        .id(editor_id)
        .font(egui::TextStyle::Monospace)
        .desired_rows(rows)
        .desired_width(f32::INFINITY)
        .layouter(&mut layouter)
        .show(ui);
    ui.visuals_mut().selection.stroke = ring;

    // Kind-argument completion popup: with the caret in the argument
    // slot of `closest(` / `exists(`, list the kinds — ↑↓ + Enter or a
    // click inserts one. The caret is cached in the Doc because on the
    // frame a popup entry is clicked the TextEdit has lost focus and
    // reports no cursor — the popup must persist through that frame for
    // the click to land.
    if output.response.has_focus() {
        doc.completion_cursor = output.cursor_range.map(|c| c.primary.ccursor.index);
        // Re-derive from this frame's text so the popup tracks typing.
        completion = doc
            .completion_cursor
            .and_then(|cursor| completion_at(&doc.code, cursor))
            .filter(|c| doc.completion_muted != Some((c.partial_start, c.partial.clone())));
    }
    if let Some(c) = completion {
        doc.completion_selected %= c.suggestions.len();
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
                            .selectable_label(i == doc.completion_selected, label)
                            .clicked()
                        {
                            let after = insert_kind(
                                &mut doc.code,
                                ui.ctx(),
                                editor_id,
                                c.partial_start,
                                c.cursor,
                                kind,
                            );
                            doc.completion_cursor = Some(after);
                        }
                    }
                });
            });
        // Editor unfocused and pointer not on the popup: dismiss, so it
        // doesn't linger after clicking elsewhere.
        if !output.response.has_focus() && !area.response.contains_pointer() {
            doc.completion_cursor = None;
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
        if on_text && let Some(word) = word_at(&doc.code, cursor.ccursor.index) {
            let costs = &sim.costs;
            if let Some(doc_entry) = sim::host::builtin_doc(costs, &word) {
                let cost = costs.cost_display(&word);
                egui::show_tooltip_at_pointer(
                    ui.ctx(),
                    ui.layer_id(),
                    editor_id.with("hover_doc"),
                    |ui| {
                        ui.monospace(egui::RichText::new(&doc_entry.signature).color(HL_FUNCTION));
                        ui.label(&doc_entry.summary);
                        ui.small(format!("cost: {cost} cycles{}", doc_entry.cost_note));
                        // The signal-safe gate, greyed vs warm (docs/01):
                        // windows may only call safe functions.
                        if doc_entry.signal_safe {
                            ui.small(
                                egui::RichText::new("signal-safe: callable from handler windows")
                                    .color(egui::Color32::from_rgb(140, 170, 140)),
                            );
                        } else {
                            ui.small(
                                egui::RichText::new(
                                    "NOT signal-safe — greyed out inside handler windows",
                                )
                                .color(egui::Color32::from_rgb(170, 130, 110)),
                            );
                        }
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
            } else if let Some(func) = user_function_at(prefix, &doc.code, &word) {
                // A user def — local, from-imported, or a module function
                // called qualified. Its docstring is the documentation.
                egui::show_tooltip_at_pointer(
                    ui.ctx(),
                    ui.layer_id(),
                    editor_id.with("hover_doc"),
                    |ui| {
                        ui.monospace(
                            egui::RichText::new(format!(
                                "{}({})",
                                func.name,
                                func.params
                                    .iter()
                                    .map(|p| p.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                            .color(HL_FUNCTION),
                        );
                        match &func.doc {
                            Some(d) => {
                                ui.label(d);
                            }
                            None => {
                                ui.label(
                                    egui::RichText::new(
                                        "no docstring — a \"\"\"…\"\"\" first line documents it",
                                    )
                                    .weak(),
                                );
                            }
                        }
                        ui.small(format!("cost: {} cycles + body", costs.user_call));
                    },
                );
            }
        }
    }

    // Live parse status, so squiggles come with words.
    match live_parse(prefix, window, &doc.code) {
        LiveParse::Ok => {}
        LiveParse::Local(e) => {
            ui.colored_label(HL_ERROR, format!("parse error: {e}"));
        }
        LiveParse::Library(e) => {
            ui.colored_label(HL_ERROR, format!("module library error: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_start_char_walks_lines() {
        let src = "abc\ndef\nghi";
        assert_eq!(line_start_char(src, 1), 0);
        assert_eq!(line_start_char(src, 2), 4);
        assert_eq!(line_start_char(src, 3), 8);
        assert_eq!(line_start_char(src, 99), src.chars().count());
    }

    #[test]
    fn line_start_char_counts_chars_not_bytes() {
        let src = "héllo…\nnext";
        assert_eq!(line_start_char(src, 2), 7); // 6 chars + newline
    }

    #[test]
    fn outline_lists_handlers_and_functions_in_source_order() {
        let src = "\
def haul_home():
    \"\"\"Take it home.\"\"\"
    deposit()

on hurt:
    wait(10)

on boot:
    log(1)

mine()
";
        let outline = doc_outline(src);
        assert_eq!(
            outline,
            vec![
                ("def haul_home()".to_string(), 1, Some("Take it home.".to_string())),
                ("on hurt:  [1/6 instr]".to_string(), 5, None),
                ("on boot:  [1/4 instr]".to_string(), 8, None),
            ]
        );
    }

    #[test]
    fn outline_is_empty_for_broken_or_flat_code() {
        assert_eq!(doc_outline("mine()\n"), Vec::new());
        assert_eq!(doc_outline("def ???"), Vec::new());
    }

    /// Hovering a user function resolves its docstring through every call
    /// shape: a local def, a from-import alias, and the bare tail of a
    /// qualified `module.fn()` call.
    #[test]
    fn user_function_hover_resolves_local_alias_and_qualified_names() {
        let prefix = "\
module hauling:
    def haul_home():
        \"\"\"Take it home.\"\"\"
        deposit()

";
        let local = "def scout():\n    \"\"\"Look around.\"\"\"\n    wait(1)\n\nscout()\n";
        let f = user_function_at("", local, "scout").expect("local def resolves");
        assert_eq!(f.doc.as_deref(), Some("Look around."));

        let from_import = "from hauling import haul_home\n\nhaul_home()\n";
        let f = user_function_at(prefix, from_import, "haul_home").expect("alias resolves");
        assert_eq!(f.doc.as_deref(), Some("Take it home."));

        let qualified = "import hauling\n\nhauling.haul_home()\n";
        let f = user_function_at(prefix, qualified, "haul_home").expect("qualified resolves");
        assert_eq!(f.doc.as_deref(), Some("Take it home."));

        assert!(user_function_at(prefix, from_import, "nosuch").is_none());
    }

    fn files(entries: &[(HandlerSlot, &str)]) -> std::collections::BTreeMap<HandlerSlot, String> {
        entries.iter().map(|(s, t)| (*s, t.to_string())).collect()
    }

    #[test]
    fn assemble_then_split_round_trips_per_signal_files() {
        let body = "mine()\ndeposit()\n";
        let f = files(&[
            (HandlerSlot::Error, "log(last_error())\nupload_log()\n"),
            (HandlerSlot::Bump, "wait(35)\n"),
            (HandlerSlot::Boot, "setenv(hurt_line, 30)\n"),
        ]);
        let assembled = assemble_color(body, &f);
        assert!(assembled.contains("on error:\n    log(last_error())\n    upload_log()"));
        assert!(assembled.contains("on bump:\n    wait(35)"));
        assert!(assembled.contains("on boot:\n    setenv(hurt_line, 30)"));
        let (split_body, split_files) = split_source(&assembled);
        assert_eq!(split_body, body);
        assert_eq!(split_files, f);
        // And splitting again from a reassembly is stable (byte-exact).
        assert_eq!(assemble_color(&split_body, &split_files), assembled);
    }

    #[test]
    fn assembled_stub_files_parse_as_five_windows() {
        let f: std::collections::BTreeMap<HandlerSlot, String> =
            HandlerSlot::ALL.iter().map(|s| (*s, s.stub().to_string())).collect();
        let assembled = assemble_color("mine()\n", &f);
        let program = pyrite::parse(&assembled, &pyrite::UnlockSet::all())
            .expect("stub files must assemble into a parseable program");
        assert_eq!(program.handlers.len(), 5); // one window per signal
        pyrite::check_windows(&program, editor_costs())
            .expect("the stub windows must clear the deploy analysis");
    }

    #[test]
    fn split_leaves_plain_programs_alone() {
        let src = "mine()\nif cargo_full():\n    deposit()\n";
        let (body, files) = split_source(src);
        assert_eq!(body, src);
        assert!(files.is_empty());
    }

    #[test]
    fn unknown_on_blocks_stay_in_the_body() {
        // Not a window signal (stale syntax, typos) — survives untouched
        // in the body, where the live parse squiggles it.
        let src = "mine()\non signal(s):\n    wait(10)\n";
        let (body, files) = split_source(src);
        assert_eq!(body, src);
        assert!(files.is_empty());
    }

    #[test]
    fn empty_files_are_left_out_of_the_assembly() {
        let f = files(&[(HandlerSlot::Hurt, "   \n"), (HandlerSlot::Boot, "")]);
        assert_eq!(assemble_color("mine()\n", &f), "mine()\n");
    }

    #[test]
    fn assemble_adds_a_newline_when_the_body_lacks_one() {
        let f = files(&[(HandlerSlot::Boot, "log(1)\n")]);
        assert_eq!(assemble_color("mine()", &f), "mine()\non boot:\n    log(1)\n");
        assert_eq!(assemble_color("", &f), "on boot:\n    log(1)\n");
    }
}
