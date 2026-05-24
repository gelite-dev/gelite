extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use schema_model::{Field, ObjectType, ScalarField, ScalarType, SchemaCatalog, SingleCardinality};
use sqlite_schema_plan::SQLiteValuePlan;

use crate::{SQLiteRunner, SQLiteRunnerError};

#[derive(Debug, PartialEq, Eq)]
pub enum RecordedCall {
    Execute(String),
    ExecuteWithValues(String, Vec<SQLiteValuePlan>),
}

#[derive(Default)]
pub struct RecordingRunner {
    calls: Vec<RecordedCall>,
}

impl RecordingRunner {
    pub fn calls(&self) -> &[RecordedCall] {
        &self.calls
    }
}

impl SQLiteRunner for RecordingRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        self.calls.push(RecordedCall::Execute(sql.to_string()));
        Ok(())
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        self.calls.push(RecordedCall::ExecuteWithValues(
            sql.to_string(),
            values.to_vec(),
        ));
        Ok(())
    }
}

pub fn post_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new(
        "Post",
        vec![Field::Scalar(ScalarField::new(
            "title",
            ScalarType::Str,
            SingleCardinality::Required,
        ))],
    )])
    .expect("test catalog should be valid")
}
