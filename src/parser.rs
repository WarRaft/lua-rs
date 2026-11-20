//! A recursive-descent parser for Lua 5.3, following the reference
//! grammar (`lparser.c`). Binary operators use Lua's own priority pairs, so
//! `..` and `^` come out right-associative and `-x^2` is `-(x^2)`.

use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::{Kw, Punct, Token, TokenKind};
use parser_rs::{TokenParser, TokenStream};

pub use parser_rs::ParseError;

type PResult<T> = Result<T, ParseError>;

pub struct Parser {
    ts: TokenStream<TokenKind>,
}

impl TokenParser for Parser {
    type Kind = TokenKind;

    fn tokens(&mut self) -> &mut TokenStream<TokenKind> {
        &mut self.ts
    }
}

/// Unary operators bind tighter than every binary one except `^`.
const UNARY_PRIORITY: u8 = 12;

/// `(left, right)` priorities, as in `lparser.c`: right < left makes an
/// operator right-associative.
fn binop(kind: &TokenKind) -> Option<(BinOp, u8, u8)> {
    Some(match kind {
        TokenKind::Keyword(Kw::Or) => (BinOp::Or, 1, 1),
        TokenKind::Keyword(Kw::And) => (BinOp::And, 2, 2),
        TokenKind::Punct(p) => match p {
            Punct::Lt => (BinOp::Lt, 3, 3),
            Punct::Gt => (BinOp::Gt, 3, 3),
            Punct::LtEq => (BinOp::Le, 3, 3),
            Punct::GtEq => (BinOp::Ge, 3, 3),
            Punct::TildeEq => (BinOp::Ne, 3, 3),
            Punct::EqEq => (BinOp::Eq, 3, 3),
            Punct::Pipe => (BinOp::BOr, 4, 4),
            Punct::Tilde => (BinOp::BXor, 5, 5),
            Punct::Amp => (BinOp::BAnd, 6, 6),
            Punct::Shl => (BinOp::Shl, 7, 7),
            Punct::Shr => (BinOp::Shr, 7, 7),
            Punct::DotDot => (BinOp::Concat, 9, 8),
            Punct::Plus => (BinOp::Add, 10, 10),
            Punct::Minus => (BinOp::Sub, 10, 10),
            Punct::Star => (BinOp::Mul, 11, 11),
            Punct::Slash => (BinOp::Div, 11, 11),
            Punct::SlashSlash => (BinOp::IDiv, 11, 11),
            Punct::Percent => (BinOp::Mod, 11, 11),
            Punct::Caret => (BinOp::Pow, 14, 13),
            _ => return None,
        },
        _ => return None,
    })
}

fn kw(k: Kw) -> TokenKind {
    TokenKind::Keyword(k)
}

fn p(p: Punct) -> TokenKind {
    TokenKind::Punct(p)
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            ts: TokenStream::new(tokens),
        }
    }

    pub fn parse_str(src: &str) -> PResult<Chunk> {
        Parser::new(Lexer::tokenize(src)?).chunk()
    }

    fn check(&self, k: &TokenKind) -> bool {
        self.ts.check(k)
    }

    fn eat(&mut self, k: &TokenKind) -> bool {
        self.ts.eat(k)
    }

    fn expect(&mut self, k: TokenKind) -> PResult<Token> {
        self.ts.expect(k)
    }

    fn error(&self, msg: impl Into<String>) -> ParseError {
        self.ts.error(msg)
    }

    fn line(&self) -> usize {
        self.ts.peek().line
    }

    /// `expect` for a block's closing keyword, naming the opener and its
    /// line, the way Lua reports it: `'end' expected (to close 'if' at line 3)`.
    fn close(&mut self, what: Kw, opener: &str, line: usize) -> PResult<()> {
        if self.eat(&kw(what)) {
            return Ok(());
        }
        let found = self.ts.peek_kind().clone();
        let to_close = if line == self.line() {
            String::new()
        } else {
            format!(" (to close '{opener}' at line {line})")
        };
        Err(self.error(format!("'{what}' expected{to_close}, found {found}")))
    }

    fn name(&mut self) -> PResult<Name> {
        match self.ts.peek_kind().clone() {
            TokenKind::Ident(name) => {
                let t = self.ts.advance();
                Ok(Name {
                    name,
                    span: t.span(),
                })
            }
            other => Err(self.error(format!("expected a name, found {other}"))),
        }
    }

    pub fn chunk(&mut self) -> PResult<Chunk> {
        let body = self.block()?;
        if !self.ts.at_eof() {
            let found = self.ts.peek_kind().clone();
            return Err(self.error(format!("expected end of file, found {found}")));
        }
        Ok(Chunk { body })
    }

    /// Whether the next token ends a block.
    fn block_follows(&self) -> bool {
        matches!(
            self.ts.peek_kind(),
            TokenKind::Eof
                | TokenKind::Keyword(Kw::Else | Kw::ElseIf | Kw::End | Kw::Until)
        )
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.block_follows() {
            if self.check(&kw(Kw::Return)) {
                stmts.push(self.return_stmt()?);
                break; // `return` must be the last statement of a block
            }
            if let Some(stmt) = self.stmt()? {
                stmts.push(stmt);
            }
        }
        Ok(stmts)
    }

    fn return_stmt(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect(kw(Kw::Return))?;
        let values = if self.block_follows() || self.check(&p(Punct::Semicolon)) {
            Vec::new()
        } else {
            self.expr_list()?
        };
        self.eat(&p(Punct::Semicolon));
        if !self.block_follows() {
            let found = self.ts.peek_kind().clone();
            return Err(self.error(format!(
                "'return' must be the last statement in its block, found {found} after it"
            )));
        }
        Ok(Stmt::Return { values, line })
    }

    /// One statement, or `None` for an empty `;`.
    fn stmt(&mut self) -> PResult<Option<Stmt>> {
        let line = self.line();
        let stmt = match self.ts.peek_kind().clone() {
            TokenKind::Punct(Punct::Semicolon) => {
                self.ts.advance();
                return Ok(None);
            }
            TokenKind::Keyword(Kw::If) => self.if_stmt()?,
            TokenKind::Keyword(Kw::While) => {
                self.ts.advance();
                let cond = self.expr()?;
                self.expect(kw(Kw::Do))?;
                let body = self.block()?;
                self.close(Kw::End, "while", line)?;
                Stmt::While { cond, body, line }
            }
            TokenKind::Keyword(Kw::Do) => {
                self.ts.advance();
                let body = self.block()?;
                self.close(Kw::End, "do", line)?;
                Stmt::Do { body, line }
            }
            TokenKind::Keyword(Kw::For) => self.for_stmt()?,
            TokenKind::Keyword(Kw::Repeat) => {
                self.ts.advance();
                let body = self.block()?;
                self.close(Kw::Until, "repeat", line)?;
                let cond = self.expr()?;
                Stmt::Repeat { body, cond, line }
            }
            TokenKind::Keyword(Kw::Function) => {
                self.ts.advance();
                let mut path = vec![self.name()?];
                while self.eat(&p(Punct::Dot)) {
                    path.push(self.name()?);
                }
                let method = if self.eat(&p(Punct::Colon)) {
                    Some(self.name()?)
                } else {
                    None
                };
                let func = self.func_body(line)?;
                Stmt::Function {
                    name: FuncName { path, method },
                    func,
                    line,
                }
            }
            TokenKind::Keyword(Kw::Local) => {
                self.ts.advance();
                if self.eat(&kw(Kw::Function)) {
                    let name = self.name()?;
                    let func = self.func_body(line)?;
                    Stmt::LocalFunction { name, func, line }
                } else {
                    let mut names = Vec::new();
                    loop {
                        let name = self.name()?;
                        let attrib = if self.eat(&p(Punct::Lt)) {
                            let a = self.name()?;
                            self.expect(p(Punct::Gt))?;
                            Some(a.name)
                        } else {
                            None
                        };
                        names.push(LocalName { name, attrib });
                        if !self.eat(&p(Punct::Comma)) {
                            break;
                        }
                    }
                    let values = if self.eat(&p(Punct::Assign)) {
                        self.expr_list()?
                    } else {
                        Vec::new()
                    };
                    Stmt::Local {
                        names,
                        values,
                        line,
                    }
                }
            }
            TokenKind::Punct(Punct::ColonColon) => {
                self.ts.advance();
                let name = self.name()?;
                self.expect(p(Punct::ColonColon))?;
                Stmt::Label { name, line }
            }
            TokenKind::Keyword(Kw::Break) => {
                self.ts.advance();
                Stmt::Break { line }
            }
            TokenKind::Keyword(Kw::Goto) => {
                self.ts.advance();
                let label = self.name()?;
                Stmt::Goto { label, line }
            }
            _ => self.expr_stmt(line)?,
        };
        Ok(Some(stmt))
    }

    fn if_stmt(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect(kw(Kw::If))?;
        let mut branches = Vec::new();
        loop {
            let cond = self.expr()?;
            self.expect(kw(Kw::Then))?;
            branches.push((cond, self.block()?));
            if !self.eat(&kw(Kw::ElseIf)) {
                break;
            }
        }
        let else_body = if self.eat(&kw(Kw::Else)) {
            Some(self.block()?)
        } else {
            None
        };
        self.close(Kw::End, "if", line)?;
        Ok(Stmt::If {
            branches,
            else_body,
            line,
        })
    }

    fn for_stmt(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect(kw(Kw::For))?;
        let first = self.name()?;
        if self.eat(&p(Punct::Assign)) {
            let start = self.expr()?;
            self.expect(p(Punct::Comma))?;
            let stop = self.expr()?;
            let step = if self.eat(&p(Punct::Comma)) {
                Some(self.expr()?)
            } else {
                None
            };
            self.expect(kw(Kw::Do))?;
            let body = self.block()?;
            self.close(Kw::End, "for", line)?;
            return Ok(Stmt::NumericFor {
                var: first,
                start,
                stop,
                step,
                body,
                line,
            });
        }
        let mut names = vec![first];
        while self.eat(&p(Punct::Comma)) {
            names.push(self.name()?);
        }
        if !self.eat(&kw(Kw::In)) {
            let found = self.ts.peek_kind().clone();
            return Err(self.error(format!("'=' or 'in' expected, found {found}")));
        }
        let exprs = self.expr_list()?;
        self.expect(kw(Kw::Do))?;
        let body = self.block()?;
        self.close(Kw::End, "for", line)?;
        Ok(Stmt::GenericFor {
            names,
            exprs,
            body,
            line,
        })
    }

    /// `(params) block end`
    fn func_body(&mut self, line: usize) -> PResult<Function> {
        self.expect(p(Punct::LParen))?;
        let mut params = Vec::new();
        let mut vararg = false;
        if !self.check(&p(Punct::RParen)) {
            loop {
                if self.eat(&p(Punct::Ellipsis)) {
                    vararg = true;
                    break;
                }
                params.push(self.name()?);
                if !self.eat(&p(Punct::Comma)) {
                    break;
                }
            }
        }
        self.expect(p(Punct::RParen))?;
        let body = self.block()?;
        self.close(Kw::End, "function", line)?;
        Ok(Function {
            params,
            vararg,
            body,
            line,
        })
    }

    /// An assignment (`a, b.c = ...`) or a call statement.
    fn expr_stmt(&mut self, line: usize) -> PResult<Stmt> {
        let start = self.ts.peek().clone();
        let first = self.suffixed_expr()?;
        if self.check(&p(Punct::Assign)) || self.check(&p(Punct::Comma)) {
            let mut targets = vec![first];
            while self.eat(&p(Punct::Comma)) {
                targets.push(self.suffixed_expr()?);
            }
            for t in &targets {
                if !matches!(
                    t.kind,
                    ExprKind::Name(_) | ExprKind::Field(..) | ExprKind::Index(..)
                ) {
                    return Err(ParseError::at_token("cannot assign to this expression", &start));
                }
            }
            self.expect(p(Punct::Assign))?;
            let values = self.expr_list()?;
            return Ok(Stmt::Assign {
                targets,
                values,
                line,
            });
        }
        if matches!(first.kind, ExprKind::Call(..) | ExprKind::MethodCall(..)) {
            return Ok(Stmt::Call { call: first, line });
        }
        let found = self.ts.peek_kind().clone();
        Err(ParseError::at_token(
            format!("syntax error: expected '=' or a call, found {found}"),
            &start,
        ))
    }

    fn expr_list(&mut self) -> PResult<Vec<Expr>> {
        let mut exprs = vec![self.expr()?];
        while self.eat(&p(Punct::Comma)) {
            exprs.push(self.expr()?);
        }
        Ok(exprs)
    }

    pub fn expr(&mut self) -> PResult<Expr> {
        self.sub_expr(0)
    }

    /// Precedence climbing with Lua's `(left, right)` priorities.
    fn sub_expr(&mut self, limit: u8) -> PResult<Expr> {
        let unop = match self.ts.peek_kind() {
            TokenKind::Keyword(Kw::Not) => Some(UnOp::Not),
            TokenKind::Punct(Punct::Minus) => Some(UnOp::Neg),
            TokenKind::Punct(Punct::Hash) => Some(UnOp::Len),
            TokenKind::Punct(Punct::Tilde) => Some(UnOp::BNot),
            _ => None,
        };
        let mut left = match unop {
            Some(op) => {
                let op_tok = self.ts.advance();
                let operand = self.sub_expr(UNARY_PRIORITY)?;
                let span = op_tok.span().join(operand.span);
                Expr::new(ExprKind::Unary(op, Box::new(operand)), span)
            }
            None => self.simple_expr()?,
        };
        while let Some((op, lp, rp)) = binop(self.ts.peek_kind()) {
            if lp <= limit {
                break;
            }
            self.ts.advance();
            let right = self.sub_expr(rp)?;
            let span = left.span.join(right.span);
            left = Expr::new(ExprKind::Binary(Box::new(left), op, Box::new(right)), span);
        }
        Ok(left)
    }

    fn simple_expr(&mut self) -> PResult<Expr> {
        let tok = self.ts.peek().clone();
        let leaf = |p: &mut Self, kind| {
            p.ts.advance();
            Ok(Expr::new(kind, tok.span()))
        };
        match tok.kind.clone() {
            TokenKind::Keyword(Kw::Nil) => leaf(self, ExprKind::Nil),
            TokenKind::Keyword(Kw::True) => leaf(self, ExprKind::Bool(true)),
            TokenKind::Keyword(Kw::False) => leaf(self, ExprKind::Bool(false)),
            TokenKind::Int(v) => leaf(self, ExprKind::Int(v)),
            TokenKind::Real(v) => leaf(self, ExprKind::Real(v)),
            TokenKind::Str(s) => leaf(self, ExprKind::Str(s)),
            TokenKind::Punct(Punct::Ellipsis) => leaf(self, ExprKind::Vararg),
            TokenKind::Punct(Punct::LBrace) => self.table(),
            TokenKind::Keyword(Kw::Function) => {
                self.ts.advance();
                let func = self.func_body(tok.line)?;
                let span = tok.span().join(self.ts.prev().span());
                Ok(Expr::new(ExprKind::Function(Box::new(func)), span))
            }
            _ => self.suffixed_expr(),
        }
    }

    fn primary_expr(&mut self) -> PResult<Expr> {
        let tok = self.ts.peek().clone();
        match tok.kind.clone() {
            TokenKind::Ident(name) => {
                self.ts.advance();
                Ok(Expr::new(ExprKind::Name(name), tok.span()))
            }
            TokenKind::Punct(Punct::LParen) => {
                self.ts.advance();
                let inner = self.expr()?;
                let close = self.expect(p(Punct::RParen))?;
                Ok(Expr::new(
                    ExprKind::Paren(Box::new(inner)),
                    tok.span().join(close.span()),
                ))
            }
            other => Err(self.error(format!("unexpected {other} in expression"))),
        }
    }

    /// `primary { .name | [exp] | :name args | args }`
    fn suffixed_expr(&mut self) -> PResult<Expr> {
        let mut e = self.primary_expr()?;
        loop {
            match self.ts.peek_kind().clone() {
                TokenKind::Punct(Punct::Dot) => {
                    self.ts.advance();
                    let name = self.name()?;
                    let span = e.span.join(name.span);
                    e = Expr::new(ExprKind::Field(Box::new(e), name.name), span);
                }
                TokenKind::Punct(Punct::LBracket) => {
                    self.ts.advance();
                    let key = self.expr()?;
                    let close = self.expect(p(Punct::RBracket))?;
                    let span = e.span.join(close.span());
                    e = Expr::new(ExprKind::Index(Box::new(e), Box::new(key)), span);
                }
                TokenKind::Punct(Punct::Colon) => {
                    self.ts.advance();
                    let name = self.name()?;
                    let args = self.call_args()?;
                    let span = e.span.join(self.ts.prev().span());
                    e = Expr::new(ExprKind::MethodCall(Box::new(e), name.name, args), span);
                }
                TokenKind::Punct(Punct::LParen | Punct::LBrace) | TokenKind::Str(_) => {
                    let args = self.call_args()?;
                    let span = e.span.join(self.ts.prev().span());
                    e = Expr::new(ExprKind::Call(Box::new(e), args), span);
                }
                _ => return Ok(e),
            }
        }
    }

    /// `( explist )`, `{ table }`, or `"string"`.
    fn call_args(&mut self) -> PResult<Vec<Expr>> {
        match self.ts.peek_kind().clone() {
            TokenKind::Str(s) => {
                let t = self.ts.advance();
                Ok(vec![Expr::new(ExprKind::Str(s), t.span())])
            }
            TokenKind::Punct(Punct::LBrace) => Ok(vec![self.table()?]),
            TokenKind::Punct(Punct::LParen) => {
                self.ts.advance();
                let args = if self.check(&p(Punct::RParen)) {
                    Vec::new()
                } else {
                    self.expr_list()?
                };
                self.expect(p(Punct::RParen))?;
                Ok(args)
            }
            other => Err(self.error(format!("function arguments expected, found {other}"))),
        }
    }

    /// `{ [k] = v, name = v, v; ... }`
    fn table(&mut self) -> PResult<Expr> {
        let open = self.expect(p(Punct::LBrace))?;
        let mut fields = Vec::new();
        while !self.check(&p(Punct::RBrace)) {
            let field = if self.check(&p(Punct::LBracket)) {
                self.ts.advance();
                let key = self.expr()?;
                self.expect(p(Punct::RBracket))?;
                self.expect(p(Punct::Assign))?;
                Field::Keyed(key, self.expr()?)
            } else if matches!(self.ts.peek_kind(), TokenKind::Ident(_))
                && self.ts.nth_kind(1) == &p(Punct::Assign)
            {
                let name = self.name()?;
                self.ts.advance(); // `=`
                Field::Named(name, self.expr()?)
            } else {
                Field::Positional(self.expr()?)
            };
            fields.push(field);
            if !self.eat(&p(Punct::Comma)) && !self.eat(&p(Punct::Semicolon)) {
                break;
            }
        }
        let close = self.expect(p(Punct::RBrace))?;
        Ok(Expr::new(ExprKind::Table(fields), open.span().join(close.span())))
    }
}

#[cfg(test)]
#[path = "parser.test.rs"]
mod tests;
