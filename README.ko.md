# Gelite

Gelite는 Gel 같은 query language를 실용적으로 재현해 보는 Rust 프로젝트입니다.

목표는 Gel의 코드베이스를 복제하거나 모든 database feature를 한 번에 다시
만드는 것이 아닙니다. 목표는 Gel에서 유용한 언어 아이디어를 더 작은 Rust
codebase 안에서 직접 구현해 보는 것입니다.

- table-first modeling 대신 object type 중심 모델링
- object 사이의 explicit link
- shape를 가진 `select` query
- schema-aware name resolution
- typed intermediate representation
- ordinary SQLite SQL로 lowering

이 프로젝트는 학습 프로젝트이기도 합니다. 중요한 compiler 단계를 숨기지 않고
crate와 타입으로 드러내서, Gel 같은 query language가 어떤 과정을 거쳐 SQL로
내려가는지 직접 조사하고 테스트하고 확장할 수 있게 만드는 것이 목적입니다.

## 이 프로젝트가 증명하려는 것

Gel의 query language가 유용한 이유 중 하나는 query가 반환받을 object shape를
직접 말할 수 있다는 점입니다.

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

이 방식은 직접 join을 조립하고 application code에서 nested object를 다시 만드는
것보다 읽기 쉽습니다.

Gelite는 더 작은 질문에서 출발합니다.

Gel 같은 query language를 SQLite를 target으로 하는 작은 Rust engine으로
구현할 수 있는가?

현재는 이 질문에 답하기 위해 다음 pipeline을 한 단계씩 만들고 있습니다.

```text
query text
  -> syntax tree
  -> schema-resolved Semantic IR
  -> SQLite-specific plan
  -> SQL text + bind values
```

## 현재 범위

Gelite는 지금 read/query 쪽에 집중합니다. 현재 작동하는 경로는 `select` parsing,
semantic resolution, SQLite planning, SQL rendering입니다.

아직 SQLite에 SQL을 실행하지 않습니다. schema parser, migration,
insert/update/delete, server, web UI도 아직 없습니다.

이건 현재 단계의 의도입니다. runtime feature를 올리기 전에 language pipeline이
정확하고 이해 가능한지 먼저 검증하는 것이 첫 번째 유효한 milestone입니다.

## 예시

schema model은 현재 Rust catalog value로 존재합니다. 모델링 중인 언어는 다음과
같습니다.

```text
type User {
  required name: str
}

type Post {
  required title: str
  required link author: User
}
```

다음 query가 들어오면:

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

Gelite는 query를 parse하고, schema catalog에 맞춰 이름을 resolve하고, Semantic
IR을 만들고, SQLite plan을 만든 뒤, 대략 다음과 같은 SQL을 렌더링할 수 있습니다.

```sql
SELECT root.title, author.id, author.name
FROM post AS root
INNER JOIN user AS author ON root.author_id = author.id
WHERE root.title = ?
ORDER BY root.title DESC
LIMIT 10
```

정확한 SQL 문자열 자체보다 중요한 것은 query meaning이 typed하고 inspectable한
단계를 거친 뒤 SQL로 나온다는 점입니다.

## 왜 단계를 나누는가

이 프로젝트는 text에서 SQL로 바로 컴파일하는 지름길을 피합니다.

각 단계는 하나의 책임을 가집니다.

- Parser: source text를 syntax로 바꿉니다.
- Schema catalog: object type, field, link, cardinality, implicit `id`를
  저장합니다.
- Resolver: catalog를 기준으로 이름과 shape rule을 검증합니다.
- Semantic IR: backend detail 없이 query의 resolved meaning을 기록합니다.
- SQLite planner: table, column, alias, join, predicate, result-shaping
  metadata를 결정합니다.
- SQL generator: SQLite plan을 SQL text와 bind value로 렌더링합니다.

이 구조는 Gel-like language semantics와 SQLite-specific storage decision을
분리합니다. 동시에 각 compiler step을 따로 조사할 수 있어서 학습에도 좋습니다.

## 구현된 것

- `schema`: object type, scalar field, link, cardinality, deterministic
  reference, implicit `id` lookup을 가진 semantic schema catalog.
- `query-ast`: select query용 unresolved syntax tree.
- `query-parser`: 현재 select syntax용 lexer/parser와 source span.
- `resolver`: explicit select shape, filter, ordering, link traversal을 위한
  AST-to-IR semantic analysis.
- `ir`: select query용 backend-independent Semantic IR.
- `sqlite-plan`: SQLite-specific structured select plan.
- `sqlite-sqlgen`: bind placeholder를 사용하는 SQL renderer.
- `tools/repl`: 현재 pipeline을 query 하나로 확인하는 inspection tool.

## 아직 구현되지 않은 것

- Schema source parser.
- Insert, update, delete.
- Migration planning/application.
- SQLite execution runtime.
- Runtime nested result shaping.
- HTTP API.
- Full CLI workflow.
- Web playground.

## 실행 방법

전체 테스트:

```sh
cargo test --workspace
```

inspection REPL 실행:

```sh
cargo run -p repl
```

query 하나 실행:

```sh
cargo run -p repl -- 'select Post { title, author: { name } } filter .title = "Hello" order by .title desc limit 10'
```

중간 표현 출력:

```sh
cargo run -p repl -- --debug 'select Post { title, author: { name } } filter .title = "Hello"'
```

REPL은 현재 `User`와 `Post`가 있는 hard-coded schema를 사용합니다. database
shell이 아니라 compiler inspection tool입니다.

## 저장소 안내

`spec/`은 language와 engine stage의 의미를 정의합니다.

- `spec/schema.md`: schema language와 catalog semantics.
- `spec/query.md`: MVP query language surface.
- `spec/ir.md`: Semantic IR contract.
- `spec/storage-sqlite.md`: SQLite storage mapping.
- `spec/sqlite-plan.md`: SQLite planning contract.

`plan/`은 구현 순서와 설계 근거를 설명합니다.

- `plan/new-db-engine-plan.md`
- `plan/new-db-engine-design.md`
- `plan/implementation-start-plan.md`
- `plan/query-parser-implementation-plan.md`
- `plan/select-path-traversal-plan.md`
- `plan/sqlite-plan-implementation-plan.md`

문서가 충돌하면 의미는 `spec/`, 작업 순서는 `plan/`을 우선합니다.

## 개발 원칙

Gelite는 Gel 같은 query compiler가 어떻게 동작하는지 배우기 위해 중요한 조각을
작은 시스템 안에서 다시 만드는 프로젝트입니다.

학습 목적이 있다고 해서 기준을 낮추지는 않습니다. 이 프로젝트는 production
foundation에 기대하는 기준을 유지해야 합니다.

- 계약이 분명한 작은 feature
- semantic behavior를 고정하는 test
- 명시적인 crate boundary
- direct AST-to-SQL shortcut 금지
- 현재 있는 것과 아직 없는 것을 정확히 말하는 documentation

다음 기술 목표는 select pipeline을 계속 확장해서 generated SQLite SQL을 실제로
실행하고, 그 결과를 nested query result로 shape하는 것입니다.
