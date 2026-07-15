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
    /// `import m` / `from m import f` names a module the source doesn't
    /// carry — at deploy this means "no module by that name in the library".
    UnknownModule(String),
    UnknownModuleMember { module: String, name: String },
    /// `m.f()` on a module that was never `import`ed.
    ModuleNotImported(String),
    /// Module blocks hold `def`s only — a module that *did* things on
    /// import would be a program (docs/01 "Modules & the Program Library").
    StatementInModule,
    // Deploy-time window analysis (docs/01 "Signal handlers", M3)
    /// The window's worst-case instruction count exceeds its signal's cap.
    WindowOverCap { signal: &'static str, worst: u64, cap: u64 },
    /// A window (or a def it reaches) calls a function that isn't
    /// `signal_safe` — or one the registry doesn't know at all.
    WindowUnsafeCall { signal: &'static str, func: String },
    /// Loops are banned in window-reachable code (straight-line + `if`,
    /// all the way down — Q51).
    WindowLoop { signal: &'static str },
    /// Recursion makes a window's worst case unbounded.
    WindowRecursion { signal: &'static str, func: String },
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
            PyriteErrorKind::UnknownModule(name) => {
                write!(f, "unknown module '{name}' — the library has no module by that name")
            }
            PyriteErrorKind::UnknownModuleMember { module, name } => {
                write!(f, "module '{module}' has no function '{name}'")
            }
            PyriteErrorKind::ModuleNotImported(name) => {
                write!(
                    f,
                    "module '{name}' is not imported — add 'import {name}' \
                     or 'from {name} import ...'"
                )
            }
            PyriteErrorKind::StatementInModule => {
                write!(f, "modules hold only 'def' functions — no statements, handlers, or enums")
            }
            PyriteErrorKind::WindowOverCap { signal, worst, cap } => {
                write!(
                    f,
                    "the 'on {signal}:' window can run {worst} instructions in the \
                     worst case — its cap is {cap}"
                )
            }
            PyriteErrorKind::WindowUnsafeCall { signal, func } => {
                write!(
                    f,
                    "'{func}' is not signal-safe — the 'on {signal}:' window may only \
                     call signal-safe functions"
                )
            }
            PyriteErrorKind::WindowLoop { signal } => {
                write!(
                    f,
                    "loops are not allowed in the 'on {signal}:' window or anything it \
                     calls — handlers decide and delegate; loops belong to the main program"
                )
            }
            PyriteErrorKind::WindowRecursion { signal, func } => {
                write!(
                    f,
                    "'{func}' recurses — recursion is unbounded, so it cannot be called \
                     from the 'on {signal}:' window"
                )
            }
        }
    }
}

impl std::error::Error for PyriteError {}

/// Fault-identity constants (docs/01, Q80): every runtime fault carries a
/// pre-bound, `==`-comparable id returned by `last_error()`. The ids are a
/// data registry — each is auto-bound as a VM constant of the same name, so
/// handlers write `if last_error() == err_payload:`. Host-domain ids
/// (`err_tool_jam`, `err_unknown_contact`, …) arrive with their systems;
/// this module owns the language-level set.
pub mod faults {
    /// Type mismatch (wrong operand/argument/condition type).
    pub const TYPE: &str = "err_type";
    /// Read of an unset variable, or mutation of a non-variable.
    pub const NAME: &str = "err_name";
    /// Unknown function or method.
    pub const UNKNOWN_FUNCTION: &str = "err_unknown_function";
    /// Wrong argument count / unknown or duplicate keyword.
    pub const ARITY: &str = "err_arity";
    /// User-function call depth exceeded.
    pub const STACK: &str = "err_stack";
    /// List index out of range.
    pub const INDEX: &str = "err_index";
    /// Dict key not found (or unusable key type).
    pub const KEY: &str = "err_key";
    /// Division / modulo by zero.
    pub const DIV_ZERO: &str = "err_div_zero";
    /// Integer overflow.
    pub const OVERFLOW: &str = "err_overflow";
    /// `match` fell off the end with no case matching.
    pub const NO_MATCH: &str = "err_no_match";
    /// `.expect()` on `Err` / `None`.
    pub const EXPECT: &str = "err_expect";
    /// `range()` beyond `range_cap`.
    pub const RANGE: &str = "err_range";
    /// Payload exceeds `payload_cap` on a sized op (Q82).
    pub const PAYLOAD: &str = "err_payload";
    /// `break`/`continue` outside a loop, `return` outside a def.
    pub const CONTROL: &str = "err_control";
    /// A world action failed (unreachable target, nothing in range, …).
    /// Finer host-domain ids supersede this as their systems land.
    pub const ACTION: &str = "err_action";
    /// A blocking channel op timed out (reserved for M11 — registered now
    /// so programs can already name it).
    pub const TIMEOUT: &str = "err_timeout";

    /// Every language-level fault id, for constant binding.
    pub const ALL: &[&str] = &[
        TYPE, NAME, UNKNOWN_FUNCTION, ARITY, STACK, INDEX, KEY, DIV_ZERO, OVERFLOW,
        NO_MATCH, EXPECT, RANGE, PAYLOAD, CONTROL, ACTION, TIMEOUT,
    ];
}
