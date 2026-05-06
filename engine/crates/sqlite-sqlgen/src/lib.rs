#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use sqlite_plan::{
    SQLiteCompareOp, SQLiteLiteral, SQLiteOrderDirection, SQLiteSelectPlan, SQLiteValueExpr,
    SQLiteWhereExpr,
};

pub fn render_select(plan: &sqlite_plan::SQLiteSelectPlan) -> SQLiteSelectStatement {
    let select_clause = render_select_clause(plan);
    let from_clause = render_from_clause(plan);
    let (where_clause, bind_values) = render_where_clause(plan);
    let order_clause = render_order_clause(plan);

    let mut clauses = vec![select_clause, from_clause];
    if let Some(where_clause) = where_clause {
        clauses.push(where_clause);
    }
    if let Some(order_clause) = order_clause {
        clauses.push(order_clause);
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
        .map(|value| format!("{}.{}", value.source_alias(), value.column_name()))
        .collect::<Vec<_>>()
        .join(", ");

    format!("SELECT {columns}")
}

fn render_from_clause(plan: &SQLiteSelectPlan) -> String {
    let columns = plan.root_source().table_name();
    let alias = plan.root_source().alias();

    format!("FROM {columns} AS {alias}")
}

fn render_where_clause(plan: &SQLiteSelectPlan) -> (Option<String>, Vec<SQLiteBindValue>) {
    match plan.filter() {
        None => (None, vec![]),
        Some(SQLiteWhereExpr::Compare(compare)) => {
            let mut bind_values = Vec::new();
            let left_sql = render_value_expr(compare.left(), &mut bind_values);
            let op_sql = render_compare_op(compare.op());
            let right_sql = render_value_expr(compare.right(), &mut bind_values);

            (
                Some(format!("WHERE {left_sql} {op_sql} {right_sql}")),
                bind_values,
            )
        }
    }
}

fn render_compare_op(op: SQLiteCompareOp) -> &'static str {
    match op {
        SQLiteCompareOp::Eq => "=",
    }
}

fn render_value_expr(value: &SQLiteValueExpr, bind_values: &mut Vec<SQLiteBindValue>) -> String {
    match value {
        SQLiteValueExpr::Column(column) => {
            format!("{}.{}", column.source_alias(), column.column_name())
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::String(value)) => {
            bind_values.push(SQLiteBindValue::String(value.clone()));
            "?".to_string()
        }
    }
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

            format!("{source_alias}.{column_name} {dir}")
        })
        .collect::<Vec<String>>()
        .join(", ");

    Some(format!("ORDER BY {order_items}"))
}

pub struct SQLiteSelectStatement {
    sql: String,
    bind_values: Vec<SQLiteBindValue>,
}

impl SQLiteSelectStatement {
    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn bind_values(&self) -> &[SQLiteBindValue] {
        &self.bind_values
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SQLiteBindValue {
    String(String),
}

#[cfg(test)]
mod tests;
