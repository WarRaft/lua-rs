//! Lint rules for Lua. Locals are resolved with Lua's real scoping rules
//! (block scopes, `local x = x` seeing the *outer* `x`, `local function f`
//! visible in its own body, `repeat ... until cond` seeing the body's
//! locals), so "unused" means unused.

use crate::ast::*;
use std::collections::HashMap;

pub use parser_rs::{Diagnostic, Level};
use parser_rs::rule;

/// Runs every lint rule over a chunk and returns the diagnostics found,
/// sorted by source line.
pub fn lint(chunk: &Chunk) -> Vec<Diagnostic> {
    let mut l = Linter::default();
    // The main chunk is a vararg function.
    l.function(&[], true, &chunk.body, 0);
    l.diags.sort_by_key(|d| d.line);
    l.diags
}

#[derive(Clone, Copy, PartialEq)]
enum LocalKind {
    Variable,
    Function,
    /// Parameters and loop variables: never reported as unused.
    Quiet,
}

struct Local {
    name: String,
    line: usize,
    kind: LocalKind,
    used: bool,
}

#[derive(Default)]
struct Linter {
    diags: Vec<Diagnostic>,
    locals: Vec<Local>,
    /// Indices into `locals` where each open block starts.
    blocks: Vec<usize>,
    /// Per open function: whether it's vararg, and its loop depth.
    functions: Vec<(bool, usize)>,
}

impl Linter {
    fn declare(&mut self, name: &str, line: usize, kind: LocalKind) {
        self.locals.push(Local {
            name: name.to_string(),
            line,
            kind,
            used: false,
        });
    }

    fn use_name(&mut self, name: &str) {
        if let Some(l) = self.locals.iter_mut().rev().find(|l| l.name == name) {
            l.used = true;
        }
    }

    fn open_block(&mut self) {
        self.blocks.push(self.locals.len());
    }

    fn close_block(&mut self) {
        let start = self.blocks.pop().unwrap_or(0);
        for l in self.locals.drain(start..) {
            if l.used || l.kind == LocalKind::Quiet || l.name.starts_with('_') {
                continue;
            }
            let what = match l.kind {
                LocalKind::Function => "local function",
                _ => "local variable",
            };
            self.diags
                .push(Diagnostic::warning(rule::UNUSED_LOCAL, format!("unused {what} '{}'", l.name), l.line));
        }
    }

    fn function(&mut self, params: &[Name], vararg: bool, body: &[Stmt], line: usize) {
        self.functions.push((vararg, 0));
        self.open_block();
        for p in params {
            self.declare(&p.name, line, LocalKind::Quiet);
        }
        self.labels(body);
        self.check_gotos(body, &mut Vec::new());
        self.stmts(body);
        self.close_block();
        self.functions.pop();
    }

    fn block(&mut self, body: &[Stmt]) {
        self.open_block();
        self.labels(body);
        self.stmts(body);
        self.close_block();
    }

    fn in_loop<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        if let Some(func) = self.functions.last_mut() {
            func.1 += 1;
        }
        let r = f(self);
        if let Some(func) = self.functions.last_mut() {
            func.1 -= 1;
        }
        r
    }

    /// Labels of one block: duplicates are errors, and every `goto` in it
    /// (or nested blocks of the same function) must reach a visible label.
    fn labels(&mut self, body: &[Stmt]) {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for s in body {
            if let Stmt::Label { name, line } = s
                && let Some(first) = seen.insert(&name.name, *line)
            {
                self.diags.push(Diagnostic::error(rule::DUPLICATE_LABEL, 
                    format!("label '{}' already defined on line {first}", name.name),
                    *line,
                ));
            }
        }
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    /// Every `goto` in a function body must name a label visible to it:
    /// one in its own block or an enclosing block of the same function
    /// (Lua can't jump into a nested block, or out of a function).
    fn check_gotos<'a>(&mut self, stmts: &'a [Stmt], outer: &mut Vec<&'a str>) {
        let mark = outer.len();
        outer.extend(stmts.iter().filter_map(|s| match s {
            Stmt::Label { name, .. } => Some(name.name.as_str()),
            _ => None,
        }));
        for s in stmts {
            match s {
                Stmt::Goto { label, line } if !outer.contains(&label.name.as_str()) => {
                    self.diags.push(Diagnostic::error(rule::UNDEFINED_LABEL, 
                        format!("no visible label '{}' for goto", label.name),
                        *line,
                    ));
                }
                // function bodies are checked when their own block is
                Stmt::Function { .. } | Stmt::LocalFunction { .. } => {}
                _ => {
                    for body in s.bodies() {
                        self.check_gotos(body, outer);
                    }
                }
            }
        }
        outer.truncate(mark);
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local { names, values, line } => {
                self.exprs(values);
                for n in names {
                    self.declare(&n.name.name, *line, LocalKind::Variable);
                }
            }
            Stmt::Assign { targets, values, .. } => {
                for t in targets {
                    // `x = v` writes `x`; `t.k = v` / `t[k] = v` read `t`, `k`
                    if !matches!(t.kind, ExprKind::Name(_)) {
                        self.expr(t);
                    }
                }
                self.exprs(values);
            }
            Stmt::Call { call, .. } => self.expr(call),
            Stmt::Do { body, .. } => self.block(body),
            Stmt::While { cond, body, line } => {
                self.expr(cond);
                if matches!(cond.kind, ExprKind::Bool(true)) && !exits_loop(body) {
                    self.diags.push(Diagnostic::warning(rule::INFINITE_LOOP, 
                        "'while true' loop has no 'break', 'return' or 'goto'",
                        *line,
                    ));
                }
                self.in_loop(|l| l.block(body));
            }
            Stmt::Repeat { body, cond, .. } => {
                // `until` sees the body's locals
                self.in_loop(|l| {
                    l.open_block();
                    l.labels(body);
                    l.stmts(body);
                    l.expr(cond);
                    l.close_block();
                });
            }
            Stmt::If {
                branches,
                else_body,
                line,
            } => {
                for (cond, body) in branches {
                    self.expr(cond);
                    if body.is_empty() {
                        self.diags.push(Diagnostic::warning(rule::EMPTY_BRANCH, "empty 'then' block", *line));
                    }
                    self.block(body);
                }
                if let Some(body) = else_body {
                    self.block(body);
                }
            }
            Stmt::NumericFor {
                var,
                start,
                stop,
                step,
                body,
                line,
            } => {
                self.expr(start);
                self.expr(stop);
                if let Some(step) = step {
                    self.expr(step);
                }
                self.in_loop(|l| {
                    l.open_block();
                    l.declare(&var.name, *line, LocalKind::Quiet);
                    l.labels(body);
                    l.stmts(body);
                    l.close_block();
                });
            }
            Stmt::GenericFor {
                names,
                exprs,
                body,
                line,
            } => {
                self.exprs(exprs);
                self.in_loop(|l| {
                    l.open_block();
                    for n in names {
                        l.declare(&n.name, *line, LocalKind::Quiet);
                    }
                    l.labels(body);
                    l.stmts(body);
                    l.close_block();
                });
            }
            Stmt::Function { name, func, .. } => {
                if let Some(first) = name.path.first() {
                    self.use_name(&first.name);
                }
                let mut params = Vec::new();
                if name.method.is_some() {
                    params.push(Name {
                        name: "self".into(),
                        span: Span::whole_line(func.line),
                    });
                }
                params.extend(func.params.iter().cloned());
                self.function(&params, func.vararg, &func.body, func.line);
            }
            Stmt::LocalFunction { name, func, line } => {
                // visible inside its own body (recursion)
                self.declare(&name.name, *line, LocalKind::Function);
                self.function(&func.params, func.vararg, &func.body, func.line);
            }
            Stmt::Return { values, .. } => self.exprs(values),
            Stmt::Break { line } => {
                if self.functions.last().is_none_or(|f| f.1 == 0) {
                    self.diags
                        .push(Diagnostic::error(rule::BREAK_OUTSIDE_LOOP, "'break' outside a loop", *line));
                }
            }
            Stmt::Goto { .. } | Stmt::Label { .. } => {}
        }
    }

    fn exprs(&mut self, exprs: &[Expr]) {
        for e in exprs {
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Name(n) => self.use_name(n),
            ExprKind::Vararg => {
                if self.functions.last().is_some_and(|f| !f.0) {
                    self.diags.push(Diagnostic::error(rule::VARARG_OUTSIDE_VARARG_FUNCTION, 
                        "cannot use '...' outside a vararg function",
                        e.span.start_line,
                    ));
                }
            }
            ExprKind::Function(f) => {
                self.function(&f.params, f.vararg, &f.body, f.line);
            }
            _ => {}
        }
        for child in e.children() {
            self.expr(child);
        }
    }
}

/// Whether a loop body can leave the loop: a `break` for *this* loop, or
/// any `return` / `goto`.
fn exits_loop(body: &[Stmt]) -> bool {
    body.iter().any(|s| match s {
        Stmt::Break { .. } | Stmt::Return { .. } | Stmt::Goto { .. } => true,
        Stmt::While { body, .. }
        | Stmt::Repeat { body, .. }
        | Stmt::NumericFor { body, .. }
        | Stmt::GenericFor { body, .. } => leaves_function(body),
        Stmt::Function { .. } | Stmt::LocalFunction { .. } => false,
        _ => s.bodies().into_iter().any(exits_loop),
    })
}

fn leaves_function(body: &[Stmt]) -> bool {
    body.iter().any(|s| match s {
        Stmt::Return { .. } | Stmt::Goto { .. } => true,
        Stmt::Function { .. } | Stmt::LocalFunction { .. } => false,
        _ => s.bodies().into_iter().any(leaves_function),
    })
}

#[cfg(test)]
#[path = "lint.test.rs"]
mod tests;
