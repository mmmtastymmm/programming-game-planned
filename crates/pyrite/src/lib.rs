//! Pyrite — the unit language for the programming RTS.
//!
//! A custom Python-like DSL, interpreted one operation at a time with
//! per-operation cycle costs. See `docs/01-language.md` for the language
//! spec and `docs/07-architecture.md` for how this crate fits the sim.
//!
//! Layer map:
//! - [`lexer`] / [`token`]: indentation-aware tokenizer
//! - [`parser`] / [`ast`]: recursive descent into an arena AST, with
//!   construct gating via [`unlocks::UnlockSet`]
//! - [`vm`]: the resumable, cycle-metered stepper (faults, handlers,
//!   double-handle, blocking actions)
//! - [`costs`]: the data-driven cycle cost table
//!
//! This crate is deterministic by contract (CLAUDE.md): no floats, no
//! hash-ordered iteration, no clocks, no I/O.

pub mod analysis;
pub mod ast;
pub mod costs;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod unlocks;
pub mod value;
pub mod vm;

pub use ast::Program;
pub use costs::{BuiltinSpec, CostSpec, CostTable};
pub use error::{faults, PyriteError, PyriteErrorKind};
pub use parser::parse;
pub use unlocks::{Construct, UnlockSet};
pub use value::{EnumValue, Value};
pub use analysis::check_windows;
pub use vm::{
    CallCtx, EngineCtx, Fault, Host, HostCall, Outcome, Phase, RaiseOutcome, RunState, Signal,
    Vm, VmConfig,
};
