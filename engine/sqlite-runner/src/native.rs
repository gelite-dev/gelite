use rusqlite::{Connection, params_from_iter};
use schema_model::SchemaCatalog;
use sqlite_schema_plan::SQLiteValuePlan;
use std::time::Duration;

#[cfg(test)]
use crate::rusqlite_support::{SchemaVersionRow, read_latest_schema_version};
use crate::{
    SQLiteQueryResult, SQLiteQueryRunner, SQLiteRunner, SQLiteRunnerError, SQLiteSchemaReader,
    SQLiteStoredSchema, SQLiteTransactionRunner,
    rusqlite_support::{
        complete_bind_values, execute, execute_with_values, first_three_column_row,
        load_schema_catalog, load_verified_schema, query_bind_values, read_cell_value,
        sqlite_error, table_exists,
    },
};

/// Native SQLite runner backed by an owned SQLite connection.
///
/// The concrete SQLite binding stays private to this module. Public planner,
/// SQL generator, and command APIs should continue to depend on the
/// `SQLiteRunner` trait instead of this backend type.
pub struct NativeSQLiteRunner {
    connection: Connection,
}

impl NativeSQLiteRunner {
    pub fn open_in_memory() -> Result<Self, SQLiteRunnerError> {
        Self::open(":memory:")
    }

    pub fn open(path: &str) -> Result<Self, SQLiteRunnerError> {
        let connection = Connection::open(path).map_err(|error| {
            SQLiteRunnerError::execution_failed(format!(
                "failed to open SQLite database `{path}`: {error}"
            ))
        })?;
        connection
            .busy_timeout(Duration::ZERO)
            .map_err(|error| sqlite_error("configure SQLite busy timeout", error))?;
        let mut runner = Self { connection };
        runner.execute("PRAGMA foreign_keys = ON")?;

        Ok(runner)
    }

    pub fn begin_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("BEGIN")
    }

    pub fn commit_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("COMMIT")
    }

    pub fn rollback_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        self.execute("ROLLBACK")
    }

    pub fn table_exists(&self, table_name: &str) -> Result<bool, SQLiteRunnerError> {
        table_exists(&self.connection, table_name)
    }

    /// Reads the first row as owned values for native backend smoke tests.
    ///
    /// This is not the query execution API. It exists only to verify that the
    /// selected SQLite binding stores values through `SQLiteRunner` correctly.
    pub fn first_three_column_row(
        &self,
        sql: &str,
    ) -> Result<Option<(i64, String, Option<i64>)>, SQLiteRunnerError> {
        first_three_column_row(&self.connection, sql)
    }

    pub fn load_verified_schema(
        &mut self,
    ) -> Result<Option<SQLiteStoredSchema>, SQLiteRunnerError> {
        load_verified_schema(&self.connection)
    }

    /// Verifies the latest snapshot checksum and logical catalog in one read transaction.
    ///
    /// No source file is needed. An existing caller transaction is rejected and left untouched.
    pub fn verify_schema_version(&mut self) -> Result<(), SQLiteRunnerError> {
        self.load_verified_schema()?
            .ok_or_else(|| {
                SQLiteRunnerError::schema_verification_failed(
                    "database does not contain a stored schema version",
                )
            })
            .map(|_| ())
    }

    pub fn load_schema_catalog(&self) -> Result<SchemaCatalog, SQLiteRunnerError> {
        load_schema_catalog(&self.connection)
    }

    pub fn execute_select(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<SQLiteQueryResult, SQLiteRunnerError> {
        let mut prepared = self
            .connection
            .prepare(statement.sql())
            .map_err(|error| sqlite_error("prepare SELECT", error))?;

        let column_count = prepared.column_count();
        let output_names = statement.output_names();

        if !output_names.is_empty() && output_names.len() != column_count {
            return Err(SQLiteRunnerError::execution_failed(
                "result output metadata does not match SQLite column count",
            ));
        }

        let result_shape = statement.result_shape();
        let (column_indexes, columns): (Vec<_>, Vec<_>) = if result_shape.is_some() {
            ((0..column_count).collect(), Vec::new())
        } else {
            let selected_columns: Vec<(usize, String)> = if output_names.is_empty() {
                (0..column_count)
                    .map(|index| {
                        prepared
                            .column_name(index)
                            .map(|name| (index, name.to_string()))
                            .map_err(|error| sqlite_error("read result column name", error))
                    })
                    .collect::<Result<_, _>>()?
            } else {
                (0..column_count)
                    .zip(output_names)
                    .filter_map(|(index, name)| name.as_ref().map(|name| (index, name.clone())))
                    .collect()
            };

            selected_columns.into_iter().unzip()
        };

        let mut rows = Vec::new();
        let values = complete_bind_values(
            query_bind_values(statement.bind_values()),
            prepared.parameter_count(),
        );
        let mut result_rows = prepared
            .query(params_from_iter(values))
            .map_err(|error| sqlite_error("step SELECT", error))?;
        while let Some(result_row) = result_rows
            .next()
            .map_err(|error| sqlite_error("step SELECT", error))?
        {
            rows.push(
                column_indexes
                    .iter()
                    .map(|index| read_cell_value(result_row, *index))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }

        match result_shape {
            Some(shape) => {
                let follow_up_fetch_count = crate::follow_up_fetch_count(shape);
                let shaped = rows
                    .into_iter()
                    .map(|mut row| {
                        let parent_identity =
                            crate::identity_at(statement.parent_identity_column_index(), &row)?;
                        let mut follow_up_parent_identities = vec![None; follow_up_fetch_count];
                        let fields = crate::shape_fields_with_identities(
                            shape,
                            &mut row,
                            &mut follow_up_parent_identities,
                        )?;

                        Ok((fields, parent_identity, follow_up_parent_identities))
                    })
                    .collect::<Result<Vec<_>, SQLiteRunnerError>>()?;
                let mut result_rows = Vec::with_capacity(shaped.len());
                let mut parent_identities = Vec::with_capacity(shaped.len());
                let mut follow_up_parent_identities = Vec::with_capacity(shaped.len());
                for (row, parent_identity, row_follow_up_parent_identities) in shaped {
                    result_rows.push(row);
                    parent_identities.push(parent_identity);
                    follow_up_parent_identities.push(row_follow_up_parent_identities);
                }

                Ok(SQLiteQueryResult::with_identities(
                    shape
                        .fields()
                        .iter()
                        .map(|field| field.output_name().into())
                        .collect(),
                    result_rows,
                    parent_identities,
                    follow_up_parent_identities,
                ))
            }
            None => Ok(SQLiteQueryResult::new(columns, rows)),
        }
    }

    pub fn execute_insert(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<(), SQLiteRunnerError> {
        self.execute_query_statement(statement, "INSERT")
            .map(|_| ())
    }

    pub fn execute_update(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        self.execute_mutation(statement, "UPDATE")
    }

    pub fn execute_delete(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        self.execute_mutation(statement, "DELETE")
    }

    fn execute_mutation(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
        operation: &str,
    ) -> Result<i64, SQLiteRunnerError> {
        let count = self.execute_query_statement(statement, operation)?;
        i64::try_from(count).map_err(|_| {
            SQLiteRunnerError::execution_failed(format!(
                "step {operation}: affected row count exceeds i64 range"
            ))
        })
    }

    fn execute_query_statement(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
        operation: &str,
    ) -> Result<usize, SQLiteRunnerError> {
        let mut prepared = self
            .connection
            .prepare(statement.sql())
            .map_err(|error| sqlite_error(&format!("prepare {operation}"), error))?;
        let values = complete_bind_values(
            query_bind_values(statement.bind_values()),
            prepared.parameter_count(),
        );
        let count = prepared
            .execute(params_from_iter(values))
            .map_err(|error| sqlite_error(&format!("step {operation}"), error))?;
        Ok(count)
    }

    /// Reads the highest numbered stored version without verifying its contents.
    #[cfg(test)]
    pub(crate) fn read_latest_schema_version(
        &self,
    ) -> Result<Option<SchemaVersionRow>, SQLiteRunnerError> {
        read_latest_schema_version(&self.connection)
    }
}

impl SQLiteRunner for NativeSQLiteRunner {
    fn execute(&mut self, sql: &str) -> Result<(), SQLiteRunnerError> {
        execute(&self.connection, sql)
    }

    fn execute_with_values(
        &mut self,
        sql: &str,
        values: &[SQLiteValuePlan],
    ) -> Result<(), SQLiteRunnerError> {
        execute_with_values(&self.connection, sql, values)
    }
}

impl SQLiteQueryRunner for NativeSQLiteRunner {
    fn execute_select(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<SQLiteQueryResult, SQLiteRunnerError> {
        NativeSQLiteRunner::execute_select(self, statement)
    }

    fn execute_insert(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::execute_insert(self, statement)
    }

    fn execute_update(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        NativeSQLiteRunner::execute_update(self, statement)
    }

    fn execute_delete(
        &mut self,
        statement: &sqlite_query_sqlgen::SQLiteStatement,
    ) -> Result<i64, SQLiteRunnerError> {
        NativeSQLiteRunner::execute_delete(self, statement)
    }
}

impl SQLiteTransactionRunner for NativeSQLiteRunner {
    fn begin_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::begin_transaction(self)
    }

    fn commit_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::commit_transaction(self)
    }

    fn rollback_transaction(&mut self) -> Result<(), SQLiteRunnerError> {
        NativeSQLiteRunner::rollback_transaction(self)
    }
}

impl SQLiteSchemaReader for NativeSQLiteRunner {
    fn load_verified_schema(&mut self) -> Result<Option<SQLiteStoredSchema>, SQLiteRunnerError> {
        NativeSQLiteRunner::load_verified_schema(self)
    }
}
