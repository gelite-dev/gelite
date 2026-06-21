use alloc::vec::Vec;
use query_ast::{Expr, Literal, UnaryArithmeticOp};

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

pub fn assert_unary_arithmetic_expr(expr: &Expr, expected_op: UnaryArithmeticOp) -> &Expr {
    let Expr::UnaryArithmetic(unary) = expr else {
        panic!("expected unary arithmetic expression, got {expr:?}");
    };

    assert_eq!(unary.op(), expected_op);
    unary.operand()
}
