use super::*;
use crate::parser::Parser;

#[test]
fn renders_chunk_tree() {
    let chunk = Parser::parse_str("local function add(a, b)\n  return a + b\nend").unwrap();
    assert_eq!(
        render_tree(&chunk),
        "Chunk\n\
         └─ LocalFunction add(a, b) (line 1)\n   \
            └─ Return (line 2)\n      \
               └─ Binary Add\n         \
                  ├─ Name a\n         \
                  └─ Name b\n"
    );
}

#[test]
fn renders_calls_methods_and_tables() {
    let chunk = Parser::parse_str("obj:m({ x = 1 })\nstring.format('%d', 1)").unwrap();
    let tree = render_tree(&chunk);
    assert!(tree.contains("MethodCall :m"));
    assert!(tree.contains("x ="));
    assert!(tree.contains("Call string.format"));
    assert!(render_dot(&chunk).starts_with("digraph AST {"));
}
