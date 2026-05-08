use crate::{Keyword, LexError, Span, Token, TokenKind, lex};
use alloc::{string::String, vec};
use query_ast::{Path, PathStep, SelectQuery, Shape, ShapeItem};

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
    span: Option<Span>,
}

impl ParseError {
    fn new(kind: ParseErrorKind, span: Option<Span>) -> Self {
        Self { kind, span }
    }

    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }

    pub fn span(&self) -> Option<Span> {
        self.span
    }
}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self::new(ParseErrorKind::Lex(error), None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lex(LexError),
    UnexpectedEof { expected: &'static str },
    UnexpectedToken { expected: &'static str },
    Unsupported,
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, cursor: 0 }
    }

    /// Parses:
    ///
    /// ```text
    /// select_stmt := "select" type_ref shape filter_clause? order_clause?
    ///                limit_clause? offset_clause?
    /// ```
    fn parse_select_stmt(&mut self) -> Result<query_ast::SelectQuery, ParseError> {
        self.expect_keyword(Keyword::Select)?;
        let root_type_name = self.expect_ident()?;
        let shape = self.parse_shape()?;
        self.ensure_eof()?;

        Ok(SelectQuery::new(
            root_type_name,
            shape,
            None,
            vec![],
            None,
            None,
        ))
    }

    fn parse_shape(&mut self) -> Result<query_ast::Shape, ParseError> {
        let mut shape_items = vec![];
        self.expect_lbrace()?;

        loop {
            match self.peek() {
                Some(token) if token.kind() == &TokenKind::RBrace => break,
                Some(_) => {
                    shape_items.push(self.parse_shape_item()?);
                    if !self.consume_comma_if_present() {
                        match self.peek() {
                            Some(token) if token.kind() == &TokenKind::RBrace => continue,
                            Some(token) => {
                                return Err(ParseError::new(
                                    ParseErrorKind::UnexpectedToken { expected: ", or }" },
                                    Some(token.span()),
                                ));
                            }
                            None => {
                                return Err(ParseError::new(
                                    ParseErrorKind::UnexpectedEof { expected: "}" },
                                    None,
                                ));
                            }
                        }
                    }
                }
                None => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedEof { expected: "}" },
                        None,
                    ));
                }
            }
        }

        self.expect_rbrace()?;
        Ok(Shape::new(shape_items))
    }

    fn parse_shape_item(&mut self) -> Result<query_ast::ShapeItem, ParseError> {
        let field_name = self.expect_ident()?;
        let path = Path::new(vec![PathStep::new(field_name)]);
        let child_shape = match self.peek() {
            Some(token) if token.kind() == &TokenKind::Colon => {
                self.advance();
                Some(self.parse_shape()?)
            }
            _ => None,
        };
        Ok(ShapeItem::new(path, child_shape))
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.cursor);
        self.cursor += usize::from(token.is_some());
        token
    }

    fn expect_keyword(&mut self, expected: Keyword) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(expected) => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected: expected.as_str(),
                },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: expected.as_str(),
                },
                None,
            )),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some(token) => match token.kind() {
                TokenKind::Ident(value) => {
                    let value = value.clone();
                    self.advance();
                    Ok(value)
                }
                _ => Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken { expected: "IDENT" },
                    Some(token.span()),
                )),
            },
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "IDENT" },
                None,
            )),
        }
    }

    fn expect_lbrace(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::LBrace => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: "{" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "{" },
                None,
            )),
        }
    }

    fn expect_rbrace(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::RBrace => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: "}" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: "}" },
                None,
            )),
        }
    }

    fn expect_compare_op(&mut self) -> Result<query_ast::CompareOp, ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Eq => {
                self.advance();
                Ok(query_ast::CompareOp::Eq)
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected: "comparison operator",
                },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "comparison operator",
                },
                None,
            )),
        }
    }

    fn expect_literal(&mut self) -> Result<query_ast::Literal, ParseError> {
        match self.peek() {
            Some(token) => match token.kind() {
                TokenKind::String(value) => {
                    let value = value.clone();
                    self.advance();
                    Ok(query_ast::Literal::String(value))
                }
                _ => Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken {
                        expected: "literal",
                    },
                    Some(token.span()),
                )),
            },
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "literal",
                },
                None,
            )),
        }
    }

    fn ensure_eof(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) => Err(ParseError {
                kind: ParseErrorKind::UnexpectedToken { expected: "EOF" },
                span: Some(token.span()),
            }),
            None => Ok(()),
        }
    }

    fn consume_comma_if_present(&mut self) -> bool {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Comma => {
                self.advance();
                true
            }
            _ => false,
        }
    }
}
