//! Live Pyrite syntax highlighting and parse-error squiggles for the
//! in-game editor. Best-effort: half-typed programs still get colored.

use bevy_egui::egui;

pub(super) const HL_KEYWORD: egui::Color32 = egui::Color32::from_rgb(197, 134, 192);
pub(super) const HL_FUNCTION: egui::Color32 = egui::Color32::from_rgb(220, 220, 130);
pub(super) const HL_VARIABLE: egui::Color32 = egui::Color32::from_rgb(156, 220, 254);
pub(super) const HL_NUMBER: egui::Color32 = egui::Color32::from_rgb(181, 206, 168);
pub(super) const HL_STRING: egui::Color32 = egui::Color32::from_rgb(206, 145, 120);
pub(super) const HL_COMMENT: egui::Color32 = egui::Color32::from_rgb(106, 153, 85);
pub(super) const HL_PLAIN: egui::Color32 = egui::Color32::from_rgb(212, 212, 212);

pub(super) const HL_ERROR: egui::Color32 = egui::Color32::from_rgb(235, 80, 70);

/// Append `text[range]` to the job in `color`, red-underlining wherever the
/// range overlaps `squiggle` (splitting into up to three sections).
fn append_span(
    job: &mut egui::text::LayoutJob,
    text: &str,
    range: std::ops::Range<usize>,
    color: egui::Color32,
    font_id: &egui::FontId,
    squiggle: &Option<std::ops::Range<usize>>,
) {
    let fmt = |underline: bool| egui::text::TextFormat {
        font_id: font_id.clone(),
        color,
        underline: if underline {
            egui::Stroke::new(1.5, HL_ERROR)
        } else {
            egui::Stroke::NONE
        },
        ..Default::default()
    };
    let mut cuts = vec![range.start, range.end];
    if let Some(s) = squiggle {
        for b in [s.start, s.end] {
            if b > range.start && b < range.end {
                cuts.push(b);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    for w in cuts.windows(2) {
        let underlined = squiggle.as_ref().is_some_and(|s| w[0] < s.end && w[1] > s.start);
        job.append(&text[w[0]..w[1]], 0.0, fmt(underlined));
    }
}

/// Best-effort Pyrite highlighting for the editor. Unlike `pyrite::lexer`
/// this never fails, so half-typed programs still get colored. Keywords come
/// from the lexer's own table (`pyrite::token::keyword`) so the two can't
/// drift. `squiggle` is a byte range to underline in red (the live parse
/// error, if any).
pub(super) fn highlight_pyrite(
    text: &str,
    font_id: egui::FontId,
    squiggle: Option<std::ops::Range<usize>>,
) -> egui::text::LayoutJob {
    use egui::text::LayoutJob;

    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let byte_at =
        |i: usize| chars.get(i).map_or(text.len(), |&(b, _)| b);

    let mut job = LayoutJob::default();

    let n = chars.len();
    let mut plain_start = 0; // byte offset of pending uncolored text
    let mut i = 0;
    while i < n {
        let (start, c) = chars[i];
        let (end, color) = if c == '#' {
            while i < n && chars[i].1 != '\n' {
                i += 1;
            }
            (byte_at(i), HL_COMMENT)
        } else if c == '"' {
            let triple = chars.get(i + 1).map(|&(_, c)| c) == Some('"')
                && chars.get(i + 2).map(|&(_, c)| c) == Some('"');
            if triple {
                // `"""docstring"""` — may span lines; unterminated runs to
                // the end of the buffer (still best-effort, never fails).
                i += 3;
                while i < n
                    && !(chars[i].1 == '"'
                        && chars.get(i + 1).map(|&(_, c)| c) == Some('"')
                        && chars.get(i + 2).map(|&(_, c)| c) == Some('"'))
                {
                    i += 1;
                }
                if i < n {
                    i += 3;
                }
            } else {
                i += 1;
                while i < n && chars[i].1 != '"' && chars[i].1 != '\n' {
                    i += if chars[i].1 == '\\' { 2 } else { 1 };
                }
                if i < n && chars[i].1 == '"' {
                    i += 1;
                }
            }
            (byte_at(i), HL_STRING)
        } else if c.is_ascii_digit() {
            while i < n && chars[i].1.is_ascii_digit() {
                i += 1;
            }
            (byte_at(i), HL_NUMBER)
        } else if c.is_ascii_alphabetic() || c == '_' {
            while i < n && (chars[i].1.is_ascii_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            let end = byte_at(i);
            let color = if pyrite::token::keyword(&text[start..end]).is_some() {
                HL_KEYWORD
            } else {
                // A call (or `def` header) if the next non-space char is `(`.
                let mut j = i;
                while j < n && chars[j].1 == ' ' {
                    j += 1;
                }
                if j < n && chars[j].1 == '(' { HL_FUNCTION } else { HL_VARIABLE }
            };
            (end, color)
        } else {
            i += 1;
            continue;
        };
        if plain_start < start {
            append_span(&mut job, text, plain_start..start, HL_PLAIN, &font_id, &squiggle);
        }
        append_span(&mut job, text, start..end, color, &font_id, &squiggle);
        plain_start = end;
    }
    if plain_start < text.len() {
        append_span(&mut job, text, plain_start..text.len(), HL_PLAIN, &font_id, &squiggle);
    }
    job
}

/// Byte range to squiggle for a parse error at 1-based (line, col): from the
/// error column to the end of that line; whole line when the column is past
/// its end; the last character for errors past the final line (EOF).
pub(super) fn error_byte_range(text: &str, line: u32, col: u32) -> std::ops::Range<usize> {
    let line_idx = (line as usize).saturating_sub(1);
    let mut offset = 0;
    for (i, l) in text.split('\n').enumerate() {
        if i == line_idx {
            let start = offset
                + l.char_indices().nth((col as usize).saturating_sub(1)).map_or(l.len(), |(b, _)| b);
            let end = offset + l.len();
            if start < end {
                return start..end;
            }
            if !l.is_empty() {
                return offset..end;
            }
            break;
        }
        offset += l.len() + 1;
    }
    // EOF (or an empty error line): squiggle the last visible character —
    // a trailing newline would render nothing.
    text.char_indices()
        .rev()
        .find(|(_, c)| *c != '\n')
        .map_or(0..0, |(b, c)| b..b + c.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<(String, egui::Color32)> {
        highlight_pyrite(text, egui::FontId::monospace(12.0), None)
            .sections
            .iter()
            .map(|s| (text[s.byte_range.clone()].to_string(), s.format.color))
            .collect()
    }

    #[test]
    fn highlights_keywords_functions_variables_and_literals() {
        let got = spans("if x > 1:\n    move_to(target, \"hi\") # go\n");
        for expected in [
            ("if", HL_KEYWORD),
            ("x", HL_VARIABLE),
            ("1", HL_NUMBER),
            ("move_to", HL_FUNCTION),
            ("target", HL_VARIABLE),
            ("\"hi\"", HL_STRING),
            ("# go", HL_COMMENT),
        ] {
            assert!(
                got.contains(&(expected.0.to_string(), expected.1)),
                "missing span {expected:?} in {got:?}"
            );
        }
    }

    #[test]
    fn def_name_is_a_function_and_unterminated_string_stops_at_eol() {
        let got = spans("def go(n):\n    s = \"oops\nreturn n\n");
        assert!(got.contains(&("def".into(), HL_KEYWORD)));
        assert!(got.contains(&("go".into(), HL_FUNCTION)));
        assert!(got.contains(&("n".into(), HL_VARIABLE)));
        assert!(got.contains(&("\"oops".into(), HL_STRING)));
        assert!(got.contains(&("return".into(), HL_KEYWORD)));
    }

    #[test]
    fn highlight_covers_every_byte_exactly_once() {
        let text = "move_to(closest(ore).expect())\n# comment\nwhile True:\n    x = x + 1\n";
        let job = highlight_pyrite(text, egui::FontId::monospace(12.0), Some(8..15));
        let mut pos = 0;
        for s in &job.sections {
            assert_eq!(s.byte_range.start, pos, "gap or overlap at byte {pos}");
            pos = s.byte_range.end;
        }
        assert_eq!(pos, text.len());
    }

    #[test]
    fn squiggle_underlines_exactly_the_error_range() {
        let text = "move_to(closest(ore))\n";
        let job = highlight_pyrite(text, egui::FontId::monospace(12.0), Some(8..15));
        for s in &job.sections {
            let overlaps = s.byte_range.start < 15 && s.byte_range.end > 8;
            assert_eq!(
                s.format.underline != egui::Stroke::NONE,
                overlaps,
                "section {:?} ({})",
                s.byte_range,
                &text[s.byte_range.clone()]
            );
        }
    }

    #[test]
    fn error_ranges_map_line_and_column_to_bytes() {
        // Line 2, col 3 → from 'f' to that line's end.
        assert_eq!(error_byte_range("abc\ndef ghi\nx", 2, 3), 6..11);
        // Column past the line end → the whole line.
        assert_eq!(error_byte_range("ab\ncd\n", 1, 9), 0..2);
        // Line past EOF → the last character.
        assert_eq!(error_byte_range("ab\ncd", 7, 1), 4..5);
        // Multi-byte text: never split a char.
        let r = error_byte_range("é\n", 5, 1);
        assert_eq!(r, 0..2);
    }

    #[test]
    fn parse_errors_squiggle_live_programs() {
        // `if` needs a colon — the parser reports somewhere on line 1.
        let err = pyrite::parse("if cargo_full()\n    mine()\n", &pyrite::UnlockSet::all())
            .unwrap_err();
        let range = error_byte_range("if cargo_full()\n    mine()\n", err.line, err.col);
        assert!(!range.is_empty(), "error range must be visible");
    }
}
