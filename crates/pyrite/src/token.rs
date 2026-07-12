//! Token definitions for the Pyrite lexer.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // Atoms
    Int(i64),
    Str(String),
    Ident(String),
    // Keywords
    If,
    Elif,
    Else,
    While,
    For,
    In,
    Break,
    Continue,
    Def,
    Return,
    On,
    Enum,
    Match,
    Case,
    True,
    False,
    Not,
    And,
    Or,
    // Operators / punctuation
    Plus,
    Minus,
    Star,
    SlashSlash, // integer division `//`
    Percent,
    EqEq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    Assign,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    // Layout
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
}

/// Map a word to its keyword token, if it is one. Public so tooling
/// (e.g. the editor's syntax highlighter) shares the lexer's keyword table.
pub fn keyword(s: &str) -> Option<Tok> {
    Some(match s {
        "if" => Tok::If,
        "elif" => Tok::Elif,
        "else" => Tok::Else,
        "while" => Tok::While,
        "for" => Tok::For,
        "in" => Tok::In,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "def" => Tok::Def,
        "return" => Tok::Return,
        "on" => Tok::On,
        "enum" => Tok::Enum,
        "match" => Tok::Match,
        "case" => Tok::Case,
        "True" => Tok::True,
        "False" => Tok::False,
        "not" => Tok::Not,
        "and" => Tok::And,
        "or" => Tok::Or,
        _ => return None,
    })
}
