//! Kind-argument completion and identifier-under-pointer lookup for the
//! in-game editor (hover docs use `word_at`).

use bevy_egui::egui;

/// The identifier under (or immediately left of) char index `idx`.
pub(super) fn word_at(text: &str, idx: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let is_ident = |c: &char| c.is_ascii_alphanumeric() || *c == '_';
    let mut i = idx.min(chars.len());
    if !chars.get(i).is_some_and(|c| is_ident(c)) {
        if i > 0 && chars.get(i - 1).is_some_and(|c| is_ident(c)) {
            i -= 1;
        } else {
            return None;
        }
    }
    let mut start = i;
    while start > 0 && is_ident(&chars[start - 1]) {
        start -= 1;
    }
    let mut end = i + 1;
    while end < chars.len() && is_ident(&chars[end]) {
        end += 1;
    }
    Some(chars[start..end].iter().collect())
}

/// If `cursor` (a char index) sits in the kind-argument slot of a generic
/// query — `closest(` or `exists(`, then an optional partial word — return
/// (partial start char index, partial word, function name).
fn kind_arg_context(text: &str, cursor: usize) -> Option<(usize, String, &'static str)> {
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    // Walk back over the partial identifier under the caret.
    let mut i = cursor;
    while i > 0 && (chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_') {
        i -= 1;
    }
    let partial: String = chars[i..cursor].iter().collect();
    // Before it (spaces allowed): the opening paren...
    let mut j = i;
    while j > 0 && chars[j - 1] == ' ' {
        j -= 1;
    }
    if j == 0 || chars[j - 1] != '(' {
        return None;
    }
    j -= 1;
    // ...of one of the kind-taking query functions.
    let mut k = j;
    while k > 0 && (chars[k - 1].is_ascii_alphanumeric() || chars[k - 1] == '_') {
        k -= 1;
    }
    let func: String = chars[k..j].iter().collect();
    ["closest", "exists"]
        .into_iter()
        .find(|name| func == *name)
        .map(|name| (i, partial, name))
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices().nth(char_idx).map_or(text.len(), |(b, _)| b)
}

/// A live kind-argument completion: where it is, what's typed, what fits.
pub(super) struct Completion {
    pub(super) cursor: usize,
    pub(super) partial_start: usize,
    pub(super) partial: String,
    pub(super) func: &'static str,
    pub(super) suggestions: Vec<&'static str>,
}

pub(super) fn completion_at(code: &str, cursor: usize) -> Option<Completion> {
    let (partial_start, partial, func) = kind_arg_context(code, cursor)?;
    let suggestions: Vec<&'static str> = sim::host::KINDS
        .iter()
        .copied()
        .filter(|k| k.starts_with(&partial) && *k != partial)
        .collect();
    if suggestions.is_empty() {
        return None;
    }
    Some(Completion { cursor, partial_start, partial, func, suggestions })
}

/// Replace the partial word `[partial_start, cursor)` with `kind`, park the
/// caret right after it, and keep focus on the editor. Returns the caret's
/// new char index.
pub(super) fn insert_kind(
    code: &mut String,
    ctx: &egui::Context,
    editor_id: egui::Id,
    partial_start: usize,
    cursor: usize,
    kind: &str,
) -> usize {
    let from = char_to_byte(code, partial_start);
    let to = char_to_byte(code, cursor);
    code.replace_range(from..to, kind);
    let after = partial_start + kind.chars().count();
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, editor_id) {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(after))));
        state.store(ctx, editor_id);
    }
    ctx.memory_mut(|m| m.request_focus(editor_id));
    after
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_context_detected_after_open_paren() {
        let src = "move_to(closest(";
        assert_eq!(
            kind_arg_context(src, src.chars().count()),
            Some((src.chars().count(), String::new(), "closest"))
        );
    }

    #[test]
    fn kind_context_carries_the_partial_word() {
        let src = "if exists(blu";
        assert_eq!(kind_arg_context(src, src.chars().count()), Some((10, "blu".into(), "exists")));
    }

    #[test]
    fn kind_context_ignores_other_calls_and_positions() {
        for (src, cursor) in [
            ("wait(", 5),            // not a kind-taking function
            ("closest(ore)", 12),    // cursor past the closing paren
            ("closest", 7),          // no paren yet
            ("closest(ore, ", 14),   // second argument slot
        ] {
            assert_eq!(kind_arg_context(src, cursor), None, "src {src:?}");
        }
    }

    #[test]
    fn kind_context_survives_multibyte_text() {
        let src = "log(\"héllo…\")\nclosest( d";
        assert_eq!(
            kind_arg_context(src, src.chars().count()),
            Some((src.chars().count() - 1, "d".into(), "closest"))
        );
        assert_eq!(char_to_byte(src, src.chars().count()), src.len());
    }

    #[test]
    fn word_under_pointer() {
        let src = "move_to(closest(ore))";
        assert_eq!(word_at(src, 0), Some("move_to".into()));
        assert_eq!(word_at(src, 3), Some("move_to".into()));
        assert_eq!(word_at(src, 7), Some("move_to".into())); // on the '('
        assert_eq!(word_at(src, 10), Some("closest".into()));
        assert_eq!(word_at(src, 16), Some("ore".into()));
        assert_eq!(word_at(src, src.len()), None); // after the final ')'
        assert_eq!(word_at("  ", 1), None);
    }
}
