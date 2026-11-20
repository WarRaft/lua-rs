//! A parser, linter and semantic highlighter for Lua 5.3, the scripting
//! language of Warcraft III Reforged. Structured like `jass-rs` and
//! `zinc-rs`; everything not Lua-specific comes from `parser-rs` (see its
//! README for the shared design).

pub mod ast;
pub mod html;
pub mod lexer;
pub mod lint;
pub mod parser;
pub mod printer;
pub mod query;
pub mod semantic;
pub mod token;

pub use ast::{Chunk, ColumnEncoding, Span};
pub use html::render_html;
pub use lint::{lint, Diagnostic, Level};
pub use parser::{ParseError, Parser};
pub use printer::{render_dot, render_tree};
pub use query::{find_node_at, find_node_in, AstRef};
pub use semantic::{encoded_semantic_tokens, semantic_tokens, semantic_tokens_and_symbols, symbols};

/// Parses Lua source into an AST.
pub fn parse(src: &str) -> Result<Chunk, ParseError> {
    Parser::parse_str(src)
}
