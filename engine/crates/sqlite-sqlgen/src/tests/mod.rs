mod fixtures;

use crate::{SQLiteBindValue, render_select};
use alloc::string::ToString;
use alloc::vec;
use fixtures::{
    post_id_shape_field, post_query_with_filter, post_query_with_limit_and_offset,
    post_query_with_order_by, post_query_with_shape, post_title_field, post_title_shape_field,
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
fn sqlite_sqlgen_can_render_root_scalar_equals_string_filter() {
    let filter = ir::Expr::Compare(ir::CompareExpr::new(
        ir::ValueExpr::Field(post_title_field()),
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
fn sqlite_sqlgen_can_render_order_by_root_scalar_field_desc() {
    let order_by = ir::OrderExpr::new(
        ir::ValueExpr::Field(post_title_field()),
        ir::OrderDirection::Desc,
    );

    let ir = post_query_with_order_by(vec![order_by]);
    let plan = sqlite_plan::plan_select(&ir);

    let statement = render_select(&plan);

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root ORDER BY root.title DESC"
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
