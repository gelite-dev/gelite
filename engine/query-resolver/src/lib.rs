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
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use query_ast::Path;

#[derive(Debug, Clone, PartialEq)]
struct TypedValueExpr {
    value: query_ir::ValueExpr,
    source: ValueSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueSource {
    Path(schema_model::ScalarType),
    Literal(schema_model::ScalarType),
    Computed(schema_model::ScalarType),
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
        resolve_shape(catalog, root_object_type, query.shape())?,
        filter,
        order_by,
        query.limit(),
        query.offset(),
    ))
}

fn resolve_shape(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: schema_model::ObjectTypeRef,
    shape: &query_ast::Shape,
) -> Result<query_ir::ResolvedShape, ResolveError> {
    let mut items = Vec::new();
    let mut output_names = Vec::<String>::new();

    for item in shape.items() {
        let resolved_item = resolve_shape_item(catalog, &source_object_type, item)?;
        let output_name = resolved_item.output_name().to_string();

        if output_names.iter().any(|name| name == &output_name) {
            return Err(ResolveError::DuplicateOutputName { name: output_name });
        }

        output_names.push(output_name);
        items.push(resolved_item);
    }

    Ok(query_ir::ResolvedShape::with_items(
        source_object_type,
        items,
    ))
}

fn resolve_shape_item(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    item: &query_ast::ShapeItem,
) -> Result<query_ir::ResolvedShapeItem, ResolveError> {
    match item.kind() {
        query_ast::ShapeItemKind::Field(field) => {
            resolve_shape_field(catalog, source_object_type, field)
                .map(query_ir::ResolvedShapeItem::Field)
        }
        query_ast::ShapeItemKind::Computed(computed) => {
            resolve_computed_shape_item(catalog, source_object_type, computed)
                .map(query_ir::ResolvedShapeItem::Computed)
        }
    }
}

fn resolve_shape_field(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    item: &query_ast::ShapeField,
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

            let resolved_child_shape = resolve_shape(catalog, target_object_type, child_shape)?;

            Ok(query_ir::ResolvedShapeField::new(
                field_name,
                field_ref,
                field.cardinality(),
                Some(resolved_child_shape),
            ))
        }
    }
}

fn resolve_computed_shape_item(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    item: &query_ast::ComputedShapeItem,
) -> Result<query_ir::ResolvedComputedField, ResolveError> {
    if !computed_projection_expr_is_supported(item.expr()) {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "computed projection".to_string(),
        });
    }

    let typed = resolve_typed_value_expr(catalog, source_object_type, item.expr())?;

    if !value_expr_contains_path(&typed.value) {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "computed projection".to_string(),
        });
    }

    let cardinality = value_expr_cardinality(&typed.value)?;
    let scalar_type = source_scalar_type(typed.source);

    Ok(query_ir::ResolvedComputedField::new(
        item.output_name(),
        typed.value,
        scalar_type,
        cardinality,
    ))
}

fn computed_projection_expr_is_supported(expr: &query_ast::Expr) -> bool {
    matches!(
        expr,
        query_ast::Expr::Arithmetic(_)
            | query_ast::Expr::UnaryArithmetic(_)
            | query_ast::Expr::FunctionCall(_)
    )
}

fn resolve_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<query_ir::Expr, ResolveError> {
    match expr {
        query_ast::Expr::Compare(compare) => {
            resolve_compare_expr(catalog, source_object_type, compare)
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
        query_ast::Expr::In(in_expr) => resolve_in_expr(catalog, source_object_type, in_expr),
        query_ast::Expr::Literal(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "literal".to_string(),
        }),
        query_ast::Expr::Path(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "path".to_string(),
        }),
        query_ast::Expr::Arithmetic(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "arithmetic value".to_string(),
        }),
        query_ast::Expr::UnaryArithmetic(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "unary arithmetic value".to_string(),
        }),
        query_ast::Expr::FunctionCall(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "function call".to_string(),
        }),
    }
}

fn resolve_compare_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    compare: &query_ast::CompareExpr,
) -> Result<query_ir::Expr, ResolveError> {
    if let Some(expr) = resolve_null_compare_expr(
        catalog,
        source_object_type,
        compare.left(),
        compare.op(),
        compare.right(),
    )? {
        return Ok(expr);
    }

    if let Some(expr) = resolve_null_compare_expr(
        catalog,
        source_object_type,
        compare.right(),
        compare.op(),
        compare.left(),
    )? {
        return Ok(expr);
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

fn resolve_null_compare_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    null_candidate: &query_ast::Expr,
    op: query_ast::CompareOp,
    compared_expr: &query_ast::Expr,
) -> Result<Option<query_ir::Expr>, ResolveError> {
    if !is_null_literal(null_candidate) {
        return Ok(None);
    }

    match op {
        query_ast::CompareOp::Eq => {
            let value = resolve_null_comparable_path_value_expr(
                catalog,
                source_object_type,
                compared_expr,
            )?;
            Ok(Some(query_ir::Expr::IsNull(value)))
        }
        query_ast::CompareOp::Ne => {
            let value = resolve_null_comparable_path_value_expr(
                catalog,
                source_object_type,
                compared_expr,
            )?;
            Ok(Some(query_ir::Expr::IsNotNull(value)))
        }
        _ => Err(ResolveError::UnsupportedExpr {
            expr_type: "null comparison operator".to_string(),
        }),
    }
}

fn resolve_in_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    in_expr: &query_ast::InExpr,
) -> Result<query_ir::Expr, ResolveError> {
    let left = resolve_typed_value_expr(catalog, source_object_type, in_expr.left())?;
    let op = resolve_in_op(in_expr.op());
    let right = resolve_membership_items(in_expr.right())?;

    for item in &right {
        ensure_compatible_membership_item(&left.source, &item.source)?;
    }

    let right = right.into_iter().map(|item| item.value).collect();

    Ok(query_ir::Expr::In(query_ir::InExpr::new(
        left.value, op, right,
    )))
}

fn resolve_in_op(op: query_ast::InOp) -> query_ir::InOp {
    match op {
        query_ast::InOp::In => query_ir::InOp::In,
        query_ast::InOp::NotIn => query_ir::InOp::NotIn,
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
        query_ir::ValueExpr::Arithmetic(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "null comparison value".to_string(),
        }),
        query_ir::ValueExpr::UnaryArithmetic(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "null comparison value".to_string(),
        }),
        query_ir::ValueExpr::Cast(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "null comparison value".to_string(),
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
        query_ast::Expr::Arithmetic(arithmetic) => {
            resolve_typed_arithmetic_expr(catalog, source_object_type, arithmetic)
        }
        query_ast::Expr::UnaryArithmetic(unary) => {
            resolve_typed_unary_arithmetic_expr(catalog, source_object_type, unary)
        }
        query_ast::Expr::FunctionCall(function) => {
            resolve_typed_function_call_expr(catalog, source_object_type, function)
        }
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

fn resolve_typed_function_call_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    function: &query_ast::FunctionCallExpr,
) -> Result<TypedValueExpr, ResolveError> {
    let target_type = numeric_cast_target(function.name())?;

    if function.args().len() != 1 {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "numeric cast arity".to_string(),
        });
    }

    let operand = resolve_typed_value_expr(catalog, source_object_type, &function.args()[0])?;
    let operand_type = source_scalar_type(operand.source);

    ensure_numeric_arithmetic_operand(operand_type)?;
    if value_expr_cardinality(&operand.value)? == schema_model::Cardinality::Many {
        return Err(ResolveError::UnsupportedPath);
    }

    Ok(TypedValueExpr {
        value: query_ir::ValueExpr::Cast(query_ir::CastExpr::new(operand.value, target_type)),
        source: ValueSource::Computed(target_type),
    })
}

fn numeric_cast_target(name: &str) -> Result<schema_model::ScalarType, ResolveError> {
    match name {
        "i64" => Ok(schema_model::ScalarType::Int64),
        "f64" => Ok(schema_model::ScalarType::Float64),
        _ => Err(ResolveError::UnsupportedExpr {
            expr_type: "function call".to_string(),
        }),
    }
}

fn resolve_typed_arithmetic_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    arithmetic: &query_ast::ArithmeticExpr,
) -> Result<TypedValueExpr, ResolveError> {
    let left = resolve_typed_value_expr(catalog, source_object_type, arithmetic.left())?;
    let right = resolve_typed_value_expr(catalog, source_object_type, arithmetic.right())?;

    let scalar_type =
        ensure_compatible_arithmetic_operands(arithmetic.op(), &left.source, &right.source)?;

    Ok(TypedValueExpr {
        value: query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            left.value,
            resolve_arithmetic_op(arithmetic.op()),
            right.value,
            scalar_type,
        )),
        source: ValueSource::Computed(scalar_type),
    })
}

fn resolve_arithmetic_op(op: query_ast::ArithmeticOp) -> query_ir::ArithmeticOp {
    match op {
        query_ast::ArithmeticOp::Add => query_ir::ArithmeticOp::Add,
        query_ast::ArithmeticOp::Sub => query_ir::ArithmeticOp::Sub,
        query_ast::ArithmeticOp::Mul => query_ir::ArithmeticOp::Mul,
        query_ast::ArithmeticOp::Div => query_ir::ArithmeticOp::Div,
        query_ast::ArithmeticOp::Mod => query_ir::ArithmeticOp::Mod,
    }
}

fn resolve_typed_unary_arithmetic_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    unary: &query_ast::UnaryArithmeticExpr,
) -> Result<TypedValueExpr, ResolveError> {
    let operand = resolve_typed_value_expr(catalog, source_object_type, unary.operand())?;
    let scalar_type = source_scalar_type(operand.source);

    ensure_numeric_arithmetic_operand(scalar_type)?;
    if value_expr_cardinality(&operand.value)? == schema_model::Cardinality::Many {
        return Err(ResolveError::UnsupportedPath);
    }

    Ok(TypedValueExpr {
        value: query_ir::ValueExpr::UnaryArithmetic(query_ir::UnaryArithmeticExpr::new(
            resolve_unary_arithmetic_op(unary.op()),
            operand.value,
            scalar_type,
        )),
        source: ValueSource::Computed(scalar_type),
    })
}

fn resolve_unary_arithmetic_op(op: query_ast::UnaryArithmeticOp) -> query_ir::UnaryArithmeticOp {
    match op {
        query_ast::UnaryArithmeticOp::Plus => query_ir::UnaryArithmeticOp::Plus,
        query_ast::UnaryArithmeticOp::Minus => query_ir::UnaryArithmeticOp::Minus,
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
        query_ast::Literal::Float64(value) => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Float64(*value)),
            source: ValueSource::Literal(schema_model::ScalarType::Float64),
        }),
        query_ast::Literal::Bool(value) => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Bool(*value)),
            source: ValueSource::Literal(schema_model::ScalarType::Bool),
        }),
        query_ast::Literal::Null => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Null),
            source: ValueSource::Literal(schema_model::ScalarType::Str),
        }),
    }
}

fn resolve_membership_items(
    exprs: &[query_ast::Expr],
) -> Result<Vec<TypedValueExpr>, ResolveError> {
    if exprs.is_empty() {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "empty membership list".to_string(),
        });
    }

    exprs.iter().map(resolve_membership_item).collect()
}

fn resolve_membership_item(expr: &query_ast::Expr) -> Result<TypedValueExpr, ResolveError> {
    match expr {
        query_ast::Expr::Literal(literal) => resolve_membership_literal(literal),
        query_ast::Expr::Arithmetic(arithmetic) => resolve_membership_arithmetic(arithmetic),
        query_ast::Expr::UnaryArithmetic(unary) => resolve_membership_unary_arithmetic(unary),
        query_ast::Expr::FunctionCall(function) => resolve_membership_function_call(function),
        query_ast::Expr::Path(_)
        | query_ast::Expr::Compare(_)
        | query_ast::Expr::And(_, _)
        | query_ast::Expr::Or(_, _)
        | query_ast::Expr::Not(_)
        | query_ast::Expr::In(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "membership list item".to_string(),
        }),
    }
}

fn resolve_membership_function_call(
    function: &query_ast::FunctionCallExpr,
) -> Result<TypedValueExpr, ResolveError> {
    let target_type = numeric_cast_target(function.name())?;

    if function.args().len() != 1 {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "numeric cast arity".to_string(),
        });
    }

    let operand = resolve_membership_item(&function.args()[0])?;
    let operand_type = source_scalar_type(operand.source);

    ensure_numeric_arithmetic_operand(operand_type)?;

    Ok(TypedValueExpr {
        value: query_ir::ValueExpr::Cast(query_ir::CastExpr::new(operand.value, target_type)),
        source: ValueSource::Computed(target_type),
    })
}

fn resolve_membership_unary_arithmetic(
    unary: &query_ast::UnaryArithmeticExpr,
) -> Result<TypedValueExpr, ResolveError> {
    let operand = resolve_membership_item(unary.operand())?;
    let scalar_type = source_scalar_type(operand.source);

    ensure_numeric_arithmetic_operand(scalar_type)?;

    Ok(TypedValueExpr {
        value: query_ir::ValueExpr::UnaryArithmetic(query_ir::UnaryArithmeticExpr::new(
            resolve_unary_arithmetic_op(unary.op()),
            operand.value,
            scalar_type,
        )),
        source: ValueSource::Computed(scalar_type),
    })
}

fn resolve_membership_literal(
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
        query_ast::Literal::Float64(value) => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Float64(*value)),
            source: ValueSource::Literal(schema_model::ScalarType::Float64),
        }),
        query_ast::Literal::Bool(value) => Ok(TypedValueExpr {
            value: query_ir::ValueExpr::Literal(query_ir::Literal::Bool(*value)),
            source: ValueSource::Literal(schema_model::ScalarType::Bool),
        }),
        query_ast::Literal::Null => Err(ResolveError::UnsupportedExpr {
            expr_type: "null membership item".to_string(),
        }),
    }
}

fn resolve_membership_arithmetic(
    arithmetic: &query_ast::ArithmeticExpr,
) -> Result<TypedValueExpr, ResolveError> {
    let left = resolve_membership_item(arithmetic.left())?;
    let right = resolve_membership_item(arithmetic.right())?;

    let scalar_type = ensure_compatible_arithmetic_operand_types(
        arithmetic.op(),
        source_scalar_type(left.source),
        source_scalar_type(right.source),
    )?;

    Ok(TypedValueExpr {
        value: query_ir::ValueExpr::Arithmetic(query_ir::ArithmeticExpr::new(
            left.value,
            resolve_arithmetic_op(arithmetic.op()),
            right.value,
            scalar_type,
        )),
        source: ValueSource::Computed(scalar_type),
    })
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
    item: &ValueSource,
) -> Result<(), ResolveError> {
    if value_sources_are_compatible(*left, *item) {
        return Ok(());
    }

    Err(type_mismatch_error(
        source_scalar_type(*left),
        source_scalar_type(*item),
    ))
}

fn ensure_compatible_arithmetic_operands(
    op: query_ast::ArithmeticOp,
    left: &ValueSource,
    right: &ValueSource,
) -> Result<schema_model::ScalarType, ResolveError> {
    let left = source_scalar_type(*left);
    let right = source_scalar_type(*right);

    ensure_compatible_arithmetic_operand_types(op, left, right)
}

fn ensure_compatible_arithmetic_operand_types(
    op: query_ast::ArithmeticOp,
    left: schema_model::ScalarType,
    right: schema_model::ScalarType,
) -> Result<schema_model::ScalarType, ResolveError> {
    ensure_numeric_arithmetic_operand(left)?;
    ensure_numeric_arithmetic_operand(right)?;

    if left != right {
        return Err(type_mismatch_error(left, right));
    }

    if matches!(op, query_ast::ArithmeticOp::Mod) && left != schema_model::ScalarType::Int64 {
        return Err(type_mismatch_error(schema_model::ScalarType::Int64, left));
    }

    Ok(left)
}

fn ensure_numeric_arithmetic_operand(
    scalar_type: schema_model::ScalarType,
) -> Result<(), ResolveError> {
    if scalar_type_is_numeric(scalar_type) {
        return Ok(());
    }

    Err(ResolveError::NonNumericArithmeticOperand {
        actual: scalar_type_name(scalar_type).to_string(),
    })
}

fn value_sources_are_compatible(left: ValueSource, right: ValueSource) -> bool {
    match (left, right) {
        (ValueSource::Path(left), ValueSource::Path(right))
        | (ValueSource::Path(left), ValueSource::Computed(right))
        | (ValueSource::Computed(left), ValueSource::Path(right))
        | (ValueSource::Computed(left), ValueSource::Computed(right))
        | (ValueSource::Literal(left), ValueSource::Literal(right)) => left == right,
        (ValueSource::Path(expected), ValueSource::Literal(actual))
        | (ValueSource::Computed(expected), ValueSource::Literal(actual))
        | (ValueSource::Literal(actual), ValueSource::Path(expected))
        | (ValueSource::Literal(actual), ValueSource::Computed(expected)) => {
            literal_type_matches_scalar(expected, actual)
        }
    }
}

fn scalar_type_is_numeric(scalar_type: schema_model::ScalarType) -> bool {
    matches!(
        scalar_type,
        schema_model::ScalarType::Int64 | schema_model::ScalarType::Float64
    )
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
        ValueSource::Path(scalar_type)
        | ValueSource::Literal(scalar_type)
        | ValueSource::Computed(scalar_type) => scalar_type,
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

fn resolve_order_value_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    expr: &query_ast::Expr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    match expr {
        query_ast::Expr::Path(path) => {
            let value = resolve_path_expr(catalog, source_object_type, path)?;
            ensure_order_value_is_single_cardinality(&value)?;

            Ok(value)
        }
        query_ast::Expr::Arithmetic(arithmetic) => {
            resolve_order_arithmetic_expr(catalog, source_object_type, arithmetic)
        }
        query_ast::Expr::UnaryArithmetic(unary) => {
            resolve_order_unary_arithmetic_expr(catalog, source_object_type, unary)
        }
        query_ast::Expr::FunctionCall(function) => {
            resolve_order_function_call_expr(catalog, source_object_type, function)
        }
        query_ast::Expr::Literal(_) => Err(ResolveError::UnsupportedExpr {
            expr_type: "order value".to_string(),
        }),
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

fn resolve_order_function_call_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    function: &query_ast::FunctionCallExpr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    let typed = resolve_typed_function_call_expr(catalog, source_object_type, function)?;

    if !value_expr_contains_path(&typed.value) {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "order value".to_string(),
        });
    }
    ensure_order_value_is_single_cardinality(&typed.value)?;

    Ok(typed.value)
}

fn resolve_order_arithmetic_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    arithmetic: &query_ast::ArithmeticExpr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    let typed = resolve_typed_arithmetic_expr(catalog, source_object_type, arithmetic)?;

    if !value_expr_contains_path(&typed.value) {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "order value".to_string(),
        });
    }
    ensure_order_value_is_single_cardinality(&typed.value)?;

    Ok(typed.value)
}

fn resolve_order_unary_arithmetic_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    unary: &query_ast::UnaryArithmeticExpr,
) -> Result<query_ir::ValueExpr, ResolveError> {
    let typed = resolve_typed_unary_arithmetic_expr(catalog, source_object_type, unary)?;

    if !value_expr_contains_path(&typed.value) {
        return Err(ResolveError::UnsupportedExpr {
            expr_type: "order value".to_string(),
        });
    }
    ensure_order_value_is_single_cardinality(&typed.value)?;

    Ok(typed.value)
}

fn ensure_order_value_is_single_cardinality(
    value: &query_ir::ValueExpr,
) -> Result<(), ResolveError> {
    match value {
        query_ir::ValueExpr::Path(path) => match path.result_cardinality() {
            schema_model::Cardinality::Many => Err(ResolveError::UnsupportedPath),
            schema_model::Cardinality::Optional | schema_model::Cardinality::Required => Ok(()),
        },
        query_ir::ValueExpr::Literal(_) => Ok(()),
        query_ir::ValueExpr::Arithmetic(arithmetic) => {
            ensure_order_value_is_single_cardinality(arithmetic.left())?;
            ensure_order_value_is_single_cardinality(arithmetic.right())
        }
        query_ir::ValueExpr::UnaryArithmetic(unary) => {
            ensure_order_value_is_single_cardinality(unary.operand())
        }
        query_ir::ValueExpr::Cast(cast) => ensure_order_value_is_single_cardinality(cast.operand()),
    }
}

fn value_expr_cardinality(
    value: &query_ir::ValueExpr,
) -> Result<schema_model::Cardinality, ResolveError> {
    fn combine(
        left: schema_model::Cardinality,
        right: schema_model::Cardinality,
    ) -> schema_model::Cardinality {
        match (left, right) {
            (schema_model::Cardinality::Many, _) | (_, schema_model::Cardinality::Many) => {
                schema_model::Cardinality::Many
            }
            (schema_model::Cardinality::Optional, _) | (_, schema_model::Cardinality::Optional) => {
                schema_model::Cardinality::Optional
            }
            (schema_model::Cardinality::Required, schema_model::Cardinality::Required) => {
                schema_model::Cardinality::Required
            }
        }
    }

    match value {
        query_ir::ValueExpr::Path(path) => Ok(path.result_cardinality()),
        query_ir::ValueExpr::Literal(_) => Ok(schema_model::Cardinality::Required),
        query_ir::ValueExpr::Arithmetic(arithmetic) => {
            let left = value_expr_cardinality(arithmetic.left())?;
            let right = value_expr_cardinality(arithmetic.right())?;
            let mut cardinality = combine(left, right);

            if cardinality == schema_model::Cardinality::Required
                && arithmetic_can_return_null(arithmetic)
            {
                cardinality = schema_model::Cardinality::Optional;
            }

            match cardinality {
                schema_model::Cardinality::Many => Err(ResolveError::UnsupportedPath),
                schema_model::Cardinality::Optional | schema_model::Cardinality::Required => {
                    Ok(cardinality)
                }
            }
        }
        query_ir::ValueExpr::UnaryArithmetic(unary) => {
            let cardinality = value_expr_cardinality(unary.operand())?;

            match cardinality {
                schema_model::Cardinality::Many => Err(ResolveError::UnsupportedPath),
                schema_model::Cardinality::Optional | schema_model::Cardinality::Required => {
                    Ok(cardinality)
                }
            }
        }
        query_ir::ValueExpr::Cast(cast) => {
            let cardinality = value_expr_cardinality(cast.operand())?;

            match cardinality {
                schema_model::Cardinality::Many => Err(ResolveError::UnsupportedPath),
                schema_model::Cardinality::Optional | schema_model::Cardinality::Required => {
                    Ok(cardinality)
                }
            }
        }
    }
}

fn value_expr_contains_path(value: &query_ir::ValueExpr) -> bool {
    match value {
        query_ir::ValueExpr::Path(_) => true,
        query_ir::ValueExpr::Literal(_) => false,
        query_ir::ValueExpr::Arithmetic(arithmetic) => {
            value_expr_contains_path(arithmetic.left())
                || value_expr_contains_path(arithmetic.right())
        }
        query_ir::ValueExpr::UnaryArithmetic(unary) => value_expr_contains_path(unary.operand()),
        query_ir::ValueExpr::Cast(cast) => value_expr_contains_path(cast.operand()),
    }
}

fn arithmetic_can_return_null(arithmetic: &query_ir::ArithmeticExpr) -> bool {
    matches!(
        arithmetic.op(),
        query_ir::ArithmeticOp::Div | query_ir::ArithmeticOp::Mod
    ) && !is_nonzero_numeric_literal(arithmetic.right())
}

fn is_nonzero_numeric_literal(value: &query_ir::ValueExpr) -> bool {
    match value {
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(value)) => *value != 0,
        query_ir::ValueExpr::Literal(query_ir::Literal::Float64(value)) => *value != 0.0,
        query_ir::ValueExpr::UnaryArithmetic(unary) => is_nonzero_numeric_literal(unary.operand()),
        query_ir::ValueExpr::Cast(_) => false,
        _ => false,
    }
}

fn resolve_order_expr(
    catalog: &schema_model::SchemaCatalog,
    source_object_type: &schema_model::ObjectTypeRef,
    order: &query_ast::OrderExpr,
) -> Result<query_ir::OrderExpr, ResolveError> {
    let value = resolve_order_value_expr(catalog, source_object_type, order.expr())?;

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
    DuplicateOutputName { name: String },
}

#[cfg(test)]
mod tests;
