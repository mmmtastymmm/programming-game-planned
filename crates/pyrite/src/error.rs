//! Structured errors. `LockedConstruct` is the editor-facing "requires <unlock>"
//! error mandated by docs/01-language.md.

use crate::unlocks::Construct;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct PyriteError {
    pub line: u32,
    pub col: u32,
    pub kind: PyriteErrorKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PyriteErrorKind {
    // Lexing
    TabIndentation,
    BadDedent,
    UnterminatedString,
    BadEscape(char),
    IntOutOfRange,
    UnexpectedChar(char),
    // Parsing
    UnexpectedToken { found: String, expected: String },
    /// The construct exists in Pyrite but this colony hasn't unlocked it.
    LockedConstruct(Construct),
    DuplicateDefinition(String),
    HandlerNotAtTopLevel,
    EmptyBlock,
    UnknownEnum(String),
    UnknownEnumVariant { enum_name: String, variant: String },
}

impl fmt::Display for PyriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: ", self.line, self.col)?;
        match &self.kind {
            PyriteErrorKind::TabIndentation => write!(f, "tabs are not allowed; indent with spaces"),
            PyriteErrorKind::BadDedent => write!(f, "dedent does not match any outer indentation level"),
            PyriteErrorKind::UnterminatedString => write!(f, "unterminated string literal"),
            PyriteErrorKind::BadEscape(c) => write!(f, "unknown escape sequence \\{c}"),
            PyriteErrorKind::IntOutOfRange => write!(f, "integer literal out of range"),
            PyriteErrorKind::UnexpectedChar(c) => write!(f, "unexpected character {c:?}"),
            PyriteErrorKind::UnexpectedToken { found, expected } => {
                write!(f, "expected {expected}, found {found}")
            }
            PyriteErrorKind::LockedConstruct(c) => {
                write!(f, "requires unlock: {}", c.display_name())
            }
            PyriteErrorKind::DuplicateDefinition(name) => write!(f, "duplicate definition of {name}"),
            PyriteErrorKind::HandlerNotAtTopLevel => {
                write!(f, "handlers, functions, and enums must be at top level")
            }
            PyriteErrorKind::EmptyBlock => write!(f, "block cannot be empty"),
            PyriteErrorKind::UnknownEnum(name) => write!(f, "unknown enum {name}"),
            PyriteErrorKind::UnknownEnumVariant { enum_name, variant } => {
                write!(f, "enum {enum_name} has no variant {variant}")
            }
        }
    }
}

impl std::error::Error for PyriteError {}
