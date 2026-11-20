//! Semantic highlighting for Lua: the shared lexical layer (keywords,
//! literals, operators, comments, including multi-line long strings and
//! comments) from [`parser_rs::semantic`], plus identifier classification:
//! locals, parameters, local functions, globals, standard-library globals
//! (`defaultLibrary`), fields, methods, table keys, labels.
//!
//! It works on the tolerant token stream, not the AST, so half-typed code
//! still highlights. Scopes follow Lua's block keywords: `do`, `then`,
//! `else`, `repeat`, and a function's parameter list open a scope, while
//! `end`, `until`, `else`, `elseif` close one.
//!
//! The same pass records every name as a symbol [`Occurrence`]
//! ([`semantic_tokens_and_symbols`]), so the editor's symbol index agrees
//! with the highlighting:
//!
//! - **Locals, parameters, loop variables, local functions**: each
//!   declaration is a binding of its own ([`SymScope::Local`] numbered per
//!   file), and a use refers to the binding the scope lookup finds, so
//!   shadowing (`local x = x + 1`) is two symbols.
//! - **Globals** are project-wide: a Warcraft III map's Lua files share
//!   one global table. `function Foo()` declares one, and so does an
//!   assignment at the top level of the file (`Foo = {}`); one inside a
//!   function or block is a write.
//! - **Fields** (`t.x`, `t:m()`, `{ x = 1 }`, `function t.f()`) are
//!   [`SymScope::Member`]: which table a name belongs to isn't known
//!   without types. The definitions are `function t.f` / `t:m`, table
//!   keys, and `t.x = ...` at the top level.
//!
//! Labels aren't symbols.

use std::collections::HashMap;

use crate::lexer::SPEC;
use crate::token::{Kw, Punct, Token, TokenKind};
use parser_rs::semantic::{self, comment_tokens, lexical_type};
use parser_rs::{
    Modifiers, Occurrence, Scope as SymScope, SemanticModifier as M, SemanticToken, SemanticType as T,
    SymbolKind as K,
};

/// Lua 5.3's standard-library globals.
const STDLIB: &[&str] = &[
    "assert", "collectgarbage", "dofile", "error", "getmetatable", "ipairs", "load", "loadfile",
    "next", "pairs", "pcall", "print", "rawequal", "rawget", "rawlen", "rawset", "require",
    "select", "setmetatable", "tonumber", "tostring", "type", "xpcall", "_G", "_VERSION",
    "coroutine", "debug", "io", "math", "os", "package", "string", "table", "utf8",
];

/// Every semantic token in `src`, unencoded.
pub fn semantic_tokens(src: &str) -> Vec<SemanticToken> {
    semantic_tokens_and_symbols(src).0
}

/// Every name declared or referenced in `src`.
pub fn symbols(src: &str) -> Vec<Occurrence> {
    semantic_tokens_and_symbols(src).1
}

/// Semantic tokens and symbol occurrences in one pass: they come from the
/// same classification.
pub fn semantic_tokens_and_symbols(src: &str) -> (Vec<SemanticToken>, Vec<Occurrence>) {
    let lexed = parser_rs::tokenize_lossy(src, &SPEC);
    let mut out = comment_tokens(&lexed.comments, src);
    let syms = Classifier::default().run(&lexed.tokens, &mut out);
    (out, syms)
}

fn occurrence(t: &Token, name: &str, kind: K, declaration: bool, scope: SymScope) -> Occurrence {
    Occurrence {
        name: name.to_string(),
        line: t.line,
        col: t.col,
        end_col: t.end_col,
        kind,
        declaration,
        scope,
        namespace: String::new(),
        qualifier: None,
    }
}

/// [`semantic_tokens`] in the editor's wire format.
pub fn encoded_semantic_tokens(src: &str) -> Vec<u32> {
    semantic::encode(semantic_tokens(src), src)
}

#[derive(Clone, Copy, PartialEq)]
enum Local {
    Variable,
    Parameter,
    Function,
}

impl Local {
    fn kind(self) -> K {
        match self {
            Local::Variable => K::Local,
            Local::Parameter => K::Parameter,
            Local::Function => K::Function,
        }
    }
}

/// A name in scope: what it is, and its binding number.
type Binding = (Local, u32);

enum Ctx {
    Scope(HashMap<String, Binding>),
    /// Inside a `{ ... }` table constructor.
    Table,
}

/// A `function` keyword seen, its parameter list not yet complete.
struct PendingFn {
    params: Vec<(String, u32)>,
    /// Inside the `( ... )` already.
    in_params: bool,
    /// `function a:m()`: an implicit `self` parameter.
    method: bool,
}

#[derive(Default)]
struct Classifier {
    stack: Vec<Ctx>,
    pending_fn: Option<PendingFn>,
    /// Between `function` and its `(`: reading the function's name.
    in_func_name: bool,
    /// After `local function`: the next name is the local function.
    local_function: bool,
    /// After `local`: names are being declared (until `=` or the end).
    local_decl: bool,
    /// After `for`, until `=`/`in`: reading loop variable names.
    in_for_names: bool,
    /// Loop variable names and bindings, declared when `do` opens the
    /// loop body.
    for_names: Vec<(String, u32)>,
    /// The last binding number handed out.
    bindings: u32,
    syms: Vec<Occurrence>,
}

fn kind(t: Option<&Token>) -> Option<&TokenKind> {
    t.map(|t| &t.kind)
}

fn is_punct(t: Option<&Token>, p: Punct) -> bool {
    kind(t) == Some(&TokenKind::Punct(p))
}

/// Whether `t` starts call arguments: `(`, a table, or a string.
fn starts_args(t: Option<&Token>) -> bool {
    matches!(
        kind(t),
        Some(TokenKind::Punct(Punct::LParen | Punct::LBrace) | TokenKind::Str(_))
    )
}

impl Classifier {
    fn push_scope(&mut self, names: impl IntoIterator<Item = (String, Binding)>) {
        self.stack.push(Ctx::Scope(names.into_iter().collect()));
    }

    fn new_binding(&mut self) -> u32 {
        self.bindings += 1;
        self.bindings
    }

    /// At the top level of the file: not in any function, block or table.
    fn at_top_level(&self) -> bool {
        self.stack.len() == 1
    }

    fn pop_scope(&mut self) {
        // drop any unclosed tables first (broken code)
        while let Some(Ctx::Table) = self.stack.last() {
            self.stack.pop();
        }
        self.stack.pop();
    }

    /// Declares a new binding of `name` in the innermost scope (shadowing
    /// any earlier one) and returns its number.
    fn declare(&mut self, name: &str, local: Local) -> u32 {
        let id = self.new_binding();
        let scope = self.stack.iter_mut().rev().find_map(|c| match c {
            Ctx::Scope(s) => Some(s),
            Ctx::Table => None,
        });
        match scope {
            Some(s) => {
                s.insert(name.to_string(), (local, id));
            }
            None => self.push_scope([(name.to_string(), (local, id))]),
        }
        id
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        self.stack.iter().rev().find_map(|c| match c {
            Ctx::Scope(s) => s.get(name).copied(),
            Ctx::Table => None,
        })
    }

    /// Classifies every token into `out`; returns the symbols.
    fn run(mut self, toks: &[Token], out: &mut Vec<SemanticToken>) -> Vec<Occurrence> {
        let decl = Modifiers::NONE.with(M::Declaration);
        self.push_scope([]); // the chunk

        for (i, t) in toks.iter().enumerate() {
            let prev = i.checked_sub(1).and_then(|j| toks.get(j));
            let next = toks.get(i + 1);

            let name = match &t.kind {
                TokenKind::Ident(name) => name.as_str(),
                k => {
                    self.on_token(k, next);
                    if let Some(ty) = lexical_type(k) {
                        out.push(SemanticToken::at(t, ty, Modifiers::NONE));
                    }
                    continue;
                }
            };

            // A parameter is always followed by `,` or `)`; anything else
            // means the list was never closed (half-typed code), so close it.
            if self.pending_fn.as_ref().is_some_and(|f| f.in_params)
                && !is_punct(next, Punct::Comma)
                && !is_punct(next, Punct::RParen)
            {
                self.close_params();
            }

            let assigned = is_punct(next, Punct::Assign);
            let modified = Modifiers::NONE.with_if(assigned, M::Modification);
            let token = if self.pending_fn.as_ref().is_some_and(|f| f.in_params) {
                let id = self.new_binding();
                if let Some(f) = self.pending_fn.as_mut() {
                    f.params.push((name.to_string(), id));
                }
                self.syms.push(occurrence(t, name, K::Parameter, true, SymScope::Local(id)));
                SemanticToken::at(t, T::Parameter, decl)
            } else if self.local_function {
                self.local_function = false;
                let id = self.declare(name, Local::Function);
                self.syms.push(occurrence(t, name, K::Function, true, SymScope::Local(id)));
                SemanticToken::at(t, T::Function, decl)
            } else if self.in_func_name {
                // `function a.b.c:m(`: the head is a reference, the middle
                // fields, the last name the function (or method) itself
                let continues = is_punct(next, Punct::Dot) || is_punct(next, Punct::Colon);
                if is_punct(prev, Punct::Colon) {
                    self.syms.push(occurrence(t, name, K::Method, true, SymScope::Member));
                    SemanticToken::at(t, T::Method, decl)
                } else if continues && !is_punct(prev, Punct::Dot) {
                    self.reference(t, name, Modifiers::NONE, false)
                } else if continues {
                    self.syms.push(occurrence(t, name, K::Field, false, SymScope::Member));
                    SemanticToken::at(t, T::Property, Modifiers::NONE)
                } else if is_punct(prev, Punct::Dot) {
                    self.syms.push(occurrence(t, name, K::Function, true, SymScope::Member));
                    SemanticToken::at(t, T::Function, decl)
                } else {
                    // `function foo()`: a global, unless a local `foo` is in
                    // scope, which this assigns.
                    match self.lookup(name) {
                        Some((local, id)) => {
                            self.syms.push(occurrence(t, name, local.kind(), false, SymScope::Local(id)))
                        }
                        None => self.syms.push(occurrence(t, name, K::Function, true, SymScope::Project)),
                    }
                    SemanticToken::at(t, T::Function, decl)
                }
            } else if self.local_decl {
                if is_punct(prev, Punct::Lt) {
                    // `local x <const>`
                    SemanticToken::at(t, T::Decorator, Modifiers::NONE)
                } else {
                    let id = self.declare(name, Local::Variable);
                    self.syms.push(occurrence(t, name, K::Local, true, SymScope::Local(id)));
                    SemanticToken::at(t, T::Variable, decl)
                }
            } else if self.in_for_names {
                let id = self.new_binding();
                self.for_names.push((name.to_string(), id));
                self.syms.push(occurrence(t, name, K::Local, true, SymScope::Local(id)));
                SemanticToken::at(t, T::Variable, decl)
            } else if is_punct(prev, Punct::ColonColon) {
                SemanticToken::at(t, T::Label, decl)
            } else if kind(prev) == Some(&TokenKind::Keyword(Kw::Goto)) {
                SemanticToken::at(t, T::Label, Modifiers::NONE)
            } else if is_punct(prev, Punct::Colon) {
                self.syms.push(occurrence(t, name, K::Method, false, SymScope::Member));
                SemanticToken::at(t, T::Method, Modifiers::NONE)
            } else if is_punct(prev, Punct::Dot) {
                let called = starts_args(next);
                // `t.x = v` at the top level defines the field.
                let defines = assigned && self.at_top_level();
                let kind = if called { K::Method } else { K::Field };
                self.syms.push(occurrence(t, name, kind, defines, SymScope::Member));
                let ty = if called { T::Function } else { T::Property };
                SemanticToken::at(t, ty, modified)
            } else if assigned
                && matches!(self.stack.last(), Some(Ctx::Table))
                && matches!(
                    kind(prev),
                    Some(TokenKind::Punct(Punct::LBrace | Punct::Comma | Punct::Semicolon))
                )
            {
                // `{ key = value }`
                self.syms.push(occurrence(t, name, K::Field, true, SymScope::Member));
                SemanticToken::at(t, T::Property, decl)
            } else {
                self.reference(t, name, modified, starts_args(next))
            };
            out.push(token);
        }
        self.syms
    }

    /// A name used as a value: a local, a parameter, or a global. A
    /// global assigned at the top level of the file is declared there.
    fn reference(&mut self, t: &Token, name: &str, mods: Modifiers, called: bool) -> SemanticToken {
        let binding = self.lookup(name);
        let symbol = match binding {
            Some((local, id)) => occurrence(t, name, local.kind(), false, SymScope::Local(id)),
            None => {
                let kind = if called { K::Function } else { K::Global };
                let defines = mods.contains(M::Modification) && self.at_top_level();
                occurrence(t, name, kind, defines, SymScope::Project)
            }
        };
        self.syms.push(symbol);
        match binding.map(|(local, _)| local) {
            Some(Local::Parameter) => SemanticToken::at(t, T::Parameter, mods),
            Some(Local::Function) => SemanticToken::at(t, T::Function, mods),
            Some(Local::Variable) => {
                let ty = if called { T::Function } else { T::Variable };
                SemanticToken::at(t, ty, mods)
            }
            None => {
                let ty = if called { T::Function } else { T::Variable };
                let lib = STDLIB.contains(&name);
                SemanticToken::at(t, ty, mods.with_if(lib, M::DefaultLibrary))
            }
        }
    }

    /// Ends a function's parameter list: its body scope opens with them.
    fn close_params(&mut self) {
        if let Some(f) = self.pending_fn.take_if(|f| f.in_params) {
            let mut params: Vec<(String, Binding)> =
                f.params.into_iter().map(|(p, id)| (p, (Local::Parameter, id))).collect();
            if f.method {
                let id = self.new_binding();
                params.push(("self".to_string(), (Local::Parameter, id)));
            }
            self.push_scope(params);
        }
    }

    /// Scope and declaration bookkeeping for a non-name token.
    fn on_token(&mut self, k: &TokenKind, next: Option<&Token>) {
        // a `local` list ends at its `=` or at anything that can't continue it
        if self.local_decl
            && !matches!(
                k,
                TokenKind::Punct(Punct::Comma | Punct::Lt | Punct::Gt)
            )
        {
            self.local_decl = false;
        }
        match k {
            TokenKind::Keyword(Kw::Local) => {
                if kind(next) == Some(&TokenKind::Keyword(Kw::Function)) {
                    self.local_function = true;
                } else {
                    self.local_decl = true;
                }
            }
            TokenKind::Keyword(Kw::Function) => {
                self.pending_fn = Some(PendingFn {
                    params: Vec::new(),
                    in_params: false,
                    method: false,
                });
                // a statement `function name...`; `local function` names
                // are handled by `local_function`
                self.in_func_name = !self.local_function && !is_punct(next, Punct::LParen);
            }
            TokenKind::Punct(Punct::Colon) if self.in_func_name => {
                if let Some(f) = self.pending_fn.as_mut() {
                    f.method = true;
                }
            }
            TokenKind::Punct(Punct::LParen) => {
                if let Some(f) = self.pending_fn.as_mut().filter(|f| !f.in_params) {
                    f.in_params = true;
                    self.in_func_name = false;
                    self.local_function = false;
                }
            }
            TokenKind::Punct(Punct::RParen) => self.close_params(),
            TokenKind::Keyword(Kw::For) => {
                self.in_for_names = true;
                self.for_names.clear();
            }
            TokenKind::Keyword(Kw::In) | TokenKind::Punct(Punct::Assign) => {
                self.in_for_names = false;
            }
            TokenKind::Keyword(Kw::Do) => {
                let names = std::mem::take(&mut self.for_names);
                self.push_scope(names.into_iter().map(|(n, id)| (n, (Local::Variable, id))));
            }
            TokenKind::Keyword(Kw::Then | Kw::Repeat) => self.push_scope([]),
            TokenKind::Keyword(Kw::Else) => {
                self.pop_scope();
                self.push_scope([]);
            }
            TokenKind::Keyword(Kw::ElseIf | Kw::End | Kw::Until) => self.pop_scope(),
            TokenKind::Punct(Punct::LBrace) => self.stack.push(Ctx::Table),
            TokenKind::Punct(Punct::RBrace) => {
                if let Some(Ctx::Table) = self.stack.last() {
                    self.stack.pop();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "semantic.test.rs"]
mod tests;
