use super::*;

fn parse(src: &str) -> Chunk {
    Parser::parse_str(src).unwrap_or_else(|e| panic!("should parse: {e:?}"))
}

fn expr(src: &str) -> Expr {
    match &parse(&format!("return {src}")).body[0] {
        Stmt::Return { values, .. } => values[0].clone(),
        other => panic!("expected return, got {other:?}"),
    }
}

fn name(n: &str) -> Box<Expr> {
    Box::new(Expr::new(ExprKind::Name(n.into()), Span::whole_line(0)))
}

fn bin(l: Box<Expr>, op: BinOp, r: Box<Expr>) -> Box<Expr> {
    Box::new(Expr::new(ExprKind::Binary(l, op, r), Span::whole_line(0)))
}

#[test]
fn parses_statements() {
    let chunk = parse(
        "local a, b <const> = 1, 2\n\
         a, t.x, t[1] = 3, 4, 5\n\
         f()\n\
         do end\n\
         while a do break end\n\
         repeat a = a - 1 until a == 0\n\
         if a then elseif b then else end\n\
         for i = 1, 10, 2 do end\n\
         for k, v in pairs(t) do end\n\
         function m.n:o(x, ...) end\n\
         local function g() end\n\
         ::top:: goto top\n\
         ;;\n\
         return a",
    );
    let kinds: Vec<&str> = chunk
        .body
        .iter()
        .map(|s| match s {
            Stmt::Local { .. } => "local",
            Stmt::Assign { .. } => "assign",
            Stmt::Call { .. } => "call",
            Stmt::Do { .. } => "do",
            Stmt::While { .. } => "while",
            Stmt::Repeat { .. } => "repeat",
            Stmt::If { .. } => "if",
            Stmt::NumericFor { .. } => "numfor",
            Stmt::GenericFor { .. } => "genfor",
            Stmt::Function { .. } => "function",
            Stmt::LocalFunction { .. } => "localfunction",
            Stmt::Label { .. } => "label",
            Stmt::Goto { .. } => "goto",
            Stmt::Return { .. } => "return",
            Stmt::Break { .. } => "break",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "local", "assign", "call", "do", "while", "repeat", "if", "numfor", "genfor",
            "function", "localfunction", "label", "goto", "return"
        ]
    );
    match &chunk.body[0] {
        Stmt::Local { names, values, .. } => {
            assert_eq!(names[1].attrib.as_deref(), Some("const"));
            assert_eq!(values.len(), 2);
        }
        _ => unreachable!(),
    }
    match &chunk.body[9] {
        Stmt::Function { name, func, .. } => {
            assert_eq!(name.display(), "m.n:o");
            assert!(func.vararg);
        }
        _ => unreachable!(),
    }
}

#[test]
fn respects_lua_precedence_and_associativity() {
    // `and` binds tighter than `or` in Lua (unlike JASS)
    assert_eq!(
        expr("a or b and c").kind,
        ExprKind::Binary(name("a"), BinOp::Or, bin(name("b"), BinOp::And, name("c")))
    );
    // `..` and `^` are right-associative
    assert_eq!(
        expr("a .. b .. c").kind,
        ExprKind::Binary(name("a"), BinOp::Concat, bin(name("b"), BinOp::Concat, name("c")))
    );
    assert_eq!(
        expr("a ^ b ^ c").kind,
        ExprKind::Binary(name("a"), BinOp::Pow, bin(name("b"), BinOp::Pow, name("c")))
    );
    // `-x ^ 2` is `-(x ^ 2)`; `not a == b` is `(not a) == b`
    assert!(matches!(expr("-x ^ 2").kind, ExprKind::Unary(UnOp::Neg, _)));
    assert!(matches!(expr("not a == b").kind, ExprKind::Binary(_, BinOp::Eq, _)));
    // `..` binds looser than `+`
    assert!(matches!(expr("1 + 2 .. 3").kind, ExprKind::Binary(_, BinOp::Concat, _)));
}

#[test]
fn parses_calls_methods_tables_and_functions() {
    assert!(matches!(expr("obj:m(1)(2)").kind, ExprKind::Call(..)));
    assert!(matches!(expr("f 'x'").kind, ExprKind::Call(_, ref a) if a.len() == 1));
    assert!(matches!(expr("f { 1 }").kind, ExprKind::Call(..)));
    match expr("{ 1, x = 2, [k] = 3; }").kind {
        ExprKind::Table(fields) => {
            assert!(matches!(fields[0], Field::Positional(_)));
            assert!(matches!(&fields[1], Field::Named(n, _) if n.name == "x"));
            assert!(matches!(fields[2], Field::Keyed(..)));
        }
        other => panic!("expected table, got {other:?}"),
    }
    assert!(matches!(expr("function(a) return a end").kind, ExprKind::Function(_)));
    assert!(matches!(expr("(f())").kind, ExprKind::Paren(_)));
}

#[test]
fn records_expression_spans() {
    let e = expr("t.field[1]");
    assert_eq!(e.span, Span::new(1, 8, 1, 18));
}

#[test]
fn reports_errors_like_lua() {
    let err = Parser::parse_str("if a then\n  x = 1\n").unwrap_err();
    assert_eq!(err.message, "'end' expected (to close 'if' at line 1), found end of file");

    let err = Parser::parse_str("x").unwrap_err();
    assert!(err.message.contains("expected '=' or a call"));

    let err = Parser::parse_str("return 1\nx = 2").unwrap_err();
    assert!(err.message.contains("'return' must be the last statement"));

    let err = Parser::parse_str("f() = 1").unwrap_err();
    assert!(err.message.contains("cannot assign"));

    let err = Parser::parse_str("for x do end").unwrap_err();
    assert!(err.message.contains("'=' or 'in' expected"));
}
