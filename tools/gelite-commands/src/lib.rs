//! Shared command orchestration for Gelite tools.
//!
//! This crate belongs to the tools layer. It composes parser, planner,
//! renderer, and runner crates into user-facing commands, but it does not own
//! process argument parsing, stdout/stderr, or process exit codes.

use sqlite_runner::{SQLiteRunner, SQLiteRunnerError, apply_schema_statements};
use sqlite_schema_plan::SQLiteValuePlan;
use sqlite_schema_sqlgen::RenderedSchemaStatement;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandError {
    message: String,
}

impl CommandError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaPlanOutput {
    statements: Vec<SchemaPlanStatement>,
}

impl SchemaPlanOutput {
    pub fn statements(&self) -> &[SchemaPlanStatement] {
        &self.statements
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaPlanStatement {
    Sql(String),
    Insert {
        sql: String,
        values: Vec<SQLiteValuePlan>,
    },
}

impl SchemaPlanStatement {
    pub fn sql(&self) -> &str {
        match self {
            Self::Sql(sql) => sql,
            Self::Insert { sql, .. } => sql,
        }
    }

    pub fn values(&self) -> Option<&[SQLiteValuePlan]> {
        match self {
            Self::Sql(_) => None,
            Self::Insert { values, .. } => Some(values),
        }
    }
}

pub fn plan_schema(source: &str) -> Result<SchemaPlanOutput, CommandError> {
    let catalog = schema_parser::parse_schema(source).map_err(|error| CommandError {
        message: format!("failed to parse schema: {error:?}"),
    })?;
    let plan = sqlite_schema_plan::plan_initial_schema(&catalog);
    let statements = sqlite_schema_sqlgen::render_initial_schema(&plan)
        .into_iter()
        .map(schema_plan_statement_from_rendered)
        .collect();

    Ok(SchemaPlanOutput { statements })
}

pub fn apply_schema(source: &str, runner: &mut impl SQLiteRunner) -> Result<(), CommandError> {
    let catalog = schema_parser::parse_schema(source).map_err(|error| CommandError {
        message: format!("failed to parse schema: {error:?}"),
    })?;
    let plan = sqlite_schema_plan::plan_initial_schema(&catalog);
    let statements = sqlite_schema_sqlgen::render_initial_schema(&plan);

    apply_schema_statements(runner, &statements).map_err(command_error_from_runner)
}

fn command_error_from_runner(error: SQLiteRunnerError) -> CommandError {
    CommandError {
        message: format!("failed to apply schema: {}", error.message()),
    }
}

fn schema_plan_statement_from_rendered(statement: RenderedSchemaStatement) -> SchemaPlanStatement {
    match statement {
        RenderedSchemaStatement::Sql(sql) => SchemaPlanStatement::Sql(sql),
        RenderedSchemaStatement::Insert(insert) => SchemaPlanStatement::Insert {
            sql: insert.sql().to_string(),
            values: insert.values().to_vec(),
        },
    }
}

#[cfg(test)]
mod tests;
