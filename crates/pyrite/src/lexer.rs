//! Indentation-aware lexer: Python-style layout with `Indent`/`Dedent` tokens.
//!
//! Rules:
//! - Indentation is spaces only; a tab in leading whitespace is a lex error.
//! - Blank lines and comment-only lines (`# ...`) emit nothing.
//! - Every non-blank logical line ends with `Newline`.
//! - Triple-quoted strings (`"""..."""`, the docstring form) may span
//!   lines; the content is raw — no escape processing, newlines literal.
//!   Continuation lines belong to the string, not the layout.

use crate::error::{PyriteError, PyriteErrorKind};
use crate::token::{keyword, Tok, Token};

pub fn lex(source: &str) -> Result<Vec<Token>, PyriteError> {
    let mut tokens = Vec::new();
    // Indentation stack of column widths; always starts with 0.
    let mut indents: Vec<usize> = vec![0];
    let lines: Vec<Vec<char>> = source.lines().map(|l| l.chars().collect()).collect();

    let mut line_idx = 0;
    while line_idx < lines.len() {
        let line_no = (line_idx + 1) as u32;
        let bytes = &lines[line_idx];

        // Measure leading whitespace.
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                ' ' => i += 1,
                '\t' => {
                    return Err(PyriteError {
                        line: line_no,
                        col: (i + 1) as u32,
                        kind: PyriteErrorKind::TabIndentation,
                    });
                }
                _ => break,
            }
        }
        // Blank or comment-only line: contributes nothing to layout.
        if i >= bytes.len() || bytes[i] == '#' {
            line_idx += 1;
            continue;
        }

        let indent = i;
        let current = *indents.last().expect("indent stack never empty");
        if indent > current {
            indents.push(indent);
            tokens.push(Token { tok: Tok::Indent, line: line_no, col: 1 });
        } else if indent < current {
            while *indents.last().expect("indent stack never empty") > indent {
                indents.pop();
                tokens.push(Token { tok: Tok::Dedent, line: line_no, col: 1 });
            }
            if *indents.last().expect("indent stack never empty") != indent {
                return Err(PyriteError {
                    line: line_no,
                    col: (indent + 1) as u32,
                    kind: PyriteErrorKind::BadDedent,
                });
            }
        }

        // A triple-quoted string may consume continuation lines; the
        // logical line (and its Newline token) ends where lexing ended.
        let end_idx = lex_line(&lines, line_idx, i, &mut tokens)?;
        tokens.push(Token {
            tok: Tok::Newline,
            line: (end_idx + 1) as u32,
            col: (lines[end_idx].len() + 1) as u32,
        });
        line_idx = end_idx + 1;
    }

    // Close any open blocks at EOF.
    let eof_line = (lines.len() + 1) as u32;
    while indents.len() > 1 {
        indents.pop();
        tokens.push(Token { tok: Tok::Dedent, line: eof_line, col: 1 });
    }
    tokens.push(Token { tok: Tok::Eof, line: eof_line, col: 1 });
    Ok(tokens)
}

/// Lex one logical line starting at `lines[start_line][start]`. Returns
/// the index of the physical line where the logical line ended (greater
/// than `start_line` only when a triple-quoted string spanned lines).
fn lex_line(
    lines: &[Vec<char>],
    start_line: usize,
    start: usize,
    out: &mut Vec<Token>,
) -> Result<usize, PyriteError> {
    let mut line_idx = start_line;
    let mut chars = &lines[line_idx];
    let mut line = (line_idx + 1) as u32;
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        let col = (i + 1) as u32;
        match c {
            ' ' => {
                i += 1;
            }
            '\t' => {
                return Err(PyriteError { line, col, kind: PyriteErrorKind::TabIndentation });
            }
            '#' => break, // trailing comment
            '0'..='9' => {
                let begin = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let text: String = chars[begin..i].iter().collect();
                let value: i64 = text.parse().map_err(|_| PyriteError {
                    line,
                    col,
                    kind: PyriteErrorKind::IntOutOfRange,
                })?;
                out.push(Token { tok: Tok::Int(value), line, col });
            }
            '"' => {
                let triple = chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"');
                if triple {
                    // `"""..."""` — the docstring form. Raw content (no
                    // escapes), may span lines; continuation lines are
                    // string content, not layout.
                    let (open_line, open_col) = (line, col);
                    i += 3;
                    let mut s = String::new();
                    'scan: loop {
                        while i < chars.len() {
                            if chars[i] == '"'
                                && chars.get(i + 1) == Some(&'"')
                                && chars.get(i + 2) == Some(&'"')
                            {
                                i += 3;
                                break 'scan;
                            }
                            s.push(chars[i]);
                            i += 1;
                        }
                        if line_idx + 1 >= lines.len() {
                            return Err(PyriteError {
                                line: open_line,
                                col: open_col,
                                kind: PyriteErrorKind::UnterminatedString,
                            });
                        }
                        s.push('\n');
                        line_idx += 1;
                        chars = &lines[line_idx];
                        line = (line_idx + 1) as u32;
                        i = 0;
                    }
                    out.push(Token { tok: Tok::Str(s), line: open_line, col: open_col });
                    continue;
                }
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= chars.len() {
                        return Err(PyriteError {
                            line,
                            col,
                            kind: PyriteErrorKind::UnterminatedString,
                        });
                    }
                    match chars[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' => {
                            i += 1;
                            let esc = chars.get(i).copied().ok_or(PyriteError {
                                line,
                                col,
                                kind: PyriteErrorKind::UnterminatedString,
                            })?;
                            match esc {
                                '"' => s.push('"'),
                                '\\' => s.push('\\'),
                                'n' => s.push('\n'),
                                other => {
                                    return Err(PyriteError {
                                        line,
                                        col: (i + 1) as u32,
                                        kind: PyriteErrorKind::BadEscape(other),
                                    });
                                }
                            }
                            i += 1;
                        }
                        other => {
                            s.push(other);
                            i += 1;
                        }
                    }
                }
                out.push(Token { tok: Tok::Str(s), line, col });
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let begin = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[begin..i].iter().collect();
                let tok = keyword(&word).unwrap_or(Tok::Ident(word));
                out.push(Token { tok, line, col });
            }
            '+' => {
                out.push(Token { tok: Tok::Plus, line, col });
                i += 1;
            }
            '-' => {
                out.push(Token { tok: Tok::Minus, line, col });
                i += 1;
            }
            '*' => {
                out.push(Token { tok: Tok::Star, line, col });
                i += 1;
            }
            '/' => {
                if chars.get(i + 1) == Some(&'/') {
                    out.push(Token { tok: Tok::SlashSlash, line, col });
                    i += 2;
                } else {
                    return Err(PyriteError {
                        line,
                        col,
                        kind: PyriteErrorKind::UnexpectedChar('/'),
                    });
                }
            }
            '%' => {
                out.push(Token { tok: Tok::Percent, line, col });
                i += 1;
            }
            '=' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token { tok: Tok::EqEq, line, col });
                    i += 2;
                } else {
                    out.push(Token { tok: Tok::Assign, line, col });
                    i += 1;
                }
            }
            '!' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token { tok: Tok::NotEq, line, col });
                    i += 2;
                } else {
                    return Err(PyriteError {
                        line,
                        col,
                        kind: PyriteErrorKind::UnexpectedChar('!'),
                    });
                }
            }
            '<' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token { tok: Tok::Le, line, col });
                    i += 2;
                } else {
                    out.push(Token { tok: Tok::Lt, line, col });
                    i += 1;
                }
            }
            '>' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Token { tok: Tok::Ge, line, col });
                    i += 2;
                } else {
                    out.push(Token { tok: Tok::Gt, line, col });
                    i += 1;
                }
            }
            '(' => {
                out.push(Token { tok: Tok::LParen, line, col });
                i += 1;
            }
            ')' => {
                out.push(Token { tok: Tok::RParen, line, col });
                i += 1;
            }
            '[' => {
                out.push(Token { tok: Tok::LBracket, line, col });
                i += 1;
            }
            ']' => {
                out.push(Token { tok: Tok::RBracket, line, col });
                i += 1;
            }
            '{' => {
                out.push(Token { tok: Tok::LBrace, line, col });
                i += 1;
            }
            '}' => {
                out.push(Token { tok: Tok::RBrace, line, col });
                i += 1;
            }
            ',' => {
                out.push(Token { tok: Tok::Comma, line, col });
                i += 1;
            }
            ':' => {
                out.push(Token { tok: Tok::Colon, line, col });
                i += 1;
            }
            '.' => {
                out.push(Token { tok: Tok::Dot, line, col });
                i += 1;
            }
            other => {
                return Err(PyriteError {
                    line,
                    col,
                    kind: PyriteErrorKind::UnexpectedChar(other),
                });
            }
        }
    }
    Ok(line_idx)
}
