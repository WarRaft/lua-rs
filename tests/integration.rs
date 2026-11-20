use lua_rs::ast::Stmt;
use lua_rs::{lint, parse};

const SAMPLE: &str = include_str!("fixtures/sample.lua");

#[test]
fn sample_file_parses_successfully() {
    let chunk = parse(SAMPLE).expect("sample.lua should parse");
    let functions: Vec<String> = chunk
        .body
        .iter()
        .filter_map(|s| match s {
            Stmt::Function { name, .. } => Some(name.display()),
            Stmt::LocalFunction { name, .. } => Some(name.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(functions, ["Counter.new", "Counter:bump", "isEven", "sum", "InitCounters"]);
}

#[test]
fn sample_file_has_no_lint_diagnostics() {
    let chunk = parse(SAMPLE).expect("sample.lua should parse");
    let diagnostics = lint(&chunk);
    assert!(diagnostics.is_empty(), "expected sample.lua to be clean, got {diagnostics:?}");
}

#[test]
fn reports_parse_error_with_location() {
    let err = parse("local x = \n  + 1").unwrap_err();
    assert_eq!((err.line, err.col), (2, 3));
}

#[test]
fn lint_catches_multiple_issues_at_once() {
    let src = "local unused = 1\nwhile true do print(1) end\nbreak\ngoto nowhere\n";
    let d = lint(&parse(src).unwrap());
    for needle in ["unused local variable 'unused'", "'while true'", "'break' outside", "no visible label"] {
        assert!(d.iter().any(|d| d.message.contains(needle)), "missing {needle:?} in {d:?}");
    }
}
