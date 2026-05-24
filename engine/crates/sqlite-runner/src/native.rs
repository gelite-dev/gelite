extern crate alloc;

use alloc::ffi::CString;
use alloc::string::{String, ToString};

use powersync_sqlite_nostd::{ColumnType, Connection, Destructor, ManagedConnection, ResultCode};
use sqlite_schema_plan::SQLiteValuePlan;

use crate::{SQLiteRunner, SQLiteRunnerError};

/// Native SQLite runner backed by an owned SQLite connection.
///
/// The concrete SQLite binding stays private to this module. Public planner,
/// SQL generator, and command APIs should continue to depend on the
/// `SQLiteRunner` trait instead of this backend type.
pub struct NativeSQLiteRunner {
    connection: ManagedConnection,
}

impl NativeSQLiteRunner {
    pub fn open_in_memory() -> Result<Self, SQLiteRunnerError> {
        let filename = CString::new(":memory:").map_err(|_| SQLiteRunnerError::ExecutionFailed)?;
        let connection = powersync_sqlite_nostd::open(filename.as_ptr())
            .map_err(|_| SQLiteRunnerError::ExecutionFailed)?;

        Ok(Self { connection })
    }

    pub fn table_exists(&self, table_name: &str) -> Result<bool, SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
            .map_err(|_| SQLiteRunnerError::ExecutionFailed)?;

        statement
            .bind_text(1, table_name, Destructor::TRANSIENT)
            .map_err(|_| SQLiteRunnerError::ExecutionFailed)?;

        match statement.step() {
            Ok(ResultCode::ROW) => Ok(true),
            Ok(ResultCode::DONE) => Ok(false),
            Ok(_) | Err(_) => Err(SQLiteRunnerError::ExecutionFailed),
        }
    }

    /// Reads the first row as owned values for native backend smoke tests.
    ///
    /// This is not the query execution API. It exists only to verify that the
    /// selected SQLite binding stores values through `SQLiteRunner` correctly.
    pub fn first_three_column_row(
        &self,
        sql: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(sql)
            .map_err(|_| SQLiteRunnerError::ExecutionFailed)?;

        match statement.step() {
            Ok(ResultCode::ROW) => {
                let first = statement.column_int64(0);
                let second = statement
                    .column_text(1)
                    .map_err(|_| SQLiteRunnerError::ExecutionFailed)?
                    .to_string();
                let third = match statement
                    .column_type(2)
                    .map_err(|_| SQLiteRunnerError::ExecutionFailed)?
                {
                    ColumnType::Null => None,
                    ColumnType::Integer => Some(statement.column_int64(2)),
                    _ => return Err(SQLiteRunnerError::ExecutionFailed),
                };

                Ok(Some((first, second, third)))
            }
            Ok(ResultCode::DONE) => Ok(None),
            Ok(_) | Err(_) => Err(SQLiteRunnerError::ExecutionFailed),
        }
    }
}

impl SQLiteRunner for NativeSQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        self.connection
            .exec_safe(sql)
            .map(|_| ())
            .map_err(|_| SQLiteRunnerError::ExecutionFailed)
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(sql)
            .map_err(|_| SQLiteRunnerError::ExecutionFailed)?;

        for (index, value) in values.iter().enumerate() {
            let parameter_index =
                i32::try_from(index + 1).map_err(|_| SQLiteRunnerError::ExecutionFailed)?;
            match value {
                SQLiteValuePlan::Integer(value) => statement
                    .bind_int64(parameter_index, *value)
                    .map_err(|_| SQLiteRunnerError::ExecutionFailed)?,
                SQLiteValuePlan::Text(value) => statement
                    .bind_text(parameter_index, value, Destructor::TRANSIENT)
                    .map_err(|_| SQLiteRunnerError::ExecutionFailed)?,
                SQLiteValuePlan::Null => statement
                    .bind_null(parameter_index)
                    .map_err(|_| SQLiteRunnerError::ExecutionFailed)?,
            };
        }

        match statement.step() {
            Ok(ResultCode::DONE) => Ok(()),
            Ok(_) | Err(_) => Err(SQLiteRunnerError::ExecutionFailed),
        }
    }
}
