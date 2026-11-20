use super::*;
use parser_rs::SemanticModifier as M;
use parser_rs::{Scope as S, SymbolKind as K};

/// `(type, modifiers)` of the token starting at `line:col`.
fn at(src: &str, line: usize, col: usize) -> Option<(T, Modifiers)> {
    semantic_tokens(src)
        .into_iter()
        .find(|t| t.line == line && t.start_col == col)
        .map(|t| (t.ty, t.mods))
}

fn m(mods: &[M]) -> Modifiers {
    Modifiers::of(mods)
}

#[test]
fn classifies_locals_params_and_globals() {
    let src = "local x <const> = 1\nlocal function f(a)\n  return a + x + y\nend\nprint(f(2))";
    assert_eq!(at(src, 1, 7), Some((T::Variable, m(&[M::Declaration]))));
    assert_eq!(at(src, 1, 10), Some((T::Decorator, m(&[]))));
    assert_eq!(at(src, 2, 16), Some((T::Function, m(&[M::Declaration]))));
    assert_eq!(at(src, 2, 18), Some((T::Parameter, m(&[M::Declaration]))));
    assert_eq!(at(src, 3, 10), Some((T::Parameter, m(&[]))));
    assert_eq!(at(src, 3, 14), Some((T::Variable, m(&[])))); // local x
    assert_eq!(at(src, 3, 18), Some((T::Variable, m(&[])))); // global y
    assert_eq!(at(src, 5, 1), Some((T::Function, m(&[M::DefaultLibrary])))); // print
    assert_eq!(at(src, 5, 7), Some((T::Function, m(&[])))); // f
}

#[test]
fn classifies_function_names_fields_methods_and_table_keys() {
    let src = "function M.sub.run(x) end\nfunction Obj:go() return self end\nlocal t = { key = 1 }\nt.key = string.rep('a', 2)\nt:go()";
    assert_eq!(at(src, 1, 10), Some((T::Variable, m(&[])))); // M
    assert_eq!(at(src, 1, 12), Some((T::Property, m(&[])))); // sub
    assert_eq!(at(src, 1, 16), Some((T::Function, m(&[M::Declaration])))); // run
    assert_eq!(at(src, 2, 14), Some((T::Method, m(&[M::Declaration])))); // go
    assert_eq!(at(src, 2, 26), Some((T::Parameter, m(&[])))); // implicit self
    assert_eq!(at(src, 3, 13), Some((T::Property, m(&[M::Declaration])))); // key =
    assert_eq!(at(src, 4, 3), Some((T::Property, m(&[M::Modification])))); // t.key =
    assert_eq!(at(src, 4, 9), Some((T::Variable, m(&[M::DefaultLibrary])))); // string
    assert_eq!(at(src, 4, 16), Some((T::Function, m(&[])))); // .rep(
    assert_eq!(at(src, 5, 3), Some((T::Method, m(&[])))); // :go
}

#[test]
fn scopes_follow_blocks_and_loops() {
    let src = "for i, v in pairs(t) do print(i) end\nprint(i)";
    assert_eq!(at(src, 1, 5), Some((T::Variable, m(&[M::Declaration]))));
    assert_eq!(at(src, 1, 31), Some((T::Variable, m(&[])))); // loop i
    // after `end`, `i` is a global again (still a variable, no change);
    // a parameter shadowed by nothing goes back to global after its function
    let src = "local function f(p) end\nprint(p)";
    assert_eq!(at(src, 2, 7), Some((T::Variable, m(&[]))));
}

#[test]
fn labels_comments_and_multiline_strings() {
    let src = "::top::\ngoto top\n--[[ long\ncomment ]]\nlocal s = [[a\nb]]";
    assert_eq!(at(src, 1, 3), Some((T::Label, m(&[M::Declaration]))));
    assert_eq!(at(src, 2, 6), Some((T::Label, m(&[]))));
    assert_eq!(at(src, 3, 1).map(|t| t.0), Some(T::Comment));
    assert_eq!(at(src, 4, 1).map(|t| t.0), Some(T::Comment));
    // one multi-line string token; `encode` splits it for the editor
    let data = encoded_semantic_tokens(src);
    let strings = data.chunks(5).filter(|c| c[3] == T::String.index()).count();
    assert_eq!(strings, 2);
}

#[test]
fn still_highlights_unparsable_code() {
    let src = "local function f(a\n  print(a";
    assert!(crate::parse(src).is_err());
    assert_eq!(at(src, 2, 3), Some((T::Function, m(&[M::DefaultLibrary]))));
}

/// Every occurrence of `name` in `src` as (line, col, kind, declaration, scope).
fn occ(src: &str, name: &str) -> Vec<(usize, usize, K, bool, S)> {
    symbols(src)
        .into_iter()
        .filter(|o| o.name == name)
        .map(|o| (o.line, o.col, o.kind, o.declaration, o.scope))
        .collect()
}

/// The scopes of `name`'s occurrences, in order.
fn scopes(src: &str, name: &str) -> Vec<S> {
    occ(src, name).into_iter().map(|o| o.4).collect()
}

#[test]
fn each_local_declaration_is_its_own_binding() {
    // Shadowing: the second `x` is new; its initializer reads the first.
    let src = "local x = 1\nlocal x = x + 1\nprint(x)";
    assert_eq!(
        occ(src, "x"),
        vec![
            (1, 7, K::Local, true, S::Local(1)),
            (2, 7, K::Local, true, S::Local(2)),
            (2, 11, K::Local, false, S::Local(2)),
            (3, 7, K::Local, false, S::Local(2)),
        ]
    );
}

#[test]
fn block_scopes_end_with_their_block() {
    let src = "local v = 0\nif c then\n  local v = 1\n  print(v)\nend\nprint(v)";
    assert_eq!(scopes(src, "v"), vec![S::Local(1), S::Local(2), S::Local(2), S::Local(1)]);
}

#[test]
fn parameters_loop_variables_and_local_functions() {
    let src = "local function f(a)\n  for i = 1, a do print(i) end\n  return f(a)\nend";
    assert_eq!(occ(src, "f")[0], (1, 16, K::Function, true, S::Local(1)));
    assert_eq!(scopes(src, "f"), vec![S::Local(1), S::Local(1)]);
    assert_eq!(scopes(src, "a"), vec![S::Local(2), S::Local(2), S::Local(2)]);
    assert_eq!(
        occ(src, "i"),
        vec![(2, 7, K::Local, true, S::Local(3)), (2, 25, K::Local, false, S::Local(3))]
    );
}

#[test]
fn globals_are_project_wide_and_declared_at_the_top_level() {
    let src = "function Init()\n  Count = 0\nend\nCount = 1\nInit()\nCreateTimer()";
    assert_eq!(occ(src, "Init"), vec![(1, 10, K::Function, true, S::Project), (5, 1, K::Function, false, S::Project)]);
    // Assigned in a function: a write; at the top level: the declaration.
    assert_eq!(occ(src, "Count"), vec![(2, 3, K::Global, false, S::Project), (4, 1, K::Global, true, S::Project)]);
    // A native from common.j, say.
    assert_eq!(occ(src, "CreateTimer"), vec![(6, 1, K::Function, false, S::Project)]);
}

#[test]
fn fields_and_methods_are_members() {
    let src = "\
Point = { x = 0 }
function Point.new() return {} end
function Point:move(dx) self.x = self.x + dx end
Point.y = 1
local p = Point.new()
p:move(1)
";
    assert_eq!(occ(src, "Point")[0], (1, 1, K::Global, true, S::Project));
    assert!(occ(src, "Point").iter().skip(1).all(|o| o.4 == S::Project && !o.3));
    assert_eq!(
        occ(src, "x"),
        vec![(1, 11, K::Field, true, S::Member), (3, 30, K::Field, false, S::Member), (3, 39, K::Field, false, S::Member)]
    );
    assert_eq!(occ(src, "new"), vec![(2, 16, K::Function, true, S::Member), (5, 17, K::Method, false, S::Member)]);
    assert_eq!(occ(src, "move"), vec![(3, 16, K::Method, true, S::Member), (6, 3, K::Method, false, S::Member)]);
    // `Point.y = 1` at the top level defines the field.
    assert_eq!(occ(src, "y"), vec![(4, 7, K::Field, true, S::Member)]);
    // `self` is the method's implicit parameter: one binding, no declaration.
    let selfs = occ(src, "self");
    assert_eq!(selfs.len(), 2);
    assert!(selfs.iter().all(|o| o.2 == K::Parameter && !o.3 && o.4 == selfs[0].4));
}

#[test]
fn a_global_function_statement_assigns_a_local_in_scope() {
    let src = "local helper\nfunction helper() end";
    assert_eq!(scopes(src, "helper"), vec![S::Local(1), S::Local(1)]);
}

#[test]
fn half_typed_code_is_still_indexed() {
    let names: Vec<_> = symbols("local t = setmetatable({}, {\n  __index = function(s, k")
        .into_iter()
        .map(|o| (o.name, o.declaration))
        .collect();
    assert_eq!(
        names,
        // The list is still open at the end of the file, so like the
        // highlighting, the last name isn't taken for a parameter.
        vec![("t".into(), true), ("setmetatable".into(), false), ("__index".into(), true), ("s".into(), true), ("k".into(), false)]
    );
}
