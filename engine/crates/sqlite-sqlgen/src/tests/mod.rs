mod fixtures;

use crate::{SQLiteBindValue, render_select};
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
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(statement.sql(), "SELECT root.title FROM post AS root");
}

#[test]
fn sqlite_sqlgen_can_render_multiple_root_selected_values() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_id_shape_field()]);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title, root.id FROM post AS root"
    );
}

#[test]
fn sqlite_sqlgen_can_render_selected_single_link_join() {
    let ir = post_query_with_shape(vec![post_title_shape_field(), post_author_shape_field()]);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title, author.id, author.name FROM post AS root INNER JOIN user AS author ON root.author_id = author.id"
    );
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_string_filter() {
    let filter = ir::Expr::Compare(ir::CompareExpr::new(
        post_title_path_value(),
        ir::CompareOp::Eq,
        ir::ValueExpr::Literal(ir::Literal::String("Hello".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_plan::plan_select(&ir);

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
fn sqlite_sqlgen_can_render_single_link_scalar_equals_string_filter() {
    let filter = ir::Expr::Compare(ir::CompareExpr::new(
        post_author_name_path_value(),
        ir::CompareOp::Eq,
        ir::ValueExpr::Literal(ir::Literal::String("Sheri".to_string())),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_plan::plan_select(&ir);

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
    let filter = ir::Expr::Compare(ir::CompareExpr::new(
        post_title_path_value(),
        ir::CompareOp::Eq,
        ir::ValueExpr::Literal(ir::Literal::Int64(42)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title = ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Int64(42)]);
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_equals_bool_filter() {
    let filter = ir::Expr::Compare(ir::CompareExpr::new(
        post_title_path_value(),
        ir::CompareOp::Eq,
        ir::ValueExpr::Literal(ir::Literal::Bool(true)),
    ));

    let ir = post_query_with_filter(filter);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title = ?"
    );

    assert_eq!(statement.bind_values(), &[SQLiteBindValue::Bool(true)]);
}

#[test]
fn sqlite_sqlgen_can_render_root_scalar_is_null_filter() {
    let filter = ir::Expr::IsNull(post_title_path_value());

    let ir = post_query_with_filter(filter);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title IS NULL"
    );

    assert!(statement.bind_values().is_empty());
}

#[test]
fn sqlite_sqlgen_can_render_order_by_root_scalar_field_desc() {
    let order_by = ir::OrderExpr::new(post_title_path_value(), ir::OrderDirection::Desc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root ORDER BY root.title DESC"
    );
}

#[test]
fn sqlite_sqlgen_can_render_order_by_single_link_scalar_field() {
    let order_by = ir::OrderExpr::new(post_author_name_path_value(), ir::OrderDirection::Asc);

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root INNER JOIN user AS author ON root.author_id = author.id ORDER BY author.name ASC"
    );
}

#[test]
fn sqlite_sqlgen_can_render_limit_and_offset() {
    let ir = post_query_with_limit_and_offset(10, 20);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root LIMIT 10 OFFSET 20"
    );
}
