mod fixtures;

use crate::{ResolveError, resolve_select};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;
use fixtures::{
    filter_eq_bool, filter_eq_int, filter_eq_null, filter_eq_string, filter_in_bools,
    filter_in_empty, filter_in_ints, filter_in_null, filter_in_path_item, filter_in_strings,
    filter_not_in_strings, filter_null_eq, post_only_catalog, post_with_author_catalog,
    post_with_scalar_fields_catalog, post_with_title_catalog,
};
use query_ast::{Expr, Path, PathStep, SelectQuery, Shape, ShapeItem};

#[test]
fn resolves_select_root_object_type() {
    let query = SelectQuery::new("Post", Shape::new(vec![]), None, vec![], None, None);
    let catalog = post_only_catalog();
    assert_eq!(query.root_type_name(), "Post");

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    assert_eq!(resolved.root_object_type().name(), "Post");
}

#[test]
fn rejects_unknown_root_object_type() {
    let catalog = post_only_catalog();
    let query = SelectQuery::new("Book", Shape::new(vec![]), None, vec![], None, None);

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnknownObjectType {
            name: "Book".to_string()
        })
    );
}

#[test]
fn resolves_scalar_shape_field() {
    let catalog = post_with_title_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "title");
    assert_eq!(fields[0].field().owner_object_type().name(), "Post");
    assert_eq!(fields[0].field().name(), "title");
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn rejects_unknown_shape_field() {
    let catalog = post_with_title_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("missing")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnknownField {
            object_type: "Post".to_string(),
            field: "missing".to_string(),
        })
    );
}

#[test]
fn resolves_implicit_id_shape_field() {
    let catalog = post_with_title_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "id");
    assert_eq!(fields[0].field().owner_object_type().name(), "Post");
    assert_eq!(fields[0].field().name(), "id");
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);
    assert!(fields[0].child_shape().is_none());
}

#[test]
fn rejects_nested_shape_on_scalar_field() {
    let catalog = post_with_title_catalog();

    let child_shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("name")]),
        None,
    )]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            Some(child_shape),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::NestedShapeOnScalarField {
            object_type: "Post".to_string(),
            field: "title".to_string(),
        })
    );
}

#[test]
fn resolves_link_shape_with_child_shape() {
    let catalog = post_with_author_catalog();

    let child_shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("name")]),
        None,
    )]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("author")]),
            Some(child_shape),
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].output_name(), "author");
    assert_eq!(fields[0].field().owner_object_type().name(), "Post");
    assert_eq!(fields[0].field().name(), "author");
    assert_eq!(fields[0].cardinality(), schema_model::Cardinality::Required);

    let child_shape = fields[0]
        .child_shape()
        .expect("link field should resolve child shape");

    assert_eq!(child_shape.source_object_type().name(), "User");
    assert_eq!(child_shape.fields().len(), 1);
    assert_eq!(
        child_shape.fields()[0].field().owner_object_type().name(),
        "User"
    );
    assert_eq!(child_shape.fields()[0].field().name(), "name");
    assert_eq!(
        child_shape.fields()[0].cardinality(),
        schema_model::Cardinality::Required
    );
    assert!(child_shape.fields()[0].child_shape().is_none());
}

#[test]
fn rejects_link_shape_without_child_shape() {
    let catalog = post_with_author_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("author")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::MissingShapeOnLinkField {
            object_type: "Post".to_string(),
            field: "author".to_string(),
        })
    );
}

#[test]
fn rejects_multi_step_shape_path() {
    let catalog = post_with_author_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("author"), PathStep::new("name")]),
            None,
        )]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(resolved, Err(ResolveError::UnsupportedPath));
}

#[test]
fn preserves_shape_field_order() {
    let catalog = post_with_author_catalog();

    let child_shape = Shape::new(vec![ShapeItem::new(
        Path::new(vec![PathStep::new("name")]),
        None,
    )]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![
            ShapeItem::new(Path::new(vec![PathStep::new("title")]), None),
            ShapeItem::new(Path::new(vec![PathStep::new("author")]), Some(child_shape)),
        ]),
        None,
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let fields = resolved.shape().fields();

    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].field().name(), "title");
    assert_eq!(fields[1].field().name(), "author");
}

#[test]
fn resolves_filter_compare_path_to_field_and_literal() {
    let catalog = post_with_title_catalog();

    let filter = filter_eq_string(&["title"], "Hello");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("filter left side should resolve to a path"),
    }

    assert_eq!(compare.op(), query_ir::CompareOp::Eq);

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => {
            assert_eq!(value, "Hello");
        }
        _ => panic!("filter right side should resolve to a literal"),
    }
}

#[test]
fn resolves_filter_compare_int_path_to_int_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_int(&["view_count"], 42);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(value)) => assert_eq!(*value, 42),
        _ => panic!("filter right side should resolve to an int64 literal"),
    }
}

#[test]
fn resolves_filter_compare_bool_path_to_bool_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_bool(&["published"], true);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::Bool(value)) => assert!(*value),
        _ => panic!("filter right side should resolve to a bool literal"),
    }
}

#[test]
fn rejects_filter_compare_string_path_to_int_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_int(&["title"], 42);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "str".to_string(),
            actual: "int64".to_string()
        })
    );
}

#[test]
fn rejects_filter_compare_bool_path_to_string_literal() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_eq_string(&["published"], "true");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "bool".to_string(),
            actual: "str".to_string()
        })
    );
}

#[test]
fn resolves_filter_compare_uuid_path_to_string_literal() {
    let catalog = post_with_title_catalog();

    let filter = filter_eq_string(&["id"], "01987211-d8f1-7b31-8b3e-f5043e6b08f0");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => {
            assert_eq!(value, "01987211-d8f1-7b31-8b3e-f5043e6b08f0");
        }
        _ => panic!("filter right side should resolve to a string literal"),
    }
}

#[test]
fn resolves_filter_compare_null_literal_to_is_null_expr() {
    let catalog = post_with_title_catalog();

    let filter = filter_eq_null(&["title"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::IsNull(value) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an is null expression");
    };

    match value {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("is null expression should reference a path"),
    }
}

#[test]
fn resolves_filter_compare_left_null_literal_to_is_null_expr() {
    let catalog = post_with_title_catalog();

    let filter = filter_null_eq(&["title"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::IsNull(value) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an is null expression");
    };

    match value {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("is null expression should reference a path"),
    }
}

#[test]
fn resolves_filter_in_literal_list_to_in_expr() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_strings(&["title"], &["Draft", "Published"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    match in_expr.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("in expression left side should be a path"),
    }

    assert_eq!(in_expr.op(), query_ir::InOp::In);
    assert_eq!(
        in_expr.right(),
        &[
            query_ir::Literal::String("Draft".to_string()),
            query_ir::Literal::String("Published".to_string())
        ]
    );
}

#[test]
fn resolves_filter_in_int_literal_list_to_in_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_ints(&["view_count"], &[1, 2]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("view_count")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(
        in_expr.right(),
        &[query_ir::Literal::Int64(1), query_ir::Literal::Int64(2)]
    );
}

#[test]
fn resolves_filter_in_bool_literal_list_to_in_expr() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_bools(&["published"], &[true, false]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(
        in_expr.right(),
        &[
            query_ir::Literal::Bool(true),
            query_ir::Literal::Bool(false)
        ]
    );
}

#[test]
fn rejects_filter_in_literal_list_with_incompatible_item() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_ints(&["title"], &[1]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "str".to_string(),
            actual: "int64".to_string()
        })
    );
}

#[test]
fn rejects_filter_in_bool_path_with_string_literal_item() {
    let catalog = post_with_scalar_fields_catalog();

    let filter = filter_in_strings(&["published"], &["true"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("published")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::IncompatibleOperandTypes {
            expected: "bool".to_string(),
            actual: "str".to_string()
        })
    );
}

#[test]
fn resolves_filter_in_uuid_path_with_string_literal_list() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_strings(
        &["id"],
        &[
            "01987211-d8f1-7b31-8b3e-f5043e6b08f0",
            "01987211-e162-7a3f-9934-7ab05658ef7f",
        ],
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("id")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(
        in_expr.right(),
        &[
            query_ir::Literal::String("01987211-d8f1-7b31-8b3e-f5043e6b08f0".to_string()),
            query_ir::Literal::String("01987211-e162-7a3f-9934-7ab05658ef7f".to_string())
        ]
    );
}

#[test]
fn resolves_filter_not_in_literal_list_to_not_in_expr() {
    let catalog = post_with_title_catalog();

    let filter = filter_not_in_strings(&["title"], &["Archived"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");
    let query_ir::Expr::In(in_expr) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an in expression");
    };

    assert_eq!(in_expr.op(), query_ir::InOp::NotIn);
}

#[test]
fn rejects_filter_in_empty_literal_list() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_empty(&["title"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "empty membership list".to_string()
        })
    );
}

#[test]
fn rejects_filter_in_null_literal_item() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_null(&["title"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "null membership item".to_string()
        })
    );
}

#[test]
fn rejects_filter_in_non_literal_item() {
    let catalog = post_with_title_catalog();

    let filter = filter_in_path_item(&["title"], &["title"]);

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnsupportedExpr {
            expr_type: "membership list item".to_string()
        })
    );
}

#[test]
fn resolves_filter_and_expression() {
    let catalog = post_with_title_catalog();

    let filter = Expr::And(
        Box::new(filter_eq_string(&["title"], "Hello")),
        Box::new(filter_eq_null(&["title"])),
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");

    let query_ir::Expr::And(left, right) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an and expression");
    };

    assert!(matches!(left.as_ref(), query_ir::Expr::Compare(_)));
    assert!(matches!(right.as_ref(), query_ir::Expr::IsNull(_)));
}

#[test]
fn resolves_filter_or_expression() {
    let catalog = post_with_title_catalog();

    let filter = Expr::Or(
        Box::new(filter_eq_string(&["title"], "Hello")),
        Box::new(filter_eq_null(&["title"])),
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");

    let query_ir::Expr::Or(left, right) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to an or expression");
    };

    assert!(matches!(left.as_ref(), query_ir::Expr::Compare(_)));
    assert!(matches!(right.as_ref(), query_ir::Expr::IsNull(_)));
}

#[test]
fn resolves_filter_not_expression() {
    let catalog = post_with_title_catalog();

    let filter = Expr::Not(Box::new(filter_eq_string(&["title"], "Hello")));

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolved");

    let query_ir::Expr::Not(inner) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a not expression");
    };

    assert!(matches!(inner.as_ref(), query_ir::Expr::Compare(_)));
}

#[test]
fn rejects_filter_path_with_unknown_field() {
    let catalog = post_with_title_catalog();

    let filter = filter_eq_string(&["missing"], "Hello");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(
        resolved,
        Err(ResolveError::UnknownField {
            object_type: "Post".to_string(),
            field: "missing".to_string(),
        })
    );
}

#[test]
fn rejects_filter_path_with_link_field() {
    let catalog = post_with_author_catalog();

    let filter = filter_eq_string(&["author"], "Sheri");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(resolved, Err(ResolveError::UnsupportedPath));
}

#[test]
fn resolves_order_path_to_resolved_path() {
    let catalog = post_with_title_catalog();

    let order = query_ast::OrderExpr::new(
        Path::new(vec![PathStep::new("title")]),
        query_ast::OrderDirection::Desc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Desc
    );

    match resolved.order_by()[0].value() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 1);
            assert_eq!(path.steps()[0].field().name(), "title");
        }
        query_ir::ValueExpr::Literal(_) => panic!("order by should resolve to a path"),
    }
}

#[test]
fn rejects_order_path_with_link_field() {
    let catalog = post_with_author_catalog();

    let order = query_ast::OrderExpr::new(
        Path::new(vec![PathStep::new("author")]),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query);

    assert_eq!(resolved, Err(ResolveError::UnsupportedPath));
}

#[test]
fn passes_limit_and_offset_through() {
    let catalog = post_with_title_catalog();

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![],
        Some(10),
        Some(20),
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.limit(), Some(10));
    assert_eq!(resolved.offset(), Some(20));
}

#[test]
fn resolves_filter_path_through_single_link_to_scalar_field() {
    let catalog = post_with_author_catalog();

    let filter = filter_eq_string(&["author", "name"], "Sheri");

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        Some(filter),
        vec![],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");
    let query_ir::Expr::Compare(compare) = resolved.filter().expect("filter should resolve") else {
        panic!("filter should resolve to a compare expression");
    };

    assert_eq!(compare.op(), query_ir::CompareOp::Eq);

    match compare.left() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 2);
            assert_eq!(
                path.result_cardinality(),
                schema_model::Cardinality::Required
            );

            let link_step = &path.steps()[0];
            assert_eq!(link_step.field().owner_object_type().name(), "Post");
            assert_eq!(link_step.field().name(), "author");
            assert_eq!(link_step.cardinality(), schema_model::Cardinality::Required);

            match link_step.kind() {
                query_ir::ResolvedPathStepKind::Link { target_object_type } => {
                    assert_eq!(target_object_type.name(), "User");
                }
                query_ir::ResolvedPathStepKind::Scalar => {
                    panic!("first path step should resolve to a link")
                }
            }

            let scalar_step = &path.steps()[1];
            assert_eq!(scalar_step.field().owner_object_type().name(), "User");
            assert_eq!(scalar_step.field().name(), "name");
            assert_eq!(
                scalar_step.cardinality(),
                schema_model::Cardinality::Required
            );

            match scalar_step.kind() {
                query_ir::ResolvedPathStepKind::Scalar => {}
                query_ir::ResolvedPathStepKind::Link { .. } => {
                    panic!("terminal path step should resolve to a scalar")
                }
            }
        }
        query_ir::ValueExpr::Literal(_) => panic!("filter left side should resolve to a path"),
    }

    match compare.right() {
        query_ir::ValueExpr::Literal(query_ir::Literal::String(value)) => {
            assert_eq!(value, "Sheri");
        }
        _ => panic!("filter right side should resolve to a string literal"),
    }
}

#[test]
fn resolves_order_path_through_single_link_to_scalar_field() {
    let catalog = post_with_author_catalog();

    let order = query_ast::OrderExpr::new(
        Path::new(vec![PathStep::new("author"), PathStep::new("name")]),
        query_ast::OrderDirection::Asc,
    );

    let query = SelectQuery::new(
        "Post",
        Shape::new(vec![ShapeItem::new(
            Path::new(vec![PathStep::new("title")]),
            None,
        )]),
        None,
        vec![order],
        None,
        None,
    );

    let resolved = resolve_select(&catalog, &query).expect("select query resolves");

    assert_eq!(resolved.order_by().len(), 1);
    assert_eq!(
        resolved.order_by()[0].direction(),
        query_ir::OrderDirection::Asc
    );

    match resolved.order_by()[0].value() {
        query_ir::ValueExpr::Path(path) => {
            assert_eq!(path.root_object_type().name(), "Post");
            assert_eq!(path.steps().len(), 2);
            assert_eq!(
                path.result_cardinality(),
                schema_model::Cardinality::Required
            );

            let link_step = &path.steps()[0];
            assert_eq!(link_step.field().owner_object_type().name(), "Post");
            assert_eq!(link_step.field().name(), "author");
            assert_eq!(link_step.cardinality(), schema_model::Cardinality::Required);

            match link_step.kind() {
                query_ir::ResolvedPathStepKind::Link { target_object_type } => {
                    assert_eq!(target_object_type.name(), "User");
                }
                query_ir::ResolvedPathStepKind::Scalar => {
                    panic!("first path step should resolve to a link")
                }
            }

            let scalar_step = &path.steps()[1];
            assert_eq!(scalar_step.field().owner_object_type().name(), "User");
            assert_eq!(scalar_step.field().name(), "name");
            assert_eq!(
                scalar_step.cardinality(),
                schema_model::Cardinality::Required
            );

            match scalar_step.kind() {
                query_ir::ResolvedPathStepKind::Scalar => {}
                query_ir::ResolvedPathStepKind::Link { .. } => {
                    panic!("terminal path step should resolve to a scalar")
                }
            }
        }
        query_ir::ValueExpr::Literal(_) => panic!("order by should resolve to a path"),
    }
}
