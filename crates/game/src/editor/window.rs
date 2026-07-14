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

/// The per-signal handler files (the file viewer's Handlers folder). Each
/// signal gets its own document. Error/Hurt/Bump/Bumped are arms of the
/// engine's ONE unified `on signal(s):` handler — colony-wide, so the
/// editor generates the `match s:` dispatch around them at deploy — and
/// Death is the separate black-box handler. Ord = assembly order. (Boot
/// and recall have no player window in the language yet; the file viewer
/// shows them as locked engine rows.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum HandlerSlot {
    Error,
    Hurt,
    Bump,
    Bumped,
    Death,
}

impl HandlerSlot {
    pub(crate) const ALL: [HandlerSlot; 5] = [
        HandlerSlot::Error,
        HandlerSlot::Hurt,
        HandlerSlot::Bump,
        HandlerSlot::Bumped,
        HandlerSlot::Death,
    ];
    /// The arms of the generated unified handler, in emission order.
    const UNIFIED: [HandlerSlot; 4] =
        [HandlerSlot::Error, HandlerSlot::Hurt, HandlerSlot::Bump, HandlerSlot::Bumped];

    /// Display name (also the doc name in the file viewer).
    pub(crate) fn name(self) -> &'static str {
        match self {
            HandlerSlot::Error => "on error",
            HandlerSlot::Hurt => "on hurt",
            HandlerSlot::Bump => "on bump",
            HandlerSlot::Bumped => "on bumped",
            HandlerSlot::Death => "on death",
        }
    }

    /// The locked `case` line the engine wraps this file's code in
    /// (unified arms only; death has its own block).
    pub(crate) fn case_line(self) -> Option<&'static str> {
        match self {
            HandlerSlot::Error => Some("case Signal.Error(msg):"),
            HandlerSlot::Hurt => Some("case Signal.Hurt:"),
            HandlerSlot::Bump => Some("case Signal.Bump:"),
            HandlerSlot::Bumped => Some("case Signal.Bumped:"),
            HandlerSlot::Death => None,
        }
    }

    /// Starter body for a freshly added handler file (written at column 0;
    /// the assembler indents it into its block).
    pub(crate) fn stub(self) -> &'static str {
        match self {
            HandlerSlot::Error => "log(msg)\n",
            HandlerSlot::Hurt => "drop_cargo()\n",
            HandlerSlot::Bump => "wait(35)\n",
            HandlerSlot::Bumped => "wait(15)\n",
            HandlerSlot::Death => "upload_log()\n",
        }
    }
}

const UNIFIED_HEADER: &str = "on signal(s):\n    match s:\n";
const UNIFIED_FALLBACK: &str = "        case _:\n            wait(0)\n";
const DEATH_HEADER: &str = "on death:\n";

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

/// The deployable source for one color: the body, then the generated
/// unified `on signal(s):` block (if any unified file has code), then the
/// death block. Empty/whitespace-only files are treated as absent.
pub(crate) fn assemble_color(body: &str, files: &std::collections::BTreeMap<HandlerSlot, String>) -> String {
    fn append_block(out: &mut String, block: &str) {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(block);
    }
    let mut out = body.to_string();
    let arms: Vec<(HandlerSlot, &String)> = HandlerSlot::UNIFIED
        .iter()
        .filter_map(|s| files.get(s).filter(|t| !t.trim().is_empty()).map(|t| (*s, t)))
        .collect();
    if !arms.is_empty() {
        let mut block = String::from(UNIFIED_HEADER);
        for (slot, arm_body) in &arms {
            block.push_str("        ");
            block.push_str(slot.case_line().expect("unified arms have case lines"));
            block.push('\n');
            block.push_str(&indent_lines(arm_body, "            "));
        }
        block.push_str(UNIFIED_FALLBACK);
        append_block(&mut out, &block);
    }
    if let Some(death) = files.get(&HandlerSlot::Death).filter(|t| !t.trim().is_empty()) {
        let mut block = String::from(DEATH_HEADER);
        block.push_str(&indent_lines(death, "    "));
        append_block(&mut out, &block);
    }
    out
}

/// Re-split a generated unified block into its per-signal arm bodies.
/// Returns None for anything that isn't exactly the editor's emitted
/// shape — such blocks stay in the program body untouched.
fn parse_unified(block: &str) -> Option<std::collections::BTreeMap<HandlerSlot, String>> {
    enum Cur {
        Arm(HandlerSlot, String),
        Fallback,
    }
    let rest = block.strip_prefix(UNIFIED_HEADER)?;
    let mut files = std::collections::BTreeMap::new();
    let mut cur: Option<Cur> = None;
    for line in rest.lines() {
        if let Some(slot) =
            HandlerSlot::UNIFIED.iter().find(|s| line.trim_start() == s.case_line().unwrap())
        {
            if !line.starts_with("        case ") {
                return None;
            }
            if let Some(Cur::Arm(s, t)) = cur.take() {
                files.insert(s, t);
            }
            cur = Some(Cur::Arm(*slot, String::new()));
        } else if line == "        case _:" {
            if let Some(Cur::Arm(s, t)) = cur.take() {
                files.insert(s, t);
            }
            cur = Some(Cur::Fallback);
        } else if line.trim().is_empty() {
            if let Some(Cur::Arm(_, t)) = &mut cur {
                t.push('\n');
            }
        } else if let Some(body_line) = line.strip_prefix("            ") {
            match &mut cur {
                Some(Cur::Arm(_, t)) => {
                    t.push_str(body_line);
                    t.push('\n');
                }
                Some(Cur::Fallback) => {} // the wait(0) filler — dropped
                None => return None,
            }
        } else {
            return None;
        }
    }
    if let Some(Cur::Arm(s, t)) = cur {
        files.insert(s, t);
    }
    Some(files)
}

/// Split a deployed source into (body, per-signal handler files) at the
/// text level: a top-level `on …:` line starts a block owning every
/// following blank/indented line. Death blocks become the death file
/// (header stripped, body dedented); `on signal(s):` blocks that match the
/// editor's generated shape become per-signal arm files; anything else
/// stays in the body. Sources the editor itself assembled round-trip
/// byte-exactly.
pub(crate) fn split_source(source: &str) -> (String, std::collections::BTreeMap<HandlerSlot, String>) {
    enum Kind {
        Death,
        Unified,
    }
    let header_kind = |line: &str| -> Option<Kind> {
        let rest = line.strip_prefix("on ").or_else(|| line.strip_prefix("on\t"))?;
        let rest = rest.trim_start();
        if rest.starts_with("death") {
            Some(Kind::Death)
        } else if rest.starts_with("signal") {
            Some(Kind::Unified)
        } else {
            None
        }
    };

    let mut body = String::new();
    let mut files = std::collections::BTreeMap::new();
    let mut current: Option<(Kind, String)> = None;
    let mut flush =
        |current: &mut Option<(Kind, String)>, body: &mut String, files: &mut std::collections::BTreeMap<HandlerSlot, String>| {
            let Some((kind, text)) = current.take() else { return };
            match kind {
                Kind::Death => {
                    let body_text = text.strip_prefix(DEATH_HEADER).unwrap_or(&text);
                    files
                        .entry(HandlerSlot::Death)
                        .or_insert_with(|| dedent_lines(body_text, 4));
                }
                Kind::Unified => match parse_unified(&text) {
                    Some(arms) => {
                        for (slot, arm) in arms {
                            files.entry(slot).or_insert(arm);
                        }
                    }
                    // Not our shape (hand-written) — leave it in the body.
                    None => body.push_str(&text),
                },
            }
        };
    for line in source.split_inclusive('\n') {
        let top_level = !line.starts_with(' ') && !line.starts_with('\t') && !line.trim().is_empty();
        if top_level {
            if let Some(kind) = header_kind(line) {
                flush(&mut current, &mut body, &mut files);
                current = Some((kind, line.to_string()));
                continue;
            }
            flush(&mut current, &mut body, &mut files);
            body.push_str(line);
        } else {
            match &mut current {
                Some((_, text)) => text.push_str(line),
                None => body.push_str(line),
            }
        }
    }
    flush(&mut current, &mut body, &mut files);
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
    use pyrite::ast::SignalKind;
    let Ok(prog) = pyrite::parse(code, &pyrite::UnlockSet::all()) else {
        return Vec::new();
    };
    let mut items: Vec<(String, u32, Option<String>)> = prog
        .handlers
        .values()
        .map(|h| {
            let label = match (h.kind, &h.binding) {
                (SignalKind::Signal, Some(b)) => format!("on signal({b}):"),
                (SignalKind::Signal, None) => "on signal:".into(),
                (SignalKind::Death, _) => "on death:".into(),
            };
            (label, h.line, None)
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

fn live_parse(prefix: &str, text: &str) -> LiveParse {
    match pyrite::parse(&format!("{prefix}{text}"), &pyrite::UnlockSet::all()) {
        Ok(_) => LiveParse::Ok,
        Err(mut e) => {
            let prefix_lines = prefix.lines().count() as u32;
            if e.line > prefix_lines {
                e.line -= prefix_lines;
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
        let squiggle = match live_parse(prefix, text) {
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
            if let Some(doc_entry) = sim::host::builtin_doc(&word) {
                // upload_crash_dump is costed by its dedicated field.
                let cost = if doc_entry.name == "upload_crash_dump" {
                    costs.crash_dump
                } else {
                    costs.builtin_cost(doc_entry.name)
                };
                egui::show_tooltip_at_pointer(
                    ui.ctx(),
                    ui.layer_id(),
                    editor_id.with("hover_doc"),
                    |ui| {
                        ui.monospace(egui::RichText::new(doc_entry.signature).color(HL_FUNCTION));
                        ui.label(doc_entry.summary);
                        ui.small(format!("cost: {cost} cycles{}", doc_entry.cost_note));
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
                                func.params.join(", ")
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
    match live_parse(prefix, &doc.code) {
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

on signal(s):
    wait(10)

on death:
    log(1)

mine()
";
        let outline = doc_outline(src);
        assert_eq!(
            outline,
            vec![
                ("def haul_home()".to_string(), 1, Some("Take it home.".to_string())),
                ("on signal(s):".to_string(), 5, None),
                ("on death:".to_string(), 8, None),
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
            (HandlerSlot::Error, "log(msg)\nupload_log()\n"),
            (HandlerSlot::Bump, "wait(35)\n"),
            (HandlerSlot::Death, "upload_log()\n"),
        ]);
        let assembled = assemble_color(body, &f);
        assert!(assembled.contains("on signal(s):"));
        assert!(assembled.contains("        case Signal.Error(msg):"));
        assert!(assembled.contains("            log(msg)"));
        assert!(assembled.contains("        case _:"));
        assert!(assembled.contains("on death:\n    upload_log()"));
        let (split_body, split_files) = split_source(&assembled);
        assert_eq!(split_body, body);
        assert_eq!(split_files, f);
        // And splitting again from a reassembly is stable (byte-exact).
        assert_eq!(assemble_color(&split_body, &split_files), assembled);
    }

    #[test]
    fn assembled_stub_files_parse_as_one_unified_handler() {
        let f: std::collections::BTreeMap<HandlerSlot, String> =
            HandlerSlot::ALL.iter().map(|s| (*s, s.stub().to_string())).collect();
        let assembled = assemble_color("mine()\n", &f);
        let program = pyrite::parse(&assembled, &pyrite::UnlockSet::all())
            .expect("stub files must assemble into a parseable program");
        assert_eq!(program.handlers.len(), 2); // unified signal + death
    }

    #[test]
    fn split_leaves_plain_programs_alone() {
        let src = "mine()\nif cargo_full():\n    deposit()\n";
        let (body, files) = split_source(src);
        assert_eq!(body, src);
        assert!(files.is_empty());
    }

    #[test]
    fn hand_written_signal_blocks_stay_in_the_body() {
        // Not the editor's generated shape — must survive untouched in the
        // body rather than be mangled into per-signal files.
        let src = "mine()\non signal(s):\n    wait(10)\n";
        let (body, files) = split_source(src);
        assert_eq!(body, src);
        assert!(files.is_empty());
    }

    #[test]
    fn empty_files_are_left_out_of_the_assembly() {
        let f = files(&[(HandlerSlot::Hurt, "   \n"), (HandlerSlot::Death, "")]);
        assert_eq!(assemble_color("mine()\n", &f), "mine()\n");
    }

    #[test]
    fn assemble_adds_a_newline_when_the_body_lacks_one() {
        let f = files(&[(HandlerSlot::Death, "log(1)\n")]);
        assert_eq!(assemble_color("mine()", &f), "mine()\non death:\n    log(1)\n");
        assert_eq!(assemble_color("", &f), "on death:\n    log(1)\n");
    }
}
