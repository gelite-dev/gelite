#![no_std]
//! Backend-independent Semantic IR for resolved queries.
//!
//! The IR is the boundary between language semantics and backend-specific
//! execution planning. It contains resolved schema references, result shapes,
//! path steps, expression nodes, and cardinality metadata, but it does not know
//! SQLite table names, column names, join aliases, or SQL syntax.
//!
//! The resolver builds these types from `query-ast` and `schema`. Downstream
//! crates such as `sqlite-plan` must be able to lower the IR without looking
//! back at raw query text.
//!
//! The implemented subset currently represents `select` queries. Insert,
//! update, and delete are part of the MVP query spec but are deferred until the
//! select pipeline is stable.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use schema::{Cardinality, FieldRef, ObjectTypeRef};

/// Resolved select query.
///
/// The root object type, output shape, filter, and order expressions have all
/// been checked against the schema catalog before this value is constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectQuery {
    root_object_type: ObjectTypeRef,
    shape: ResolvedShape,
    filter: Option<Expr>,
    order_by: Vec<OrderExpr>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl SelectQuery {
    pub fn new(
        root_object_type: ObjectTypeRef,
        shape: ResolvedShape,
        filter: Option<Expr>,
        order_by: Vec<OrderExpr>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Self {
        Self {
            root_object_type,
            shape,
            filter,
            order_by,
            limit,
            offset,
        }
    }

    pub fn root_object_type(&self) -> &ObjectTypeRef {
        &self.root_object_type
    }

    pub fn filter(&self) -> Option<&Expr> {
        self.filter.as_ref()
    }

    pub fn limit(&self) -> Option<i64> {
        self.limit
    }

    pub fn offset(&self) -> Option<i64> {
        self.offset
    }

    pub fn order_by(&self) -> &[OrderExpr] {
        &self.order_by
    }

    pub fn shape(&self) -> &ResolvedShape {
        &self.shape
    }
}

/// Resolved result shape for one object source.
///
/// Fields are ordered in the same order requested by the query. Nested shapes
/// preserve link cardinality so runtime shaping can distinguish optional,
/// required, and multi-valued relations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedShape {
    source_object_type: ObjectTypeRef,
    fields: Vec<ResolvedShapeField>,
}

impl ResolvedShape {
    pub fn new(source_object_type: ObjectTypeRef, fields: Vec<ResolvedShapeField>) -> Self {
        Self {
            source_object_type,
            fields,
        }
    }

    pub fn source_object_type(&self) -> &ObjectTypeRef {
        &self.source_object_type
    }

    pub fn fields(&self) -> &[ResolvedShapeField] {
        &self.fields
    }
}

/// One resolved output field in a [`ResolvedShape`].
///
/// Scalar fields have no child shape. Link fields selected in output must have
/// a child shape, because relations are returned as nested objects rather than
/// as raw foreign key columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedShapeField {
    output_name: String,
    field: FieldRef,
    cardinality: Cardinality,
    child_shape: Option<ResolvedShape>,
}

impl ResolvedShapeField {
    pub fn new(
        output_name: impl Into<String>,
        field: FieldRef,
        cardinality: Cardinality,
        child_shape: Option<ResolvedShape>,
    ) -> Self {
        Self {
            output_name: output_name.into(),
            field,
            cardinality,
            child_shape,
        }
    }
    pub fn output_name(&self) -> &str {
        &self.output_name
    }
    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
    pub fn child_shape(&self) -> Option<&ResolvedShape> {
        self.child_shape.as_ref()
    }
    pub fn field(&self) -> &FieldRef {
        &self.field
    }
}

/// Resolved field path from a root object type.
///
/// The path stores each schema field reference and the combined result
/// cardinality. A path through an optional link is optional; a path through a
/// multi link is many.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPath {
    root_object_type: ObjectTypeRef,
    steps: Vec<ResolvedPathStep>,
    result_cardinality: Cardinality,
}

impl ResolvedPath {
    pub fn try_new(
        root_object_type: ObjectTypeRef,
        steps: Vec<ResolvedPathStep>,
    ) -> Result<Self, ResolvedPathError> {
        if steps.is_empty() {
            return Err(ResolvedPathError::EmptyPath);
        }

        Ok(Self::new(root_object_type, steps))
    }

    fn new(root_object_type: ObjectTypeRef, steps: Vec<ResolvedPathStep>) -> Self {
        fn combine_cardinality(left: Cardinality, right: Cardinality) -> Cardinality {
            match (left, right) {
                (Cardinality::Many, _) | (_, Cardinality::Many) => Cardinality::Many,
                (Cardinality::Optional, _) | (_, Cardinality::Optional) => Cardinality::Optional,
                (Cardinality::Required, Cardinality::Required) => Cardinality::Required,
            }
        }

        let result_cardinality = steps
            .iter()
            .map(|step| step.cardinality())
            .fold(Cardinality::Required, combine_cardinality);

        Self {
            root_object_type,
            steps,
            result_cardinality,
        }
    }

    pub fn root_object_type(&self) -> &ObjectTypeRef {
        &self.root_object_type
    }

    pub fn steps(&self) -> &[ResolvedPathStep] {
        &self.steps
    }

    pub fn result_cardinality(&self) -> Cardinality {
        self.result_cardinality
    }
}

/// Errors that can occur while constructing a resolved path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedPathError {
    EmptyPath,
}

/// One resolved step in a [`ResolvedPath`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPathStep {
    field: FieldRef,
    kind: ResolvedPathStepKind,
    cardinality: Cardinality,
}

impl ResolvedPathStep {
    pub fn scalar(field: FieldRef, cardinality: Cardinality) -> Self {
        Self {
            field,
            kind: ResolvedPathStepKind::Scalar,
            cardinality,
        }
    }

    pub fn link(
        field: FieldRef,
        target_object_type: ObjectTypeRef,
        cardinality: Cardinality,
    ) -> Self {
        Self {
            field,
            kind: ResolvedPathStepKind::Link { target_object_type },
            cardinality,
        }
    }

    pub fn field(&self) -> &FieldRef {
        &self.field
    }

    pub fn kind(&self) -> &ResolvedPathStepKind {
        &self.kind
    }

    pub fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}

/// Semantic kind of a resolved path step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPathStepKind {
    Scalar,
    Link { target_object_type: ObjectTypeRef },
}

/// Resolved boolean expression.
///
/// `IsNull` is used when the source query writes `field = null`. Keeping it as
/// a separate node lets SQL generation render `IS NULL` instead of `= ?`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Compare(CompareExpr),
    IsNull(ValueExpr),
}

/// Resolved comparison expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareExpr {
    left: ValueExpr,
    op: CompareOp,
    right: ValueExpr,
}

impl CompareExpr {
    pub fn new(left: ValueExpr, op: CompareOp, right: ValueExpr) -> Self {
        Self { left, op, right }
    }

    pub fn left(&self) -> &ValueExpr {
        &self.left
    }

    pub fn op(&self) -> CompareOp {
        self.op
    }

    pub fn right(&self) -> &ValueExpr {
        &self.right
    }
}

/// Comparison operators implemented by the current IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
}

/// Resolved ordering expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderExpr {
    value: ValueExpr,
    direction: OrderDirection,
}

impl OrderExpr {
    pub fn new(value: ValueExpr, direction: OrderDirection) -> Self {
        Self { value, direction }
    }

    pub fn value(&self) -> &ValueExpr {
        &self.value
    }

    pub fn direction(&self) -> OrderDirection {
        self.direction
    }
}

/// Sort direction for a resolved order expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

/// Scalar value expression used in filters and ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueExpr {
    Path(ResolvedPath),
    Literal(Literal),
}

/// Literal values represented by the current IR.
///
/// The query AST accepts floats, but the resolver does not lower them yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    String(String),
    Int64(i64),
    Bool(bool),
    Null,
}

#[cfg(test)]
mod tests;
