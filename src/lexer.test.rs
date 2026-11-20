use super::*;
use crate::token::{Kw, TokenKind};

fn kinds(src: &str) -> Vec<TokenKind> {
    Lexer::tokenize(src)
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

#[test]
fn tokenizes_keywords_names_and_literals() {
    assert_eq!(
        kinds("local function f() return nil end"),
        vec![
            TokenKind::Keyword(Kw::Local),
            TokenKind::Keyword(Kw::Function),
            TokenKind::Ident("f".into()),
            TokenKind::Punct(Punct::LParen),
            TokenKind::Punct(Punct::RParen),
            TokenKind::Keyword(Kw::Return),
            TokenKind::Keyword(Kw::Nil),
            TokenKind::Keyword(Kw::End),
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("42 0xff 3.5 .5 1e3 'a\\'b' \"c\" [==[d]==]"),
        vec![
            TokenKind::Int(42),
            TokenKind::Int(255),
            TokenKind::Real(3.5),
            TokenKind::Real(0.5),
            TokenKind::Real(1000.0),
            TokenKind::Str("a'b".into()),
            TokenKind::Str("c".into()),
            TokenKind::Str("d".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn tokenizes_lua_operators() {
    assert_eq!(
        kinds("... .. . ~= ~ // / :: : # ^ << >>"),
        [
            Punct::Ellipsis,
            Punct::DotDot,
            Punct::Dot,
            Punct::TildeEq,
            Punct::Tilde,
            Punct::SlashSlash,
            Punct::Slash,
            Punct::ColonColon,
            Punct::Colon,
            Punct::Hash,
            Punct::Caret,
            Punct::Shl,
            Punct::Shr,
        ]
        .into_iter()
        .map(TokenKind::Punct)
        .chain([TokenKind::Eof])
        .collect::<Vec<_>>()
    );
}

#[test]
fn skips_line_and_long_comments() {
    assert_eq!(
        kinds("-- line\n--[[ long\ncomment ]] x --[==[ ]] ]==] y"),
        vec![
            TokenKind::Ident("x".into()),
            TokenKind::Ident("y".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn library_and_other_jass_words_are_plain_names() {
    assert_eq!(kinds("library")[0], TokenKind::Ident("library".into()));
    assert_eq!(kinds("then")[0], TokenKind::Keyword(Kw::Then));
}
