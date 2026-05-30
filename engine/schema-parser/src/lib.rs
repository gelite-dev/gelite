#![no_std]
//! Parser frontend for the `.geli` schema language.
//!
//! This crate is responsible for turning schema source text into `schema`
//! values. The first implementation starts with lexing only; parsing will
//! build `schema_model::SchemaCatalog` directly instead of introducing a separate
//! schema AST.

extern crate alloc;

mod lexer;
mod parser;

pub use lexer::{Keyword, LexError, LexErrorKind, Position, Span, Token, TokenKind, lex};
pub use parser::{ParseError, ParseErrorKind, parse_schema};

#[cfg(test)]
mod tests;
