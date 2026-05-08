#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use sqlite_plan::{
    SQLiteCompareOp, SQLiteJoinKind, SQLiteLiteral, SQLiteOrderDirection, SQLiteSelectPlan,
    SQLiteValueExpr, SQLiteWhereExpr,
};

pub fn render_select(plan: &sqlite_plan::SQLiteSelectPlan) -> SQLiteSelectStatement {
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
        Some(SQLiteWhereExpr::IsNull(value)) => {
            let mut bind_values = Vec::new();
            let value_sql = render_value_expr(value, &mut bind_values);

            (Some(format!("WHERE {value_sql} IS NULL")), bind_values)
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
                "{join_kind} {} AS {} ON {}.{} = {}.{}",
                join.target_table(),
                join.target_alias(),
                on.left_alias(),
                on.left_column(),
                on.right_alias(),
                on.right_column(),
            )
        })
        .collect()
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
        SQLiteValueExpr::Literal(SQLiteLiteral::Int64(value)) => {
            bind_values.push(SQLiteBindValue::Int64(*value));
            "?".to_string()
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Bool(value)) => {
            bind_values.push(SQLiteBindValue::Bool(*value));
            "?".to_string()
        }
        SQLiteValueExpr::Literal(SQLiteLiteral::Null) => {
            bind_values.push(SQLiteBindValue::Null);
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
    Int64(i64),
    Bool(bool),
    Null,
}

#[cfg(test)]
mod tests;
