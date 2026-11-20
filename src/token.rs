//! Lua's token vocabulary: which of the shared [`Kw`]s are active, on top
//! of the shared [`parser_rs::TokenKind`] (identifiers, literals, [`Punct`]).

pub use parser_rs::{Kw, KwSet, Punct};

/// Every Lua 5.3 keyword.
pub const LUA_KEYWORDS: KwSet = KwSet::of(&[
    Kw::And,
    Kw::Break,
    Kw::Do,
    Kw::Else,
    Kw::ElseIf,
    Kw::End,
    Kw::False,
    Kw::For,
    Kw::Function,
    Kw::Goto,
    Kw::If,
    Kw::In,
    Kw::Local,
    Kw::Nil,
    Kw::Not,
    Kw::Or,
    Kw::Repeat,
    Kw::Return,
    Kw::Then,
    Kw::True,
    Kw::Until,
    Kw::While,
]);

/// A Lua token kind (the shared one).
pub type TokenKind = parser_rs::TokenKind;

/// A Lua token.
pub type Token = parser_rs::Token<TokenKind>;
