use alloc::vec::Vec;
use query_ast::{Expr, Literal};

pub fn assert_path_expr(expr: &Expr, expected: &[&str]) {
    let Expr::Path(path) = expr else {
        panic!("expected path expression, got {expr:?}");
    };

    let actual = path
        .steps()
        .iter()
        .map(|step| step.field_name())
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}

pub fn assert_literal_expr(expr: &Expr, expected: &Literal) {
    let Expr::Literal(actual) = expr else {
        panic!("expected literal expression, got {expr:?}");
    };

    assert_eq!(actual, expected);
}
