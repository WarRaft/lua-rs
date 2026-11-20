/// [`Span`] and [`ColumnEncoding`] live in the shared `parser-rs` crate.
pub use parser_rs::{ColumnEncoding, Span};

/// A Lua chunk (a file): one block, run as a vararg function.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub body: Vec<Stmt>,
}

/// A declared name with its exact location.
#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    pub name: String,
    pub span: Span,
}

/// `local x <const>`: a local name and its optional attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalName {
    pub name: Name,
    /// `const` / `close` (Lua 5.4 syntax, accepted for forward
    /// compatibility).
    pub attrib: Option<String>,
}

/// A function body: `(params) block end`.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub params: Vec<Name>,
    /// Whether the parameter list ends in `...`.
    pub vararg: bool,
    pub body: Vec<Stmt>,
    pub line: usize,
}

/// `function a.b.c:m()`: the dotted path, plus the method name after `:`.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncName {
    pub path: Vec<Name>,
    pub method: Option<Name>,
}

impl FuncName {
    /// `a.b.c:m` as written.
    pub fn display(&self) -> String {
        let mut s = self
            .path
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        if let Some(m) = &self.method {
            s.push(':');
            s.push_str(&m.name);
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `local a, b <const> = 1, 2`
    Local {
        names: Vec<LocalName>,
        values: Vec<Expr>,
        line: usize,
    },
    /// `a, t.x, t[k] = 1, 2, 3`
    Assign {
        targets: Vec<Expr>,
        values: Vec<Expr>,
        line: usize,
    },
    /// A function or method call used as a statement.
    Call {
        call: Expr,
        line: usize,
    },
    Do {
        body: Vec<Stmt>,
        line: usize,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    /// `repeat body until cond`; `cond` sees the body's locals.
    Repeat {
        body: Vec<Stmt>,
        cond: Expr,
        line: usize,
    },
    If {
        /// `if`/`elseif` conditions with their blocks, in order.
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
        line: usize,
    },
    /// `for i = start, stop[, step] do body end`
    NumericFor {
        var: Name,
        start: Expr,
        stop: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
        line: usize,
    },
    /// `for k, v in explist do body end`
    GenericFor {
        names: Vec<Name>,
        exprs: Vec<Expr>,
        body: Vec<Stmt>,
        line: usize,
    },
    /// `function a.b:c() ... end`
    Function {
        name: FuncName,
        func: Function,
        line: usize,
    },
    /// `local function f() ... end`
    LocalFunction {
        name: Name,
        func: Function,
        line: usize,
    },
    Return {
        values: Vec<Expr>,
        line: usize,
    },
    Break {
        line: usize,
    },
    Goto {
        label: Name,
        line: usize,
    },
    /// `::name::`
    Label {
        name: Name,
        line: usize,
    },
}

impl Stmt {
    pub fn line(&self) -> usize {
        match self {
            Stmt::Local { line, .. }
            | Stmt::Assign { line, .. }
            | Stmt::Call { line, .. }
            | Stmt::Do { line, .. }
            | Stmt::While { line, .. }
            | Stmt::Repeat { line, .. }
            | Stmt::If { line, .. }
            | Stmt::NumericFor { line, .. }
            | Stmt::GenericFor { line, .. }
            | Stmt::Function { line, .. }
            | Stmt::LocalFunction { line, .. }
            | Stmt::Return { line, .. }
            | Stmt::Break { line }
            | Stmt::Goto { line, .. }
            | Stmt::Label { line, .. } => *line,
        }
    }

    /// The statement's own expressions, in source order (not those inside
    /// nested blocks or function bodies).
    pub fn exprs(&self) -> Vec<&Expr> {
        match self {
            Stmt::Local { values, .. } | Stmt::Return { values, .. } => values.iter().collect(),
            Stmt::Assign { targets, values, .. } => targets.iter().chain(values).collect(),
            Stmt::Call { call, .. } => vec![call],
            Stmt::While { cond, .. } | Stmt::Repeat { cond, .. } => vec![cond],
            Stmt::If { branches, .. } => branches.iter().map(|(c, _)| c).collect(),
            Stmt::NumericFor {
                start, stop, step, ..
            } => [start, stop].into_iter().chain(step).collect(),
            Stmt::GenericFor { exprs, .. } => exprs.iter().collect(),
            Stmt::Do { .. }
            | Stmt::Function { .. }
            | Stmt::LocalFunction { .. }
            | Stmt::Break { .. }
            | Stmt::Goto { .. }
            | Stmt::Label { .. } => Vec::new(),
        }
    }

    /// The statement's nested blocks (function bodies included).
    pub fn bodies(&self) -> Vec<&[Stmt]> {
        match self {
            Stmt::Do { body, .. }
            | Stmt::While { body, .. }
            | Stmt::Repeat { body, .. }
            | Stmt::NumericFor { body, .. }
            | Stmt::GenericFor { body, .. } => vec![body],
            Stmt::If {
                branches,
                else_body,
                ..
            } => branches
                .iter()
                .map(|(_, b)| b.as_slice())
                .chain(else_body.as_deref())
                .collect(),
            Stmt::Function { func, .. } | Stmt::LocalFunction { func, .. } => vec![&func.body],
            Stmt::Local { .. }
            | Stmt::Assign { .. }
            | Stmt::Call { .. }
            | Stmt::Return { .. }
            | Stmt::Break { .. }
            | Stmt::Goto { .. }
            | Stmt::Label { .. } => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Compares only `kind`, ignoring `span`.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span }
    }

    /// Direct sub-expressions, in source order. A function literal's body
    /// is not included (it's statements, in its own scope).
    pub fn children(&self) -> Vec<&Expr> {
        match &self.kind {
            ExprKind::Index(a, b) | ExprKind::Binary(a, _, b) => vec![a, b],
            ExprKind::Field(a, _) | ExprKind::Paren(a) | ExprKind::Unary(_, a) => vec![a],
            ExprKind::Call(f, args) => std::iter::once(&**f).chain(args).collect(),
            ExprKind::MethodCall(obj, _, args) => std::iter::once(&**obj).chain(args).collect(),
            ExprKind::Table(fields) => fields
                .iter()
                .flat_map(|f| match f {
                    Field::Positional(v) | Field::Named(_, v) => vec![v],
                    Field::Keyed(k, v) => vec![k, v],
                })
                .collect(),
            ExprKind::Nil
            | ExprKind::Bool(_)
            | ExprKind::Int(_)
            | ExprKind::Real(_)
            | ExprKind::Str(_)
            | ExprKind::Vararg
            | ExprKind::Name(_)
            | ExprKind::Function(_) => Vec::new(),
        }
    }

    /// `a.b.c` as a dotted path, if this is a plain name or field chain.
    pub fn path(&self) -> Option<String> {
        match &self.kind {
            ExprKind::Name(n) => Some(n.clone()),
            ExprKind::Field(obj, name) => Some(format!("{}.{name}", obj.path()?)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Nil,
    Bool(bool),
    Int(i64),
    Real(f64),
    Str(String),
    /// `...`
    Vararg,
    /// `function (params) body end`
    Function(Box<Function>),
    /// `{ ... }`
    Table(Vec<Field>),
    Name(String),
    /// `t[k]`
    Index(Box<Expr>, Box<Expr>),
    /// `t.name`
    Field(Box<Expr>, String),
    /// `f(args)`, `f "str"`, `f { table }`
    Call(Box<Expr>, Vec<Expr>),
    /// `obj:name(args)`
    MethodCall(Box<Expr>, String, Vec<Expr>),
    /// `(e)`: truncates multiple results to one, so it's kept.
    Paren(Box<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    /// `{ v }`
    Positional(Expr),
    /// `{ name = v }`
    Named(Name, Expr),
    /// `{ [k] = v }`
    Keyed(Expr, Expr),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Or,
    And,
    Lt,
    Gt,
    Le,
    Ge,
    Ne,
    Eq,
    BOr,
    BXor,
    BAnd,
    Shl,
    Shr,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    IDiv,
    Mod,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Not,
    Neg,
    /// `#`
    Len,
    /// `~`
    BNot,
}
