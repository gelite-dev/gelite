# Query Parser Implementation Plan

## Purpose

This document describes the implementation plan for the `query-parser` crate.

The parser is the frontend layer that turns query source text into
`query-ast` values:

```text
query text -> query-parser -> query-ast -> resolver -> ir
```

It must not validate schema names, resolve fields, infer scalar/link meaning,
lower to IR, plan SQLite operations, or generate SQL. Those responsibilities
belong to later pipeline stages.

## Relationship To Existing Documents

- `spec/query.md` defines the query language surface syntax and intended MVP
  semantics.
- `plan/implementation-start-plan.md` explains why `query-ast` was implemented
  before a parser.
- This document explains how to add the parser now that `query-ast`, `resolver`,
  `ir`, `sqlite-plan`, and `sqlite-sqlgen` have a working select pipeline.

When this document conflicts with `spec/query.md`, the spec wins. When it
conflicts with current implementation, the difference should be treated as
deferred implementation work unless the current user instruction says
otherwise.

## Current Parser Scope

The first parser milestone should target the query syntax that current
`query-ast` and `resolver` can actually represent.

Include first:

- `select` statements
- root type name
- explicit shape
- nested shape items
- field paths
- single `=` filter comparison
- string literals
- order by path with optional direction
- limit
- offset

Defer for now:

- `insert`
- `update`
- `delete`
- comparison operators other than `=`
- boolean expression trees in AST/parser
- float literals
- escape sequence interpretation in string literals
- parameters
- comments
- parser recovery after an error

This keeps the parser aligned with the current executable pipeline instead of
parsing syntax that later layers cannot consume yet.

## Public API Boundary

The primary public API should accept source text:

```rust
pub fn parse_select(input: &str) -> Result<query_ast::SelectQuery, ParseError>
```

Internally it should lex first, then parse tokens:

```rust
pub fn parse_select(input: &str) -> Result<query_ast::SelectQuery, ParseError> {
    let tokens = lex(input).map_err(ParseError::from)?;
    parse_select_tokens(&tokens)
}

fn parse_select_tokens(tokens: &[Token]) -> Result<query_ast::SelectQuery, ParseError> {
    Parser::new(tokens).parse_select_stmt()
}
```

`parse_select_tokens` should remain private for now. Exposing token-level parser
APIs would make `Token` a stronger public compatibility contract than needed.

## Lexer Strategy

The lexer uses `logos` internally.

The public lexer boundary remains project-owned:

- `lex(input: &str) -> Result<Vec<Token>, LexError>`
- `Token`
- `TokenKind`
- `Span`
- `Position`
- `LexError`

`logos` types must not leak into public API. This keeps the project free to
replace the lexer implementation later without changing parser users.

### Position Tracking

Every token carries a `Span`.

`Span` contains:

- start `Position`
- end `Position`

`Position` contains:

- UTF-8 byte offset
- 1-based line number
- 1-based column number

`Span.end` means the position immediately after the token. Column calculation
is character-based, while `byte` remains UTF-8 byte-based. This distinction is
important for Unicode inside string literals.

### String Literal Policy

Current policy:

- string literals use double quotes
- string literals may span multiple lines
- string token values do not include the surrounding quotes
- escape sequence interpretation is not implemented yet
- an opening quote without a closing quote produces `LexErrorKind::UnterminatedString`

Deferred escape decisions:

- whether `\"` should produce a literal double quote
- whether `\\`, `\n`, `\t`, or Unicode escapes should be supported
- whether unknown escapes should be rejected or preserved literally

Do not add escape behavior without tests that define the exact token value.

## Parser Strategy

Use recursive descent for statement, clause, shape, and path syntax.

Use a Pratt parser only when boolean/filter expression complexity requires it.
The current `query-ast` only supports a simple compare expression, so the first
parser pass should not introduce Pratt machinery prematurely.

Recommended initial parser structure:

```text
Parser
  parse_select_stmt
  parse_shape
  parse_shape_item
  parse_path
  parse_filter_clause
  parse_order_clause
  parse_limit_clause
  parse_offset_clause
```

Expression parsing can start with:

```text
parse_filter_expr
  parse_path
  expect "="
  parse_literal
```

Later, once `query-ast::Expr` supports boolean trees, expression parsing can be
replaced or extended with:

```text
parse_expr_bp(min_binding_power)
```

## Suggested Module Structure

Keep `lib.rs` small. It should expose the crate API and re-export stable public
types:

```text
src/
  lib.rs
  lexer.rs
  parser.rs
  tests/
    mod.rs
```

`lexer.rs` should own:

- `lex`
- `Token`
- `TokenKind`
- `Keyword`
- `Span`
- `Position`
- `LexError`
- `LexErrorKind`
- `RawTokenKind` for `logos`
- line map helpers

`parser.rs` should own:

- `parse_select`
- private `parse_select_tokens`
- `Parser`
- `ParseError`
- `ParseErrorKind`
- parser helper methods

`tests/mod.rs` can contain both lexer and parser tests at first. If it becomes
large, split it later into:

```text
tests/
  mod.rs
  lexer.rs
  parser.rs
```

Do not split prematurely unless the file becomes hard to scan.

## Suggested Parser State

The parser should borrow tokens instead of owning them:

```rust
struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
}
```

This keeps parser construction cheap and avoids copying token streams.

Minimum methods:

```rust
impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self
    fn parse_select_stmt(&mut self) -> Result<query_ast::SelectQuery, ParseError>

    fn peek(&self) -> Option<&'a Token>
    fn advance(&mut self) -> Option<&'a Token>
    fn is_at_end(&self) -> bool
}
```

`peek` should not advance. `advance` should return the current token and move
the cursor by one. All higher-level parsing helpers should be built on these two
operations.

## Suggested Parser Helpers

Add small helpers as tests force them. Avoid adding a large helper API before it
has a caller.

Recommended first helpers:

```rust
fn expect_keyword(&mut self, expected: Keyword) -> Result<Token, ParseError>
fn expect_token_kind(&mut self, expected: ExpectedTokenKind) -> Result<Token, ParseError>
fn expect_ident(&mut self) -> Result<String, ParseError>
fn ensure_eof(&self) -> Result<(), ParseError>
```

`expect_token_kind` should not necessarily take `TokenKind` directly because
some `TokenKind` variants carry data. A small internal enum can make error
messages clearer:

```rust
enum ExpectedTokenKind {
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    Eq,
    Int,
    String,
    Ident,
}
```

Alternatively, start with explicit helper functions:

```rust
fn expect_lbrace(&mut self) -> Result<(), ParseError>
fn expect_rbrace(&mut self) -> Result<(), ParseError>
```

The explicit helper approach is more verbose but often easier while learning.
It can be collapsed later once patterns repeat.

## Suggested Parse Error Shape

The current skeleton has `ParseErrorKind::Unsupported`. Replace it before real
parser implementation grows.

Recommended near-term shape:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: ParseErrorKind,
    span: Option<Span>,
}
```

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lex(LexError),
    UnexpectedEof { expected: &'static str },
    UnexpectedToken { expected: &'static str },
    TrailingToken,
    InvalidInteger,
}
```

Why `span: Option<Span>`:

- unexpected token errors can point at the actual token span
- trailing token errors can point at the extra token
- lexical errors already carry their own `Position`, so the parser span may be
  absent for `Lex`
- EOF has no token span unless the parser separately stores the input end
  position

Later, if better diagnostics are needed, split lexical and parser locations more
explicitly:

```rust
pub enum ParseLocation {
    Span(Span),
    Position(Position),
}
```

Do not block the first parser tests on a perfect error model. The important part
is to avoid losing location information entirely.

## Minimal Select Implementation Sketch

The first implementation should only parse:

```text
select IDENT { IDENT }
```

Suggested control flow:

```rust
fn parse_select_stmt(&mut self) -> Result<query_ast::SelectQuery, ParseError> {
    self.expect_keyword(Keyword::Select)?;
    let root_type_name = self.expect_ident()?;
    let shape = self.parse_shape()?;
    self.ensure_eof()?;

    Ok(query_ast::SelectQuery::new(
        root_type_name,
        shape,
        None,
        alloc::vec![],
        None,
        None,
    ))
}
```

For the first test, `parse_shape` can be intentionally narrow:

```rust
fn parse_shape(&mut self) -> Result<query_ast::Shape, ParseError> {
    self.expect_lbrace()?;
    let item = self.parse_shape_item()?;
    self.expect_rbrace()?;
    Ok(query_ast::Shape::new(alloc::vec![item]))
}
```

And `parse_shape_item` can start as:

```rust
fn parse_shape_item(&mut self) -> Result<query_ast::ShapeItem, ParseError> {
    let field_name = self.expect_ident()?;
    let path = query_ast::Path::new(alloc::vec![query_ast::PathStep::new(field_name)]);
    Ok(query_ast::ShapeItem::new(path, None))
}
```

This intentionally does not parse commas, multiple items, nested child shapes,
or paths with multiple steps yet. Add those only when the corresponding tests
exist.

## Shape Parsing Expansion Sketch

After the first minimal select test passes, expand `parse_shape` into a loop:

```text
expect "{"
while next token is not "}" {
  parse shape item
  if next token is ",":
    consume it
    continue
  else:
    continue only if next token is "}"
}
expect "}"
```

Decide the comma policy with tests.

Recommended first policy:

- allow commas between shape items
- allow a trailing comma
- allow newline/whitespace without commas only if the lexer/parser can make
  that unambiguous

Because the lexer discards whitespace, allowing item separation without commas
can be ambiguous later. Prefer requiring commas between adjacent shape items for
now unless the spec is updated.

Example tests:

```text
select Post {
  id,
  title
}
```

```text
select Post {
  id,
  title,
}
```

If the current spec allows optional commas, document the implemented rule in the
parser function comment so future changes are intentional.

## Nested Shape Parsing Sketch

`shape_item` starts as:

```text
shape_item := IDENT (":" shape)?
```

Implementation flow:

```text
field_name = expect_ident()
if next token is ":":
  consume ":"
  child_shape = parse_shape()
else:
  child_shape = None
return ShapeItem(path(field_name), child_shape)
```

Do not validate whether the field is actually a link. The resolver owns that
semantic check.

## Path Parsing Sketch

The parser needs two path forms:

```text
shape item path: IDENT ("." IDENT)*
filter/order path: "."? IDENT ("." IDENT)*
```

Current `query-ast::Path` stores only steps, not whether the path was
root-relative. That means `.title` and `title` both become the same
`Path(["title"])`.

Recommended helper:

```rust
fn parse_path(&mut self, allow_leading_dot: bool) -> Result<query_ast::Path, ParseError>
```

Initial behavior:

- if `allow_leading_dot` is true and next token is `.`, consume it
- read one identifier
- while next token is `.`, consume it and read another identifier

Use `allow_leading_dot = false` for shape items at first. Use
`allow_leading_dot = true` for filter and order clauses.

Open decision:

- Should filter/order paths require a leading dot or merely allow it?

The spec says the leading `.` in filter paths refers to the current row root, so
requiring it may be clearer. Current `query-ast` cannot preserve that
distinction, so this decision should be made with a parser test.

## Clause Parsing Sketch

After `shape`, parse optional clauses in spec order:

```text
filter?
order by?
limit?
offset?
```

Recommended `parse_select_stmt` shape after clauses are added:

```rust
let filter = self.parse_optional_filter_clause()?;
let order_by = self.parse_optional_order_clause()?;
let limit = self.parse_optional_limit_clause()?;
let offset = self.parse_optional_offset_clause()?;
self.ensure_eof()?;
```

Do not accept arbitrary clause order unless there is a spec decision to support
it. Fixed order keeps the first parser simpler and makes errors easier.

### Filter Clause

Initial grammar:

```text
filter_clause := "filter" path "=" string_literal
```

Implementation:

```text
if next token is not keyword filter:
  return None
consume filter
left = parse_path(allow_leading_dot = true)
expect "="
right = expect string literal
return Expr::Compare(CompareExpr::new(left, CompareOp::Eq, Literal::String(right)))
```

Do not parse boolean `and/or/not` yet even though the lexer recognizes the
keywords.

### Order Clause

Initial grammar:

```text
order_clause := "order" "by" order_item ("," order_item)*
order_item := path ("asc" | "desc")?
```

Implementation:

```text
if next token is not keyword order:
  return empty vec
consume order
expect keyword by
parse at least one order item
while comma:
  parse another order item
```

Direction default:

- missing direction means `OrderDirection::Asc`

### Limit And Offset

Initial grammar:

```text
limit_clause := "limit" INT
offset_clause := "offset" INT
```

Implementation:

```text
if next token is keyword limit:
  consume limit
  parse u64 from int token
else:
  None
```

The parser should reject integer overflow with `ParseErrorKind::InvalidInteger`.
Negative numbers are not lexed as `Int`, so they should fail as unexpected
tokens unless negative number support is added later.

## Parser Documentation Policy

Parser functions should include short doc comments with the grammar fragment
they implement.

Example:

```rust
/// Parses:
///
/// ```text
/// select_stmt := "select" type_ref shape filter_clause? order_clause?
///                limit_clause? offset_clause?
/// ```
fn parse_select_stmt(&mut self) -> Result<query_ast::SelectQuery, ParseError>
```

Keep these comments close to the implementation. They should document the
currently implemented subset, not the entire future spec.

## Error Model

`ParseError` should carry enough location information to explain what went
wrong.

Minimum useful forms:

```rust
pub enum ParseErrorKind {
    Lex(LexError),
    UnexpectedEof,
    UnexpectedToken,
}
```

A production-quality version should distinguish:

- expected keyword
- expected token kind
- expected identifier
- expected literal
- trailing tokens after a complete statement
- unsupported syntax that is lexically valid but outside the current parser
  scope

`ParseError` should report a `Position` or `Span` where possible. EOF errors may
need the parser to remember the end position of the last token.

## Implementation Order

### 1. Parser Skeleton

Goal:

- make `parse_select` call `lex`
- make `Parser` consume `&[Token]`
- keep token-level parsing private

Tests:

- none required beyond current lexer tests until the first parser behavior test

### 2. Parse Minimal Select

Supported syntax:

```text
select Post {
  title
}
```

Test:

- `parser_can_parse_select_with_single_scalar_shape`

Verify:

- root type name is `Post`
- shape has one item
- item path has one step: `title`
- child shape is absent
- filter is absent
- order list is empty
- limit is absent
- offset is absent

### 3. Parse Multiple Shape Items

Supported syntax:

```text
select Post {
  id,
  title
}
```

Test:

- `parser_preserves_shape_item_order`

Verify:

- `id` appears before `title`
- optional trailing comma policy is explicit

### 4. Parse Nested Shape

Supported syntax:

```text
select Post {
  author: {
    name
  }
}
```

Test:

- `parser_can_parse_nested_shape_item`

Verify:

- outer item path is `author`
- outer item has child shape
- child shape has `name`

### 5. Parse Filter Compare

Supported syntax:

```text
select Post {
  title
}
filter .title = "Hello"
```

Test:

- `parser_can_parse_filter_compare_path_equals_string_literal`

Verify:

- filter is `Expr::Compare`
- left path is `.title`
- operator is `CompareOp::Eq`
- right literal is `Literal::String("Hello")`

Current `query-ast::CompareExpr` is asymmetric: left is `Path`, right is
`Literal`. Do not parse `Literal = Path` until `query-ast` supports it.

### 6. Parse Order By

Supported syntax:

```text
order by .title desc
```

Tests:

- `parser_can_parse_order_by_path_desc`
- `parser_defaults_order_direction_to_asc`

Verify:

- path uses the root-relative path syntax
- `desc` maps to `OrderDirection::Desc`
- omitted direction maps to `OrderDirection::Asc`

### 7. Parse Limit And Offset

Supported syntax:

```text
limit 10
offset 20
```

Tests:

- `parser_can_parse_limit`
- `parser_can_parse_offset`
- `parser_can_parse_limit_and_offset`

Verify:

- integer token converts to `u64`
- invalid integer conversion becomes a parse error

### 8. End-To-End Parser Smoke Test

Supported syntax:

```text
select Post {
  id,
  title
}
filter .title = "Hello"
order by .title desc
limit 10
offset 20
```

Test:

- `parser_can_parse_select_with_filter_order_limit_and_offset`

This test should remain focused on AST construction only. Resolver, IR, SQLite
plan, and SQL generation should have their own tests.

## Open Questions

Resolve these when tests force the decision:

- Should shape items require commas, allow optional commas, or allow both?
- Should empty shapes be lexically/parser-valid and rejected later by resolver,
  or rejected by parser?
- Should root-relative filter/order paths require a leading dot, while shape
  item paths omit it?
- Should string literals preserve actual newline characters in token values?
- Which escape sequences should be supported later?
- Should keywords be allowed as identifiers through escaping or quoting?
- Should parser errors carry `Span` or only `Position`?
- Should `parse_query` eventually dispatch among `select`, `insert`, `update`,
  and `delete`, while `parse_select` remains a test/helper API?

## Current Status

Implemented:

- `query-parser` crate
- `logos`-based lexer
- public `Token`, `TokenKind`, `Span`, `Position`, `LexError`
- line, column, and byte tracking
- multiline string literal tokenization
- unterminated string error
- parser module skeleton
- `parse_select` facade

Not implemented yet:

- actual `Parser::parse_select_stmt`
- parse errors with precise expected/actual information
- AST construction from tokens
- expression parser beyond planned simple compare support
