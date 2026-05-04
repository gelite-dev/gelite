mod fixtures;

use crate::render_select;
use fixtures::{post_id_shape_field, post_query_with_shape, post_title_shape_field};

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
