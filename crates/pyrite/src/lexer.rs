//! Indentation-aware lexer: Python-style layout with `Indent`/`Dedent` tokens.
//!
//! Rules:
//! - Indentation is spaces only; a tab in leading whitespace is a lex error.
//! - Blank lines and comment-only lines (`# ...`) emit nothing.
//! - Every non-blank logical line ends with `Newline`.

use crate::error::{PyriteError, PyriteErrorKind};
use crate::token::{keyword, Tok, Token};

pub fn lex(source: &str) -> Result<Vec<Token>, PyriteError> {
    let mut tokens = Vec::new();
    // Indentation stack of column widths; always starts with 0.
    let mut indents: Vec<usize> = vec![0];

    for (line_idx, raw_line) in source.lines().enumerate() {
        let line_no = (line_idx + 1) as u32;
        let bytes: Vec<char> = raw_line.chars().collect();

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

        lex_line(&bytes, i, line_no, &mut tokens)?;
        tokens.push(Token { tok: Tok::Newline, line: line_no, col: (bytes.len() + 1) as u32 });
    }

    // Close any open blocks at EOF.
    let eof_line = (source.lines().count() + 1) as u32;
    while indents.len() > 1 {
        indents.pop();
        tokens.push(Token { tok: Tok::Dedent, line: eof_line, col: 1 });
    }
    tokens.push(Token { tok: Tok::Eof, line: eof_line, col: 1 });
    Ok(tokens)
}

fn lex_line(
    chars: &[char],
    start: usize,
    line: u32,
    out: &mut Vec<Token>,
) -> Result<(), PyriteError> {
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
    Ok(())
}
