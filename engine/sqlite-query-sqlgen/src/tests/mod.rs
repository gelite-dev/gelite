mod fixtures;

use crate::{SQLiteBindValue, render_select};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;
use fixtures::{
    post_author_name_path_value, post_author_shape_field, post_id_shape_field,
    post_query_with_filter, post_query_with_limit_and_offset, post_query_with_order_by,
    post_query_with_shape, post_title_path_value, post_title_shape_field,
};

#[test]
fn sqlite_sqlgen_can_render_simple_root_scalar_select() {
    let ir = post_query_with_shape(vec![post_title_shape_field()]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(statement.sql(), "SELECT root.title FROM post AS root");
}

#[test]
fn sqlite_sqlgen_can_render_multiple_root_selected_values() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_id_shape_field()]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title, root.id FROM post AS root"
    );
}

#[test]
fn sqlite_sqlgen_can_render_selected_single_link_join() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_author_shape_field()]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title, author.id, author.name FROM post AS root INNER JOIN user AS author ON root.author_id = author.id"
    );
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_string_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title = ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Hello".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_comparison_operators() {
    let cases = [
        (query_ir::CompareOp::Ne, "!="),
        (query_ir::CompareOp::Lt, "<"),
        (query_ir::CompareOp::Le, "<="),
        (query_ir::CompareOp::Gt, ">"),
        (query_ir::CompareOp::Ge, ">="),
    ];

    for (op, expected_sql_op) in cases {
        let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
            post_title_path_value(),
            op,
            query_ir::ValueExpr::Literal(query_ir::Literal::String("Archived".to_string())),
        ));

        let ir = post_query_with_filter(filter);
        let plan = sqlite_query_plan::plan_select(&ir);

        let statement = render_select(&plan);

        assert_eq!(
            statement.sql(),
            alloc::format!(
                "SELECT root.title FROM post AS root WHERE root.title {expected_sql_op} ?"
            )
        );

        assert_eq!(
            statement.bind_values(),
            &[SQLiteBindValue::String("Archived".to_string())]
        );
    }
}

#[test]
fn sqlite_sqlgen_can_render_single_link_scalar_equals_string_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_author_name_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Sheri".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root INNER JOIN user AS author ON root.author_id = author.id WHERE author.name = ?"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Sheri".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_int_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::Int64(42)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title = ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(42)]);
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_bool_filter() {
    let filter = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::Bool(true)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title = ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Bool(true)]);
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_is_null_filter() {
    let filter = query_ir::Expr::IsNull(post_title_path_value());

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title IS NULL"
    );

    assert!(statement.bind_values().is_empty());
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_is_not_null_filter() {
    let filter = query_ir::Expr::IsNotNull(post_title_path_value());

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title IS NOT NULL"
    );

    assert!(statement.bind_values().is_empty());
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_in_filter() {
    let filter = query_ir::Expr::In(query_ir::InExpr::new(
        post_title_path_value(),
        query_ir::InOp::In,
        vec![
            query_ir::Literal::String("Draft".to_string()),
            query_ir::Literal::String("Published".to_string()),
        ],
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title IN (?, ?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Draft".to_string()),
            SQLiteBindValue::String("Published".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_single_link_scalar_not_in_filter() {
    let filter = query_ir::Expr::In(query_ir::InExpr::new(
        post_author_name_path_value(),
        query_ir::InOp::NotIn,
        vec![query_ir::Literal::String("Sheri".to_string())],
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root INNER JOIN user AS author ON root.author_id = author.id WHERE author.name NOT IN (?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Sheri".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_and_filter() {
    let left = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));
    let right = query_ir::Expr::IsNull(post_title_path_value());
    let filter = query_ir::Expr::And(Box::new(left), Box::new(right));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE (root.title = ? AND root.title IS NULL)"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Hello".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_or_filter_with_bind_order() {
    let left = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));
    let right = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Draft".to_string())),
    ));
    let filter = query_ir::Expr::Or(Box::new(left), Box::new(right));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE (root.title = ? OR root.title = ?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[
            SQLiteBindValue::String("Hello".to_string()),
            SQLiteBindValue::String("Draft".to_string()),
        ]
    );
}

#[test]
fn sqlite_sqlgen_can_render_not_filter() {
    let inner = query_ir::Expr::Compare(query_ir::CompareExpr::new(
        post_title_path_value(),
        query_ir::CompareOp::Eq,
        query_ir::ValueExpr::Literal(query_ir::Literal::String("Hello".to_string())),
    ));
    let filter = query_ir::Expr::Not(Box::new(inner));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE NOT (root.title = ?)"
    );

    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("Hello".to_string())]
    );
}

#[test]
fn sqlite_sqlgen_can_render_order_by_root_scalar_field_desc() {
    let order_by =
        query_ir::OrderExpr::new(post_title_path_value(), query_ir::OrderDirection::Desc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root ORDER BY root.title DESC"
    );
}

#[test]
fn sqlite_sqlgen_can_render_order_by_single_link_scalar_field() {
    let order_by =
        query_ir::OrderExpr::new(post_author_name_path_value(), query_ir::OrderDirection::Asc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root INNER JOIN user AS author ON root.author_id = author.id ORDER BY author.name ASC"
    );
}

#[test]
fn sqlite_sqlgen_can_render_limit_and_offset() {
    let ir = post_query_with_limit_and_offset(10, 20);
    let plan = sqlite_query_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root LIMIT 10 OFFSET 20"
    );
}
