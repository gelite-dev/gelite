use alloc::vec::Vec;
use query_ast::{ArithmeticOp, CompareOp, Expr, Literal, UnaryArithmeticOp};

pub fn assert_or_expr(expr: &Expr) -> (&Expr, &Expr) {
    let Expr::Or(left, right) = expr else {
        panic!("expected or expression, got {expr:?}");
    };

    (left, right)
}

pub fn assert_and_expr(expr: &Expr) -> (&Expr, &Expr) {
    let Expr::And(left, right) = expr else {
        panic!("expected and expression, got {expr:?}");
    };

    (left, right)
}

pub fn assert_not_expr(expr: &Expr) -> &Expr {
    let Expr::Not(inner) = expr else {
        panic!("expected not expression, got {expr:?}");
    };

    inner
}

pub fn assert_compare_expr(expr: &Expr, expected_op: CompareOp) -> (&Expr, &Expr) {
    let Expr::Compare(compare) = expr else {
        panic!("expected compare expression, got {expr:?}");
    };

    assert_eq!(compare.op(), expected_op);
    (compare.left(), compare.right())
}

pub fn assert_arithmetic_expr(expr: &Expr, expected_op: ArithmeticOp) -> (&Expr, &Expr) {
    let Expr::Arithmetic(arithmetic) = expr else {
        panic!("expected arithmetic expression, got {expr:?}");
    };

    assert_eq!(arithmetic.op(), expected_op);
    (arithmetic.left(), arithmetic.right())
}

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
