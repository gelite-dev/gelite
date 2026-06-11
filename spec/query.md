# Query MVP Spec

## Goal

Define a small query language that is expressive enough to prove the pipeline:

- parse text into AST
- resolve names against the schema catalog
- build typed IR
- lower IR to SQLite
- shape nested results

The language is intentionally smaller than Gel and should stay small until the
typed IR and lowering model are stable.

## Supported Statements

The MVP supports exactly four top-level statements:

- `select`
- `insert`
- `update`
- `delete`

Only one statement is executed per request in the first version.

## Shared Conventions

- Identifiers refer to schema types or fields.
- String literals use double quotes.
- Numeric literals support unsigned integers and decimal floats.
- Decimal float literals must include digits on both sides of the decimal point,
  such as `0.5` or `10.5`. Shorthand forms such as `.5` and `5.` are not part
  of the MVP grammar.
- A leading `+` or `-` is not part of a numeric literal. Signed values require
  unary arithmetic operators, which are deferred.
- Boolean literals are `true` and `false`.
- `null` is supported only where the schema allows an optional value.
- Parameters are deferred. The first milestone may inline literals only.
- Queries operate on the semantic schema catalog, not raw SDL text.
- Relation fields are the fields declared with `link` in schema.
- Scalar fields are the non-`link` fields declared in schema.

## Select

### Shape

`select` returns objects with explicit shape selection.

Example:

```text
select Post {
  id,
  title,
  author: {
    id,
    name
  }
}
```

### Grammar Sketch

```text
select_stmt     := "select" type_ref shape filter_clause? order_clause?
                   limit_clause? offset_clause?
shape           := "{" shape_item* "}"
shape_item       := IDENT ","?
                | IDENT ":" shape ","?
filter_clause    := "filter" expr
order_clause     := "order" "by" order_item ("," order_item)*
order_item       := path ("asc" | "desc")?
limit_clause     := "limit" INT
offset_clause    := "offset" INT
```

### Select Semantics

- The target after `select` must be an object type.
- The shape must list fields explicitly.
- Selecting a relation field requires a nested shape.
- Selecting a scalar field does not allow a nested shape.
- Multi relation fields may be selected only with a nested shape.
- Omitted fields are not returned.
- `id` may be selected explicitly even though it is implicit in schema source.
- Backlink traversal and inferred inverse relations are not supported in the
  MVP.

### Filters

Filters use the shared expression grammar. The first implemented expression
surface is intentionally small, but it must be represented as a general
expression tree in the parser, AST, resolver, and Semantic IR so later select
features do not require another filter rewrite.

Supported filter expressions:

- root-relative field paths
- literal values
- arithmetic value expressions
- comparisons
- bracketed-list `in`
- bracketed-list `not in`
- `and`
- `or`
- `not`
- parentheses

Supported comparison operators:

- `=`
- `!=`
- `<`
- `<=`
- `>`
- `>=`

Supported filter example:

```text
select Post {
  id,
  title
}
filter .author.id = "00000000-0000-0000-0000-000000000001"
order by .title asc
limit 20
```

The leading `.` in filter paths refers to the current row root.

Parentheses control boolean grouping:

```text
select Post {
  id,
  title
}
filter (.title = "Hello" or .title = "Draft") and not .archived = true
```

Membership checks use bracketed value expressions:

```text
select Post {
  id,
  title
}
filter .status not in ["archived", "deleted"]
```

The right-hand list may contain row-independent scalar value expressions:

```text
filter .view_count in [1 + 1, 2 * 3]
```

`null` comparisons are supported only with equality operators. `= null` and
`null = .field` match absent optional values. `!= null` and `null != .field`
match present values. Other comparison operators with `null` are rejected by
the resolver before SQLite planning.

Arithmetic expressions may be used as value operands inside filters:

```text
select Post {
  title
}
filter .view_count + 10 >= 100
```

Supported arithmetic operators:

- `+`
- `-`
- `*`
- `/`
- `%`

These are binary operators. Unary `+` and unary `-` are not part of this
milestone.

Arithmetic is numeric only. The resolver accepts same-type numeric operands:

- `int64 op int64`
- `float64 op float64`

`float64` arithmetic may use declared `float64` fields and decimal float
literals. Integer literals are `int64` literals unless an explicit cast is used
in a later milestone.

Mixed numeric operands such as `int64 + float64` are rejected until explicit
cast expressions are supported. String, boolean, uuid, `null`, object, and link
operands are rejected before SQLite planning. `%` is accepted only for
`int64 % int64`.

`int64 / int64` follows SQLite integer division semantics. If fractional
division is required, the query must use explicit casts once `f64(expr)` is
supported. Division by zero is not normalized by Gelite in this milestone; if a
runtime operand is zero, SQLite's result is used.

### Expression Grammar

The expression grammar is shared by filters and later value positions such as
mutation assignments and computed shape fields.

```text
expr              := or_expr
or_expr           := and_expr ("or" and_expr)*
and_expr          := not_expr ("and" not_expr)*
not_expr          := "not" not_expr
                  | compare_expr
compare_expr      := in_expr
                  | additive_expr compare_op additive_expr
                  | additive_expr
compare_op        := "=" | "!=" | "<" | "<=" | ">" | ">="
in_expr           := additive_expr in_op in_rhs
in_op             := "in"
                  | "not" "in"
additive_expr     := multiplicative_expr (("+" | "-") multiplicative_expr)*
multiplicative_expr
                  := primary_expr (("*" | "/" | "%") primary_expr)*
primary_expr      := literal
                  | path
                  | "(" expr ")"
                  | function_call
                  | subquery_expr
path              := "."? IDENT ("." IDENT)*
function_call     := IDENT "(" argument_list? ")"
argument_list     := expr ("," expr)*
in_rhs            := "[" expr_list? "]"
                  | "(" select_stmt ")"
expr_list         := expr ("," expr)*
subquery_expr     := "(" select_stmt ")"
```

Only path, literal, arithmetic, comparison, bracketed-list `in`,
bracketed-list `not in`, boolean, and parenthesized expressions are accepted by
the resolver in the arithmetic filter milestone. `function_call` and
`subquery_expr` are reserved syntax positions. The parser may produce AST nodes
for them before the resolver accepts specific forms.

The first accepted `in_rhs` form is a non-empty bracketed list. The parser may
accept `null` as a list item because it is a literal expression, but the
resolver must reject `in []`, `not in []`, `null` list items, and list items
that are not scalar value expressions. List items must be row-independent in
this milestone: literals and arithmetic expressions over literals are accepted,
but paths, link traversals, subqueries, and boolean predicates inside the list
are rejected before SQLite planning. Use an explicit null comparison when a
filter should match null and non-null values together:

```text
filter .deleted_at = null or .deleted_at in ["2323"]
```

Subquery RHS forms such as `.author.id in (select User { id })` are reserved by
the grammar but rejected until subquery expression scope is defined.

Precedence from strongest to weakest:

1. primary expressions
2. `*`, `/`, `%`
3. `+`, `-`
4. membership and comparisons
5. `not`
6. `and`
7. `or`

### Filter Expression Scope

The MVP supports:

- field paths from the root object
- traversal through declared single relation fields
- scalar comparisons against literals
- numeric arithmetic expressions used as comparison or membership operands
- scalar membership checks against non-empty lists of non-null scalar value
  expressions
- boolean composition
- parenthesized grouping

The MVP does not support:

- traversal through backlinks or inferred inverse relations
- arbitrary subqueries
- aggregation
- `exists`
- subquery `in`
- function calls
- implicit numeric casts
- unary arithmetic operators
- path scoping with aliases

## Insert

### Shape

`insert` creates one object of a target type.

Example:

```text
insert User {
  name := "Sheri",
  email := "sheri@example.com"
}
```

### Grammar Sketch

```text
insert_stmt     := "insert" type_ref object_literal
object_literal  := "{" assign_item* "}"
assign_item     := IDENT ":=" value_expr ","?
```

### Insert Semantics

- The target must be an object type.
- Assignments may target declared scalar fields and declared single relation
  fields only.
- Required scalar fields must be supplied unless a built-in default exists.
- Optional scalar fields may be omitted.
- Assigning `id` is not allowed.
- Single relation fields may be assigned by target object id only in the MVP.
- Multi relation inserts are deferred from the first execution milestone.
- Relation assignments must target declared `link` fields.

Allowed single relation insert example:

```text
insert Post {
  title := "Case File",
  author := "00000000-0000-0000-0000-000000000001"
}
```

This assignment should resolve to the related object's identity. The assigned
field must be a declared single `link`.

## Update

### Shape

`update` modifies zero or more objects selected by a filter.

Example:

```text
update Post
filter .id = "00000000-0000-0000-0000-000000000010"
set {
  title := "Updated Title"
}
```

### Grammar Sketch

```text
update_stmt     := "update" type_ref filter_clause? set_clause
set_clause      := "set" object_literal
```

### Update Semantics

- The target must be an object type.
- `filter` is optional, but omitting it updates every row. The CLI and server
  should consider adding safety confirmation later.
- Only scalar fields and single relations may be updated in the MVP.
- Multi relations may not be updated in the MVP.
- Relation assignments must target declared `link` fields.
- Updating `id` is not allowed.

## Delete

### Shape

`delete` removes zero or more objects selected by a filter.

Example:

```text
delete Post
filter .id = "00000000-0000-0000-0000-000000000010"
```

### Grammar Sketch

```text
delete_stmt     := "delete" type_ref filter_clause?
```

### Delete Semantics

- The target must be an object type.
- `filter` is optional, but omitting it deletes every row.
- Relation cleanup behavior is defined by the storage model, not query syntax.

## Values

Supported literal values:

- strings
- integers
- floats
- booleans
- `null`

The first version does not support:

- array literals
- nested object literals in expressions
- computed expressions in assignment
- function calls
- `in`
- subqueries

## Error Conditions

The analyzer should report:

- unknown type names
- unknown field names
- writes to undeclared fields
- scalar field used with nested shape
- relation field selected without nested shape
- relation assignment to a non-`link` field
- `id` assignment in insert or update
- type mismatches in assignment
- type mismatches in filter comparisons
- unsupported expression forms
- invalid cardinality usage
- use of backlink or inferred inverse traversal

## Canonical MVP Examples

### Select

```text
select Post {
  id,
  title,
  author: {
    id,
    name
  }
}
filter .author.name = "Sheri"
limit 10
```

### Insert

```text
insert User {
  name := "Sheri"
}
```

### Update

```text
update User
filter .name = "Sheri"
set {
  email := "assistant@example.com"
}
```

### Delete

```text
delete User
filter .name = "Sheri"
```

## Deferred Features

These are intentionally out of scope until the end-to-end path is stable:

- aliases
- `with` bindings
- nested inserts
- multi relation mutation syntax
- aggregation
- grouping
- pagination cursors
- function calls
- explicit numeric casts
- unary arithmetic operators
- subqueries
- query parameters
- upsert
