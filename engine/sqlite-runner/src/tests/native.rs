extern crate alloc;

use alloc::string::ToString;
use alloc::vec;
use sqlite_schema_plan::SQLiteValuePlan;

use crate::{
    SQLiteRunner, apply_schema_statements, native::NativeSQLiteRunner,
    tests::fixtures::rendered_post_schema_statements,
};

#[test]
fn native_runner_can_open_in_memory_database() {
    let runner = NativeSQLiteRunner::open_in_memory();

    assert!(runner.is_ok());
}

#[test]
fn native_runner_can_execute_create_table_statement() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE post (id TEXT PRIMARY KEY)")
        .expect("create table should execute");

    assert_eq!(runner.table_exists("post"), Ok(true));
    assert_eq!(runner.table_exists("missing"), Ok(false));
}

#[test]
fn native_runner_can_execute_insert_statement_with_bind_values() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute(
            "CREATE TABLE metadata (
                object_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                target_object_id INTEGER NULL
            )",
        )
        .expect("create table should execute");

    runner
        .execute_with_values(
            "INSERT INTO metadata (object_id, name, target_object_id) VALUES (?, ?, ?)",
            &[
                SQLiteValuePlan::Integer(1),
                SQLiteValuePlan::Text("Post".to_string()),
                SQLiteValuePlan::Null,
            ],
        )
        .expect("insert should execute");

    let row = runner
        .first_three_column_row(
            "SELECT object_id, name, target_object_id FROM metadata WHERE object_id = 1",
        )
        .expect("row should be readable");

    assert_eq!(row, Some((1, "Post".to_string(), None)));
}

#[test]
fn native_runner_can_apply_rendered_initial_schema() {
    let statements = rendered_post_schema_statements();
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    apply_schema_statements(&mut runner, &statements).expect("schema statements should apply");

    assert_eq!(runner.table_exists("_engine_schema_versions"), Ok(true));
    assert_eq!(runner.table_exists("_engine_catalog_objects"), Ok(true));
    assert_eq!(runner.table_exists("_engine_catalog_fields"), Ok(true));
    assert_eq!(runner.table_exists("post"), Ok(true));

    let row = runner
        .first_three_column_row(
            "SELECT object_id, name, NULL FROM _engine_catalog_objects WHERE name = 'Post'",
        )
        .expect("catalog object row should be readable");

    assert_eq!(row, Some((1, "Post".to_string(), None)));
}

#[test]
fn native_runner_can_load_schema_catalog_from_metadata() {
    let statements = rendered_post_schema_statements();
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    apply_schema_statements(&mut runner, &statements).expect("schema statements should apply");

    let catalog = runner
        .load_schema_catalog()
        .expect("catalog should load from metadata");

    assert!(catalog.find_type("Post").is_some());
    assert!(catalog.find_field("Post", "title").is_some());
    assert!(catalog.find_field("Post", "id").is_some());
}

#[test]
fn native_runner_can_execute_select_statement_with_bind_values() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    runner
        .execute("CREATE TABLE post (id TEXT PRIMARY KEY, title TEXT NOT NULL)")
        .expect("create table should execute");
    runner
        .execute_with_values(
            "INSERT INTO post (id, title) VALUES (?, ?)",
            &[
                SQLiteValuePlan::Text("post-1".to_string()),
                SQLiteValuePlan::Text("Hello".to_string()),
            ],
        )
        .expect("insert should execute");

    let statement = sqlite_query_sqlgen::SQLiteSelectStatement::new(
        "SELECT title FROM post WHERE title = ?",
        vec![sqlite_query_sqlgen::SQLiteBindValue::String(
            "Hello".to_string(),
        )],
    );
    let result = runner
        .execute_select(&statement)
        .expect("select should execute");

    assert_eq!(result.columns(), &["title".to_string()]);
    assert_eq!(
        result.rows(),
        &[vec![crate::SQLiteCellValue::Text("Hello".to_string())]]
    );
}

#[test]
fn native_runner_reports_execution_errors() {
    let mut runner = NativeSQLiteRunner::open_in_memory().expect("in-memory database should open");

    let error = runner
        .execute("CREATE TABLE")
        .expect_err("invalid SQL should fail");

    assert!(error.message().contains("execute SQL"));
    assert!(!error.message().is_empty());
}
