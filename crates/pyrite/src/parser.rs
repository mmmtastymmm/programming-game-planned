//! Recursive-descent parser with construct gating.
//!
//! The parser takes an `UnlockSet`; locked syntax produces a structured
//! `LockedConstruct` error (docs/01-language.md: "using a locked construct is
//! a parse error with a friendly 'requires <unlock>' message").

use crate::ast::*;
use crate::error::{PyriteError, PyriteErrorKind};
use crate::token::{Tok, Token};
use crate::unlocks::{Construct, UnlockSet};

pub fn parse(source: &str, unlocks: &UnlockSet) -> Result<Program, PyriteError> {
    let tokens = crate::lexer::lex(source)?;
    Parser { tokens, pos: 0, unlocks, program: Program::default() }.parse_program()
}

/// (positional args, keyword args) of one call.
type CallArgs = (Vec<ExprId>, Vec<(String, ExprId)>);

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    unlocks: &'a UnlockSet,
    program: Program,
}

impl<'a> Parser<'a> {
    // --- token helpers ---

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek2(&self) -> Option<&Token> {
        self.tokens.get(self.pos + 1)
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn check(&self, tok: &Tok) -> bool {
        &self.peek().tok == tok
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.check(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: Tok, expected: &str) -> Result<Token, PyriteError> {
        if self.check(&tok) {
            Ok(self.advance())
        } else {
            Err(self.unexpected(expected))
        }
    }

    fn unexpected(&self, expected: &str) -> PyriteError {
        let t = self.peek();
        PyriteError {
            line: t.line,
            col: t.col,
            kind: PyriteErrorKind::UnexpectedToken {
                found: format!("{:?}", t.tok),
                expected: expected.to_string(),
            },
        }
    }

    fn require(&self, construct: Construct) -> Result<(), PyriteError> {
        if self.unlocks.has(construct) {
            Ok(())
        } else {
            let t = self.peek();
            Err(PyriteError {
                line: t.line,
                col: t.col,
                kind: PyriteErrorKind::LockedConstruct(construct),
            })
        }
    }

    fn add_expr(&mut self, expr: Expr) -> ExprId {
        self.program.exprs.push(expr);
        (self.program.exprs.len() - 1) as ExprId
    }

    fn add_stmt(&mut self, stmt: Stmt) -> StmtId {
        self.program.stmts.push(stmt);
        (self.program.stmts.len() - 1) as StmtId
    }

    // --- program structure ---

    fn parse_program(mut self) -> Result<Program, PyriteError> {
        let mut body = Block::new();
        loop {
            match &self.peek().tok {
                Tok::Eof => break,
                Tok::Newline => {
                    self.advance();
                }
                Tok::Def => self.parse_def(None)?,
                Tok::On => self.parse_handler()?,
                Tok::Enum => self.parse_enum()?,
                Tok::Module => self.parse_module()?,
                Tok::Import => self.parse_import()?,
                Tok::From => self.parse_from_import()?,
                _ => {
                    let stmt = self.parse_statement(false)?;
                    body.push(stmt);
                }
            }
        }
        self.program.body = body;
        // `from m import f` binds f bare: rewrite every bare call through
        // the alias table so the VM only ever resolves qualified names.
        // (Aliases can't collide with local defs — both directions are
        // duplicate-definition errors — so the rewrite is unambiguous.)
        for i in 0..self.program.exprs.len() {
            let Expr::Call { name, .. } = &self.program.exprs[i] else { continue };
            let Some(qualified) = self.program.aliases.get(name).cloned() else { continue };
            let Expr::Call { name, .. } = &mut self.program.exprs[i] else { unreachable!() };
            *name = qualified;
        }
        Ok(self.program)
    }

    /// A `def`, either at top level (bare name) or inside a `module` block
    /// (registered under `module.name`).
    fn parse_def(&mut self, module: Option<&str>) -> Result<(), PyriteError> {
        self.require(Construct::Functions)?;
        let def_tok = self.expect(Tok::Def, "def")?;
        let name = self.expect_ident("function name")?;
        // Fully reserved engine verbs (M3): a player def would shadow the
        // scuttle / the forced prologue at call resolution, which runs
        // user functions before the VM intercepts these names.
        if matches!(name.as_str(), "abort" | "handler_init" | "become_disabled") {
            return Err(PyriteError {
                line: def_tok.line,
                col: def_tok.col,
                kind: PyriteErrorKind::ReservedName(name),
            });
        }
        let key = match module {
            Some(m) => format!("{m}.{name}"),
            None => name.clone(),
        };
        if self.program.functions.contains_key(&key) || self.program.aliases.contains_key(&key) {
            return Err(PyriteError {
                line: def_tok.line,
                col: def_tok.col,
                kind: PyriteErrorKind::DuplicateDefinition(key),
            });
        }
        self.expect(Tok::LParen, "(")?;
        let mut params: Vec<Param> = Vec::new();
        if !self.check(&Tok::RParen) {
            loop {
                let pname = self.expect_ident("parameter name")?;
                let default = if self.eat(&Tok::Assign) {
                    Some(self.parse_default_literal()?)
                } else {
                    // Python's rule: once a default appears, every later
                    // parameter needs one (else call binding is ambiguous).
                    if params.iter().any(|p| p.default.is_some()) {
                        return Err(self.unexpected(
                            "= (parameters after a defaulted one need defaults too)",
                        ));
                    }
                    None
                };
                params.push(Param { name: pname, default });
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, ")")?;
        self.expect(Tok::Colon, ":")?;
        let (body, doc) = self.parse_def_body()?;
        self.program
            .functions
            .insert(key.clone(), Function { name: key, params, body, line: def_tok.line, doc });
        Ok(())
    }

    /// Parameter defaults are literals only (docs/01: keyword defaults are
    /// values like `None`, `100`, `info`-style constants stay call-site).
    fn parse_default_literal(&mut self) -> Result<DefaultLit, PyriteError> {
        let t = self.advance();
        Ok(match t.tok {
            Tok::Int(v) => DefaultLit::Int(v),
            Tok::Minus => match self.advance().tok {
                Tok::Int(v) => DefaultLit::Int(-v),
                _ => return Err(self.unexpected("an integer after -")),
            },
            Tok::Str(s) => DefaultLit::Str(s),
            Tok::True => DefaultLit::Bool(true),
            Tok::False => DefaultLit::Bool(false),
            Tok::NoneKw => DefaultLit::NoneVal,
            _ => {
                return Err(self.unexpected(
                    "a literal default (int, string, True/False, or None)",
                ));
            }
        })
    }

    /// Call arguments: positionals, then `name=value` keywords (Python
    /// order — a positional after a keyword is an error).
    fn parse_call_args(&mut self) -> Result<CallArgs, PyriteError> {
        let mut args = Vec::new();
        let mut kwargs: Vec<(String, ExprId)> = Vec::new();
        if !self.check(&Tok::RParen) {
            loop {
                let is_kwarg = matches!(&self.peek().tok, Tok::Ident(_))
                    && matches!(self.peek2().map(|t| &t.tok), Some(Tok::Assign));
                if is_kwarg {
                    let key = self.expect_ident("keyword name")?;
                    if kwargs.iter().any(|(k, _)| *k == key) {
                        return Err(self.unexpected("a distinct keyword (duplicate)"));
                    }
                    self.expect(Tok::Assign, "=")?;
                    kwargs.push((key, self.parse_expr()?));
                } else {
                    if !kwargs.is_empty() {
                        return Err(self
                            .unexpected("a keyword argument (positionals precede keywords)"));
                    }
                    args.push(self.parse_expr()?);
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, ")")?;
        Ok((args, kwargs))
    }

    /// A def body: `parse_block`, plus Python's docstring rule — a leading
    /// bare string literal is documentation, captured here and stripped
    /// from the runtime block (it costs nothing because it doesn't exist
    /// at runtime). A docstring alone is a legal (do-nothing) body.
    fn parse_def_body(&mut self) -> Result<(Block, Option<String>), PyriteError> {
        self.expect(Tok::Newline, "newline")?;
        self.expect(Tok::Indent, "indented block")?;
        let mut doc = None;
        if let Tok::Str(s) = &self.peek().tok
            && matches!(self.peek2(), Some(t) if t.tok == Tok::Newline)
        {
            doc = Some(s.clone());
            self.advance();
            self.advance();
        }
        let mut block = Block::new();
        while !self.check(&Tok::Dedent) {
            if self.eat(&Tok::Newline) {
                continue;
            }
            match &self.peek().tok {
                Tok::Def | Tok::On | Tok::Enum | Tok::Module | Tok::Import | Tok::From => {
                    let t = self.peek();
                    return Err(PyriteError {
                        line: t.line,
                        col: t.col,
                        kind: PyriteErrorKind::HandlerNotAtTopLevel,
                    });
                }
                _ => block.push(self.parse_statement(true)?),
            }
        }
        self.expect(Tok::Dedent, "dedent")?;
        if block.is_empty() && doc.is_none() {
            return Err(PyriteError {
                line: self.peek().line,
                col: self.peek().col,
                kind: PyriteErrorKind::EmptyBlock,
            });
        }
        Ok((block, doc))
    }

    /// A `module <name>:` block — the inlined library section of a deploy
    /// artifact (the editor generates these; docs/01 "imports resolve at
    /// deploy"). Holds `def`s only.
    fn parse_module(&mut self) -> Result<(), PyriteError> {
        self.require(Construct::Functions)?;
        let module_tok = self.expect(Tok::Module, "module")?;
        let name = self.expect_ident("module name")?;
        if self.program.modules.contains(&name) {
            return Err(PyriteError {
                line: module_tok.line,
                col: module_tok.col,
                kind: PyriteErrorKind::DuplicateDefinition(name),
            });
        }
        self.expect(Tok::Colon, ":")?;
        self.expect(Tok::Newline, "newline")?;
        self.expect(Tok::Indent, "indented module body")?;
        let expr_start = self.program.exprs.len();
        let mut saw_def = false;
        while !self.check(&Tok::Dedent) {
            if self.eat(&Tok::Newline) {
                continue;
            }
            if !self.check(&Tok::Def) {
                let t = self.peek();
                return Err(PyriteError {
                    line: t.line,
                    col: t.col,
                    kind: PyriteErrorKind::StatementInModule,
                });
            }
            self.parse_def(Some(&name))?;
            saw_def = true;
        }
        self.expect(Tok::Dedent, "dedent")?;
        if !saw_def {
            return Err(PyriteError {
                line: module_tok.line,
                col: module_tok.col,
                kind: PyriteErrorKind::EmptyBlock,
            });
        }
        // Sibling calls resolve within the module: a bare call parsed in
        // this block that names a sibling def becomes qualified. Done after
        // the whole block so forward references work.
        for i in expr_start..self.program.exprs.len() {
            let Expr::Call { name: callee, .. } = &self.program.exprs[i] else { continue };
            if callee.contains('.') {
                continue;
            }
            let qualified = format!("{name}.{callee}");
            if !self.program.functions.contains_key(&qualified) {
                continue;
            }
            let Expr::Call { name: callee, .. } = &mut self.program.exprs[i] else { unreachable!() };
            *callee = qualified;
        }
        self.program.modules.insert(name);
        Ok(())
    }

    /// `import m` — makes qualified `m.f()` calls resolvable.
    fn parse_import(&mut self) -> Result<(), PyriteError> {
        self.require(Construct::Import)?;
        let import_tok = self.expect(Tok::Import, "import")?;
        let name = self.expect_ident("module name")?;
        if !self.program.modules.contains(&name) {
            return Err(PyriteError {
                line: import_tok.line,
                col: import_tok.col,
                kind: PyriteErrorKind::UnknownModule(name),
            });
        }
        self.end_of_line()?;
        self.program.imported.insert(name);
        Ok(())
    }

    /// `from m import f, g` — binds the named functions bare.
    fn parse_from_import(&mut self) -> Result<(), PyriteError> {
        self.require(Construct::Import)?;
        let from_tok = self.expect(Tok::From, "from")?;
        let module = self.expect_ident("module name")?;
        if !self.program.modules.contains(&module) {
            return Err(PyriteError {
                line: from_tok.line,
                col: from_tok.col,
                kind: PyriteErrorKind::UnknownModule(module),
            });
        }
        self.expect(Tok::Import, "import")?;
        loop {
            let t = self.peek().clone();
            let name = self.expect_ident("function name")?;
            let qualified = format!("{module}.{name}");
            if !self.program.functions.contains_key(&qualified) {
                return Err(PyriteError {
                    line: t.line,
                    col: t.col,
                    kind: PyriteErrorKind::UnknownModuleMember { module, name },
                });
            }
            if self.program.functions.contains_key(&name)
                || self.program.aliases.contains_key(&name)
            {
                return Err(PyriteError {
                    line: t.line,
                    col: t.col,
                    kind: PyriteErrorKind::DuplicateDefinition(name),
                });
            }
            self.program.aliases.insert(name, qualified);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.end_of_line()?;
        Ok(())
    }

    /// Imports are one-per-line declarations: anything trailing is an error
    /// (a bare `import m x` would otherwise parse `x` as its own statement).
    fn end_of_line(&mut self) -> Result<(), PyriteError> {
        if self.check(&Tok::Newline) || self.check(&Tok::Eof) {
            Ok(())
        } else {
            Err(self.unexpected("end of line"))
        }
    }

    fn parse_handler(&mut self) -> Result<(), PyriteError> {
        let on_tok = self.expect(Tok::On, "on")?;
        let which = self.expect_ident("signal name (error/hurt/bump/bumped/boot)")?;
        // The five player windows (docs/01). `abort` and `recall` are fully
        // engine-reserved — writing them is the same error as a typo.
        let (kind, construct) = match which.as_str() {
            "error" => (SignalKind::Error, Construct::OnError),
            "hurt" => (SignalKind::Hurt, Construct::OnHurt),
            "bump" => (SignalKind::Bump, Construct::OnBumpBumped),
            "bumped" => (SignalKind::Bumped, Construct::OnBumpBumped),
            "boot" => (SignalKind::Boot, Construct::OnBoot),
            _ => return Err(self.unexpected("error, hurt, bump, bumped, or boot")),
        };
        self.require(construct)?;
        if self.program.handlers.contains_key(&kind) {
            return Err(PyriteError {
                line: on_tok.line,
                col: on_tok.col,
                kind: PyriteErrorKind::DuplicateDefinition(format!("on {which}:")),
            });
        }
        self.expect(Tok::Colon, ":")?;
        let body = self.parse_block(false)?;
        self.program.handlers.insert(kind, Handler { kind, body, line: on_tok.line });
        Ok(())
    }

    fn parse_enum(&mut self) -> Result<(), PyriteError> {
        self.require(Construct::Enums)?;
        let enum_tok = self.expect(Tok::Enum, "enum")?;
        let name = self.expect_ident("enum name")?;
        // The builtin enums are language furniture (`None` = Option.None,
        // `.expect()` semantics) — a player redefinition would shadow them
        // in pattern/constructor resolution, which checks declared enums
        // before the builtin fallback.
        if matches!(name.as_str(), "Option" | "Result") {
            return Err(PyriteError {
                line: enum_tok.line,
                col: enum_tok.col,
                kind: PyriteErrorKind::ReservedName(name),
            });
        }
        if self.program.enums.contains_key(&name) {
            return Err(PyriteError {
                line: enum_tok.line,
                col: enum_tok.col,
                kind: PyriteErrorKind::DuplicateDefinition(name),
            });
        }
        self.expect(Tok::Colon, ":")?;
        self.expect(Tok::Newline, "newline")?;
        self.expect(Tok::Indent, "indented enum body")?;
        let mut variants = std::collections::BTreeMap::new();
        while !self.check(&Tok::Dedent) {
            if self.eat(&Tok::Newline) {
                continue;
            }
            let variant = self.expect_ident("variant name")?;
            let mut fields = Vec::new();
            if self.eat(&Tok::LParen) {
                if !self.check(&Tok::RParen) {
                    loop {
                        fields.push(self.expect_ident("field name")?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                }
                self.expect(Tok::RParen, ")")?;
            }
            if variants.contains_key(&variant) {
                return Err(PyriteError {
                    line: self.peek().line,
                    col: self.peek().col,
                    kind: PyriteErrorKind::DuplicateDefinition(variant),
                });
            }
            variants.insert(variant, fields);
            self.expect(Tok::Newline, "newline")?;
        }
        self.expect(Tok::Dedent, "dedent")?;
        if variants.is_empty() {
            return Err(PyriteError {
                line: enum_tok.line,
                col: enum_tok.col,
                kind: PyriteErrorKind::EmptyBlock,
            });
        }
        self.program.enums.insert(name.clone(), EnumDecl { name, variants, line: enum_tok.line });
        Ok(())
    }

    /// Parse an indented block of statements. `in_function` allows `return`.
    fn parse_block(&mut self, in_function: bool) -> Result<Block, PyriteError> {
        self.expect(Tok::Newline, "newline")?;
        self.expect(Tok::Indent, "indented block")?;
        let mut block = Block::new();
        while !self.check(&Tok::Dedent) {
            if self.eat(&Tok::Newline) {
                continue;
            }
            match &self.peek().tok {
                Tok::Def | Tok::On | Tok::Enum | Tok::Module | Tok::Import | Tok::From => {
                    let t = self.peek();
                    return Err(PyriteError {
                        line: t.line,
                        col: t.col,
                        kind: PyriteErrorKind::HandlerNotAtTopLevel,
                    });
                }
                _ => block.push(self.parse_statement(in_function)?),
            }
        }
        self.expect(Tok::Dedent, "dedent")?;
        if block.is_empty() {
            return Err(PyriteError {
                line: self.peek().line,
                col: self.peek().col,
                kind: PyriteErrorKind::EmptyBlock,
            });
        }
        Ok(block)
    }

    // --- statements ---

    fn parse_statement(&mut self, in_function: bool) -> Result<StmtId, PyriteError> {
        let line = self.peek().line;
        match &self.peek().tok {
            Tok::If => self.parse_if(in_function),
            Tok::While => self.parse_while(in_function),
            Tok::For => self.parse_for(in_function),
            Tok::Match => self.parse_match(in_function),
            Tok::Break => {
                self.require(Construct::WhileLoop)?;
                self.advance();
                self.expect(Tok::Newline, "newline")?;
                Ok(self.add_stmt(Stmt::Break { line }))
            }
            Tok::Continue => {
                self.require(Construct::WhileLoop)?;
                self.advance();
                self.expect(Tok::Newline, "newline")?;
                Ok(self.add_stmt(Stmt::Continue { line }))
            }
            Tok::Return => {
                self.require(Construct::Functions)?;
                if !in_function {
                    return Err(self.unexpected("return only allowed inside def"));
                }
                self.advance();
                let value = if self.check(&Tok::Newline) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect(Tok::Newline, "newline")?;
                Ok(self.add_stmt(Stmt::Return { value, line }))
            }
            // Assignment or expression statement.
            Tok::Ident(_) => {
                if matches!(self.peek2().map(|t| &t.tok), Some(Tok::Assign)) {
                    self.require(Construct::Variables)?;
                    let name = self.expect_ident("variable name")?;
                    self.expect(Tok::Assign, "=")?;
                    let value = self.parse_expr()?;
                    self.expect(Tok::Newline, "newline")?;
                    Ok(self.add_stmt(Stmt::Assign { name, value, line }))
                } else {
                    let expr = self.parse_expr()?;
                    if self.check(&Tok::Assign) {
                        // `name[index] = value` — the only other lvalue.
                        // Containers are values, so writes are rooted at a
                        // variable; nested targets (`a[0][1] = v`) aren't.
                        let Expr::Index { base, index } =
                            self.program.exprs[expr as usize].clone()
                        else {
                            return Err(self
                                .unexpected("newline (only `x = ...` or `x[key] = ...` assign)"));
                        };
                        let Expr::Name(name) = self.program.exprs[base as usize].clone() else {
                            return Err(self.unexpected(
                                "a variable before `[` (nested container writes aren't supported)",
                            ));
                        };
                        self.require(Construct::Variables)?;
                        self.advance();
                        let value = self.parse_expr()?;
                        self.expect(Tok::Newline, "newline")?;
                        return Ok(self.add_stmt(Stmt::IndexAssign { name, index, value, line }));
                    }
                    self.expect(Tok::Newline, "newline")?;
                    Ok(self.add_stmt(Stmt::Expr { expr, line }))
                }
            }
            _ => {
                let expr = self.parse_expr()?;
                self.expect(Tok::Newline, "newline")?;
                Ok(self.add_stmt(Stmt::Expr { expr, line }))
            }
        }
    }

    fn parse_if(&mut self, in_function: bool) -> Result<StmtId, PyriteError> {
        self.require(Construct::If)?;
        let line = self.peek().line;
        self.expect(Tok::If, "if")?;
        let mut arms = Vec::new();
        let cond = self.parse_expr()?;
        self.expect(Tok::Colon, ":")?;
        let body = self.parse_block(in_function)?;
        arms.push((cond, body));
        let mut else_body = None;
        loop {
            if self.check(&Tok::Elif) {
                self.advance();
                let cond = self.parse_expr()?;
                self.expect(Tok::Colon, ":")?;
                let body = self.parse_block(in_function)?;
                arms.push((cond, body));
            } else if self.check(&Tok::Else) {
                self.advance();
                self.expect(Tok::Colon, ":")?;
                else_body = Some(self.parse_block(in_function)?);
                break;
            } else {
                break;
            }
        }
        Ok(self.add_stmt(Stmt::If { arms, else_body, line }))
    }

    fn parse_while(&mut self, in_function: bool) -> Result<StmtId, PyriteError> {
        self.require(Construct::WhileLoop)?;
        let line = self.peek().line;
        self.expect(Tok::While, "while")?;
        let cond = self.parse_expr()?;
        self.expect(Tok::Colon, ":")?;
        let body = self.parse_block(in_function)?;
        Ok(self.add_stmt(Stmt::While { cond, body, line }))
    }

    fn parse_for(&mut self, in_function: bool) -> Result<StmtId, PyriteError> {
        self.require(Construct::Lists)?;
        let line = self.peek().line;
        self.expect(Tok::For, "for")?;
        let var = self.expect_ident("loop variable")?;
        self.expect(Tok::In, "in")?;
        let iter = self.parse_expr()?;
        self.expect(Tok::Colon, ":")?;
        let body = self.parse_block(in_function)?;
        Ok(self.add_stmt(Stmt::For { var, iter, body, line }))
    }

    fn parse_match(&mut self, in_function: bool) -> Result<StmtId, PyriteError> {
        self.require(Construct::Enums)?;
        let line = self.peek().line;
        self.expect(Tok::Match, "match")?;
        let scrutinee = self.parse_expr()?;
        self.expect(Tok::Colon, ":")?;
        self.expect(Tok::Newline, "newline")?;
        self.expect(Tok::Indent, "indented match body")?;
        let mut cases = Vec::new();
        while !self.check(&Tok::Dedent) {
            if self.eat(&Tok::Newline) {
                continue;
            }
            self.expect(Tok::Case, "case")?;
            // Rust-style catch-all: `case _:`.
            if let Tok::Ident(name) = &self.peek().tok
                && name == "_"
            {
                self.advance();
                self.expect(Tok::Colon, ":")?;
                let body = self.parse_block(in_function)?;
                cases.push(MatchCase { pattern: Pattern::Wildcard, body });
                continue;
            }
            // `case None:` — sugar for `case Option.None:` (docs/01 Types).
            if self.check(&Tok::NoneKw) {
                self.advance();
                self.expect(Tok::Colon, ":")?;
                let body = self.parse_block(in_function)?;
                cases.push(MatchCase {
                    pattern: Pattern::EnumVariant {
                        enum_name: "Option".to_string(),
                        variant: "None".to_string(),
                        binds: Vec::new(),
                    },
                    body,
                });
                continue;
            }
            let enum_name = self.expect_ident("enum name")?;
            self.expect(Tok::Dot, ".")?;
            let variant = self.expect_ident_or_none("variant name")?;
            let mut binds = Vec::new();
            if self.eat(&Tok::LParen) {
                if !self.check(&Tok::RParen) {
                    loop {
                        binds.push(self.expect_ident("binding name")?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                }
                self.expect(Tok::RParen, ")")?;
            }
            self.expect(Tok::Colon, ":")?;
            let body = self.parse_block(in_function)?;
            cases.push(MatchCase { pattern: Pattern::EnumVariant { enum_name, variant, binds }, body });
        }
        self.expect(Tok::Dedent, "dedent")?;
        if cases.is_empty() {
            return Err(PyriteError {
                line,
                col: 1,
                kind: PyriteErrorKind::EmptyBlock,
            });
        }
        Ok(self.add_stmt(Stmt::Match { scrutinee, cases, line }))
    }

    // --- expressions (precedence climbing) ---

    fn parse_expr(&mut self) -> Result<ExprId, PyriteError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<ExprId, PyriteError> {
        let mut lhs = self.parse_and()?;
        while self.check(&Tok::Or) {
            self.advance();
            let rhs = self.parse_and()?;
            lhs = self.add_expr(Expr::Binary { op: BinOp::Or, lhs, rhs });
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<ExprId, PyriteError> {
        let mut lhs = self.parse_not()?;
        while self.check(&Tok::And) {
            self.advance();
            let rhs = self.parse_not()?;
            lhs = self.add_expr(Expr::Binary { op: BinOp::And, lhs, rhs });
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<ExprId, PyriteError> {
        if self.check(&Tok::Not) {
            self.advance();
            let operand = self.parse_not()?;
            Ok(self.add_expr(Expr::Unary { op: UnOp::Not, operand }))
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Result<ExprId, PyriteError> {
        let lhs = self.parse_additive()?;
        let op = match &self.peek().tok {
            Tok::EqEq => Some(BinOp::Eq),
            Tok::NotEq => Some(BinOp::NotEq),
            Tok::Lt => Some(BinOp::Lt),
            Tok::Gt => Some(BinOp::Gt),
            Tok::Le => Some(BinOp::Le),
            Tok::Ge => Some(BinOp::Ge),
            // Membership: `key in dict`, `item in list`, `sub in string`.
            // (`for x in xs` never reaches here — parse_for eats its `in`.)
            Tok::In => Some(BinOp::In),
            _ => None,
        };
        if let Some(op) = op {
            if op == BinOp::In {
                self.require(Construct::Lists)?;
            }
            self.advance();
            let rhs = self.parse_additive()?;
            Ok(self.add_expr(Expr::Binary { op, lhs, rhs }))
        } else {
            Ok(lhs)
        }
    }

    fn parse_additive(&mut self) -> Result<ExprId, PyriteError> {
        let mut lhs = self.parse_multiplicative()?;
        loop {
            let op = match &self.peek().tok {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_multiplicative()?;
            lhs = self.add_expr(Expr::Binary { op, lhs, rhs });
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> Result<ExprId, PyriteError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match &self.peek().tok {
                Tok::Star => BinOp::Mul,
                Tok::SlashSlash => BinOp::FloorDiv,
                Tok::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = self.add_expr(Expr::Binary { op, lhs, rhs });
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<ExprId, PyriteError> {
        if self.check(&Tok::Minus) {
            self.advance();
            let operand = self.parse_unary()?;
            Ok(self.add_expr(Expr::Unary { op: UnOp::Neg, operand }))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<ExprId, PyriteError> {
        let mut expr = self.parse_atom()?;
        loop {
            if self.check(&Tok::Dot) {
                let dot_line = self.peek().line;
                self.advance();
                let name = self.expect_ident_or_none("attribute name")?;
                // `EnumName.Variant` / `EnumName.Variant(args)` resolve at
                // parse time when the enum is declared in this program.
                let (base_enum, base_module) = match self.program.exprs[expr as usize].clone() {
                    Expr::Name(n) if self.program.enums.contains_key(&n) => (Some(n), None),
                    // Builtin enums are constructible from source (docs/01
                    // Types): Option.Some(v)/Option.None, Result.Ok/Err.
                    Expr::Name(n) if n == "Option" || n == "Result" => (Some(n), None),
                    Expr::Name(n) if self.program.modules.contains(&n) => (None, Some(n)),
                    _ => (None, None),
                };
                if let Some(module) = base_module {
                    // `hauling.haul_home(...)` — a qualified module call,
                    // legal only under a plain `import hauling`.
                    if !self.program.imported.contains(&module) {
                        return Err(PyriteError {
                            line: dot_line,
                            col: self.peek().col,
                            kind: PyriteErrorKind::ModuleNotImported(module),
                        });
                    }
                    let qualified = format!("{module}.{name}");
                    if !self.program.functions.contains_key(&qualified) {
                        return Err(PyriteError {
                            line: dot_line,
                            col: self.peek().col,
                            kind: PyriteErrorKind::UnknownModuleMember { module, name },
                        });
                    }
                    self.expect(Tok::LParen, "( — modules export functions, call them")?;
                    let (args, kwargs) = self.parse_call_args()?;
                    expr =
                        self.add_expr(Expr::Call { name: qualified, args, kwargs, line: dot_line });
                } else if let Some(enum_name) = base_enum {
                    let arity = match self.program.enums.get(&enum_name) {
                        Some(decl) => match decl.variants.get(&name) {
                            Some(fields) => fields.len(),
                            None => {
                                let t = self.peek();
                                return Err(PyriteError {
                                    line: t.line,
                                    col: t.col,
                                    kind: PyriteErrorKind::UnknownEnumVariant {
                                        enum_name,
                                        variant: name,
                                    },
                                });
                            }
                        },
                        // Builtin enums: fixed variants and arities.
                        None => match (enum_name.as_str(), name.as_str()) {
                            ("Option", "Some") | ("Result", "Ok") | ("Result", "Err") => 1,
                            ("Option", "None") => 0,
                            _ => {
                                let t = self.peek();
                                return Err(PyriteError {
                                    line: t.line,
                                    col: t.col,
                                    kind: PyriteErrorKind::UnknownEnumVariant {
                                        enum_name,
                                        variant: name,
                                    },
                                });
                            }
                        },
                    };
                    if self.eat(&Tok::LParen) {
                        let mut args = Vec::new();
                        if !self.check(&Tok::RParen) {
                            loop {
                                args.push(self.parse_expr()?);
                                if !self.eat(&Tok::Comma) {
                                    break;
                                }
                            }
                        }
                        self.expect(Tok::RParen, ")")?;
                        expr = self.add_expr(Expr::EnumCtor { enum_name, variant: name, args });
                    } else {
                        if arity != 0 {
                            let t = self.peek();
                            return Err(PyriteError {
                                line: t.line,
                                col: t.col,
                                kind: PyriteErrorKind::UnexpectedToken {
                                    found: format!("{:?}", t.tok),
                                    expected: format!("({arity} field(s) for this variant)"),
                                },
                            });
                        }
                        expr = self.add_expr(Expr::EnumUnit { enum_name, variant: name });
                    }
                } else if self.eat(&Tok::LParen) {
                    let (args, kwargs) = self.parse_call_args()?;
                    if !kwargs.is_empty() {
                        return Err(self.unexpected("positional arguments (methods take no keywords)"));
                    }
                    expr = self.add_expr(Expr::MethodCall {
                        base: expr,
                        name,
                        args,
                        line: dot_line,
                    });
                } else {
                    expr = self.add_expr(Expr::Attr { base: expr, name });
                }
            } else if self.check(&Tok::LBracket) {
                self.require(Construct::Lists)?;
                self.advance();
                let index = self.parse_expr()?;
                self.expect(Tok::RBracket, "]")?;
                expr = self.add_expr(Expr::Index { base: expr, index });
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_atom(&mut self) -> Result<ExprId, PyriteError> {
        let t = self.advance();
        match t.tok {
            Tok::Int(v) => Ok(self.add_expr(Expr::Int(v))),
            Tok::Str(s) => Ok(self.add_expr(Expr::Str(s))),
            Tok::True => Ok(self.add_expr(Expr::Bool(true))),
            Tok::False => Ok(self.add_expr(Expr::Bool(false))),
            Tok::NoneKw => {
                // `None` is reserved sugar for Option.None (docs/01 Types).
                Ok(self.add_expr(Expr::EnumUnit {
                    enum_name: "Option".to_string(),
                    variant: "None".to_string(),
                }))
            }
            Tok::Ident(name) => {
                if self.check(&Tok::LParen) {
                    self.advance();
                    let (args, kwargs) = self.parse_call_args()?;
                    Ok(self.add_expr(Expr::Call { name, args, kwargs, line: t.line }))
                } else {
                    Ok(self.add_expr(Expr::Name(name)))
                }
            }
            Tok::LParen => {
                let inner = self.parse_expr()?;
                self.expect(Tok::RParen, ")")?;
                Ok(inner)
            }
            Tok::LBracket => {
                self.pos -= 1; // restore for require()'s error position
                self.require(Construct::Lists)?;
                self.pos += 1;
                let mut items = Vec::new();
                if !self.check(&Tok::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                }
                self.expect(Tok::RBracket, "]")?;
                Ok(self.add_expr(Expr::List(items)))
            }
            Tok::LBrace => {
                self.pos -= 1; // restore for require()'s error position
                self.require(Construct::Lists)?;
                self.pos += 1;
                let mut entries = Vec::new();
                if !self.check(&Tok::RBrace) {
                    loop {
                        let key = self.parse_expr()?;
                        self.expect(Tok::Colon, ":")?;
                        let value = self.parse_expr()?;
                        entries.push((key, value));
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                }
                self.expect(Tok::RBrace, "}")?;
                Ok(self.add_expr(Expr::Dict(entries)))
            }
            other => Err(PyriteError {
                line: t.line,
                col: t.col,
                kind: PyriteErrorKind::UnexpectedToken {
                    found: format!("{other:?}"),
                    expected: "expression".to_string(),
                },
            }),
        }
    }

    fn expect_ident(&mut self, expected: &str) -> Result<String, PyriteError> {
        match &self.peek().tok {
            Tok::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(self.unexpected(expected)),
        }
    }

    /// An identifier, or the reserved `None` (as the literal name "None") —
    /// for variant positions: `Option.None` in expressions and patterns.
    fn expect_ident_or_none(&mut self, expected: &str) -> Result<String, PyriteError> {
        if self.check(&Tok::NoneKw) {
            self.advance();
            return Ok("None".to_string());
        }
        self.expect_ident(expected)
    }
}
