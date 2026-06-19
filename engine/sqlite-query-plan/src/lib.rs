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

    // SELECT values and result-shape refs must assign computed aliases in the
    // same shape traversal order so the shaper reads the rendered SQL columns.
    let selected_column_names = selected_field_column_names(ir.shape());
    let mut select_aliases = SQLiteComputedAliasAllocator::new(selected_column_names.clone());
    let mut join_aliases = SQLiteJoinAliasAllocator::new(selected_link_aliases(ir.shape()));
    let planned_shape_values = plan_shape_values(
        ir.shape(),
        "root",
        false,
        &mut select_aliases,
        &mut join_aliases,
    );
    let selected_values = planned_shape_values.values;
    let mut result_aliases = SQLiteComputedAliasAllocator::new(selected_column_names);
    let result_shape = plan_result_shape(ir.shape(), "root", false, &mut result_aliases);

    let planned_orders: Vec<PlannedOrder> = ir
        .order_by()
        .iter()
        .map(|order| plan_order_expr(order, &mut join_aliases))
        .collect();

    let mut order_by = Vec::new();
    let mut order_joins = Vec::new();

    for planned in planned_orders {
        order_by.push(planned.order);
        order_joins.extend(planned.joins);
    }

    let (filter, filter_joins) = match ir.filter() {
        Some(expr) => {
            let planned = plan_where_expr(expr, &mut join_aliases);
            (Some(planned.expr), planned.joins)
        }
        None => (None, vec![]),
    };

    let mut joins: Vec<SQLiteJoin> = ir
        .shape()
        .fields()
        .into_iter()
        .filter(|field| field.child_shape().is_some())
        .map(SQLiteJoin::selected_single_link)
        .collect();

    joins.extend(planned_shape_values.joins);
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
    Computed,
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

/// One value selected by the generated SQL.
pub enum SQLiteSelectValue {
    Field(SQLiteFieldSelectValue),
    Computed(SQLiteComputedSelectValue),
}

/// Schema-backed column selected by the generated SQL.
pub struct SQLiteFieldSelectValue {
    output_name: String,
    value: SQLiteValueExpr,
    field: schema_model::FieldRef,
    role: SQLiteValueRole,
}

/// Query-local computed value selected by the generated SQL.
pub struct SQLiteComputedSelectValue {
    output_name: String,
    sql_alias: String,
    value: SQLiteValueExpr,
}

impl SQLiteSelectValue {
    pub fn from_field(
        source_alias: impl Into<String>,
        field: schema_model::FieldRef,
        output_name: impl Into<String>,
    ) -> Self {
        let source_alias = source_alias.into();
        let column_name = field.name().to_string();
        Self::Field(SQLiteFieldSelectValue {
            output_name: output_name.into(),
            value: SQLiteValueExpr::Column(SQLiteColumnRef {
                source_alias,
                column_name,
            }),
            role: SQLiteValueRole::for_field(&field),
            field,
        })
    }

    pub fn computed(
        output_name: impl Into<String>,
        sql_alias: impl Into<String>,
        value: SQLiteValueExpr,
    ) -> Self {
        Self::Computed(SQLiteComputedSelectValue {
            output_name: output_name.into(),
            sql_alias: sql_alias.into(),
            value,
        })
    }

    pub fn as_field(&self) -> Option<&SQLiteFieldSelectValue> {
        match self {
            Self::Field(value) => Some(value),
            Self::Computed(_) => None,
        }
    }

    pub fn as_computed(&self) -> Option<&SQLiteComputedSelectValue> {
        match self {
            Self::Field(_) => None,
            Self::Computed(value) => Some(value),
        }
    }

    pub fn output_name(&self) -> &str {
        match self {
            Self::Field(value) => value.output_name(),
            Self::Computed(value) => value.output_name(),
        }
    }

    pub fn source_alias(&self) -> Option<&str> {
        self.as_field().map(SQLiteFieldSelectValue::source_alias)
    }

    pub fn column_name(&self) -> Option<&str> {
        self.as_field().map(SQLiteFieldSelectValue::column_name)
    }

    pub fn field(&self) -> Option<&schema_model::FieldRef> {
        self.as_field().map(SQLiteFieldSelectValue::field)
    }

    pub fn value(&self) -> &SQLiteValueExpr {
        match self {
            Self::Field(value) => value.value(),
            Self::Computed(value) => value.value(),
        }
    }

    pub fn role(&self) -> SQLiteValueRole {
        match self {
            Self::Field(value) => value.role(),
            Self::Computed(_) => SQLiteValueRole::Computed,
        }
    }
}

impl SQLiteFieldSelectValue {
    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn source_alias(&self) -> &str {
        match &self.value {
            SQLiteValueExpr::Column(column) => column.source_alias(),
            SQLiteValueExpr::Literal(_) | SQLiteValueExpr::Arithmetic(_) => {
                unreachable!("field selected values are always columns")
            }
        }
    }

    pub fn column_name(&self) -> &str {
        match &self.value {
            SQLiteValueExpr::Column(column) => column.column_name(),
            SQLiteValueExpr::Literal(_) | SQLiteValueExpr::Arithmetic(_) => {
                unreachable!("field selected values are always columns")
            }
        }
    }

    pub fn field(&self) -> &schema_model::FieldRef {
        &self.field
    }

    pub fn value(&self) -> &SQLiteValueExpr {
        &self.value
    }

    pub fn role(&self) -> SQLiteValueRole {
        self.role
    }
}

impl SQLiteComputedSelectValue {
    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn sql_alias(&self) -> &str {
        &self.sql_alias
    }

    pub fn value(&self) -> &SQLiteValueExpr {
        &self.value
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

struct PlannedShapeValues {
    values: Vec<SQLiteSelectValue>,
    joins: Vec<SQLiteJoin>,
}

struct SQLiteComputedAliasAllocator {
    next: usize,
    reserved: Vec<String>,
}

impl SQLiteComputedAliasAllocator {
    fn new(reserved: Vec<String>) -> Self {
        Self { next: 0, reserved }
    }

    fn next_alias(&mut self) -> String {
        loop {
            let alias = format!("__gelite_value_{}", self.next);
            self.next += 1;

            if !self.reserved.iter().any(|reserved| reserved == &alias) {
                return alias;
            }
        }
    }
}

struct SQLiteJoinAliasAllocator {
    next: usize,
    reserved: Vec<String>,
    path_aliases: Vec<SQLiteJoinPathAlias>,
}

struct SQLiteJoinPathAlias {
    source_alias: String,
    path: Vec<String>,
    target_alias: String,
}

impl SQLiteJoinAliasAllocator {
    fn new(reserved: Vec<String>) -> Self {
        Self {
            next: 0,
            reserved,
            path_aliases: Vec::new(),
        }
    }

    fn next_alias(&mut self) -> String {
        loop {
            let alias = format!("__gelite_join_{}", self.next);
            self.next += 1;

            if !self.reserved.iter().any(|reserved| reserved == &alias) {
                return alias;
            }
        }
    }

    fn alias_for_path(&mut self, source_alias: &str, path: &[String]) -> String {
        if let Some(cached) = self
            .path_aliases
            .iter()
            .find(|cached| cached.source_alias == source_alias && cached.path == path)
        {
            return cached.target_alias.clone();
        }

        let target_alias = self.next_alias();
        self.path_aliases.push(SQLiteJoinPathAlias {
            source_alias: source_alias.to_string(),
            path: path.to_vec(),
            target_alias: target_alias.clone(),
        });
        target_alias
    }
}

fn selected_field_column_names(shape: &query_ir::ResolvedShape) -> Vec<String> {
    let mut column_names = Vec::new();
    collect_selected_field_column_names(shape, false, &mut column_names);
    column_names
}

fn collect_selected_field_column_names(
    shape: &query_ir::ResolvedShape,
    include_identity: bool,
    column_names: &mut Vec<String>,
) {
    if include_identity {
        column_names.push("id".to_string());
    }

    for item in shape.items() {
        if let query_ir::ResolvedShapeItem::Field(field) = item {
            match field.child_shape() {
                Some(child_shape) => {
                    collect_selected_field_column_names(child_shape, true, column_names);
                }
                None => column_names.push(field.field().name().to_string()),
            }
        }
    }
}

fn selected_link_aliases(shape: &query_ir::ResolvedShape) -> Vec<String> {
    let mut aliases = Vec::new();
    collect_selected_link_aliases(shape, &mut aliases);
    aliases
}

fn collect_selected_link_aliases(shape: &query_ir::ResolvedShape, aliases: &mut Vec<String>) {
    for item in shape.items() {
        if let query_ir::ResolvedShapeItem::Field(field) = item {
            if let Some(child_shape) = field.child_shape() {
                aliases.push(field.output_name().to_string());
                collect_selected_link_aliases(child_shape, aliases);
            }
        }
    }
}

fn plan_shape_values(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
    source_nullable: bool,
    computed_aliases: &mut SQLiteComputedAliasAllocator,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedShapeValues {
    let mut values = Vec::new();
    let mut joins = Vec::new();

    for item in shape.items() {
        match item {
            query_ir::ResolvedShapeItem::Field(field) => match field.child_shape() {
                Some(child_shape) => {
                    let nested_alias = field.output_name();
                    let child_id_field = FieldRef::new(
                        schema_model::FieldId::new(1),
                        child_shape.source_object_type().clone(),
                        "id",
                    );

                    values.push(SQLiteSelectValue::from_field(
                        nested_alias,
                        child_id_field,
                        "id",
                    ));

                    let planned_child_values = plan_shape_values(
                        child_shape,
                        nested_alias,
                        source_nullable || field.cardinality() == Cardinality::Optional,
                        computed_aliases,
                        join_aliases,
                    );
                    values.extend(planned_child_values.values);
                    joins.extend(planned_child_values.joins);
                }
                None => values.push(SQLiteSelectValue::from_field(
                    source_alias,
                    field.field().clone(),
                    field.output_name(),
                )),
            },
            query_ir::ResolvedShapeItem::Computed(computed) => {
                let planned =
                    plan_value_expr(computed.value(), source_alias, source_nullable, join_aliases);
                values.push(SQLiteSelectValue::computed(
                    computed.output_name(),
                    computed_aliases.next_alias(),
                    planned.value,
                ));
                joins.extend(planned.joins);
            }
        }
    }

    PlannedShapeValues { values, joins }
}

fn plan_result_shape(
    shape: &query_ir::ResolvedShape,
    source_alias: &str,
    include_identity: bool,
    computed_aliases: &mut SQLiteComputedAliasAllocator,
) -> SQLiteResultShapePlan {
    let fields = shape
        .items()
        .iter()
        .map(|item| match item {
            query_ir::ResolvedShapeItem::Field(field) => match field.child_shape() {
                Some(child_shape) => SQLiteResultField {
                    output_name: field.output_name().to_string(),
                    cardinality: field.cardinality(),
                    value: None,
                    nested_shape: Some(plan_result_shape(
                        child_shape,
                        field.output_name(),
                        true,
                        computed_aliases,
                    )),
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
            },
            query_ir::ResolvedShapeItem::Computed(computed) => SQLiteResultField {
                output_name: computed.output_name().to_string(),
                cardinality: computed.cardinality(),
                value: Some(SQLiteResultValueRef {
                    source_alias: source_alias.to_string(),
                    column_name: computed_aliases.next_alias(),
                    role: SQLiteValueRole::Computed,
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
    value: SQLiteValueExpr,
    direction: SQLiteOrderDirection,
}

impl SQLiteOrder {
    pub fn value(&self) -> &SQLiteValueExpr {
        &self.value
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
#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteValueExpr {
    Column(SQLiteColumnRef),
    Literal(SQLiteLiteral),
    Arithmetic(SQLiteArithmeticExpr),
}

/// Backend-specific arithmetic value expression.
#[derive(Debug, Clone, PartialEq)]
pub struct SQLiteArithmeticExpr {
    left: Box<SQLiteValueExpr>,
    op: SQLiteArithmeticOp,
    right: Box<SQLiteValueExpr>,
}

impl SQLiteArithmeticExpr {
    pub fn left(&self) -> &SQLiteValueExpr {
        &self.left
    }

    pub fn op(&self) -> SQLiteArithmeticOp {
        self.op
    }

    pub fn right(&self) -> &SQLiteValueExpr {
        &self.right
    }
}

/// SQLite arithmetic operators emitted by the planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl SQLiteArithmeticOp {
    pub fn from_ir(op: query_ir::ArithmeticOp) -> Self {
        match op {
            query_ir::ArithmeticOp::Add => Self::Add,
            query_ir::ArithmeticOp::Sub => Self::Sub,
            query_ir::ArithmeticOp::Mul => Self::Mul,
            query_ir::ArithmeticOp::Div => Self::Div,
            query_ir::ArithmeticOp::Mod => Self::Mod,
        }
    }
}

/// Backend-specific membership expression.
pub struct SQLiteInExpr {
    left: SQLiteValueExpr,
    op: SQLiteInOp,
    right: Vec<SQLiteValueExpr>,
}

impl SQLiteInExpr {
    pub fn left(&self) -> &SQLiteValueExpr {
        &self.left
    }

    pub fn op(&self) -> SQLiteInOp {
        self.op
    }

    pub fn right(&self) -> &[SQLiteValueExpr] {
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

fn plan_resolved_path(
    path: &query_ir::ResolvedPath,
    source_alias: &str,
    source_nullable: bool,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedPath {
    let mut current_alias = source_alias.to_string();
    let mut current_nullable = source_nullable;
    let mut joins = vec![];
    let mut alias_parts = if source_alias == "root" {
        Vec::new()
    } else {
        vec![source_alias.to_string()]
    };
    let mut path_parts = Vec::new();
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
                path_parts.push(step.field().name().to_string());
                let target_alias = if source_alias == "root" {
                    alias_parts.join("_")
                } else {
                    join_aliases.alias_for_path(&source_alias, &path_parts)
                };

                let link_field = step.field();
                let join_cardinality = if current_nullable {
                    Cardinality::Optional
                } else {
                    step.cardinality()
                };

                joins.push(SQLiteJoin::path_traversal(
                    source_alias,
                    link_field,
                    target_object_type,
                    target_alias.clone(),
                    path_parts.clone(),
                    join_cardinality,
                ));

                current_alias = target_alias;
                current_nullable =
                    current_nullable || step.cardinality() == Cardinality::Optional;
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

fn plan_value_expr(
    expr: &query_ir::ValueExpr,
    source_alias: &str,
    source_nullable: bool,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedValueExpr {
    match expr {
        query_ir::ValueExpr::Path(path) => {
            let planned_path =
                plan_resolved_path(path, source_alias, source_nullable, join_aliases);

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
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(value)) => PlannedValueExpr {
            value: SQLiteValueExpr::Literal(sqlite_literal_from_ir(&query_ir::Literal::Float64(
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
        query_ir::ValueExpr::Arithmetic(arithmetic) => {
            let left = plan_value_expr(
                arithmetic.left(),
                source_alias,
                source_nullable,
                join_aliases,
            );
            let right = plan_value_expr(
                arithmetic.right(),
                source_alias,
                source_nullable,
                join_aliases,
            );

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedValueExpr {
                value: SQLiteValueExpr::Arithmetic(SQLiteArithmeticExpr {
                    left: Box::new(left.value),
                    op: SQLiteArithmeticOp::from_ir(arithmetic.op()),
                    right: Box::new(right.value),
                }),
                joins,
            }
        }
    }
}

fn sqlite_literal_from_ir(literal: &query_ir::Literal) -> SQLiteLiteral {
    match literal {
        query_ir::Literal::String(value) => SQLiteLiteral::String(value.clone()),
        query_ir::Literal::Int64(value) => SQLiteLiteral::Int64(*value),
        query_ir::Literal::Float64(value) => SQLiteLiteral::Float64(*value),
        query_ir::Literal::Bool(value) => SQLiteLiteral::Bool(*value),
        query_ir::Literal::Null => SQLiteLiteral::Null,
    }
}

struct PlannedWhereExpr {
    expr: SQLiteWhereExpr,
    joins: Vec<SQLiteJoin>,
}

fn plan_where_expr(
    expr: &Expr,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedWhereExpr {
    match expr {
        Expr::Compare(compare) => {
            let left = plan_value_expr(compare.left(), "root", false, join_aliases);
            let right = plan_value_expr(compare.right(), "root", false, join_aliases);

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
            let value = plan_value_expr(value, "root", false, join_aliases);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::IsNull(value.value),
                joins: value.joins,
            }
        }
        Expr::IsNotNull(value) => {
            let value = plan_value_expr(value, "root", false, join_aliases);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::IsNotNull(value.value),
                joins: value.joins,
            }
        }
        Expr::In(in_expr) => {
            let left = plan_value_expr(in_expr.left(), "root", false, join_aliases);
            let planned_right = in_expr
                .right()
                .iter()
                .map(|value| plan_value_expr(value, "root", false, join_aliases))
                .collect::<Vec<_>>();
            let mut joins = left.joins;
            let mut right = Vec::new();

            for planned in planned_right {
                joins.extend(planned.joins);
                right.push(planned.value);
            }

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::In(SQLiteInExpr {
                    left: left.value,
                    op: SQLiteInOp::from_ir(in_expr.op()),
                    right,
                }),
                joins,
            }
        }
        Expr::And(left, right) => {
            let left = plan_where_expr(left, join_aliases);
            let right = plan_where_expr(right, join_aliases);

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::And(Box::new(left.expr), Box::new(right.expr)),
                joins,
            }
        }
        Expr::Or(left, right) => {
            let left = plan_where_expr(left, join_aliases);
            let right = plan_where_expr(right, join_aliases);

            let mut joins = left.joins;
            joins.extend(right.joins);

            PlannedWhereExpr {
                expr: SQLiteWhereExpr::Or(Box::new(left.expr), Box::new(right.expr)),
                joins,
            }
        }
        Expr::Not(inner) => {
            let inner = plan_where_expr(inner, join_aliases);

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

fn plan_order_expr(
    order: &query_ir::OrderExpr,
    join_aliases: &mut SQLiteJoinAliasAllocator,
) -> PlannedOrder {
    let planned_value = plan_value_expr(order.value(), "root", false, join_aliases);

    PlannedOrder {
        order: SQLiteOrder {
            value: planned_value.value,
            direction: SQLiteOrderDirection::from_ir(order.direction()),
        },
        joins: planned_value.joins,
    }
}

/// Literal values supported by SQLite SQL generation.
#[derive(Debug, Clone, PartialEq)]
pub enum SQLiteLiteral {
    String(String),
    Int64(i64),
    Float64(f64),
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
