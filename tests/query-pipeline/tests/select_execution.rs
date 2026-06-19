use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sqlite_query_sqlgen::{SQLiteBindValue, SQLiteSelectStatement, render_select};
use sqlite_runner::{
    SQLiteCellValue, SQLiteQueryResult, SQLiteRunner, apply_schema_statements,
    native::NativeSQLiteRunner,
};
use sqlite_schema_plan::SQLiteValuePlan;

const BLOG_SCHEMA_SOURCE: &str = r#"
type User {
  required email: str
  required score: int64
  multi link posts: Post
}

type Post {
  required title: str
  required view_count: int64
  required link author: User
}
"#;

static TEMP_SCHEMA_COUNTER: AtomicU64 = AtomicU64::new(0);

fn parse_blog_catalog_from_geli_file() -> schema_model::SchemaCatalog {
    let path = write_temp_geli_schema(BLOG_SCHEMA_SOURCE);
    let source = fs::read_to_string(&path).expect("temporary .geli schema should be readable");
    let catalog = schema_parser::parse_schema(&source).expect("schema source should parse");
    fs::remove_file(&path).expect("temporary .geli schema should be removed");

    catalog
}

fn write_temp_geli_schema(source: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "gelite-query-pipeline-{}-{}.geli",
        std::process::id(),
        unique_suffix()
    ));

    fs::write(&path, source).expect("temporary .geli schema should be writable");

    path
}

fn unique_suffix() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();
    let counter = TEMP_SCHEMA_COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{timestamp}-{counter}")
}

fn setup_blog_database() -> NativeSQLiteRunner {
    let catalog = parse_blog_catalog_from_geli_file();
    let schema_plan = sqlite_schema_plan::plan_initial_schema(&catalog);
    let schema_statements = sqlite_schema_sqlgen::render_initial_schema(&schema_plan);
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    apply_schema_statements(&mut runner, &schema_statements)
        .expect("schema statements should apply");

    insert_blog_fixture_rows(&mut runner);

    runner
}

fn insert_blog_fixture_rows(runner: &mut NativeSQLiteRunner) {
    // Temporary fixture setup: Gelite does not have an insert pipeline yet.
    // Replace these raw SQL inserts with Gelite insert statements once insert
    // parsing, resolution, planning, and execution exist.
    runner
        .execute_with_values(
            "INSERT INTO user (id, email, score) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Text("user-1".to_string()),
                SQLiteValuePlan::Text("alice@example.com".to_string()),
                SQLiteValuePlan::Integer(100),
            ],
        )
        .expect("first user fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO user (id, email, score) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Text("user-2".to_string()),
                SQLiteValuePlan::Text("blocked@example.com".to_string()),
                SQLiteValuePlan::Integer(0),
            ],
        )
        .expect("second user fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO post (id, title, view_count, author_id) VALUES (?, ?, ?, ?)",
            &[
                SQLiteValuePlan::Text("post-1".to_string()),
                SQLiteValuePlan::Text("Draft".to_string()),
                SQLiteValuePlan::Integer(5),
                SQLiteValuePlan::Text("user-1".to_string()),
            ],
        )
        .expect("draft post fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO post (id, title, view_count, author_id) VALUES (?, ?, ?, ?)",
            &[
                SQLiteValuePlan::Text("post-2".to_string()),
                SQLiteValuePlan::Text("Published".to_string()),
                SQLiteValuePlan::Integer(20),
                SQLiteValuePlan::Text("user-1".to_string()),
            ],
        )
        .expect("published post fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO post (id, title, view_count, author_id) VALUES (?, ?, ?, ?)",
            &[
                SQLiteValuePlan::Text("post-3".to_string()),
                SQLiteValuePlan::Text("Archived".to_string()),
                SQLiteValuePlan::Integer(100),
                SQLiteValuePlan::Text("user-2".to_string()),
            ],
        )
        .expect("archived post fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO user__posts (source_id, target_id, position) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Text("user-1".to_string()),
                SQLiteValuePlan::Text("post-1".to_string()),
                SQLiteValuePlan::Integer(0),
            ],
        )
        .expect("first multi-link fixture row should insert");
    runner
        .execute_with_values(
            "INSERT INTO user__posts (source_id, target_id, position) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Text("user-1".to_string()),
                SQLiteValuePlan::Text("post-2".to_string()),
                SQLiteValuePlan::Integer(1),
            ],
        )
        .expect("second multi-link fixture row should insert");
}

fn render_query(source: &str) -> SQLiteSelectStatement {
    let catalog = parse_blog_catalog_from_geli_file();
    let ast = query_parser::parse_select(source).expect("query should parse");
    let ir = query_resolver::resolve_select(&catalog, &ast).expect("query should resolve");
    let plan = sqlite_query_plan::plan_select(&ir);

    render_select(&plan)
}

fn execute_query(source: &str) -> SQLiteQueryResult {
    let mut runner = setup_blog_database();
    let statement = render_query(source);

    runner
        .execute_select(&statement)
        .expect("select statement should execute")
}

#[test]
fn select_pipeline_renders_in_filter_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .title in ["Draft", "Published"] order by .title asc limit 20"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" IN (?, ?) ORDER BY \"root\".\"title\" ASC LIMIT 20"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Draft".to_string()),
            SQLiteBindValue::String("Published".to_string()),
        ]
    );
}

#[test]
fn select_pipeline_renders_not_in_filter_through_single_link_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .author.email not in ["blocked@example.com"]"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" INNER JOIN \"user\" AS \"author\" ON \"root\".\"author_id\" = \"author\".\"id\" WHERE \"author\".\"email\" NOT IN (?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("blocked@example.com".to_string())]
    );
}

#[test]
fn select_pipeline_renders_comparison_filter_from_query_text() {
    let statement = render_query(r#"select Post { title } filter .view_count >= 10"#);

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"view_count\" >= ?"
    );
    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(10)]);
}

#[test]
fn select_pipeline_renders_arithmetic_order_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .title != "Archived" order by .view_count + 1 desc"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT \"root\".\"title\" FROM \"post\" AS \"root\" WHERE \"root\".\"title\" != ? ORDER BY (\"root\".\"view_count\" + ?) DESC"
    );
    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Archived".to_string()),
            SQLiteBindValue::Int64(1),
        ]
    );
}

#[test]
fn select_pipeline_renders_computed_projection_from_query_text() {
    let statement = render_query(r#"select Post { score := .view_count + 1 }"#);

    assert_eq!(
        statement.sql(),
        "SELECT (\"root\".\"view_count\" + ?) AS \"score\" FROM \"post\" AS \"root\""
    );
    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(1)]);
}

#[test]
fn select_pipeline_executes_root_scalar_comparison_filter() {
    let result =
        execute_query(r#"select Post { title } filter .view_count >= 10 order by .title asc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_computed_projection() {
    let result =
        execute_query(r#"select Post { score := .view_count + 1 } order by .view_count asc"#);

    assert_eq!(result.columns(), &["score".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Integer(6)],
            vec![SQLiteCellValue::Integer(21)],
            vec![SQLiteCellValue::Integer(101)],
        ]
    );
}

#[test]
fn select_pipeline_executes_root_scalar_arithmetic_filter() {
    let result =
        execute_query(r#"select Post { title } filter .view_count + 6 > 25 order by .title asc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_root_scalar_arithmetic_order() {
    let result = execute_query(r#"select Post { title } order by .view_count + 1 desc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
            vec![SQLiteCellValue::Text("Draft".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_single_link_arithmetic_order() {
    let result = execute_query(r#"select Post { title } order by .author.score + .view_count asc"#);

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Archived".to_string())],
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_membership_filter_with_arithmetic_items() {
    let result = execute_query(
        r#"select Post { title } filter .view_count in [5 + 0, 10 + 10] order by .title asc"#,
    );

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn select_pipeline_executes_single_link_membership_filter() {
    let result = execute_query(
        r#"select Post { title } filter .author.email not in ["blocked@example.com"] order by .title asc"#,
    );

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[
            vec![SQLiteCellValue::Text("Draft".to_string())],
            vec![SQLiteCellValue::Text("Published".to_string())],
        ]
    );
}

#[test]
fn query_pipeline_executes_multi_link_schema_storage_setup() {
    let mut runner = setup_blog_database();

    assert_eq!(runner.table_exists("user__posts"), Ok(true));

    let statement = SQLiteSelectStatement::new(
        "SELECT source_id, target_id, position FROM user__posts WHERE source_id = ? ORDER BY position ASC",
        vec![SQLiteBindValue::String("user-1".to_string())],
    );
    let result = runner
        .execute_select(&statement)
        .expect("multi-link join table query should execute");

    assert_eq!(
        result.columns(),
        &[
            "source_id".to_string(),
            "target_id".to_string(),
            "position".to_string(),
        ]
    );
    assert_eq!(
        result.rows(),
        &[
            vec![
                SQLiteCellValue::Text("user-1".to_string()),
                SQLiteCellValue::Text("post-1".to_string()),
                SQLiteCellValue::Integer(0),
            ],
            vec![
                SQLiteCellValue::Text("user-1".to_string()),
                SQLiteCellValue::Text("post-2".to_string()),
                SQLiteCellValue::Integer(1),
            ],
        ]
    );
}
