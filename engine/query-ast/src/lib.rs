#![no_std]
//! Syntax tree for the MVP query language.
//!
//! This crate stores query structure exactly after parsing and before semantic
//! resolution. Names are still strings, paths are still field-name sequences,
//! and no schema catalog has been consulted yet. That separation is important:
//! parser tests can focus on source syntax, while the resolver owns type,
//! field, relation, and cardinality checks.
//!
//! The implemented AST currently covers `select` queries with explicit result
//! shapes, filter expression trees, ordering, limit, and offset. Insert, update,
//! and delete are specified in `spec/query.md` but are not represented here yet.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// Parsed `select` query before schema resolution.
///
/// `root_type_name` is an unresolved object type name. The resolver turns it
/// into `schema_model::ObjectTypeRef` and validates the shape, filter, and ordering
/// against the schema catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectQuery {
    root_type_name: String,
    shape: Shape,
    filter: Option<Expr>,
    order_by: Vec<OrderExpr>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Explicit result shape requested by a select query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    items: Vec<ShapeItem>,
}

/// One output field in a [`Shape`].
///
/// A child shape means the item syntactically selected a nested object, as in
/// `author: { name }`. The parser does not know whether the path is a link; the
/// resolver validates that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeItem {
    path: Path,
    child_shape: Option<Shape>,
}

/// Unresolved field path.
///
/// Filter and order paths may contain multiple steps such as `.author.name`.
/// Shape items currently use one-step paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    steps: Vec<PathStep>,
}

/// One unresolved field name in a [`Path`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathStep {
    field_name: String,
}

/// Parsed expression forms accepted by the query syntax tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Path(Path),
    Compare(CompareExpr),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    In(InExpr),
}

/// Membership expression parsed from an `in` or `not in` filter clause.
#[derive(Debug, Clone, PartialEq)]
pub struct InExpr {
    left: Box<Expr>,
    op: InOp,
    right: Vec<Expr>,
}

/// Membership operators implemented by the current parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InOp {
    In,
    NotIn,
}

impl InExpr {
    pub fn new(left: Expr, op: InOp, right: Vec<Expr>) -> Self {
        Self {
            left: Box::new(left),
            op,
            right,
        }
    }

    pub fn left(&self) -> &Expr {
        &self.left
    }

    pub fn op(&self) -> InOp {
        self.op
    }

    pub fn right(&self) -> &[Expr] {
        &self.right
    }
}

/// Binary comparison expression parsed from a filter clause.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareExpr {
    left: Box<Expr>,
    op: CompareOp,
    right: Box<Expr>,
}

/// Comparison operators implemented by the current parser and resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
}

/// Literal values accepted by the query parser.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Null,
}

/// Parsed ordering item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderExpr {
    path: Path,
    direction: OrderDirection,
}

/// Sort direction for an ordering item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDirection {
    Asc,
    Desc,
}

impl SelectQuery {
    pub fn new(
        root_type_name: impl Into<String>,
        shape: Shape,
        filter: Option<Expr>,
        order_by: Vec<OrderExpr>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Self {
        Self {
            root_type_name: root_type_name.into(),
            shape,
            filter,
            order_by,
            limit,
            offset,
        }
    }

    pub fn root_type_name(&self) -> &str {
        &self.root_type_name
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    pub fn filter(&self) -> Option<&Expr> {
        self.filter.as_ref()
    }

    pub fn order_by(&self) -> &[OrderExpr] {
        &self.order_by
    }

    pub fn limit(&self) -> Option<i64> {
        self.limit
    }

    pub fn offset(&self) -> Option<i64> {
        self.offset
    }
}

impl Shape {
    pub fn new(items: Vec<ShapeItem>) -> Self {
        Self { items }
    }

    pub fn items(&self) -> &[ShapeItem] {
        &self.items
    }
}

impl ShapeItem {
    pub fn new(path: Path, child_shape: Option<Shape>) -> Self {
        Self { path, child_shape }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn child_shape(&self) -> Option<&Shape> {
        self.child_shape.as_ref()
    }
}

impl Path {
    pub fn new(steps: Vec<PathStep>) -> Self {
        Path { steps }
    }

    pub fn steps(&self) -> &[PathStep] {
        self.steps.as_ref()
    }
}

impl PathStep {
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
        }
    }

    pub fn field_name(&self) -> &str {
        &self.field_name
    }
}

impl OrderExpr {
    pub fn new(path: Path, direction: OrderDirection) -> Self {
        Self { path, direction }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn direction(&self) -> OrderDirection {
        self.direction
    }
}

impl CompareExpr {
    pub fn new(left: Expr, op: CompareOp, right: Expr) -> Self {
        Self {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }
    pub fn left(&self) -> &Expr {
        &self.left
    }
    pub fn op(&self) -> CompareOp {
        self.op
    }
    pub fn right(&self) -> &Expr {
        &self.right
    }
}

#[cfg(test)]
mod tests;
