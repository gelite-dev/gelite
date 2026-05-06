use alloc::string::String;
use alloc::vec::Vec;

pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(input).lex()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    kind: TokenKind,
    span: Span,
}

impl Token {
    fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn kind(&self) -> &TokenKind {
        &self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(Keyword),
    Ident(String),
    String(String),
    Int(String),
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    Eq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Select,
    Filter,
    Order,
    By,
    Limit,
    Offset,
    Asc,
    Desc,
    And,
    Or,
    Not,
    True,
    False,
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    start: Position,
    end: Position,
}

impl Span {
    fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn start(&self) -> Position {
        self.start
    }

    pub fn end(&self) -> Position {
        self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    byte: usize,
    line: usize,
    column: usize,
}

impl Position {
    fn new(byte: usize, line: usize, column: usize) -> Self {
        Self { byte, line, column }
    }

    pub fn byte(&self) -> usize {
        self.byte
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    kind: LexErrorKind,
    position: Position,
}

impl LexError {
    fn new(kind: LexErrorKind, position: Position) -> Self {
        Self { kind, position }
    }

    pub fn kind(&self) -> &LexErrorKind {
        &self.kind
    }

    pub fn position(&self) -> Position {
        self.position
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexErrorKind {
    UnexpectedChar(char),
    UnterminatedString,
}

struct Lexer<'a> {
    input: &'a str,
    cursor: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            cursor: 0,
            line: 1,
            column: 1,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek_char() {
            match ch {
                ch if ch.is_whitespace() => {
                    self.advance_char();
                }
                ch if is_ident_start(ch) => {
                    tokens.push(self.lex_ident_or_keyword());
                }
                ch if ch.is_ascii_digit() => {
                    tokens.push(self.lex_int());
                }
                '"' => {
                    tokens.push(self.lex_string()?);
                }
                '{' => {
                    tokens.push(self.lex_single_char(TokenKind::LBrace));
                }
                '}' => {
                    tokens.push(self.lex_single_char(TokenKind::RBrace));
                }
                ',' => {
                    tokens.push(self.lex_single_char(TokenKind::Comma));
                }
                ':' => {
                    tokens.push(self.lex_single_char(TokenKind::Colon));
                }
                '.' => {
                    tokens.push(self.lex_single_char(TokenKind::Dot));
                }
                '=' => {
                    tokens.push(self.lex_single_char(TokenKind::Eq));
                }
                ch => {
                    return Err(LexError::new(
                        LexErrorKind::UnexpectedChar(ch),
                        self.position(),
                    ));
                }
            }
        }

        Ok(tokens)
    }

    fn lex_ident_or_keyword(&mut self) -> Token {
        let start = self.position();
        let mut value = String::new();

        while let Some(ch) = self.peek_char() {
            if !is_ident_continue(ch) {
                break;
            }

            value.push(ch);
            self.advance_char();
        }

        let kind = match keyword(&value) {
            Some(keyword) => TokenKind::Keyword(keyword),
            None => TokenKind::Ident(value),
        };

        Token::new(kind, Span::new(start, self.position()))
    }

    fn lex_int(&mut self) -> Token {
        let start = self.position();
        let mut value = String::new();

        while let Some(ch) = self.peek_char() {
            if !ch.is_ascii_digit() {
                break;
            }

            value.push(ch);
            self.advance_char();
        }

        Token::new(TokenKind::Int(value), Span::new(start, self.position()))
    }

    fn lex_string(&mut self) -> Result<Token, LexError> {
        let start = self.position();
        self.advance_char();

        let mut value = String::new();

        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                self.advance_char();
                return Ok(Token::new(
                    TokenKind::String(value),
                    Span::new(start, self.position()),
                ));
            }

            value.push(ch);
            self.advance_char();
        }

        Err(LexError::new(LexErrorKind::UnterminatedString, start))
    }

    fn lex_single_char(&mut self, kind: TokenKind) -> Token {
        let start = self.position();
        self.advance_char();
        Token::new(kind, Span::new(start, self.position()))
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.cursor += ch.len_utf8();

        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        Some(ch)
    }

    fn position(&self) -> Position {
        Position::new(self.cursor, self.line, self.column)
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}

fn keyword(value: &str) -> Option<Keyword> {
    match value {
        "select" => Some(Keyword::Select),
        "filter" => Some(Keyword::Filter),
        "order" => Some(Keyword::Order),
        "by" => Some(Keyword::By),
        "limit" => Some(Keyword::Limit),
        "offset" => Some(Keyword::Offset),
        "asc" => Some(Keyword::Asc),
        "desc" => Some(Keyword::Desc),
        "and" => Some(Keyword::And),
        "or" => Some(Keyword::Or),
        "not" => Some(Keyword::Not),
        "true" => Some(Keyword::True),
        "false" => Some(Keyword::False),
        "null" => Some(Keyword::Null),
        _ => None,
    }
}
