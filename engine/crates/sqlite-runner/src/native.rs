extern crate alloc;

use alloc::ffi::CString;
use alloc::format;
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
        let filename = CString::new(":memory:").map_err(|_| {
            SQLiteRunnerError::execution_failed("failed to build in-memory SQLite filename")
        })?;
        let connection = powersync_sqlite_nostd::open(filename.as_ptr()).map_err(|error| {
            SQLiteRunnerError::execution_failed(format!(
                "failed to open in-memory SQLite database: {error:?}"
            ))
        })?;

        Ok(Self { connection })
    }

    pub fn table_exists(&self, table_name: &str) -> Result<bool, SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
            .map_err(|_| self.connection_error("prepare table existence query"))?;

        statement
            .bind_text(1, table_name, Destructor::TRANSIENT)
            .map_err(|_| self.connection_error("bind table name"))?;

        match statement.step() {
            Ok(ResultCode::ROW) => Ok(true),
            Ok(ResultCode::DONE) => Ok(false),
            Ok(result) => Err(self.result_error("step table existence query", result)),
            Err(result) => Err(self.result_error("step table existence query", result)),
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
            .map_err(|_| self.connection_error("prepare read-back query"))?;

        match statement.step() {
            Ok(ResultCode::ROW) => {
                let first = statement.column_int64(0);
                let second = statement
                    .column_text(1)
                    .map_err(|error| self.result_error("read text column", error))?
                    .to_string();
                let third = match statement
                    .column_type(2)
                    .map_err(|error| self.result_error("read nullable integer column", error))?
                {
                    ColumnType::Null => None,
                    ColumnType::Integer => Some(statement.column_int64(2)),
                    column_type => {
                        return Err(SQLiteRunnerError::execution_failed(format!(
                            "read nullable integer column: unexpected column type {column_type:?}"
                        )));
                    }
                };

                Ok(Some((first, second, third)))
            }
            Ok(ResultCode::DONE) => Ok(None),
            Ok(result) => Err(self.result_error("step read-back query", result)),
            Err(result) => Err(self.result_error("step read-back query", result)),
        }
    }

    fn connection_error(&self, context: &str) -> SQLiteRunnerError {
        let message = self
            .connection
            .errmsg()
            .unwrap_or_else(|_| "unknown SQLite error".to_string());

        SQLiteRunnerError::execution_failed(format!("{context}: {message}"))
    }

    fn result_error(&self, context: &str, result: ResultCode) -> SQLiteRunnerError {
        let message = self
            .connection
            .errmsg()
            .unwrap_or_else(|_| "unknown SQLite error".to_string());

        SQLiteRunnerError::execution_failed(format!("{context}: {result:?}: {message}"))
    }
}

impl SQLiteRunner for NativeSQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        self.connection
            .exec_safe(sql)
            .map(|_| ())
            .map_err(|_| self.connection_error("execute SQL"))
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        let statement = self
            .connection
            .prepare_v2(sql)
            .map_err(|_| self.connection_error("prepare SQL"))?;

        for (index, value) in values.iter().enumerate() {
            let parameter_index = i32::try_from(index + 1).map_err(|_| {
                SQLiteRunnerError::execution_failed("bind parameter index exceeds i32 range")
            })?;
            match value {
                SQLiteValuePlan::Integer(value) => statement
                    .bind_int64(parameter_index, *value)
                    .map_err(|error| self.result_error("bind integer value", error))?,
                SQLiteValuePlan::Text(value) => statement
                    .bind_text(parameter_index, value, Destructor::TRANSIENT)
                    .map_err(|error| self.result_error("bind text value", error))?,
                SQLiteValuePlan::Null => statement
                    .bind_null(parameter_index)
                    .map_err(|error| self.result_error("bind null value", error))?,
            };
        }

        match statement.step() {
            Ok(ResultCode::DONE) => Ok(()),
            Ok(result) => Err(self.result_error("step prepared SQL", result)),
            Err(result) => Err(self.result_error("step prepared SQL", result)),
        }
    }
}
