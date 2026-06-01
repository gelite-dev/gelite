#![no_std]
//! SQLite-specific execution plan for resolved select queries.
//!
//! This crate is the first backend-specific compiler layer. It lowers Semantic
//! IR into structured SQLite access information: root table, aliases, selected
//! columns, joins, predicates, ordering, and result-shaping metadata.
//!
//! The plan is not SQL text. Keeping this layer structured lets tests inspect
//! physical decisions before `sqlite-query-sqlgen` serializes them. It also keeps
//! SQLite naming and join rules out of the backend-independent IR.
//!
//! The current planner handles one object table per object type, direct scalar
//! columns, single-link joins, path traversal through single links, equality
//! predicates, `IS NULL`, ordering, limit, and offset. Multi-link planning and
//! follow-up fetch plans are specified but not implemented yet.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use query_ir::{CompareOp, Expr, InOp, SelectQuery};
use schema_model::{Cardinality, FieldRef, ObjectTypeRef};

/// Lowers a resolved select query to a structured SQLite select plan.
pub fn plan_select(ir: &SelectQuery) -> SQLiteSelectPlan {
    let root_object_type = ir.root_object_type().clone();

    let selected_values = plan_shape_values(ir.shape(), "root");
    let result_shape = plan_result_shape(ir.shape(), "root", false);

    let planned_orders: Vec<PlannedOrder> = ir.order_by().iter().map(plan_order_expr).collect();

    let mut order_by = Vec::new();
    let mut order_joins = Vec::new();

    for planned in planned_orders {
        order_by.push(planned.order);
        order_joins.extend(planned.joins);
    }

    let (filter, filter_joins) = match ir.filter() {
        Some(expr) => {
            let planned = plan_where_expr(expr);
            (Some(planned.expr), planned.joins)
        }
        None => (None, vec![]),
    };

    let mut joins: Vec<SQLiteJoin> = ir
        .shape()
        .fields()
        .iter()
        .filter(|field| field.child_shape().is_some())
        .map(SQLiteJoin::selected_single_link)
        .collect();

    joins.extend(filter_joins);
    joins.extend(order_joins);
    joins = dedup_joins(joins);

    SQLiteSelectPlan {
        root_source: SQLiteObjectSource {
            table_name: root_object_type.name().to_ascii_lowercase().to_string(),
            alias: "root".to_string(),
            id_column: "id".to_string(),
            object_type: root_object_type,
        },
        selected_values,
        order_by,
        filter,
        limit: ir.limit(),
        offset: ir.offset(),
        joins,
        result_shape,
    }
}

/// Role of a selected SQLite value in result shaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteValueRole {
    ObjectId,
    Scalar,
}

/// Structured SQLite plan for a select query.
///
/// It records all physical values and joins needed to render SQL and later
/// reconstruct the logical result shape.
pub struct SQLiteSelectPlan {
    root_source: SQLiteObjectSource,
    selected_values: Vec<SQLiteSelectValue>,
    order_by: Vec<SQLiteOrder>,
    limit: Option<i64>,
    offset: Option<i64>,
    filter: Option<SQLiteWhereExpr>,
    joins: Vec<SQLiteJoin>,
    result_shape: SQLiteResultShapePlan,
}

impl SQLiteSelectPlan {
    pub fn root_source(&self) -> &SQLiteObjectSource {
        &self.root_source
    }

    pub fn selected_values(&self) -> &[SQLiteSelectValue] {
        &self.selected_values
    }

    pub fn order_by(&self) -> &[SQLiteOrder] {
        &self.order_by
    }

    pub fn limit(&self) -> Option<i64> {
        self.limit
    }

    pub fn offset(&self) -> Option<i64> {
        self.offset
    }

    pub fn filter(&self) -> &Option<SQLiteWhereExpr> {
        &self.filter
    }

    pub fn joins(&self) -> &[SQLiteJoin] {
        &self.joins
    }

    pub fn result_shape(&self) -> &SQLiteResultShapePlan {
        &self.result_shape
    }
}

/// One column selected by the generated SQL.
pub struct SQLiteSelectValue {
    source_alias: String,
    column_name: String,
    output_name: String,
    field: schema_model::FieldRef,
    role: SQLiteValueRole,
}

impl SQLiteSelectValue {
    pub fn from_field(
        source_alias: impl Into<String>,
        field: schema_model::FieldRef,
        output_name: impl Into<String>,
    ) -> Self {
        Self {
            source_alias: source_alias.into(),
            column_name: field.name().to_string(),
            output_name: output_name.into(),
            role: SQLiteValueRole::for_field(&field),
            field,
        }
    }

    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn field(&self) -> &schema_model::FieldRef {
        &self.field
    }

    pub fn role(&self) -> SQLiteValueRole {
        self.role
    }
}

impl SQLiteValueRole {
    fn for_field(field: &schema_model::FieldRef) -> Self {
        if field.name() == "id" {
            Self::ObjectId
        } else {
            Self::Scalar
        }
    }
}

fn plan_shape_values(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
) -> Vec<SQLiteSelectValue> {
    shape
        .fields()
        .iter()
        .flat_map(|field| match field.child_shape() {
            Some(child_shape) => {
                let nested_alias = field.output_name();
                let child_id_field = FieldRef::new(
                    schema_model::FieldId::new(1),
                    child_shape.source_object_type().clone(),
                    "id",
                );

                let mut values = vec![SQLiteSelectValue::from_field(
                    nested_alias,
                    child_id_field,
                    "id",
                )];

                values.extend(plan_shape_values(child_shape, nested_alias));
                values
            }
            None => vec![SQLiteSelectValue::from_field(
                source_alias,
                field.field().clone(),
                field.output_name(),
            )],
        })
        .collect()
}

fn plan_result_shape(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
    include_identity: bool,
) -> SQLiteResultShapePlan {
    let fields = shape
        .fields()
        .iter()
        .map(|field| match field.child_shape() {
            Some(child_shape) => SQLiteResultField {
                output_name: field.output_name().to_string(),
                cardinality: field.cardinality(),
                value: None,
                nested_shape: Some(plan_result_shape(child_shape, field.output_name(), true)),
            },
            None => SQLiteResultField {
                output_name: field.output_name().to_string(),
                cardinality: field.cardinality(),
                value: Some(SQLiteResultValueRef {
                    source_alias: source_alias.to_string(),
                    column_name: field.field().name().to_string(),
                    role: SQLiteValueRole::for_field(field.field()),
                }),
                nested_shape: None,
            },
        })
        .collect();

    SQLiteResultShapePlan {
        identity_value: include_identity.then(|| SQLiteResultValueRef {
            source_alias: source_alias.to_string(),
            column_name: "id".to_string(),
            role: SQLiteValueRole::ObjectId,
        }),
        fields,
    }
}

/// Result-shaping plan for one object level.
pub struct SQLiteResultShapePlan {
    identity_value: Option<SQLiteResultValueRef>,
    fields: Vec<SQLiteResultField>,
}

impl SQLiteResultShapePlan {
    pub fn fields(&self) -> &[SQLiteResultField] {
        &self.fields
    }
    pub fn identity_value(&self) -> Option<&SQLiteResultValueRef> {
        self.identity_value.as_ref()
    }
}

/// One output field in a result-shaping plan.
pub struct SQLiteResultField {
    output_name: String,
    cardinality: schema_model::Cardinality,
    value: Option<SQLiteResultValueRef>,
    nested_shape: Option<SQLiteResultShapePlan>,
}

impl SQLiteResultField {
    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn cardinality(&self) -> schema_model::Cardinality {
        self.cardinality
    }

    pub fn value(&self) -> Option<&SQLiteResultValueRef> {
        self.value.as_ref()
    }

    pub fn nested_shape(&self) -> Option<&SQLiteResultShapePlan> {
        self.nested_shape.as_ref()
    }
}

/// Reference to a selected value used while shaping rows into objects.
pub struct SQLiteResultValueRef {
    source_alias: String,
    column_name: String,
    role: SQLiteValueRole,
}

impl SQLiteResultValueRef {
    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn role(&self) -> SQLiteValueRole {
        self.role
    }
}

/// Physical root table for a SQLite query.
pub struct SQLiteObjectSource {
    object_type: ObjectTypeRef,
    table_name: String,
    alias: String,
    id_column: String,
}

impl SQLiteObjectSource {
    pub fn object_type(&self) -> &ObjectTypeRef {
        &self.object_type
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn id_column(&self) -> &str {
        &self.id_column
    }
}

/// SQLite sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteOrderDirection {
    Asc,
    Desc,
}

impl SQLiteOrderDirection {
    pub fn from_ir(direction: query_ir::OrderDirection) -> Self {
        match direction {
            query_ir::OrderDirection::Asc => Self::Asc,
            query_ir::OrderDirection::Desc => Self::Desc,
        }
    }
}

/// Planned SQLite ordering item.
pub struct SQLiteOrder {
    source_alias: String,
    column_name: String,
    direction: SQLiteOrderDirection,
}

impl SQLiteOrder {
    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn direction(&self) -> SQLiteOrderDirection {
        self.direction
    }
}

/// Backend-specific predicate expression.
pub enum SQLiteWhereExpr {
    Compare(SQLiteCompareExpr),
    IsNull(SQLiteValueExpr),
    IsNotNull(SQLiteValueExpr),
    In(SQLiteInExpr),
    And(Box<SQLiteWhereExpr>, Box<SQLiteWhereExpr>),
    Or(Box<SQLiteWhereExpr>, Box<SQLiteWhereExpr>),
    Not(Box<SQLiteWhereExpr>),
}

/// Backend-specific comparison expression.
pub struct SQLiteCompareExpr {
    left: SQLiteValueExpr,
    op: SQLiteCompareOp,
    right: SQLiteValueExpr,
}

impl SQLiteCompareExpr {
    pub fn left(&self) -> &SQLiteValueExpr {
        &self.left
    }

    pub fn op(&self) -> SQLiteCompareOp {
        self.op
    }

    pub fn right(&self) -> &SQLiteValueExpr {
        &self.right
    }
}

/// Backend-specific value expression.
pub enum SQLiteValueExpr {
    Column(SQLiteColumnRef),
    Literal(SQLiteLiteral),
}

/// Backend-specific membership expression.
pub struct SQLiteInExpr {
    left: SQLiteValueExpr,
    op: SQLiteInOp,
    right: Vec<SQLiteLiteral>,
}

impl SQLiteInExpr {
    pub fn left(&self) -> &SQLiteValueExpr {
        &self.left
    }

    pub fn op(&self) -> SQLiteInOp {
        self.op
    }

    pub fn right(&self) -> &[SQLiteLiteral] {
        &self.right
    }
}

/// Backend-specific membership operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteInOp {
    In,
    NotIn,
}

impl SQLiteInOp {
    pub fn from_ir(op: InOp) -> Self {
        match op {
            InOp::In => Self::In,
            InOp::NotIn => Self::NotIn,
        }
    }
}

/// Physical column reference.
pub struct SQLiteColumnRef {
    source_alias: String,
    column_name: String,
}

impl SQLiteColumnRef {
    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }
}

struct PlannedPath {
    column: SQLiteColumnRef,
    joins: Vec<SQLiteJoin>,
}

fn plan_resolved_path(path: &query_ir::ResolvedPath) -> PlannedPath {
    let mut current_alias = "root".to_string();
    let mut joins = vec![];
    let mut alias_parts = Vec::new();
    let mut column = None;

    for (index, step) in path.steps().iter().enumerate() {
        let is_last = index == path.steps().len() - 1;

        match step.kind() {
            query_ir::ResolvedPathStepKind::Link { target_object_type } => {
                if is_last {
                    todo!("link-only paths cannot be lowered to SQLite columns");
                }

                let source_alias = current_alias.clone();

                alias_parts.push(step.field().name().to_string());
                let target_alias = alias_parts.join("_");

                let link_field = step.field();

                joins.push(SQLiteJoin::path_traversal(
                    source_alias,
                    link_field,
                    target_object_type,
                    target_alias.clone(),
                    alias_parts.clone(),
                    step.cardinality(),
                ));

                current_alias = target_alias;
            }
            query_ir::ResolvedPathStepKind::Scalar => {
                if !is_last {
                    todo!("scalar path step before terminal position is not supported");
                }

                column = Some(SQLiteColumnRef {
                    source_alias: current_alias.clone(),
                    column_name: step.field().name().to_string(),
                });
            }
        }
    }

    PlannedPath {
        column: column.expect("resolved path should end in a scalar field"),
        joins,
    }
}

struct PlannedValueExpr {
    value: SQLiteValueExpr,
    joins: Vec<SQLiteJoin>,
}

fn plan_value_expr(expr: &query_ir::ValueExpr) -> PlannedValueExpr {
    match expr {
        query_ir::ValueExpr::Path(path) => {
            let planned_path = plan_resolved_path(path);

            PlannedValueExpr {
                value: SQLiteValueExpr::Column(planned_path.column),
                joins: planned_path.joins,
            }
        }
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::String(
                value.clone(),
            ))),
            joins: vec![],
        },
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Int64(
                *value,
            ))),
            joins: vec![],
        },
        query_ir::ValueExpr::Literal(query_ir::Literal::Bool(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Bool(
                *value,
            ))),
            joins: vec![],
        },
        query_ir::ValueExpr::Literal(query_ir::Literal::Null) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Null)),
            joins: vec![],
        },
    }
}

fn sqlite_literal_from_ir(literal: &query_ir::Literal) -> SQLiteLiteral {
    match literal {
        query_ir::Literal::String(value) => SQLiteLiteral::String(value.clone()),
        query_ir::Literal::Int64(value) => SQLiteLiteral::Int64(*value),
        query_ir::Literal::Bool(value) => SQLiteLiteral::Bool(*value),
        query_ir::Literal::Null => SQLiteLiteral::Null,
    }
}

struct PlannedWhereExpr {
    expr: SQLiteWhereExpr,
    joins: Vec<SQLiteJoin>,
}

fn plan_where_expr(expr: &Expr) -> PlannedWhereExpr {
    match expr {
        Expr::Compare(compare) => {
            let left = plan_value_expr(compare.left());
            let right = plan_value_expr(compare.right());

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::Compare(SQLiteCompareExpr {
                    left: left.value,
                    op: SQLiteCompareOp::from_ir(compare.op()),
                    right: right.value,
                }),
                joins,
            }
        }
        Expr::IsNull(value) => {
            let value = plan_value_expr(value);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::IsNull(value.value),
                joins: value.joins,
            }
        }
        Expr::IsNotNull(value) => {
            let value = plan_value_expr(value);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::IsNotNull(value.value),
                joins: value.joins,
            }
        }
        Expr::In(in_expr) => {
            let left = plan_value_expr(in_expr.left());
            let right = in_expr.right().iter().map(sqlite_literal_from_ir).collect();

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::In(SQLiteInExpr {
                    left: left.value,
                    op: SQLiteInOp::from_ir(in_expr.op()),
                    right,
                }),
                joins: left.joins,
            }
        }
        Expr::And(left, right) => {
            let left = plan_where_expr(left);
            let right = plan_where_expr(right);

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::And(Box::new(left.expr), Box::new(right.expr)),
                joins,
            }
        }
        Expr::Or(left, right) => {
            let left = plan_where_expr(left);
            let right = plan_where_expr(right);

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::Or(Box::new(left.expr), Box::new(right.expr)),
                joins,
            }
        }
        Expr::Not(inner) => {
            let inner = plan_where_expr(inner);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::Not(Box::new(inner.expr)),
                joins: inner.joins,
            }
        }
    }
}

struct PlannedOrder {
    order: SQLiteOrder,
    joins: Vec<SQLiteJoin>,
}

fn plan_order_expr(order: &query_ir::OrderExpr) -> PlannedOrder {
    let planned_value = plan_value_expr(order.value());

    let SQLiteValueExpr::Column(column) = planned_value.value else {
        panic!("ORDER BY must resolve to a column")
    };

    PlannedOrder {
        order: SQLiteOrder {
            source_alias: column.source_alias,
            column_name: column.column_name,
            direction: SQLiteOrderDirection::from_ir(order.direction()),
        },
        joins: planned_value.joins,
    }
}

/// Literal values supported by SQLite SQL generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SQLiteLiteral {
    String(String),
    Int64(i64),
    Bool(bool),
    Null,
}

/// SQLite comparison operators currently emitted by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteCompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl SQLiteCompareOp {
    pub fn from_ir(compare_op: CompareOp) -> Self {
        match compare_op {
            CompareOp::Eq => Self::Eq,
            CompareOp::Ne => Self::Ne,
            CompareOp::Lt => Self::Lt,
            CompareOp::Le => Self::Le,
            CompareOp::Gt => Self::Gt,
            CompareOp::Ge => Self::Ge,
        }
    }
}

/// Logical reason a join exists in the plan.
///
/// This is used for tests and future explain output; SQL generation only needs
/// the physical join fields.
pub enum SQLiteJoinReason {
    SelectedSingleLink { field: FieldRef },
    PathTraversal { path: Vec<String> },
}

/// SQLite join kind chosen from relation cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteJoinKind {
    Inner,
    Left,
}

impl SQLiteJoinKind {
    pub fn for_single_link(cardinality: schema_model::Cardinality) -> Self {
        match cardinality {
            Cardinality::Required => Self::Inner,
            Cardinality::Optional => Self::Left,
            Cardinality::Many => {
                todo!("multi link joins are not supported yet")
            }
        }
    }
}

/// Equality condition connecting two aliases in a join.
pub struct SQLiteJoinCondition {
    left_alias: String,
    left_column: String,
    right_alias: String,
    right_column: String,
}
impl SQLiteJoinCondition {
    pub fn left_alias(&self) -> &str {
        &self.left_alias
    }

    pub fn left_column(&self) -> &str {
        &self.left_column
    }

    pub fn right_alias(&self) -> &str {
        &self.right_alias
    }

    pub fn right_column(&self) -> &str {
        &self.right_column
    }
}

pub struct SQLiteJoin {
    kind: SQLiteJoinKind,
    source_alias: String,
    target_table: String,
    target_alias: String,
    on: SQLiteJoinCondition,
    reason: SQLiteJoinReason,
}

impl SQLiteJoin {
    pub fn selected_single_link(shape_field: &query_ir::ResolvedShapeField) -> Self {
        let child_shape = shape_field
            .child_shape()
            .expect("selected link field must have child shape");

        let field = shape_field.field().clone();

        Self {
            kind: SQLiteJoinKind::for_single_link(shape_field.cardinality()),
            source_alias: "root".to_string(),
            target_table: child_shape
                .source_object_type()
                .name()
                .to_ascii_lowercase()
                .to_string(),
            target_alias: shape_field.output_name().to_string(),
            on: SQLiteJoinCondition {
                left_alias: "root".to_string(),
                left_column: format!("{}_id", field.name()),
                right_alias: shape_field.output_name().to_string(),
                right_column: "id".to_string(),
            },
            reason: SQLiteJoinReason::SelectedSingleLink { field },
        }
    }

    fn path_traversal(
        source_alias: impl Into<String>,
        link_field: &FieldRef,
        target_object_type: &ObjectTypeRef,
        target_alias: impl Into<String>,
        path: Vec<String>,
        cardinality: Cardinality,
    ) -> Self {
        let source_alias = source_alias.into();
        let target_alias = target_alias.into();

        Self {
            kind: SQLiteJoinKind::for_single_link(cardinality),
            source_alias: source_alias.clone(),
            target_table: target_object_type.name().to_ascii_lowercase().to_string(),
            target_alias: target_alias.clone(),
            on: SQLiteJoinCondition {
                left_alias: source_alias,
                left_column: format!("{}_id", link_field.name()),
                right_alias: target_alias,
                right_column: "id".to_string(),
            },
            reason: SQLiteJoinReason::PathTraversal { path },
        }
    }

    pub fn kind(&self) -> SQLiteJoinKind {
        self.kind
    }

    pub fn source_alias(&self) -> &str {
        &self.source_alias
    }

    pub fn target_table(&self) -> &str {
        &self.target_table
    }

    pub fn target_alias(&self) -> &str {
        &self.target_alias
    }

    pub fn on(&self) -> &SQLiteJoinCondition {
        &self.on
    }

    pub fn reason(&self) -> &SQLiteJoinReason {
        &self.reason
    }

    fn has_same_identity(&self, other: &SQLiteJoin) -> bool {
        self.kind == other.kind
            && self.source_alias == other.source_alias
            && self.target_table == other.target_table
            && self.target_alias == other.target_alias
            && self.on.left_alias == other.on.left_alias
            && self.on.left_column == other.on.left_column
            && self.on.right_alias == other.on.right_alias
            && self.on.right_column == other.on.right_column
    }
}

fn dedup_joins(joins: Vec<SQLiteJoin>) -> Vec<SQLiteJoin> {
    let mut deduped: Vec<SQLiteJoin> = Vec::new();

    for join in joins {
        if !deduped
            .iter()
            .any(|existing| existing.has_same_identity(&join))
        {
            deduped.push(join);
        }
    }

    deduped
}

#[cfg(test)]
mod tests;
