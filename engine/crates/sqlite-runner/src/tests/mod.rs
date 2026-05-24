extern crate alloc;

mod fixtures;

use alloc::string::ToString;
use alloc::vec;
use sqlite_schema_plan::{SQLiteValuePlan, plan_initial_schema};
use sqlite_schema_sqlgen::render_initial_schema;

use crate::apply_schema_statements;
use fixtures::{RecordedCall, RecordingRunner, post_catalog};

#[test]
fn apply_schema_statements_executes_sql_and_insert_statements_in_order() {
    let catalog = post_catalog();
    let plan = plan_initial_schema(&catalog);
    let statements = render_initial_schema(&plan);
    let mut runner = RecordingRunner::default();

    apply_schema_statements(&mut runner, &statements).expect("schema statements should apply");

    assert!(matches!(
        runner.calls().first(),
        Some(RecordedCall::Execute(sql)) if sql.starts_with("CREATE TABLE _engine_schema_versions")
    ));
    assert!(runner.calls().iter().any(|call| matches!(
        call,
        RecordedCall::Execute(sql) if sql.starts_with("CREATE TABLE post")
    )));
    assert!(runner.calls().iter().any(|call| matches!(
        call,
        RecordedCall::ExecuteWithValues(sql, values)
            if sql == "INSERT INTO _engine_catalog_objects (object_id, name) VALUES (?, ?)"
                && values == &vec![
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Text("Post".to_string()),
                ]
    )));
    assert!(runner.calls().iter().any(|call| matches!(
        call,
        RecordedCall::ExecuteWithValues(sql, values)
            if sql == "INSERT INTO _engine_catalog_fields (object_id, field_id, name, field_kind, cardinality, scalar_type, target_object_id, is_implicit, is_unique) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
                && values == &vec![
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Text("id".to_string()),
                    SQLiteValuePlan::Text("scalar".to_string()),
                    SQLiteValuePlan::Text("required".to_string()),
                    SQLiteValuePlan::Text("uuid".to_string()),
                    SQLiteValuePlan::Null,
                    SQLiteValuePlan::Integer(1),
                    SQLiteValuePlan::Integer(0),
                ]
    )));
}
