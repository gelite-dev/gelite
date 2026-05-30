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
                let left = resolve_path_value_expr(catalog, source_object_type, compare.left())?;
                return Ok(query_ir::Expr::IsNull(left));
            }

            if is_null_literal(compare.left()) {
                let right = resolve_path_value_expr(catalog, source_object_type, compare.right())?;
                return Ok(query_ir::Expr::IsNull(right));
            }

            let left = resolve_value_expr(catalog, source_object_type, compare.left())?;
            let right = resolve_value_expr(catalog, source_object_type, compare.right())?;

            Ok(query_ir::Expr::Compare(query_ir::CompareExpr::new(
                left,
                query_ir::CompareOp::Eq,
                right,
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

fn resolve_value_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    match expr {
        query_ast::Expr::Path(path) => resolve_path_expr(catalog, source_object_type, path),
        query_ast::Expr::Literal(literal) => resolve_literal_expr(literal),
        query_ast::Expr::Compare(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "comparison value".to_string(),
        }),
        query_ast::Expr::And(_, _) | query_ast::Expr::Or(_, _) | query_ast::Expr::Not(_) => {
            Err(ResolveError::UnsupportedExpr {
                expr_type: "boolean value".to_string(),
            })
        }
    }
}

fn resolve_path_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    path: &Path,
) -> Result<query_ir::ValueExpr, ResolveError> {
    let steps = path.steps();
    let mut current_object_type = source_object_type.clone();
    let mut resolved_steps = Vec::new();

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
            schema_model::Field::Scalar(_) => {
                if !is_last {
                    return Err(ResolveError::UnsupportedPath);
                }

                resolved_steps.push(query_ir::ResolvedPathStep::scalar(
                    field_ref,
                    field.cardinality(),
                ));
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

    Ok(query_ir::ValueExpr::Path(resolved_path))
}

fn resolve_literal_expr(literal: &query_ast::Literal) -> Result<query_ir::ValueExpr, ResolveError> {
    match literal {
        query_ast::Literal::String(value) => Ok(query_ir::ValueExpr::Literal(
            query_ir::Literal::String(value.clone()),
        )),
        _ => Err(ResolveError::UnsupportedLiteral {
            literal: format!("{literal:?}"),
        }),
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
}

#[cfg(test)]
mod tests;
