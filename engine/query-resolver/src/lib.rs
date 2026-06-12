#![no_std]
//! Semantic resolver for query AST values.
//!
//! The resolver is the compiler stage that turns unresolved syntax from
//! `query-ast` into backend-independent Semantic IR. It checks object type
//! names, field names, nested shape rules, link traversal rules, and the small
//! literal subset currently supported by the select pipeline.
//!
//! This crate must not know SQLite table names or SQL syntax. Its output is
//! `ir`, and backend-specific choices belong to `sqlite-query-plan` and
//! `sqlite-query-sqlgen`.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use query_ast::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypedValueExpr {
    value: query_ir::ValueExpr,
    source: ValueSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueSource {
    Path(schema_model::ScalarType),
    Literal(schema_model::ScalarType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypedLiteral {
    value: query_ir::Literal,
    scalar_type: schema_model::ScalarType,
}

/// Resolves a parsed select query against a validated schema catalog.
///
/// The current implementation supports explicit shape fields, nested shapes on
/// declared links, equality filters, `null` comparisons, order expressions, and
/// limit/offset propagation. It rejects unsupported expression and path forms
/// before the query reaches backend-specific planning.
pub fn resolve_select(
    catalog: &schema_model::SchemaCatalog,
    query: &query_ast::SelectQuery,
) -> Result<query_ir::SelectQuery, ResolveError> {
    let root_object_type = catalog
        .find_type_ref(query.root_type_name())
        .ok_or_else(|| ResolveError::UnknownObjectType {
            name: query.root_type_name().to_string(),
        })?;

    let fields = query
        .shape()
        .items()
        .iter()
        .map(|item| resolve_shape_item(catalog, &root_object_type, item))
        .collect::<Result<Vec<_>, ResolveError>>()?;

    let filter = query
        .filter()
        .map(|expr| resolve_expr(catalog, &root_object_type, expr))
        .transpose()?;

    let order_by = query
        .order_by()
        .iter()
        .map(|order| resolve_order_expr(catalog, &root_object_type, order))
        .collect::<Result<Vec<_>, ResolveError>>()?;

    Ok(query_ir::SelectQuery::new(
        root_object_type.clone(),
        query_ir::ResolvedShape::new(root_object_type, fields),
        filter,
        order_by,
        query.limit(),
        query.offset(),
    ))
}

fn resolve_shape_item(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    item: &query_ast::ShapeItem,
) -> Result<query_ir::ResolvedShapeField, ResolveError> {
    let steps = item.path().steps();

    if steps.len() != 1 {
        return Err(ResolveError::UnsupportedPath);
    }

    let field_name = steps[0].field_name();

    let field = catalog
        .find_field(source_object_type.name(), field_name)
        .ok_or_else(|| ResolveError::UnknownField {
            object_type: source_object_type.name().to_string(),
            field: field_name.to_string(),
        })?;

    let field_ref = catalog
        .find_field_ref(source_object_type.name(), field_name)
        .expect("field ref should exist for a field already found in the catalog");

    match field {
        schema_model::Field::Scalar(_) => {
            if item.child_shape().is_some() {
                return Err(ResolveError::NestedShapeOnScalarField {
                    object_type: source_object_type.name().to_string(),
                    field: field_name.to_string(),
                });
            }

            Ok(query_ir::ResolvedShapeField::new(
                field_name,
                field_ref,
                field.cardinality(),
                None,
            ))
        }
        schema_model::Field::Link(link) => {
            let child_shape =
                item.child_shape()
                    .ok_or_else(|| ResolveError::MissingShapeOnLinkField {
                        object_type: source_object_type.name().to_string(),
                        field: field_name.to_string(),
                    })?;

            let target_object_type =
                catalog
                    .find_type_ref(link.target_type_name())
                    .ok_or_else(|| ResolveError::UnknownObjectType {
                        name: link.target_type_name().to_string(),
                    })?;

            let child_fields = child_shape
                .items()
                .iter()
                .map(|child_item| resolve_shape_item(catalog, &target_object_type, child_item))
                .collect::<Result<Vec<_>, ResolveError>>()?;

            let resolved_child_shape =
                query_ir::ResolvedShape::new(target_object_type, child_fields);

            Ok(query_ir::ResolvedShapeField::new(
                field_name,
                field_ref,
                field.cardinality(),
                Some(resolved_child_shape),
            ))
        }
    }
}

fn resolve_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<query_ir::Expr, ResolveError> {
    match expr {
        query_ast::Expr::Compare(compare) => {
            if is_null_literal(compare.right()) {
                if compare.op() == query_ast::CompareOp::Ne {
                    let left = resolve_null_comparable_path_value_expr(
                        catalog,
                        source_object_type,
                        compare.left(),
                    )?;
                    return Ok(query_ir::Expr::IsNotNull(left));
                }

                if compare.op() != query_ast::CompareOp::Eq {
                    return Err(ResolveError::UnsupportedExpr {
                        expr_type: "null comparison operator".to_string(),
                    });
                }

                let left = resolve_null_comparable_path_value_expr(
                    catalog,
                    source_object_type,
                    compare.left(),
                )?;
                return Ok(query_ir::Expr::IsNull(left));
            }

            if is_null_literal(compare.left()) {
                if compare.op() == query_ast::CompareOp::Ne {
                    let right = resolve_null_comparable_path_value_expr(
                        catalog,
                        source_object_type,
                        compare.right(),
                    )?;
                    return Ok(query_ir::Expr::IsNotNull(right));
                }

                if compare.op() != query_ast::CompareOp::Eq {
                    return Err(ResolveError::UnsupportedExpr {
                        expr_type: "null comparison operator".to_string(),
                    });
                }

                let right = resolve_null_comparable_path_value_expr(
                    catalog,
                    source_object_type,
                    compare.right(),
                )?;
                return Ok(query_ir::Expr::IsNull(right));
            }

            let left = resolve_typed_value_expr(catalog, source_object_type, compare.left())?;
            let right = resolve_typed_value_expr(catalog, source_object_type, compare.right())?;

            ensure_compatible_comparison(&left.source, &right.source)?;

            Ok(query_ir::Expr::Compare(query_ir::CompareExpr::new(
                left.value,
                resolve_compare_op(compare.op()),
                right.value,
            )))
        }
        query_ast::Expr::And(left, right) => Ok(query_ir::Expr::And(
            Box::new(resolve_expr(catalog, source_object_type, left)?),
            Box::new(resolve_expr(catalog, source_object_type, right)?),
        )),
        query_ast::Expr::Or(left, right) => Ok(query_ir::Expr::Or(
            Box::new(resolve_expr(catalog, source_object_type, left)?),
            Box::new(resolve_expr(catalog, source_object_type, right)?),
        )),
        query_ast::Expr::Not(inner) => Ok(query_ir::Expr::Not(Box::new(resolve_expr(
            catalog,
            source_object_type,
            inner,
        )?))),
        query_ast::Expr::In(in_expr) => {
            let left = resolve_typed_value_expr(catalog, source_object_type, in_expr.left())?;
            let op = match in_expr.op() {
                query_ast::InOp::In => query_ir::InOp::In,
                query_ast::InOp::NotIn => query_ir::InOp::NotIn,
            };
            let right = resolve_membership_literals(in_expr.right())?;

            for item in &right {
                ensure_compatible_membership_item(&left.source, item.scalar_type)?;
            }

            let right = right.into_iter().map(|literal| literal.value).collect();

            Ok(query_ir::Expr::In(query_ir::InExpr::new(
                left.value, op, right,
            )))
        }
        query_ast::Expr::Literal(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "literal".to_string(),
        }),
        query_ast::Expr::Path(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "path".to_string(),
        }),
    }
}

fn is_null_literal(expr: &query_ast::Expr) -> bool {
    matches!(expr, query_ast::Expr::Literal(query_ast::Literal::Null))
}

fn resolve_compare_op(op: query_ast::CompareOp) -> query_ir::CompareOp {
    match op {
        query_ast::CompareOp::Eq => query_ir::CompareOp::Eq,
        query_ast::CompareOp::Ne => query_ir::CompareOp::Ne,
        query_ast::CompareOp::Lt => query_ir::CompareOp::Lt,
        query_ast::CompareOp::Le => query_ir::CompareOp::Le,
        query_ast::CompareOp::Gt => query_ir::CompareOp::Gt,
        query_ast::CompareOp::Ge => query_ir::CompareOp::Ge,
    }
}

fn resolve_path_value_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    match resolve_value_expr(catalog, source_object_type, expr)? {
        query_ir::ValueExpr::Path(path) => Ok(query_ir::ValueExpr::Path(path)),
        query_ir::ValueExpr::Literal(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "null comparison literal".to_string(),
        }),
    }
}

fn resolve_null_comparable_path_value_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    let value = resolve_path_value_expr(catalog, source_object_type, expr)?;
    let query_ir::ValueExpr::Path(path) = &value else {
        unreachable!("resolve_path_value_expr should only return path values");
    };

    match path.result_cardinality() {
        schema_model::Cardinality::Optional => Ok(value),
        cardinality => Err(ResolveError::NullComparisonOnNonOptionalPath {
            cardinality: cardinality_name(cardinality).to_string(),
        }),
    }
}

fn resolve_value_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    resolve_typed_value_expr(catalog, source_object_type, expr).map(|typed| typed.value)
}

fn resolve_typed_value_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<TypedValueExpr, ResolveError> {
    match expr {
        query_ast::Expr::Path(path) => resolve_typed_path_expr(catalog, source_object_type, path),
        query_ast::Expr::Literal(literal) => resolve_typed_literal_expr(literal),
        query_ast::Expr::Compare(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "comparison value".to_string(),
        }),
        query_ast::Expr::And(_, _)
        | query_ast::Expr::Or(_, _)
        | query_ast::Expr::Not(_)
        | query_ast::Expr::In(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "boolean value".to_string(),
        }),
    }
}

fn resolve_path_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    path: &Path,
) -> Result<query_ir::ValueExpr, ResolveError> {
    resolve_typed_path_expr(catalog, source_object_type, path).map(|typed| typed.value)
}

fn resolve_typed_path_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    path: &Path,
) -> Result<TypedValueExpr, ResolveError> {
    let steps = path.steps();
    let mut current_object_type = source_object_type.clone();
    let mut resolved_steps = Vec::new();
    let mut terminal_scalar_type = None;

    for (index, step) in steps.iter().enumerate() {
        let is_last = index == steps.len() - 1;
        let field_name = step.field_name();

        let field = catalog
            .find_field(current_object_type.name(), field_name)
            .ok_or_else(|| ResolveError::UnknownField {
                object_type: current_object_type.name().to_string(),
                field: field_name.to_string(),
            })?;

        let field_ref = catalog
            .find_field_ref(current_object_type.name(), field_name)
            .expect("field ref should exist for a field already found in the catalog");

        match field {
            schema_model::Field::Scalar(scalar) => {
                if !is_last {
                    return Err(ResolveError::UnsupportedPath);
                }

                resolved_steps.push(query_ir::ResolvedPathStep::scalar(
                    field_ref,
                    field.cardinality(),
                ));
                terminal_scalar_type = Some(scalar.scalar_type());
            }
            schema_model::Field::Link(link) => {
                if is_last {
                    return Err(ResolveError::UnsupportedPath);
                }

                let target_object_type = catalog
                    .find_type_ref(link.target_type_name())
                    .ok_or_else(|| ResolveError::UnknownObjectType {
                        name: link.target_type_name().to_string(),
                    })?;

                resolved_steps.push(query_ir::ResolvedPathStep::link(
                    field_ref,
                    target_object_type.clone(),
                    field.cardinality(),
                ));

                current_object_type = target_object_type;
            }
        }
    }

    let resolved_path = query_ir::ResolvedPath::try_new(source_object_type.clone(), resolved_steps)
        .map_err(|_| ResolveError::UnsupportedPath)?;

    let scalar_type = terminal_scalar_type.ok_or(ResolveError::UnsupportedPath)?;

    Ok(TypedValueExpr {
        value: query_ir::ValueExpr::Path(resolved_path),
        source: ValueSource::Path(scalar_type),
    })
}

fn resolve_typed_literal_expr(
    literal: &query_ast::Literal,
) -> Result<TypedValueExpr, ResolveError> {
    match literal {
        query_ast::Literal::String(value) => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::String(value.clone())),
            source: ValueSource::Literal(schema_model::ScalarType::Str),
        }),
        query_ast::Literal::Int64(value) => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Int64(*value)),
            source: ValueSource::Literal(schema_model::ScalarType::Int64),
        }),
        query_ast::Literal::Bool(value) => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Bool(*value)),
            source: ValueSource::Literal(schema_model::ScalarType::Bool),
        }),
        query_ast::Literal::Null => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Null),
            source: ValueSource::Literal(schema_model::ScalarType::Str),
        }),
        _ => Err(ResolveError::UnsupportedLiteral {
            literal: format!("{literal:?}"),
        }),
    }
}

fn resolve_membership_literals(
    exprs: &[query_ast::Expr],
) -> Result<Vec<TypedLiteral>, ResolveError> {
    if exprs.is_empty() {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "empty membership list".to_string(),
        });
    }

    exprs.iter().map(resolve_membership_literal).collect()
}

fn resolve_membership_literal(expr: &query_ast::Expr) -> Result<TypedLiteral, ResolveError> {
    let query_ast::Expr::Literal(literal) = expr else {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "membership list item".to_string(),
        });
    };

    match literal {
        query_ast::Literal::String(value) => Ok(TypedLiteral {
            value: query_ir::Literal::String(value.clone()),
            scalar_type: schema_model::ScalarType::Str,
        }),
        query_ast::Literal::Int64(value) => Ok(TypedLiteral {
            value: query_ir::Literal::Int64(*value),
            scalar_type: schema_model::ScalarType::Int64,
        }),
        query_ast::Literal::Bool(value) => Ok(TypedLiteral {
            value: query_ir::Literal::Bool(*value),
            scalar_type: schema_model::ScalarType::Bool,
        }),
        query_ast::Literal::Null => Err(ResolveError::UnsupportedExpr {
            expr_type: "null membership item".to_string(),
        }),
        _ => Err(ResolveError::UnsupportedLiteral {
            literal: format!("{literal:?}"),
        }),
    }
}

fn ensure_compatible_comparison(
    left: &ValueSource,
    right: &ValueSource,
) -> Result<(), ResolveError> {
    if value_sources_are_compatible(*left, *right) {
        return Ok(());
    }

    Err(type_mismatch_error(
        source_scalar_type(*left),
        source_scalar_type(*right),
    ))
}

fn ensure_compatible_membership_item(
    left: &ValueSource,
    item_type: schema_model::ScalarType,
) -> Result<(), ResolveError> {
    let item = ValueSource::Literal(item_type);

    if value_sources_are_compatible(*left, item) {
        return Ok(());
    }

    Err(type_mismatch_error(source_scalar_type(*left), item_type))
}

fn value_sources_are_compatible(left: ValueSource, right: ValueSource) -> bool {
    match (left, right) {
        (ValueSource::Path(left), ValueSource::Path(right))
        | (ValueSource::Literal(left), ValueSource::Literal(right)) => left == right,
        (ValueSource::Path(expected), ValueSource::Literal(actual))
        | (ValueSource::Literal(actual), ValueSource::Path(expected)) => {
            literal_type_matches_scalar(expected, actual)
        }
    }
}

fn literal_type_matches_scalar(
    expected: schema_model::ScalarType,
    actual: schema_model::ScalarType,
) -> bool {
    expected == actual
        || matches!(
            (expected, actual),
            (
                schema_model::ScalarType::Uuid,
                schema_model::ScalarType::Str
            ) | (
                schema_model::ScalarType::DateTime,
                schema_model::ScalarType::Str
            )
        )
}

fn source_scalar_type(source: ValueSource) -> schema_model::ScalarType {
    match source {
        ValueSource::Path(scalar_type) | ValueSource::Literal(scalar_type) => scalar_type,
    }
}

fn type_mismatch_error(
    expected: schema_model::ScalarType,
    actual: schema_model::ScalarType,
) -> ResolveError {
    ResolveError::IncompatibleOperandTypes {
        expected: scalar_type_name(expected).to_string(),
        actual: scalar_type_name(actual).to_string(),
    }
}

fn scalar_type_name(scalar_type: schema_model::ScalarType) -> &'static str {
    match scalar_type {
        schema_model::ScalarType::Str => "str",
        schema_model::ScalarType::Int64 => "int64",
        schema_model::ScalarType::Float64 => "float64",
        schema_model::ScalarType::Bool => "bool",
        schema_model::ScalarType::Uuid => "uuid",
        schema_model::ScalarType::DateTime => "datetime",
    }
}

fn cardinality_name(cardinality: schema_model::Cardinality) -> &'static str {
    match cardinality {
        schema_model::Cardinality::Optional => "optional",
        schema_model::Cardinality::Required => "required",
        schema_model::Cardinality::Many => "many",
    }
}

fn resolve_order_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    order: &query_ast::OrderExpr,
) -> Result<query_ir::OrderExpr, ResolveError> {
    let value = resolve_path_expr(catalog, source_object_type, order.path())?;
    let direction = match order.direction() {
        query_ast::OrderDirection::Asc => query_ir::OrderDirection::Asc,
        query_ast::OrderDirection::Desc => query_ir::OrderDirection::Desc,
    };

    Ok(query_ir::OrderExpr::new(value, direction))
}

/// Semantic errors reported by the resolver.
///
/// These errors are intentionally independent from parser and SQLite errors so
/// callers can distinguish syntax failures from schema or type failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownObjectType { name: String },
    UnknownField { object_type: String, field: String },
    NestedShapeOnScalarField { object_type: String, field: String },
    MissingShapeOnLinkField { object_type: String, field: String },
    UnsupportedPath,
    UnsupportedExpr { expr_type: String },
    UnsupportedLiteral { literal: String },
    IncompatibleOperandTypes { expected: String, actual: String },
    NonNumericArithmeticOperand { actual: String },
    NullComparisonOnNonOptionalPath { cardinality: String },
}

#[cfg(test)]
mod tests;
