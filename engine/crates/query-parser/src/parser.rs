use crate::{LexError, Token, lex};

pub fn parse_select(input: &str) -> Result<query_ast::SelectQuery, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_select_tokens(&tokens)
}

fn parse_select_tokens(tokens: &[Token]) -> Result<query_ast::SelectQuery, ParseError> {
    Parser::new(tokens).parse_select_stmt()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: ParseErrorKind,
}

impl ParseError {
    fn new(kind: ParseErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }
}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self::new(ParseErrorKind::Lex(error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lex(LexError),
    Unsupported,
}

struct Parser<'a> {
    tokens: &'a [Token],
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens }
    }

    /// Parses:
    ///
    /// ```text
    /// select_stmt := "select" type_ref shape filter_clause? order_clause?
    ///                limit_clause? offset_clause?
    /// ```
    fn parse_select_stmt(&mut self) -> Result<query_ast::SelectQuery, ParseError> {
        let _tokens = self.tokens;
        todo!("select parsing is not implemented yet")
    }
}
