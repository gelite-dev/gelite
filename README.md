# Gelite

Gelite is a practical reimplementation experiment for a Gel-like query
language.

The goal is not to clone Gel's codebase or rebuild every database feature at
once. The goal is to reproduce the useful language ideas in a smaller Rust
codebase:

- object types instead of table-first modeling
- explicit links between objects
- shaped `select` queries
- schema-aware name resolution
- typed intermediate representation
- lowering into ordinary SQLite SQL

The project is also a learning project. The implementation is intentionally
split into visible compiler stages so the language pipeline can be studied,
tested, and extended without hiding the important steps behind a large engine.

## What this project is trying to prove

Gel's query language is useful because a query can describe the object shape it
wants back:

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

That style is easier to read than manually assembling a set of joins and then
reconstructing nested objects in application code.

Gelite asks a smaller question:

Can that style of query language be implemented in a compact Rust engine that
targets SQLite?

The current answer is being built one layer at a time:

```text
query text
  -> syntax tree
  -> schema-resolved Semantic IR
  -> SQLite-specific plan
  -> SQL text + bind values
```

## Current scope

Gelite currently focuses on the read/query side of the language. The working
path is `select` parsing, semantic resolution, SQLite planning, and SQL
rendering.

It does not yet execute SQL against SQLite. It does not yet implement schema
parsing, migrations, insert/update/delete, a server, or a web UI.

That is intentional for this stage. The first useful milestone is to make the
language pipeline correct and understandable before building runtime features
on top of it.

## Example

The schema model currently exists as Rust catalog values. The language being
modeled is:

```text
type User {
  required name: str
}

type Post {
  required title: str
  required link author: User
}
```

Given this query:

```text
select Post {
  title,
  author: {
    name
  }
}
filter .title = "Hello"
order by .title desc
limit 10
```

Gelite can parse the query, resolve the names against the schema catalog,
produce Semantic IR, build a SQLite plan, and render SQL similar to:

```sql
SELECT root.title, author.id, author.name
FROM post AS root
INNER JOIN user AS author ON root.author_id = author.id
WHERE root.title = ?
ORDER BY root.title DESC
LIMIT 10
```

The exact SQL is an implementation detail. The important part is that query
meaning passes through typed, inspectable stages before SQL is emitted.

## Why the stages matter

The project deliberately avoids compiling straight from text to SQL.

Each stage has one responsibility:

- Parser: turns source text into syntax.
- Schema catalog: stores object types, fields, links, cardinality, and implicit
  `id`.
- Resolver: checks names and shape rules against the catalog.
- Semantic IR: records the resolved meaning of a query without backend details.
- SQLite planner: chooses tables, columns, aliases, joins, predicates, and
  result-shaping metadata.
- SQL generator: renders the SQLite plan into SQL text and bind values.

This structure keeps Gel-like language semantics separate from SQLite-specific
storage decisions. It also makes the code useful as a study project: each
compiler step can be inspected independently.

## What is implemented

- `schema`: semantic schema catalog with object types, scalar fields, links,
  cardinality, deterministic references, and implicit `id` lookup.
- `query-ast`: unresolved syntax tree for select queries.
- `query-parser`: lexer and parser for the current select syntax, with source
  spans.
- `resolver`: AST-to-IR semantic analysis for explicit select shapes, filters,
  ordering, and link traversal.
- `ir`: backend-independent Semantic IR for select queries.
- `sqlite-plan`: SQLite-specific structured select plan.
- `sqlite-sqlgen`: SQL renderer that emits bind placeholders.
- `tools/repl`: inspection tool for running the current pipeline on a query.

## What is not implemented yet

- Schema source parser.
- Insert, update, and delete.
- Migration planning and application.
- SQLite execution runtime.
- Runtime nested result shaping.
- HTTP API.
- Full CLI workflows.
- Web playground.

## Running the project

Run all tests:

```sh
cargo test --workspace
```

Open the inspection REPL:

```sh
cargo run -p repl
```

Run one query:

```sh
cargo run -p repl -- 'select Post { title, author: { name } } filter .title = "Hello" order by .title desc limit 10'
```

Print the intermediate forms:

```sh
cargo run -p repl -- --debug 'select Post { title, author: { name } } filter .title = "Hello"'
```

The REPL currently uses a hard-coded schema with `User` and `Post`. It is meant
for compiler inspection, not as a database shell.

## Repository guide

`spec/` defines what the language and engine stages mean:

- `spec/schema.md`: schema language and catalog semantics.
- `spec/query.md`: MVP query language surface.
- `spec/ir.md`: Semantic IR contract.
- `spec/storage-sqlite.md`: SQLite storage mapping.
- `spec/sqlite-plan.md`: SQLite planning contract.

`plan/` explains the implementation order and design reasoning:

- `plan/new-db-engine-plan.md`
- `plan/new-db-engine-design.md`
- `plan/implementation-start-plan.md`
- `plan/query-parser-implementation-plan.md`
- `plan/select-path-traversal-plan.md`
- `plan/sqlite-plan-implementation-plan.md`

When these documents conflict, `spec/` wins for meaning and `plan/` wins for
work sequencing.

## Development principle

Gelite is written to learn how a Gel-like query compiler works by rebuilding
the important pieces in a smaller system.

That learning goal does not mean loose code. The project should keep the same
standard expected from production foundations:

- small features with clear contracts
- tests for semantic behavior
- explicit crate boundaries
- no direct AST-to-SQL shortcuts
- documentation that says what exists now and what is still missing

The next technical goal is to keep extending the select pipeline until the
generated SQLite SQL can be executed and shaped back into nested query results.
