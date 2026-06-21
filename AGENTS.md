# AGENTS.md

## Project role

Gelite is a learning project that is also expected to grow into usable,
production-quality software. Do not treat it as a disposable exercise. The code
may be small, but crate boundaries, type contracts, tests, and documentation
should be chosen as if later work will depend on them.

The project explores a Gel-like query language and schema model implemented in
Rust on top of SQLite. It is not a port of Gel's implementation. Reuse concepts
from Gel where they help, but do not copy its Postgres-specific or
Python/Cython-specific structure.

## Current repository state

The repository is past the first empty-scaffold phase. It already contains a
Rust workspace with compiler and tooling crates under `engine/`, `tools/`, and
`tests/`.

Current implemented areas include:

- `engine/schema-model`: semantic schema catalog, object types, scalar fields,
  links, cardinality, deterministic references, and implicit `id` lookup.
- `engine/schema-parser`: lexer and parser for the current `.geli` schema
  syntax.
- `engine/query-ast`: unresolved syntax tree for the supported `select` query
  subset.
- `engine/query-parser`: lexer and parser for the current query syntax with
  source spans.
- `engine/query-resolver`: AST-to-IR semantic analysis for explicit select
  shapes, filters, ordering, and link traversal.
- `engine/query-ir`: backend-independent Semantic IR for select queries.
- `engine/sqlite-query-plan`: SQLite-specific structured select plan.
- `engine/sqlite-query-sqlgen`: SQL renderer for select plans.
- `engine/sqlite-schema-plan`: SQLite-specific initial schema plan.
- `engine/sqlite-schema-sqlgen`: SQL renderer for initial schema DDL and
  metadata inserts.
- `engine/sqlite-runner`: runner-facing SQLite schema execution contract.
- `tools/gelite-cli`: top-level command-line binary.
- `tools/gelite-commands`: command orchestration shared by CLI-facing tools.
- `tools/repl`: query pipeline inspection tool.
- `tests/query-pipeline`: cross-crate query pipeline tests.

The project can parse schema and query sources, plan and apply an initial
SQLite schema, and run the current select subset through CLI/REPL workflows.
It does not yet provide insert/update/delete, migration diffing and migration
history, a full query execution runtime with nested result shaping, an HTTP
server, or a web UI.

## How to work in this repository

Act as a corrective pair programmer, not only as an implementation proxy.

- Explain why a change is needed, not only what changed.
- Point out awkward code, weak abstractions, missing contracts, and spec
  mismatches directly.
- Prefer small, reviewable changes over broad rewrites.
- Keep each crate responsible for its own layer. Do not move backend-specific
  decisions into syntax or semantic crates.
- Do not compile directly from source text to SQL when the existing staged
  pipeline can express the behavior.
- When introducing a concept, give the user one or two practical sentences
  about the role it plays in the pipeline.
- If a shortcut is temporary, state why it is temporary and what should replace
  it later.

The user is learning from the implementation. Do not hide important design
decisions behind "it works" changes. At the same time, avoid long theory that
does not connect to the current code.

## Document priority

The repository has `spec/` and `plan/` documents. They are not interchangeable.

- `spec/` defines behavior, meaning, and layer contracts.
- `plan/` defines sequencing, scope control, and implementation rationale.

When instructions conflict, use this priority:

1. Current user instruction.
2. Relevant `spec/` document.
3. Relevant `plan/` document.
4. Local implementation convenience.

If implementation must diverge from a spec, report the divergence clearly.
Prefer updating the spec first when the intended meaning has changed.

## Spec documents

Read the relevant spec before changing behavior in that area.

- `spec/schema.md`: minimum schema language, field kinds, `link`, `required`,
  `multi`, scalar types, object types, and implicit `id`.
- `spec/query.md`: MVP query language syntax and supported query surface.
- `spec/ir.md`: Semantic IR between AST/resolution and backend planning.
- `spec/storage-sqlite.md`: SQLite physical storage rules, tables, columns,
  links, and metadata.
- `spec/sqlite-query-plan.md`: SQLite-specific planning layer between Semantic
  IR and SQL generation.

Specs define contracts. Tests should usually assert these contracts at the
lowest layer that owns them.

## Plan documents

Use plan documents to place work in the current build sequence and explain why
the scope is appropriate.

- `plan/new-db-engine-plan.md`: product scope, MVP boundaries, major
  components, and broad build sequence.
- `plan/new-db-engine-design.md`: top-level architecture and component
  responsibilities.
- `plan/implementation-start-plan.md`: original first slice for catalog, AST,
  IR, and resolver. Useful for intent, but the repository has now moved beyond
  parts of this plan.
- `plan/schema-parser-implementation-plan.md`: schema parser sequencing and
  parser contracts.
- `plan/query-parser-implementation-plan.md`: query parser sequencing and
  parser contracts.
- `plan/query-expression-model-plan.md`: query expression model work.
- `plan/select-path-traversal-plan.md`: select path traversal semantics.
- `plan/sqlite-schema-plan-implementation-plan.md`: SQLite schema planning
  implementation sequence.
- `plan/sqlite-query-plan-implementation-plan.md`: SQLite query planner
  implementation sequence.
- `plan/cli-and-tooling-plan.md`: CLI, REPL, and tooling workflows.

Before proposing or implementing a feature, identify which spec defines its
meaning and which plan, if any, explains the intended sequence.

## Pipeline boundaries

Preserve the staged pipeline unless there is a deliberate spec or plan change:

```text
schema/query source
  -> lexer/parser
  -> AST or schema model
  -> resolver / semantic validation
  -> backend-independent IR
  -> SQLite-specific plan
  -> SQL text + bind values
  -> runner/runtime behavior
```

Keep syntax concerns in parser/AST crates, semantic concerns in model/resolver
and IR crates, SQLite decisions in SQLite planning/sqlgen/runner crates, and
CLI orchestration in `tools/`.

Examples of boundary violations to avoid:

- A parser deciding SQLite table or column names.
- SQL generation re-resolving source-level field names.
- `query-ir` depending on SQLite aliasing or join strategy.
- CLI code duplicating parser, planner, or runner logic instead of delegating
  to shared command or engine crates.

## Code change expectations

Before editing code:

- Check the relevant spec and plan documents.
- Inspect nearby tests and crate-level APIs.
- Preserve public invariants unless the task explicitly changes them.
- Look for existing naming and error-handling patterns before adding new ones.

When editing code:

- Keep changes scoped to the layer that owns the behavior.
- Add or update focused tests when behavior changes.
- Prefer typed representations over stringly typed glue.
- Preserve deterministic ordering where tests or public APIs rely on it.
- Avoid convenience APIs that make invalid schema, query, or plan states easy
  to construct.

When explaining the change:

- State the contract being protected.
- Mention which test covers the behavior, or why a test was not added.
- Call out remaining unsupported behavior explicitly.

## Review expectations

When asked to review, prioritize defects over compliments. Look first for:

- Spec mismatches.
- Crate boundary leaks.
- Incorrect type or cardinality behavior.
- Resolver and planner behavior that only works for the happy path.
- Names that sound plausible but hide unclear responsibility.
- Tests that assert output shape without checking the semantic contract.
- Over-general abstractions introduced before there are two real users.

If no issue is found, say so and identify any residual test gaps or assumptions.

## Documentation style

Repository documentation should be written in clear, concrete English. It
should record the actual code and design state, not sell the project.

Prefer:

- Actual crate, type, and function names.
- Small claims that can be checked against code or tests.
- Explicit unsupported cases.
- Short notes about temporary rules and what will replace them.
- One technical purpose per paragraph.

Avoid:

- Generic claims such as "scalable", "robust", "powerful", or "flexible"
  unless the mechanism is named.
- Inflated phrasing such as "plays a crucial role", "marks a significant
  step", or "underscores the importance".
- Unsupported generalizations such as "best practices suggest".
- Template-like summaries at the end of every section.
- Decorative formatting, unnecessary tables, and forced three-item lists.

Comments in code should explain boundaries, invariants, or non-obvious reasons.
Do not comment behavior that is already clear from the code.

## AI-assisted work

Follow `AI_POLICY.md`.

Commits that include Codex-assisted design, implementation, review, or
documentation must include this trailer:

```text
Assisted-by: Codex:gpt-5.5
```

Use professional commit messages. Do not use conversational or character voice
in commits, code, comments, docs, JSON, YAML, SQL, or other artifacts.

## Validation

The default full validation command is:

```sh
cargo test --workspace
```

Use narrower tests when they are enough for the change, but run the workspace
tests for changes that affect shared contracts or cross-crate pipeline behavior.

For CLI behavior, also check the relevant command path, for example:

```sh
cargo run -p gelite-cli -- --help
cargo run -p gelite-cli -- schema plan examples/blog.geli
```

Report any command that could not be run.
