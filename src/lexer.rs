use crate::token::{LUA_KEYWORDS, Token};
use parser_rs::{LexSpec, NumberSpec, ParseError, Punct};

/// Lua 5.3's lexical rules for the shared [`parser_rs::lexer`]: `--` line
/// comments and `--[[ ]]` long comments, `"..."`/`'...'`/`[[...]]`
/// strings, exponents and `.5` reals, and Lua's operator set.
pub const SPEC: LexSpec = LexSpec {
    keywords: LUA_KEYWORDS,
    line_comment: "--",
    single_quote_strings: true,
    long_brackets: true,
    numbers: NumberSpec {
        exponent: true,
        leading_dot: true,
        ..LexSpec::BASE.numbers
    },
    puncts: &[
        Punct::Plus,
        Punct::Minus,
        Punct::Star,
        Punct::Slash,
        Punct::SlashSlash,
        Punct::Percent,
        Punct::Caret,
        Punct::Hash,
        Punct::Amp,
        Punct::Tilde,
        Punct::Pipe,
        Punct::Shl,
        Punct::Shr,
        Punct::EqEq,
        Punct::TildeEq,
        Punct::LtEq,
        Punct::GtEq,
        Punct::Lt,
        Punct::Gt,
        Punct::Assign,
        Punct::LParen,
        Punct::RParen,
        Punct::LBrace,
        Punct::RBrace,
        Punct::LBracket,
        Punct::RBracket,
        Punct::ColonColon,
        Punct::Semicolon,
        Punct::Colon,
        Punct::Comma,
        Punct::Dot,
        Punct::DotDot,
        Punct::Ellipsis,
    ],
    ..LexSpec::BASE
};

pub struct Lexer;

impl Lexer {
    pub fn tokenize(src: &str) -> Result<Vec<Token>, ParseError> {
        parser_rs::tokenize(src, &SPEC)
    }
}

#[cfg(test)]
#[path = "lexer.test.rs"]
mod tests;
