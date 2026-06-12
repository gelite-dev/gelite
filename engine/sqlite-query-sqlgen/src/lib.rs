#![no_std]
//! SQL renderer for SQLite select plans.
//!
//! This crate serializes `sqlite-query-plan` structures into SQL text and bind
//! values. It does not resolve schema names, choose joins, or inspect query AST
//! nodes. Those responsibilities belong to earlier compiler stages.
//!
//! The renderer currently emits `SELECT`, `FROM`, `JOIN`, `WHERE`, `ORDER BY`,
//! `LIMIT`, and `OFFSET` clauses for the select subset implemented by
//! `sqlite-query-plan`. Literal values are emitted as bind placeholders instead of
//! being interpolated into SQL strings.

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use sqlite_query_plan::{
    SQLiteArithmeticOp, SQLiteCompareOp, SQLiteInOp, SQLiteJoinKind, SQLiteLiteral,
    SQLiteOrderDirection, SQLiteSelectPlan, SQLiteValueExpr, SQLiteWhereExpr,
};

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn render_qualified_identifier(source_alias: &str, column_name: &str) -> String {
    format!(
        "{}.{}",
        quote_identifier(source_alias),
        quote_identifier(column_name)
    )
}

/// Renders a structured SQLite select plan into SQL text and bind values.
pub fn render_select(plan: &sqlite_query_plan::SQLiteSelectPlan) -> SQLiteSelectStatement {
    let select_clause = render_select_clause(plan);
    let from_clause = render_from_clause(plan);
    let (where_clause, bind_values) = render_where_clause(plan);
    let order_clause = render_order_clause(plan);
    let limit_clause = render_limit_clause(plan);
    let offset_clause = render_offset_clause(plan);
    let join_clauses = render_join_clauses(plan);

    let mut clauses = vec![select_clause, from_clause];
    clauses.extend(join_clauses);
    if let Some(where_clause) = where_clause {
        clauses.push(where_clause);
    }
    if let Some(order_clause) = order_clause {
        clauses.push(order_clause);
    }
    if let Some(limit_clause) = limit_clause {
        clauses.push(limit_clause);
    }
    if let Some(offset_clause) = offset_clause {
        clauses.push(offset_clause);
    }

    SQLiteSelectStatement {
        sql: clauses.join(" "),
        bind_values,
    }
}

fn render_select_clause(plan: &SQLiteSelectPlan) -> String {
    let columns = plan
        .selected_values()
        .iter()
        .map(|value| render_qualified_identifier(value.source_alias(), value.column_name()))
        .collect::<Vec<_>>()
        .join(", ");

    format!("SELECT {columns}")
}

fn render_from_clause(plan: &SQLiteSelectPlan) -> String {
    let columns = plan.root_source().table_name();
    let alias = plan.root_source().alias();

    format!(
        "FROM {} AS {}",
        quote_identifier(columns),
        quote_identifier(alias)
    )
}

fn render_where_clause(plan: &SQLiteSelectPlan) -> (Option<String>, Vec<SQLiteBindValue>) {
    match plan.filter() {
        None => (None, vec![]),
        Some(expr) => {
            let mut bind_values = Vec::new();
            let expr_sql = render_where_expr(expr, &mut bind_values);

            (Some(format!("WHERE {expr_sql}")), bind_values)
        }
    }
}

fn render_where_expr(expr: &SQLiteWhereExpr, bind_values: &mut Vec<SQLiteBindValue>) -> String {
    match expr {
        SQLiteWhereExpr::Compare(compare) => {
            let left_sql = render_value_expr(compare.left(), bind_values);
            let op_sql = render_compare_op(compare.op());
            let right_sql = render_value_expr(compare.right(), bind_values);

            format!("{left_sql} {op_sql} {right_sql}")
        }
        SQLiteWhereExpr::IsNull(value) => {
            let value_sql = render_value_expr(value, bind_values);

            format!("{value_sql} IS NULL")
        }
        SQLiteWhereExpr::IsNotNull(value) => {
            let value_sql = render_value_expr(value, bind_values);

            format!("{value_sql} IS NOT NULL")
        }
        SQLiteWhereExpr::In(in_expr) => {
            let left_sql = render_value_expr(in_expr.left(), bind_values);
            let op_sql = render_in_op(in_expr.op());
            let placeholders = in_expr
                .right()
                .iter()
                .map(|literal| render_literal(literal, bind_values))
                .collect::<Vec<_>>()
                .join(", ");

            format!("{left_sql} {op_sql} ({placeholders})")
        }
        SQLiteWhereExpr::And(left, right) => {
            let left_sql = render_where_expr(left, bind_values);
            let right_sql = render_where_expr(right, bind_values);

            format!("({left_sql} AND {right_sql})")
        }
        SQLiteWhereExpr::Or(left, right) => {
            let left_sql = render_where_expr(left, bind_values);
            let right_sql = render_where_expr(right, bind_values);

            format!("({left_sql} OR {right_sql})")
        }
        SQLiteWhereExpr::Not(inner) => {
            let inner_sql = render_where_expr(inner, bind_values);

            format!("NOT ({inner_sql})")
        }
    }
}

fn render_join_clauses(plan: &SQLiteSelectPlan) -> Vec<String> {
    plan.joins()
        .iter()
        .map(|join| {
            let join_kind = match join.kind() {
                SQLiteJoinKind::Inner => "INNER JOIN",
                SQLiteJoinKind::Left => "LEFT JOIN",
            };

            let on = join.on();

            format!(
                "{join_kind} {} AS {} ON {} = {}",
                quote_identifier(join.target_table()),
                quote_identifier(join.target_alias()),
                render_qualified_identifier(on.left_alias(), on.left_column()),
                render_qualified_identifier(on.right_alias(), on.right_column()),
            )
        })
        .collect()
}

fn render_compare_op(op: SQLiteCompareOp) -> &'static str {
    match op {
        SQLiteCompareOp::Eq => "=",
        SQLiteCompareOp::Ne => "!=",
        SQLiteCompareOp::Lt => "<",
        SQLiteCompareOp::Le => "<=",
        SQLiteCompareOp::Gt => ">",
        SQLiteCompareOp::Ge => ">=",
    }
}

fn render_in_op(op: SQLiteInOp) -> &'static str {
    match op {
        SQLiteInOp::In => "IN",
        SQLiteInOp::NotIn => "NOT IN",
    }
}

fn render_arithmetic_op(op: SQLiteArithmeticOp) -> &'static str {
    match op {
        SQLiteArithmeticOp::Add => "+",
        SQLiteArithmeticOp::Sub => "-",
        SQLiteArithmeticOp::Mul => "*",
        SQLiteArithmeticOp::Div => "/",
        SQLiteArithmeticOp::Mod => "%",
    }
}

fn render_value_expr(value: &SQLiteValueExpr, bind_values: &mut Vec<SQLiteBindValue>) -> String {
    match value {
        SQLiteValueExpr::Column(column) => {
            render_qualified_identifier(column.source_alias(), column.column_name())
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::String(value)) => {
            render_literal(&SQLiteLiteral::String(value.clone()), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Int64(value)) => {
            render_literal(&SQLiteLiteral::Int64(*value), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Float64(value)) => {
            render_literal(&SQLiteLiteral::Float64(*value), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Bool(value)) => {
            render_literal(&SQLiteLiteral::Bool(*value), bind_values)
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Null) => {
            render_literal(&SQLiteLiteral::Null, bind_values)
        }
        SQLiteValueExpr::Arithmetic(arithmetic) => {
            let left = render_value_expr(arithmetic.left(), bind_values);
            let right = render_value_expr(arithmetic.right(), bind_values);
            let op = render_arithmetic_op(arithmetic.op());

            format!("({left} {op} {right})")
        }
    }
}

fn render_literal(literal: &SQLiteLiteral, bind_values: &mut Vec<SQLiteBindValue>) -> String {
    match literal {
        SQLiteLiteral::String(value) => bind_values.push(SQLiteBindValue::String(value.clone())),
        SQLiteLiteral::Int64(value) => bind_values.push(SQLiteBindValue::Int64(*value)),
        SQLiteLiteral::Float64(value) => bind_values.push(SQLiteBindValue::Float64(*value)),
        SQLiteLiteral::Bool(value) => bind_values.push(SQLiteBindValue::Bool(*value)),
        SQLiteLiteral::Null => bind_values.push(SQLiteBindValue::Null),
    }

    "?".to_string()
}

fn render_order_clause(plan: &SQLiteSelectPlan) -> Option<String> {
    let orders = plan.order_by();

    if orders.is_empty() {
        return None;
    }

    let order_items = orders
        .iter()
        .map(|order| {
            let source_alias = order.source_alias();
            let column_name = order.column_name();
            let dir = match order.direction() {
                SQLiteOrderDirection::Asc => "ASC",
                SQLiteOrderDirection::Desc => "DESC",
            };

            format!(
                "{} {dir}",
                render_qualified_identifier(source_alias, column_name)
            )
        })
        .collect::<Vec<String>>()
        .join(", ");

    Some(format!("ORDER BY {order_items}"))
}

fn render_limit_clause(plan: &SQLiteSelectPlan) -> Option<String> {
    let limit = plan.limit();

    match limit {
        None => None,
        Some(val) => Some(format!("LIMIT {val}")),
    }
}

fn render_offset_clause(plan: &SQLiteSelectPlan) -> Option<String> {
    let offset = plan.offset();

    match offset {
        None => None,
        Some(val) => Some(format!("OFFSET {val}")),
    }
}

/// Rendered SQLite select statement.
pub struct SQLiteSelectStatement {
    sql: String,
    bind_values: Vec<SQLiteBindValue>,
}

impl SQLiteSelectStatement {
    pub fn new(sql: impl Into<String>, bind_values: Vec<SQLiteBindValue>) -> Self {
        Self {
            sql: sql.into(),
            bind_values,
        }
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn bind_values(&self) -> &[SQLiteBindValue] {
        &self.bind_values
    }
}

/// Bind value produced while rendering SQL placeholders.
#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteBindValue {
    String(String),
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Null,
}

#[cfg(test)]
mod tests;
