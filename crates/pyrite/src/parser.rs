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
                Tok::Def => self.parse_def()?,
                Tok::On => self.parse_handler()?,
                Tok::Enum => self.parse_enum()?,
                _ => {
                    let stmt = self.parse_statement(false)?;
                    body.push(stmt);
                }
            }
        }
        self.program.body = body;
        Ok(self.program)
    }

    fn parse_def(&mut self) -> Result<(), PyriteError> {
        self.require(Construct::Functions)?;
        let def_tok = self.expect(Tok::Def, "def")?;
        let name = self.expect_ident("function name")?;
        if self.program.functions.contains_key(&name) {
            return Err(PyriteError {
                line: def_tok.line,
                col: def_tok.col,
                kind: PyriteErrorKind::DuplicateDefinition(name),
            });
        }
        self.expect(Tok::LParen, "(")?;
        let mut params = Vec::new();
        if !self.check(&Tok::RParen) {
            loop {
                params.push(self.expect_ident("parameter name")?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, ")")?;
        self.expect(Tok::Colon, ":")?;
        let body = self.parse_block(true)?;
        self.program
            .functions
            .insert(name.clone(), Function { name, params, body, line: def_tok.line });
        Ok(())
    }

    fn parse_handler(&mut self) -> Result<(), PyriteError> {
        let on_tok = self.expect(Tok::On, "on")?;
        let which = self.expect_ident("handler name (signal or death)")?;
        let (kind, construct) = match which.as_str() {
            "signal" => (SignalKind::Signal, Construct::OnSignal),
            "death" => (SignalKind::Death, Construct::OnDeath),
            _ => return Err(self.unexpected("signal or death")),
        };
        self.require(construct)?;
        // `on signal(s):` binds the incoming Signal value.
        let mut binding = None;
        if kind == SignalKind::Signal && self.eat(&Tok::LParen) {
            binding = Some(self.expect_ident("binding name")?);
            self.expect(Tok::RParen, ")")?;
        }
        if self.program.handlers.contains_key(&kind) {
            return Err(PyriteError {
                line: on_tok.line,
                col: on_tok.col,
                kind: PyriteErrorKind::DuplicateDefinition(format!("on {which}:")),
            });
        }
        self.expect(Tok::Colon, ":")?;
        let body = self.parse_block(false)?;
        self.program
            .handlers
            .insert(kind, Handler { kind, binding, body, line: on_tok.line });
        Ok(())
    }

    fn parse_enum(&mut self) -> Result<(), PyriteError> {
        self.require(Construct::Enums)?;
        let enum_tok = self.expect(Tok::Enum, "enum")?;
        let name = self.expect_ident("enum name")?;
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
                Tok::Def | Tok::On | Tok::Enum => {
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
            let enum_name = self.expect_ident("enum name")?;
            self.expect(Tok::Dot, ".")?;
            let variant = self.expect_ident("variant name")?;
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
            _ => None,
        };
        if let Some(op) = op {
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
                let name = self.expect_ident("attribute name")?;
                // `EnumName.Variant` / `EnumName.Variant(args)` resolve at
                // parse time when the enum is declared in this program.
                let base_enum = match self.program.exprs[expr as usize].clone() {
                    Expr::Name(n) if self.program.enums.contains_key(&n) => Some(n),
                    _ => None,
                };
                if let Some(enum_name) = base_enum {
                    let decl = &self.program.enums[&enum_name];
                    let Some(fields) = decl.variants.get(&name) else {
                        let t = self.peek();
                        return Err(PyriteError {
                            line: t.line,
                            col: t.col,
                            kind: PyriteErrorKind::UnknownEnumVariant { enum_name, variant: name },
                        });
                    };
                    let arity = fields.len();
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
            Tok::Ident(name) => {
                if self.check(&Tok::LParen) {
                    self.advance();
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
                    Ok(self.add_expr(Expr::Call { name, args, line: t.line }))
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
}
