//! Arena-based AST. Nodes live in flat `Vec`s inside `Program` and reference
//! each other by index (`ExprId` / `StmtId`), so the VM's program counter is a
//! plain cursor into the arena — cheap to store, snapshot, and compare
//! (lockstep determinism, docs/07-architecture.md).

use std::collections::BTreeMap;

pub type ExprId = u32;
pub type StmtId = u32;
pub type Block = Vec<StmtId>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    FloorDiv,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    /// `k in d` / `x in xs` / `sub in s` — membership, Python-style.
    In,
}

impl BinOp {
    pub fn is_arith(self) -> bool {
        matches!(self, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::FloorDiv | BinOp::Mod)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Str(String),
    Bool(bool),
    Name(String),
    List(Vec<ExprId>),
    /// `{k: v, ...}` — dict literal; keys evaluate to int/string/entity.
    Dict(Vec<(ExprId, ExprId)>),
    /// `xs[i]` / `d[k]`
    Index { base: ExprId, index: ExprId },
    /// `e.field` — runtime attribute lookup (entity properties via the Host).
    Attr { base: ExprId, name: String },
    /// `Order.Idle` — unit variant of a declared enum (resolved at parse time).
    EnumUnit { enum_name: String, variant: String },
    /// `Order.Mine(x)` — data-carrying variant construction.
    EnumCtor { enum_name: String, variant: String, args: Vec<ExprId> },
    /// `f(a, b)` — user function or builtin; resolved by the VM against the
    /// program's function table, falling back to the Host.
    Call { name: String, args: Vec<ExprId>, line: u32 },
    /// `e.expect()` — postfix method call, dispatched by the VM. Today the
    /// only method is `expect` on builtin `Result` values.
    MethodCall { base: ExprId, name: String, args: Vec<ExprId>, line: u32 },
    Unary { op: UnOp, operand: ExprId },
    Binary { op: BinOp, lhs: ExprId, rhs: ExprId },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Expression statement (a bare call, usually). Carries its line for
    /// editor gutter annotations and crash dumps.
    Expr { expr: ExprId, line: u32 },
    Assign { name: String, value: ExprId, line: u32 },
    /// `name[index] = value` — in-place container write. The target is a
    /// bare variable on purpose: containers are values, so mutation is
    /// always rooted at a name (no nested `a[0][1] = v` yet).
    IndexAssign { name: String, index: ExprId, value: ExprId, line: u32 },
    If { arms: Vec<(ExprId, Block)>, else_body: Option<Block>, line: u32 },
    While { cond: ExprId, body: Block, line: u32 },
    For { var: String, iter: ExprId, body: Block, line: u32 },
    Break { line: u32 },
    Continue { line: u32 },
    Return { value: Option<ExprId>, line: u32 },
    Match { scrutinee: ExprId, cases: Vec<MatchCase>, line: u32 },
}

impl Stmt {
    pub fn line(&self) -> u32 {
        match self {
            Stmt::Expr { line, .. }
            | Stmt::Assign { line, .. }
            | Stmt::IndexAssign { line, .. }
            | Stmt::If { line, .. }
            | Stmt::While { line, .. }
            | Stmt::For { line, .. }
            | Stmt::Break { line }
            | Stmt::Continue { line }
            | Stmt::Return { line, .. }
            | Stmt::Match { line, .. } => *line,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `case Order.Mine(target):` — matches enum name + variant, binds fields
    /// positionally. Enum names in patterns are resolved at runtime so that
    /// builtin enums (e.g. `Recv` from `try_receive`) can be matched without
    /// a declaration.
    EnumVariant { enum_name: String, variant: String, binds: Vec<String> },
    /// `case _:` — matches anything, binds nothing (the Rust catch-all).
    Wildcard,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Block,
    pub line: u32,
    /// `"""docstring"""` — a leading bare string literal in the body,
    /// captured at parse and stripped from the runtime block (free at
    /// runtime, like import lines). Editor surfaces show it.
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    /// Variant name → field names (empty for unit variants).
    pub variants: BTreeMap<String, Vec<String>>,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SignalKind {
    /// The ONE unified handler: every problem (fault, bump, bumped, hurt)
    /// arrives here as a `Signal` enum value the body can `match` on.
    Signal,
    /// Death keeps its own tiny handler: the black-box budget can't afford
    /// the unified entry ritual — the explosion doesn't wait.
    Death,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Handler {
    pub kind: SignalKind,
    /// `on signal(s):` — the name the incoming Signal value is bound to.
    pub binding: Option<String>,
    pub body: Block,
    pub line: u32,
}

/// A parsed program. Immutable once built; the VM holds it behind an `Rc`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    pub exprs: Vec<Expr>,
    pub stmts: Vec<Stmt>,
    /// Top-level statement sequence — the implicit forever-loop body.
    pub body: Block,
    /// Keyed by qualified name for module functions (`"hauling.haul_home"`),
    /// bare name for program-level defs.
    pub functions: BTreeMap<String, Function>,
    pub enums: BTreeMap<String, EnumDecl>,
    pub handlers: BTreeMap<SignalKind, Handler>,
    /// `module <name>:` blocks carried by this source (deploy artifacts —
    /// the editor inlines the library; imports resolve against these).
    pub modules: std::collections::BTreeSet<String>,
    /// Modules named by a plain `import m` — gates qualified `m.f()` calls.
    pub imported: std::collections::BTreeSet<String>,
    /// `from m import f` bindings, bare name → qualified. Calls are
    /// rewritten to the qualified name at the end of the parse, so the VM
    /// resolves everything through `functions` and never sees an alias.
    pub aliases: BTreeMap<String, String>,
}

impl Program {
    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id as usize]
    }

    pub fn stmt(&self, id: StmtId) -> &Stmt {
        &self.stmts[id as usize]
    }
}
