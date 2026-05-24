mod fixtures;

use sqlite_schema_plan::SQLiteValuePlan;

use crate::{SchemaPlanStatement, plan_schema};
use fixtures::blog_schema_source;

#[test]
fn schema_plan_command_renders_initial_schema_from_source() {
    let output = plan_schema(blog_schema_source()).expect("schema plan command should succeed");
    let statements = output.statements();

    assert_eq!(statements.len(), 13);
    assert!(
        statements[0]
            .sql()
            .starts_with("CREATE TABLE _engine_schema_versions")
    );
    assert!(
        statements[1]
            .sql()
            .starts_with("CREATE TABLE _engine_catalog_objects")
    );
    assert!(
        statements[2]
            .sql()
            .starts_with("CREATE TABLE _engine_catalog_fields")
    );
    assert!(statements[3].sql().starts_with("CREATE TABLE user"));
    assert!(statements[4].sql().starts_with("CREATE TABLE post"));
    assert_eq!(
        statements[12].sql(),
        "CREATE INDEX post__author_id_idx ON post (author_id)"
    );
}

#[test]
fn schema_plan_command_preserves_metadata_bind_values() {
    let output = plan_schema(blog_schema_source()).expect("schema plan command should succeed");

    let post_object_insert = output
        .statements()
        .iter()
        .find(|statement| {
            matches!(
                statement,
                SchemaPlanStatement::Insert { values, .. }
                    if values == &[
                        SQLiteValuePlan::Integer(2),
                        SQLiteValuePlan::Text("Post".into()),
                    ]
            )
        })
        .expect("Post catalog object insert should exist");

    assert_eq!(
        post_object_insert.sql(),
        "INSERT INTO _engine_catalog_objects (object_id, name) VALUES (?, ?)"
    );
    assert_eq!(
        post_object_insert.values(),
        Some(
            [
                SQLiteValuePlan::Integer(2),
                SQLiteValuePlan::Text("Post".into()),
            ]
            .as_slice()
        )
    );
}

#[test]
fn schema_plan_command_returns_parse_error_for_invalid_schema() {
    let error = plan_schema(
        "type Post {
  required link author: Missing
}",
    )
    .expect_err("invalid schema should fail");

    assert!(error.message().contains("failed to parse schema"));
    assert!(!error.message().is_empty());
}
