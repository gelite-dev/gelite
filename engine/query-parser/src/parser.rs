use crate::{Keyword, LexError, Span, Token, TokenKind, lex};
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{string::String, vec};
use query_ast::{
    CompareExpr, Expr, Literal, OrderExpr, Path, PathStep, SelectQuery, Shape, ShapeItem,
};

/// Parses one MVP `select` statement from source text.
///
/// The parser checks syntax only. Schema names, field names, link traversal,
/// and type compatibility are validated by the resolver.
pub fn parse_select(input: &str) -> Result<query_ast::SelectQuery, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_select_tokens(&tokens)
}

fn parse_select_tokens(tokens: &[Token]) -> Result<query_ast::SelectQuery, ParseError> {
    Parser::new(tokens).parse_select_stmt()
}

/// Parser error with an optional source span.
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

/// Parser error category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lex(LexError),
    UnexpectedEof { expected: &'static str },
    UnexpectedToken { expected: &'static str },
    UnexpectedValue { expected: &'static str },
    InvalidIntegerLiteral,
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
        let filter = self.parse_filter_clause()?;
        let order_by = self.parse_order_clause()?;
        let limit = self.parse_limit_clause()?;
        let offset = self.parse_offset_clause()?;
        self.ensure_eof()?;

        Ok(SelectQuery::new(
            root_type_name,
            shape,
            filter,
            order_by,
            limit,
            offset,
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

    fn parse_filter_clause(&mut self) -> Result<Option<query_ast::Expr>, ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Filter) => {
                self.expect_keyword(Keyword::Filter)?;
                Ok(Some(self.parse_expr()?))
            }
            _ => Ok(None),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_and_expr()?;

        while self.consume_contextual_keyword_if_present("or") {
            let right = self.parse_and_expr()?;
            expr = Expr::Or(Box::new(expr), Box::new(right));
        }

        Ok(expr)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_not_expr()?;

        while self.consume_contextual_keyword_if_present("and") {
            let right = self.parse_not_expr()?;
            expr = Expr::And(Box::new(expr), Box::new(right));
        }

        Ok(expr)
    }

    fn parse_not_expr(&mut self) -> Result<Expr, ParseError> {
        if self.consume_contextual_keyword_if_present("not") {
            return Ok(Expr::Not(Box::new(self.parse_not_expr()?)));
        }

        self.parse_compare_expr()
    }

    fn parse_compare_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_primary_expr()?;

        if self.is_compare_op() {
            let op = self.expect_compare_op()?;
            let right = self.parse_primary_expr()?;
            return Ok(Expr::Compare(CompareExpr::new(left, op, right)));
        }

        if self.peek().is_some_and(is_primary_expr_start) {
            let token = self.peek().expect("peek checked token exists");
            return Err(ParseError::new(
                ParseErrorKind::UnexpectedToken {
                    expected: "comparison operator",
                },
                Some(token.span()),
            ));
        }

        Ok(left)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect_rparen()?;
                Ok(expr)
            }
            Some(token) if token.kind() == &TokenKind::Dot => {
                Ok(Expr::Path(self.parse_path(true)?))
            }
            Some(token) => match token.kind() {
                TokenKind::Ident(_) => Ok(Expr::Path(self.parse_path(false)?)),
                TokenKind::Int(_)
                | TokenKind::String(_)
                | TokenKind::Keyword(Keyword::True)
                | TokenKind::Keyword(Keyword::False)
                | TokenKind::Keyword(Keyword::Null) => Ok(Expr::Literal(self.expect_literal()?)),
                _ => Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken {
                        expected: "expression",
                    },
                    Some(token.span()),
                )),
            },
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof {
                    expected: "expression",
                },
                None,
            )),
        }
    }

    fn parse_order_clause(&mut self) -> Result<Vec<OrderExpr>, ParseError> {
        if !self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Keyword(Keyword::Order))
        {
            return Ok(vec![]);
        }

        let mut results = vec![];

        self.expect_keyword(Keyword::Order)?;
        self.expect_keyword(Keyword::By)?;

        results.push(self.parse_order_item()?);
        while self.consume_comma_if_present() {
            results.push(self.parse_order_item()?);
        }

        Ok(results)
    }

    fn parse_order_item(&mut self) -> Result<OrderExpr, ParseError> {
        let path = self.parse_path(true)?;
        let direction = match self.peek() {
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Desc) => {
                self.advance();
                query_ast::OrderDirection::Desc
            }
            Some(token) if token.kind() == &TokenKind::Keyword(Keyword::Asc) => {
                self.advance();
                query_ast::OrderDirection::Asc
            }
            _ => query_ast::OrderDirection::Asc,
        };

        Ok(OrderExpr::new(path, direction))
    }
    fn parse_path(&mut self, allow_leading_dot: bool) -> Result<Path, ParseError> {
        if self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Dot)
        {
            if !allow_leading_dot {
                let token = self.peek().expect("peek checked token exists");
                return Err(ParseError::new(
                    ParseErrorKind::UnexpectedToken { expected: "IDENT" },
                    Some(token.span()),
                ));
            }

            self.advance();
        }

        let mut steps = vec![];
        steps.push(PathStep::new(self.expect_ident()?));

        while self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Dot)
        {
            self.advance();
            steps.push(PathStep::new(self.expect_ident()?));
        }

        Ok(Path::new(steps))
    }

    fn parse_limit_clause(&mut self) -> Result<Option<i64>, ParseError> {
        if !self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Keyword(Keyword::Limit))
        {
            return Ok(None);
        }

        self.expect_keyword(Keyword::Limit)?;

        match self.expect_literal()? {
            Literal::Int64(value) if value >= 0 => Ok(Some(value)),
            _ => Err(ParseError::new(
                ParseErrorKind::UnexpectedValue {
                    expected: "non-negative integer",
                },
                None,
            )),
        }
    }

    fn parse_offset_clause(&mut self) -> Result<Option<i64>, ParseError> {
        if !self
            .peek()
            .is_some_and(|token| token.kind() == &TokenKind::Keyword(Keyword::Offset))
        {
            return Ok(None);
        }

        self.expect_keyword(Keyword::Offset)?;
        match self.expect_literal()? {
            Literal::Int64(value) if value >= 0 => Ok(Some(value)),
            _ => Err(ParseError::new(
                ParseErrorKind::UnexpectedValue {
                    expected: "non-negative integer",
                },
                None,
            )),
        }
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

    fn expect_rparen(&mut self) -> Result<(), ParseError> {
        match self.peek() {
            Some(token) if token.kind() == &TokenKind::RParen => {
                self.advance();
                Ok(())
            }
            Some(token) => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken { expected: ")" },
                Some(token.span()),
            )),
            None => Err(ParseError::new(
                ParseErrorKind::UnexpectedEof { expected: ")" },
                None,
            )),
        }
    }

    fn is_compare_op(&self) -> bool {
        self.peek()
            .is_some_and(|token| token.kind() == &TokenKind::Eq)
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
                TokenKind::Int(value) => {
                    let parsed = value.parse::<i64>().map_err(|_| {
                        ParseError::new(ParseErrorKind::InvalidIntegerLiteral, Some(token.span()))
                    })?;
                    self.advance();
                    Ok(query_ast::Literal::Int64(parsed))
                }
                TokenKind::String(value) => {
                    let value = value.clone();
                    self.advance();
                    Ok(query_ast::Literal::String(value))
                }
                TokenKind::Keyword(Keyword::True) => {
                    self.advance();
                    Ok(query_ast::Literal::Bool(true))
                }
                TokenKind::Keyword(Keyword::False) => {
                    self.advance();
                    Ok(query_ast::Literal::Bool(false))
                }
                TokenKind::Keyword(Keyword::Null) => {
                    self.advance();
                    Ok(query_ast::Literal::Null)
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

    fn consume_contextual_keyword_if_present(&mut self, keyword: &str) -> bool {
        match self.peek() {
            Some(token) if token_is_ident(token, keyword) => {
                self.advance();
                true
            }
            _ => false,
        }
    }
}

fn is_primary_expr_start(token: &Token) -> bool {
    if token_is_ident(token, "and") || token_is_ident(token, "or") || token_is_ident(token, "in") {
        return false;
    }

    matches!(
        token.kind(),
        TokenKind::Dot
            | TokenKind::Ident(_)
            | TokenKind::Int(_)
            | TokenKind::String(_)
            | TokenKind::LParen
            | TokenKind::Keyword(Keyword::True)
            | TokenKind::Keyword(Keyword::False)
            | TokenKind::Keyword(Keyword::Null)
    )
}

fn token_is_ident(token: &Token, expected: &str) -> bool {
    matches!(token.kind(), TokenKind::Ident(value) if value == expected)
}
