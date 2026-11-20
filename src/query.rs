//! Position-based lookup over a parsed [`Chunk`]: "which AST node is at
//! (or covers) this location". Coordinates are [`Span`]'s (line,
//! codepoint column).

use crate::ast::*;

/// A reference to whichever kind of AST node a query matched.
#[derive(Debug, Clone, Copy)]
pub enum AstRef<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

impl AstRef<'_> {
    /// Statements carry only their starting line; expressions an exact span.
    pub fn span(&self) -> Span {
        match self {
            AstRef::Stmt(s) => Span::whole_line(s.line()),
            AstRef::Expr(e) => e.span,
        }
    }
}

#[derive(Clone, Copy)]
struct Q {
    line: usize,
    sc: usize,
    ec: usize,
}

impl Q {
    fn hit<'a>(self, node: AstRef<'a>) -> Option<AstRef<'a>> {
        node.span().contains(self.line, self.sc, self.ec).then_some(node)
    }
}

/// Finds the most specific (deepest) node whose span contains the
/// single-line range `[start_col, end_col)` on `line`.
pub fn find_node_in(chunk: &Chunk, line: usize, start_col: usize, end_col: usize) -> Option<AstRef<'_>> {
    let q = Q {
        line,
        sc: start_col,
        ec: end_col,
    };
    in_stmts(&chunk.body, q)
}

/// Finds the most specific node at a single position.
pub fn find_node_at(chunk: &Chunk, line: usize, col: usize) -> Option<AstRef<'_>> {
    find_node_in(chunk, line, col, col)
}

fn in_stmts(stmts: &[Stmt], q: Q) -> Option<AstRef<'_>> {
    stmts.iter().find_map(|s| in_stmt(s, q))
}

fn in_stmt(s: &Stmt, q: Q) -> Option<AstRef<'_>> {
    s.exprs()
        .into_iter()
        .find_map(|e| in_expr(e, q))
        .or_else(|| s.bodies().into_iter().find_map(|b| in_stmts(b, q)))
        .or_else(|| q.hit(AstRef::Stmt(s)))
}

fn in_expr(e: &Expr, q: Q) -> Option<AstRef<'_>> {
    let child = match &e.kind {
        ExprKind::Function(f) => in_stmts(&f.body, q),
        _ => e.children().into_iter().find_map(|c| in_expr(c, q)),
    };
    child.or_else(|| q.hit(AstRef::Expr(e)))
}

#[cfg(test)]
#[path = "query.test.rs"]
mod tests;
