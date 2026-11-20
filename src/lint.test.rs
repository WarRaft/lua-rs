use super::*;
use crate::parser::Parser;

fn lint_src(src: &str) -> Vec<Diagnostic> {
    lint(&Parser::parse_str(src).expect("source should parse"))
}

fn has(d: &[Diagnostic], level: Level, text: &str) -> bool {
    d.iter().any(|d| d.level == level && d.message.contains(text))
}

#[test]
fn flags_unused_locals_but_not_used_or_underscored_ones() {
    let d = lint_src("local a = 1\nlocal _b = 2\nlocal c = 3\nprint(c)");
    assert_eq!(d.len(), 1);
    assert!(has(&d, Level::Warning, "unused local variable 'a'"));
    assert_eq!(d[0].line, 1);

    assert!(has(&lint_src("local function f() end"), Level::Warning, "unused local function 'f'"));
    // recursion counts as a use only from outside... but calling it does
    assert!(lint_src("local function f() end\nf()").is_empty());
}

#[test]
fn resolves_scopes_like_lua() {
    // `local x = x` reads the *outer* x
    assert!(lint_src("local x = 1\ndo local x = x print(x) end").is_empty());
    // an inner local doesn't satisfy an outer one
    let d = lint_src("local x = 1\ndo local x = 2 print(x) end");
    assert!(has(&d, Level::Warning, "unused local variable 'x'"));
    assert_eq!(d[0].line, 1);
    // `until` sees the repeat body's locals
    assert!(lint_src("repeat local done = true until done").is_empty());
    // parameters and loop variables are never reported
    assert!(lint_src("local function f(a, b) end f()\nfor i = 1, 2 do end").is_empty());
    // assigning isn't using; indexing is
    assert!(has(&lint_src("local x\nx = 1"), Level::Warning, "unused"));
    assert!(lint_src("local t = {}\nt.k = 1").is_empty());
}

#[test]
fn flags_break_outside_loops() {
    assert!(has(&lint_src("break"), Level::Error, "'break' outside a loop"));
    assert!(lint_src("while x do if y then break end end").is_empty());
    // a function body is not inside the loop around it
    let d = lint_src("while x do local f = function() break end f() end");
    assert!(has(&d, Level::Error, "'break' outside a loop"));
}

#[test]
fn checks_goto_labels() {
    assert!(lint_src("::a:: goto a").is_empty());
    assert!(lint_src("do goto out end ::out::").is_empty()); // enclosing block
    assert!(has(&lint_src("goto nowhere"), Level::Error, "no visible label 'nowhere'"));
    // can't jump into a nested block
    assert!(has(&lint_src("goto inner do ::inner:: end"), Level::Error, "no visible label"));
    assert!(has(&lint_src("::a:: ::a::"), Level::Error, "label 'a' already defined"));
}

#[test]
fn flags_vararg_outside_vararg_functions() {
    assert!(lint_src("print(...)").is_empty()); // the main chunk is vararg
    assert!(lint_src("local function f(...) return ... end f()").is_empty());
    let d = lint_src("local function f() return ... end f()");
    assert!(has(&d, Level::Error, "cannot use '...' outside a vararg function"));
}

#[test]
fn flags_infinite_loops_and_empty_blocks() {
    assert!(has(&lint_src("while true do f() end"), Level::Warning, "'while true'"));
    assert!(lint_src("while true do if g() then break end end").is_empty());
    assert!(lint_src("while true do return end").is_empty());
    // a break inside a nested loop doesn't leave the outer one
    assert!(has(&lint_src("while true do for i=1,2 do break end end"), Level::Warning, "'while true'"));
    assert!(has(&lint_src("if x then end"), Level::Warning, "empty 'then' block"));
}
