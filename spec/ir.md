# Semantic IR MVP Spec

## Goal

Define the minimum semantic intermediate representation between parsed query AST
and SQLite-specific lowering.

The Semantic IR exists to:

- preserve the meaning of a query after schema resolution
- carry type and cardinality information
- represent nested result shapes explicitly
- separate query semantics from SQLite table/column details

## Role of Semantic IR

The MVP pipeline is:

1. parse query text into AST
2. resolve schema names and validate semantics
3. produce Semantic IR
4. lower Semantic IR to a SQLite execution plan
5. execute SQLite queries and shape results

Semantic IR is the stage where a query becomes:

- typed
- schema-resolved
- cardinality-aware
- independent from backend-specific naming

## Non-Goals

The Semantic IR MVP does not attempt to model:

- SQL table or column names
- join strategy details
- cost-based optimization
- polymorphism
- inheritance-aware type expansion
- backlinks or inferred inverse traversal
- computed fields
- access policies or rewrites
- function overloading machinery beyond what the MVP query language needs

These concerns belong either to earlier validation or to later backend-specific
planning.

## Core Principles

- Semantic IR must use resolved schema references, not raw identifier strings.
- Semantic IR must be backend-independent.
- Semantic IR must make result shape explicit.
- Semantic IR must preserve cardinality information on paths and fields.
- Semantic IR must be small enough to support the MVP query language without
  importing Gel's full complexity.

## Core Node Types

The MVP Semantic IR should define at least these top-level node categories:

- `SelectQuery`
- `InsertQuery`
- `UpdateQuery`
- `DeleteQuery`
- `ResolvedShape`
- `ResolvedShapeField`
- `ResolvedPath`
- `PathStep`
- `FieldRef`
- `ObjectTypeRef`
- `ScalarTypeRef`
- `Expr`
- `LiteralExpr`
- `PathExpr`
- `CompareExpr`
- `AndExpr`
- `OrExpr`
- `NotExpr`
- `OrderExpr`
- `Assignment`

These names are descriptive, not binding implementation names.

## Type References

Type references should be resolved objects, not plain strings.

### `ObjectTypeRef`

Minimum fields:

- stable type id
- type name

### `ScalarTypeRef`

Minimum fields:

- scalar kind id or enum value
- scalar name

The MVP only needs built-in scalar types:

- `str`
- `int64`
- `float64`
- `bool`
- `uuid`
- `datetime`

## Field References

`FieldRef` is a core Semantic IR concept. It should describe the schema field
that a query refers to after resolution.

Minimum fields:

- stable field id
- owning object type
- field name
- field kind: `scalar` or `link`
- target type
- cardinality: `optional`, `required`, or `multi`
- `is_implicit`

This allows the compiler to distinguish:

- explicit scalar fields
- explicit link fields
- implicit fields such as `id`

## Query Nodes

### `SelectQuery`

Minimum fields:

- root object type
- output shape
- optional filter expression
- zero or more order expressions
- optional limit
- optional offset

### `InsertQuery`

Minimum fields:

- root object type
- assignments

### `UpdateQuery`

Minimum fields:

- root object type
- optional filter expression
- assignments

### `DeleteQuery`

Minimum fields:

- root object type
- optional filter expression

## Shape Model

Nested result shaping is a primary responsibility of Semantic IR.

### `ResolvedShape`

Minimum fields:

- source object type
- ordered list of shape fields

### `ResolvedShapeField`

Minimum fields:

- output name
- field reference
- field cardinality
- optional child shape

Rules:

- scalar fields must not have a child shape
- link fields selected in result output must have a child shape
- `multi` links are represented in shape like single links, but retain `multi`
  cardinality metadata for later runtime shaping

## Path Model

Paths are needed for filters, ordering, and resolved field access.

### `ResolvedPath`

Minimum fields:

- root object type
- ordered list of path steps
- result type
- result cardinality

### `PathStep`

Minimum fields:

- field reference
- field kind
- target or result type
- step cardinality

The Semantic IR path model only needs to support:

- root field access
- traversal through declared single links
- terminal scalar access

The Semantic IR MVP does not support:

- backlinks
- inferred inverse relations
- alias scope traversal
- arbitrary subquery paths

## Expression Model

The Semantic IR expression model is a resolved expression tree. Query parsing
may produce syntax nodes for future expression forms, but the resolver should
only emit IR expression variants that are semantically accepted for the current
milestone.

Each resolved expression should carry enough information for later stages to
validate and lower it without re-reading schema names from source text.

### `Expr`

Minimum variants:

- literal
- path
- arithmetic
- comparison
- membership
- boolean `and`
- boolean `or`
- boolean `not`

Minimum metadata:

- result type
- result cardinality

The result type may be a scalar type, an object type for future subquery work,
or a dedicated boolean type for predicates. The current implementation needs
literal scalar values, resolved scalar paths, arithmetic scalar values, and
boolean predicate results.

The expression tree does not store SQLite SQL fragments. SQLite-specific
operator spelling, parentheses, bind placeholders, and joins belong to SQLite
planning and SQL generation.

### `LiteralExpr`

Supported literal kinds:

- string
- integer
- float
- boolean
- null

Minimum fields:

- literal kind
- literal value
- result type
- result cardinality

### `PathExpr`

Wraps a resolved path used in a filter or ordering context.

Minimum fields:

- resolved path
- result type
- result cardinality

### `ArithmeticExpr`

Represents a resolved numeric value expression.

Minimum fields:

- left expression
- arithmetic operator
- right expression
- result type
- result cardinality

Supported operators:

- `+`
- `-`
- `*`
- `/`
- `%`

These are binary operators. Unary arithmetic operators are not part of this
milestone.

Arithmetic operands must resolve to scalar numeric value expressions. The
resolver must reject string, boolean, uuid, `null`, object, and link operands
before SQLite planning.

Accepted operand and result types:

- `int64 + int64 -> int64`
- `int64 - int64 -> int64`
- `int64 * int64 -> int64`
- `int64 / int64 -> int64`
- `int64 % int64 -> int64`
- `float64 + float64 -> float64`
- `float64 - float64 -> float64`
- `float64 * float64 -> float64`
- `float64 / float64 -> float64`

Mixed numeric operands such as `int64 + float64` are rejected until explicit
cast expressions exist. `%` is not defined for `float64`.

`int64 / int64` preserves SQLite integer division semantics. Division by zero
is not rewritten by Semantic IR. If the divisor can only be known at runtime,
SQLite determines the result.

Arithmetic expressions may appear as value operands inside filter comparisons
and membership expressions in the arithmetic filter milestone. Arithmetic
expressions are not accepted as `order by` expressions or computed select
projections until those later milestones define their own shape and result
metadata rules.

### `CompareExpr`

Minimum fields:

- left expression
- comparison operator
- right expression

Supported operators:

- `=`
- `!=`
- `<`
- `<=`
- `>`
- `>=`

Comparison expressions must resolve to a boolean result. The resolver is
responsible for rejecting incompatible operands before the expression reaches
SQLite planning.

`= null` and `null = <path>` lower to `IsNull`. `!= null` and `null != <path>`
lower to `IsNotNull`. Other comparison operators with `null` are rejected by
the resolver because SQL three-valued null comparison semantics are not the
Gelite filter contract.

### `InExpr`

Minimum fields:

- left expression
- membership operator: `in` or `not in`
- list of right-hand value expressions

Supported right-hand side:

- a non-empty list of non-null scalar value expressions

Membership expressions must resolve to a boolean result. The resolver is
responsible for rejecting empty lists, subquery RHS forms, incompatible operand
types, `null` list items, and non-scalar list items before the expression
reaches SQLite planning.

Right-hand list items must be row-independent in the arithmetic filter
milestone. Literals and arithmetic expressions over literals are accepted.
Path expressions, link traversals, subqueries, boolean predicates, and any
expression that depends on the current row are rejected. This keeps membership
planning as a single-row predicate and avoids introducing correlated expression
semantics before subqueries and computed projections are defined.

The Semantic IR should model `not in` explicitly instead of rewriting it to a
boolean `not` around `in`. Keeping the operator in the membership node lets
SQLite planning preserve bind order and choose a direct `NOT IN` predicate.

### Boolean Expressions

The MVP also needs:

- `AndExpr`
- `OrExpr`
- `NotExpr`

Boolean expression operands must resolve to boolean expressions. Parentheses do
not need a dedicated IR node; they affect the parsed tree shape. SQL generation
must preserve grouping when rendering `and` and `or` combinations.

### Reserved Expression Forms

The parser and AST may reserve syntax for these forms before they become
accepted Semantic IR:

- `FunctionCallExpr`
- `SubqueryExpr`

The resolver must reject unsupported forms with diagnostics. Do not pass an
unsupported expression through IR as an opaque node.

## Ordering Model

### `OrderExpr`

Minimum fields:

- resolved path
- direction: `asc` or `desc`

## Mutation Model

### `Assignment`

Minimum fields:

- field reference
- value expression

MVP constraints:

- assignments may target declared scalar fields
- assignments may target declared single `link` fields
- assignments may not target implicit `id`
- assignments may not target `multi` links

## Boundary With SQLite Planning

Semantic IR must stop before backend-specific physical details.

Semantic IR includes:

- resolved types
- resolved fields
- cardinality
- shape tree
- filter expressions
- ordering
- mutation targets

Semantic IR does not include:

- SQLite table names
- SQLite column names
- join conditions
- SQL snippets
- statement batching strategy
- index selection

Those belong to a separate SQLite planning layer.

## Worked Example

For:

```text
select Post {
  id,
  title,
  author: {
    id,
    name
  }
}
filter .author.id = "00000000-0000-0000-0000-000000000001"
order by .title asc
limit 10
```

The Semantic IR should capture at least:

- root object type `Post`
- shape fields `id`, `title`, and `author`
- `author` resolved as a declared single `link` from `Post` to `User`
- nested child shape on `User` containing `id` and `name`
- filter path `Post.author.id`
- ordering path `Post.title`
- result cardinalities for all selected fields and paths

It should not yet contain:

- `post` table name
- `author_id` column name
- concrete SQL join text

## Relationship To Gel

This MVP follows Gel's high-level design principle of compiling queries into a
typed, schema-resolved intermediate form before backend lowering.

It intentionally does not adopt Gel's full IR complexity. In particular, the
MVP defers:

- separate scope-tree modeling
- advanced path identity machinery
- polymorphic and inheritance-heavy typing
- backend-specific execution overlays

The goal is to borrow Gel's staging discipline without copying its full
implementation surface.
