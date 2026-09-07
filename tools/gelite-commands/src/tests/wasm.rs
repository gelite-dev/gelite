use sqlite_runner::{SQLiteRunner, SQLiteRunnerError, SQLiteSchemaReader, wasm::WasmSQLiteRunner};
use wasm_bindgen_test::wasm_bindgen_test;

use crate::apply_schema;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

const INITIAL: &str = "type User { name: str } type Other {}";
const FIRST_MIGRATION: &str = "type User { name: str nickname: str } type Other {}";
const SECOND_MIGRATION: &str = "type User { name: str nickname: str multi link others: Other } type Other {} type Post { title: str }";

#[wasm_bindgen_test]
fn wasm_schema_workflow_applies_and_verifies_ordered_migrations() {
    let mut runner = WasmSQLiteRunner::open_in_memory().expect("in-memory database should open");

    apply_schema(INITIAL, &mut runner).expect("initial schema should apply");
    assert_eq!(stored_version(&mut runner), 1);
    assert_eq!(runner.table_exists("user"), Ok(true));

    apply_schema(FIRST_MIGRATION, &mut runner).expect("first migration should apply");
    assert_eq!(stored_version(&mut runner), 2);

    apply_schema(SECOND_MIGRATION, &mut runner).expect("second migration should apply");
    assert_eq!(stored_version(&mut runner), 3);
    assert_eq!(runner.table_exists("post"), Ok(true));
    assert_eq!(runner.table_exists("user__others"), Ok(true));

    apply_schema(SECOND_MIGRATION, &mut runner).expect("current schema should be a no-op");
    let stored = runner
        .load_verified_schema()
        .expect("stored schema should verify")
        .expect("stored schema should exist");
    assert_eq!(stored.version_number, 3);
    assert_eq!(
        stored.catalog,
        schema_parser::parse_schema(SECOND_MIGRATION).expect("schema should parse")
    );
}

#[wasm_bindgen_test]
fn wasm_schema_workflow_rolls_back_failed_migration() {
    let mut runner = WasmSQLiteRunner::open_in_memory().expect("in-memory database should open");
    apply_schema(INITIAL, &mut runner).expect("initial schema should apply");
    runner
        .execute(
            "CREATE TRIGGER reject_schema_migration
             BEFORE INSERT ON _engine_schema_versions
             WHEN NEW.version_number > 1
             BEGIN SELECT RAISE(ABORT, 'reject migration version'); END",
        )
        .expect("failure trigger should be created");

    apply_schema(SECOND_MIGRATION, &mut runner).expect_err("migration should fail");

    assert_eq!(runner.table_exists("post"), Ok(false));
    assert_eq!(runner.table_exists("user__others"), Ok(false));
    let stored = runner
        .load_verified_schema()
        .expect("original schema should remain valid")
        .expect("original schema should exist");
    assert_eq!(stored.version_number, 1);
    assert_eq!(
        stored.catalog,
        schema_parser::parse_schema(INITIAL).expect("schema should parse")
    );

    runner
        .execute("DROP TRIGGER reject_schema_migration")
        .expect("failure trigger should be removed");
    apply_schema(SECOND_MIGRATION, &mut runner).expect("migration should succeed after rollback");
}

#[wasm_bindgen_test]
fn wasm_schema_workflow_rejects_corrupt_metadata_with_structured_errors() {
    let mut tampered = WasmSQLiteRunner::open_in_memory().expect("in-memory database should open");
    apply_schema(INITIAL, &mut tampered).expect("initial schema should apply");
    tampered
        .execute("UPDATE _engine_schema_versions SET checksum = 'tampered'")
        .expect("checksum should be changed");

    assert!(matches!(
        tampered.load_verified_schema(),
        Err(SQLiteRunnerError::SchemaVerificationFailed { .. })
    ));

    let mut partial = WasmSQLiteRunner::open_in_memory().expect("in-memory database should open");
    partial
        .execute("CREATE TABLE _engine_schema_versions (version_number INTEGER)")
        .expect("partial metadata should be created");
    assert!(matches!(
        partial.load_verified_schema(),
        Err(SQLiteRunnerError::SchemaVerificationFailed { .. })
    ));
}

fn stored_version(runner: &mut WasmSQLiteRunner) -> i64 {
    runner
        .load_verified_schema()
        .expect("stored schema should verify")
        .expect("stored schema should exist")
        .version_number
}
