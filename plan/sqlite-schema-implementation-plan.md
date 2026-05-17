# SQLite Schema Implementation Plan

## Goal

Add the first schema application layer for Gelite's SQLite backend.

The immediate target is not a full migration engine. The first target is:

1. take a validated `schema::SchemaCatalog`
2. derive the SQLite object tables, relation tables, indexes, and metadata rows
3. apply them to an empty SQLite database as the initial schema
4. make the stored metadata precise enough to rebuild the same
   `schema::SchemaCatalog` later

This layer exists before the query runner becomes useful. Query execution needs
a catalog, and the first real catalog should come from the schema that was
applied to the database instead of from test fixtures.

## Crate To Add

Add a new crate:

```text
engine/crates/sqlite-schema
```

The crate is SQLite-specific and may depend on SQLite naming, SQLite DDL, and
the internal metadata table layout from `spec/storage-sqlite.md`.

Initial dependency direction:

```text
sqlite-schema -> schema
```

Do not make `schema` depend on `sqlite-schema`.
Do not make `resolver` depend on `sqlite-schema`.
Do not make `sqlite-plan` or `sqlite-sqlgen` depend on `sqlite-schema` until a
shared naming module exists.

The first version of `sqlite-schema` should stay `no_std` while it builds
structured DDL plans and SQL strings. Schema execution no longer has to move
to a `std` boundary by default. The project now expects to evaluate
`vlcn-io/sqlite-rs-embedded` as the SQLite binding for engine-integrated,
`no_std`-compatible execution.

## Responsibility Boundary

### `schema` owns

- canonical in-memory object and field model
- schema validation
- deterministic object and field references
- scalar type, field kind, cardinality, and implicit `id` semantics

### `sqlite-schema` owns

- mapping `schema::SchemaCatalog` to SQLite schema operations
- object table names
- scalar column names
- single-link foreign key column names
- multi-link join table names
- metadata table definitions
- metadata rows for object and field definitions
- initial schema apply plan
- later: catalog loading from metadata tables

### `sqlite-schema` must not own

- parsing `.geli` files
- query parsing
- name resolution for queries
- `ir` construction
- select planning
- final query SQL rendering
- SQLite connection ownership
- row decoding for query results

Those belong to parser, resolver, planner, sqlgen, and the engine runtime
layer.

## Why This Comes Before The Runner

The engine runtime will eventually execute this pipeline:

```text
query text
-> query-parser
-> load current schema::SchemaCatalog
-> resolver
-> sqlite-plan
-> sqlite-sqlgen
-> execute SQL
```

The missing dependency is the real catalog. Loading the catalog is only useful
after the database has a catalog to load. The first milestone is therefore:

```text
schema::SchemaCatalog
-> SQLite initial schema apply
-> object tables + metadata tables
```

After that, the runtime can replace fixture catalogs with metadata-backed
catalog loading.

## Existing Crates That Need Changes

### `schema`

`sqlite-schema` needs to inspect more than field names.

Likely additions:

```rust
impl ScalarField {
    pub fn scalar_type(&self) -> ScalarType
    pub fn cardinality(&self) -> SingleCardinality
    pub fn is_implicit(&self) -> bool
    pub fn is_unique(&self) -> bool
}

impl LinkField {
    pub fn cardinality(&self) -> Cardinality
}

impl Field {
    pub fn as_scalar(&self) -> Option<&ScalarField>
    pub fn as_link(&self) -> Option<&LinkField>
}

impl ObjectType {
    pub fn fields(&self) -> impl Iterator<Item = &Field>
}
```

The exact API can be smaller if tests need less. The key rule is that
`sqlite-schema` should not parse field names to infer kind, scalar type, or
cardinality.

`ObjectType::fields()` should expose implicit fields followed by declared
fields, matching the deterministic reference order already used by
`SchemaCatalog::find_field_ref`.

### `sqlite-plan`

`sqlite-plan` currently owns physical naming rules for select planning:

- object type name to table name
- scalar field name to column name
- single-link field name to `<field>_id`
- selected single-link alias rules

`sqlite-schema` will need the same naming rules for DDL. Duplicating them is
acceptable for one or two tests, but it should be treated as a short-lived
rule. Before adding more physical naming behavior, move the shared rules into
one backend-specific location.

Candidate options:

- keep naming functions in `sqlite-plan` and later extract them
- introduce `engine/crates/sqlite-naming`
- put naming in `sqlite-schema` and make `sqlite-plan` depend on it later

Do not create `sqlite-naming` before a test proves duplication is causing real
drift. For the first `sqlite-schema` tests, local naming helpers are acceptable.

### `sqlite-sqlgen`

No immediate changes are required. It renders select SQL from `sqlite-plan`.
DDL rendering should not be added to this crate unless the project decides that
all SQLite SQL rendering belongs there.

For now, keep DDL rendering inside `sqlite-schema` because DDL is tightly tied
to catalog application and metadata table layout.

### `query-parser`

No immediate changes are required for initial schema apply from Rust-built
fixtures.

Later, a schema parser will be needed:

```text
.geli source -> schema AST -> schema::SchemaCatalog
```

That should be a separate parser path from `.geliql` query parsing.

### `tools/repl`

The REPL should not construct fixture catalogs once metadata-backed catalogs
exist.

Later flow:

```text
repl startup
-> open SQLite database
-> load schema::SchemaCatalog from metadata
-> use that catalog for query resolution
```

Before catalog loading exists, the REPL can keep using its current fixture
catalog.

## `sqlite-schema` Public API Shape

Start with pure planning, not connection execution:

```rust
pub fn plan_initial_schema(catalog: &schema::SchemaCatalog) -> SQLiteSchemaPlan
```

The returned plan should be inspectable by tests before any SQL is executed.

Suggested first structure:

```rust
pub struct SQLiteSchemaPlan {
    metadata_tables: Vec<SQLiteCreateTable>,
    object_tables: Vec<SQLiteCreateTable>,
    relation_tables: Vec<SQLiteCreateTable>,
    indexes: Vec<SQLiteCreateIndex>,
    catalog_objects: Vec<SQLiteCatalogObjectRow>,
    catalog_fields: Vec<SQLiteCatalogFieldRow>,
}
```

This structure keeps DDL and catalog metadata together without requiring a
SQLite binding.

The first SQL rendering API can be:

```rust
pub fn render_initial_schema(plan: &SQLiteSchemaPlan) -> Vec<String>
```

This returns DDL and metadata insert SQL in deterministic order. It should not
execute them.

Execution can be added later through the engine runtime, using a
`no_std`-compatible SQLite binding if the binding proves suitable:

```rust
pub fn apply_initial_schema(
    connection: &mut impl SQLiteExecutor,
    catalog: &schema::SchemaCatalog,
) -> Result<(), SQLiteSchemaApplyError>
```

Do not add this executor abstraction until the project has tested the
`sqlite-rs-embedded` API surface against schema apply, prepared statements, bind
values, stepping, and result access.

## Initial Schema Plan Details

### Metadata Tables

Always create these tables first:

- `_engine_schema_versions`
- `_engine_catalog_objects`
- `_engine_catalog_fields`

The definitions should match `spec/storage-sqlite.md`.

The first implementation may omit `_engine_schema_versions` inserts if there is
no migration identity yet, but the table itself should exist.

### Object Tables

For every object type:

- create one table
- add `id TEXT PRIMARY KEY`
- add scalar fields as direct columns
- add unique scalar fields as `UNIQUE`
- add single-link fields as `<field>_id TEXT`
- add foreign key constraints for single links
- do not add columns for multi links

Required fields become `NOT NULL`.
Optional fields become nullable.
Optional unique scalar fields stay nullable. SQLite allows multiple `NULL`
values under a `UNIQUE` constraint, and Gelite treats this as the MVP meaning:
only present values must be unique.

The implicit `id` field must be generated from catalog semantics, not from a
declared schema field.

### Multi-Link Tables

For every `Cardinality::Many` link:

- create one join table named `<source_table>__<field_name>`
- add `source_id TEXT NOT NULL`
- add `target_id TEXT NOT NULL`
- add `position INTEGER NULL`
- add primary key `(source_id, target_id)`
- add foreign keys to the source and target object tables

The first query planner still does not support selecting multi links. Creating
the storage table is still correct because schema application should match the
storage spec, not the current select subset.

### Indexes

Create indexes for:

- every single-link foreign key column
- `source_id` on multi-link tables
- `target_id` on multi-link tables

Index naming should be deterministic.

### Metadata Rows

For every object type, insert one `_engine_catalog_objects` row:

- `object_id` as the deterministic `schema::ObjectTypeId` integer value
- `name`

For every field, insert one `_engine_catalog_fields` row:

- `object_id`
- `field_id`
- `name`
- `field_kind`
- `cardinality`
- `scalar_type`
- `target_object_id`
- `is_implicit`
- `is_unique`

Metadata rows must include implicit `id` fields. The resolver and future
catalog loader need the same semantic catalog that the in-memory schema layer
uses.

The MVP stores object and field ids as integers, not UUIDs. `object_id` matches
the deterministic `schema::ObjectTypeId(u64)` value. `field_id` matches the
deterministic `schema::FieldId(u64)` value inside its owning object type, so
`_engine_catalog_fields` uses `(object_id, field_id)` as its primary key. Stable
UUIDs can be revisited when rename-aware migrations need persistent identities
across schema snapshots.

## Scalar Type Mapping

Use the storage spec mapping:

```text
schema::ScalarType::Str      -> TEXT
schema::ScalarType::Int64    -> INTEGER
schema::ScalarType::Float64  -> REAL
schema::ScalarType::Bool     -> INTEGER
schema::ScalarType::Uuid     -> TEXT
schema::ScalarType::DateTime -> TEXT
```

The first implementation should model this as a small function in
`sqlite-schema`:

```rust
fn sqlite_affinity(scalar_type: schema::ScalarType) -> SQLiteAffinity
```

Use an enum internally instead of raw strings if tests need to inspect the
mapping.

## Initial Test Plan

### 1. `initial_schema_plan_creates_metadata_tables`

Input: an empty or minimal catalog.

Assert:

- `_engine_schema_versions` is present
- `_engine_catalog_objects` is present
- `_engine_catalog_fields` is present

This test fixes the internal metadata table contract before user tables.

### 2. `initial_schema_plan_creates_object_table_for_scalar_fields`

Input:

```text
type User {
  required name: str
  age: int64
}
```

Assert:

- table name is `user`
- `id` is `TEXT PRIMARY KEY`
- `name` is `TEXT NOT NULL`
- `age` is `INTEGER NULL`

This test checks scalar mapping and cardinality mapping.

### 2a. `initial_schema_plan_marks_required_unique_scalar_field`

Input:

```text
type User {
  required unique email: str
}
```

Assert:

- `email` is `TEXT NOT NULL UNIQUE`

This test fixes the required unique scalar mapping.

### 2b. `initial_schema_plan_allows_optional_unique_scalar_field`

Input:

```text
type User {
  unique nickname: str
}
```

Assert:

- `nickname` is `TEXT NULL UNIQUE`

This test documents the MVP null semantics for optional unique fields. Gelite
allows multiple rows with `nickname = null`; duplicate non-null nicknames are
rejected by SQLite.

### 3. `initial_schema_plan_creates_single_link_foreign_key_column`

Input:

```text
type User {
  required name: str
}

type Post {
  required title: str
  required link author: User
}
```

Assert:

- `post` table has `author_id TEXT NOT NULL`
- `author_id` references `user(id)`
- an index exists for `post.author_id`

This test fixes single-link physical storage.

### 4. `initial_schema_plan_creates_optional_single_link_column`

Input:

```text
type Post {
  link author: User
}
```

Assert:

- `author_id` is nullable
- the foreign key still exists

This test prevents conflating optional links with missing storage.

### 4a. `initial_schema_plan_marks_required_unique_single_link_column`

Add this after scalar uniqueness and basic single-link storage are stable.

Input:

```text
type User {
  required name: str
}

type Profile {
  required unique link user: User
}
```

Assert:

- `profile` table has `user_id TEXT NOT NULL UNIQUE`
- `user_id` references `user(id)`

This test fixes the first schema-enforced one-to-one relation form. The
constraint means each `Profile` must reference one `User`, and the same `User`
cannot be referenced by more than one `Profile`.

### 4b. `initial_schema_plan_marks_optional_unique_single_link_column`

Input:

```text
type User {
  required name: str
}

type Profile {
  unique link user: User
}
```

Assert:

- `profile` table has `user_id TEXT NULL UNIQUE`
- `user_id` references `user(id)`

This uses the same null semantics as optional unique scalar fields: duplicate
non-null target ids are rejected, but multiple rows without a target are
allowed.

### 5. `initial_schema_plan_creates_multi_link_join_table`

Input:

```text
type User {
  multi link posts: Post
}
```

Assert:

- join table is `user__posts`
- it has `source_id`, `target_id`, and `position`
- primary key is `(source_id, target_id)`
- source and target foreign keys exist
- source and target indexes exist

This test fixes storage for a feature that query planning has not implemented
yet.

### 6. `initial_schema_plan_records_catalog_object_rows`

Input: a two-type catalog.

Assert:

- object rows preserve deterministic object ids
- names match the canonical catalog
- row order is deterministic

This test is the first step toward metadata-backed catalog loading.

### 7. `initial_schema_plan_records_catalog_field_rows`

Input: a catalog with scalar, single-link, multi-link, and implicit `id`.

Assert:

- implicit `id` is recorded with `is_implicit = true`
- scalar fields record `scalar_type`
- link fields record `target_object_id`
- field kind and cardinality strings are stable

This test fixes the metadata format that catalog loading will later consume.

### 8. `render_initial_schema_outputs_deterministic_sql`

Input: a fixed catalog.

Assert:

- SQL statements are emitted in dependency-safe order
- metadata tables are created before metadata inserts
- object tables are created before link tables or indexes that reference them
- repeated planning renders byte-identical SQL

This test keeps generated migrations stable.

### 9. `catalog_can_round_trip_through_metadata_rows`

This is not the first test. Add it after metadata rows are stable.

Input:

```text
schema::SchemaCatalog
-> SQLiteSchemaPlan
-> metadata rows
-> schema::SchemaCatalog
```

Assert:

- rebuilt catalog equals the original catalog
- object and field references remain deterministic

This test is the bridge to query execution without fixture catalogs.

## Initial Implementation Sequence

1. Create `engine/crates/sqlite-schema` with `schema` as its only dependency.
2. Add it to the workspace.
3. Define inspectable plan structs without SQL strings.
4. Implement metadata table planning.
5. Implement object table planning for scalar fields.
6. Add the minimal `schema` getters needed by the tests.
7. Implement single-link column and foreign key planning.
8. Implement scalar unique column planning.
9. Implement unique single-link column planning for one-to-one constraints.
10. Implement multi-link join table planning.
11. Implement catalog metadata row planning.
12. Add SQL rendering for the initial schema plan.
13. Evaluate `sqlite-rs-embedded` with a small engine-owned execution wrapper.
14. Implement metadata-to-`schema::SchemaCatalog` loading only after metadata
    rows are tested.

## Open Decisions

### SQLite driver

The current direction is to evaluate
`https://github.com/vlcn-io/sqlite-rs-embedded` before adding a `std` SQLite
driver.

The repository describes the binding as `no_std` and WASM-compatible SQLite
bindings that stay close to the SQLite C API. That matches the engine goal
better than putting all execution behind a separate `std` runner crate, but it
also means the project must wrap the unsafe and low-level API carefully.

`sqlite-schema` should still avoid depending directly on the SQLite binding
until pure planning and rendering are tested. The execution wrapper should live
in an engine runtime crate so planning remains inspectable without opening a
connection.

### DDL SQL ownership

`sqlite-sqlgen` currently renders select SQL only. DDL can stay in
`sqlite-schema` for now because it is schema-application-specific.

If DDL rendering later grows large, split a dedicated SQLite DDL renderer.

### Shared SQLite naming

`sqlite-plan` and `sqlite-schema` will both need physical naming rules. Keep the
first implementation local, but extract the naming rules once a test or bug
shows drift.

### Migration identity

`_engine_schema_versions` requires:

- `version_id`
- `checksum`
- `applied_at`
- `schema_snapshot`

The first schema plan can create this table without inserting a version row.
Once initial apply execution exists, the project needs a deterministic snapshot
format and checksum rule.

### Transaction boundary

Applying schema changes should happen in one transaction where SQLite allows
it. This belongs to the engine runtime execution layer, not the pure planning
layer.

## Deferred Work

- diffing current catalog against desired catalog
- non-initial migrations
- table rebuild migrations
- schema parser for `.geli`
- catalog loading through the engine SQLite runtime
- migration checksum generation
- schema snapshot serialization
- rollback strategy for failed schema apply
- user-declared indexes and constraints
