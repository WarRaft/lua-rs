use super::*;
use crate::parser::Parser;

const SRC: &str = "local function f(a)\n  return a + g(function() return 42 end)\nend";

#[test]
fn finds_expressions_statements_and_function_literal_bodies() {
    let chunk = Parser::parse_str(SRC).unwrap();
    // `a` in `a + g(...)`
    match find_node_at(&chunk, 2, 10) {
        Some(AstRef::Expr(e)) => assert_eq!(e.kind, ExprKind::Name("a".into())),
        other => panic!("expected a name, got {other:?}"),
    }
    // `42` inside the anonymous function
    match find_node_at(&chunk, 2, 34) {
        Some(AstRef::Expr(e)) => assert_eq!(e.kind, ExprKind::Int(42)),
        other => panic!("expected 42, got {other:?}"),
    }
    assert!(matches!(find_node_at(&chunk, 2, 1), Some(AstRef::Stmt(Stmt::Return { .. }))));
    assert!(find_node_at(&chunk, 99, 1).is_none());
}
