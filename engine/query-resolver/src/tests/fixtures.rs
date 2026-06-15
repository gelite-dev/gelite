use alloc::string::String;
use alloc::vec;
use query_ast::{
    ArithmeticExpr, ArithmeticOp, CompareExpr, CompareOp, Expr, InExpr, InOp, Literal, Path,
    PathStep,
};
use schema_model::{Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog};

pub fn post_only_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new("Post", vec![])])
        .expect("post-only schema catalog should be valid")
}

pub fn post_with_title_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new(
        "Post",
        vec![Field::Scalar(ScalarField::new(
            "title",
            ScalarType::Str,
            schema_model::SingleCardinality::Required,
        ))],
    )])
    .expect("post-with-title-catalog schema catalog should be valid")
}

pub fn post_with_optional_subtitle_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new(
        "Post",
        vec![
            Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                schema_model::SingleCardinality::Required,
            )),
            Field::Scalar(ScalarField::new(
                "subtitle",
                ScalarType::Str,
                schema_model::SingleCardinality::Optional,
            )),
        ],
    )])
    .expect("post-with-optional-subtitle catalog should be valid")
}

pub fn post_with_scalar_fields_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![ObjectType::new(
        "Post",
        vec![
            Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                schema_model::SingleCardinality::Required,
            )),
            Field::Scalar(ScalarField::new(
                "view_count",
                ScalarType::Int64,
                schema_model::SingleCardinality::Required,
            )),
            Field::Scalar(ScalarField::new(
                "rating",
                ScalarType::Float64,
                schema_model::SingleCardinality::Required,
            )),
            Field::Scalar(ScalarField::new(
                "published",
                ScalarType::Bool,
                schema_model::SingleCardinality::Required,
            )),
        ],
    )])
    .expect("post-with-scalar-fields catalog should be valid")
}

pub fn post_with_author_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![
                Field::Scalar(ScalarField::new(
                    "name",
                    ScalarType::Str,
                    schema_model::SingleCardinality::Required,
                )),
                Field::Scalar(ScalarField::new(
                    "score",
                    ScalarType::Int64,
                    schema_model::SingleCardinality::Required,
                )),
            ],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    schema_model::SingleCardinality::Required,
                )),
                Field::Link(LinkField::new(
                    "author",
                    "User",
                    schema_model::Cardinality::Required,
                )),
            ],
        ),
    ])
    .expect("post-with-author schema catalog should be valid")
}

pub fn path_expr(path: &[&str]) -> Expr {
    Expr::Path(Path::new(path.iter().copied().map(PathStep::new).collect()))
}

pub fn literal_string_expr(value: &str) -> Expr {
    Expr::Literal(Literal::String(String::from(value)))
}

pub fn literal_int_expr(value: i64) -> Expr {
    Expr::Literal(Literal::Int64(value))
}

pub fn literal_float_expr(value: f64) -> Expr {
    Expr::Literal(Literal::Float64(value))
}

pub fn literal_bool_expr(value: bool) -> Expr {
    Expr::Literal(Literal::Bool(value))
}

pub fn literal_null_expr() -> Expr {
    Expr::Literal(Literal::Null)
}

pub fn arithmetic_expr(left: Expr, op: ArithmeticOp, right: Expr) -> Expr {
    Expr::Arithmetic(ArithmeticExpr::new(left, op, right))
}

pub fn filter_eq_string(path: &[&str], value: &str) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        CompareOp::Eq,
        literal_string_expr(value),
    ))
}

pub fn filter_eq_int(path: &[&str], value: i64) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        CompareOp::Eq,
        literal_int_expr(value),
    ))
}

pub fn filter_compare_int(path: &[&str], op: CompareOp, value: i64) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        op,
        literal_int_expr(value),
    ))
}

pub fn filter_eq_bool(path: &[&str], value: bool) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        CompareOp::Eq,
        literal_bool_expr(value),
    ))
}

pub fn filter_eq_null(path: &[&str]) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        CompareOp::Eq,
        literal_null_expr(),
    ))
}

pub fn filter_null_eq(path: &[&str]) -> Expr {
    Expr::Compare(CompareExpr::new(
        literal_null_expr(),
        CompareOp::Eq,
        path_expr(path),
    ))
}

pub fn filter_ne_null(path: &[&str]) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        CompareOp::Ne,
        literal_null_expr(),
    ))
}

pub fn filter_null_ne(path: &[&str]) -> Expr {
    Expr::Compare(CompareExpr::new(
        literal_null_expr(),
        CompareOp::Ne,
        path_expr(path),
    ))
}

pub fn filter_lt_null(path: &[&str]) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        CompareOp::Lt,
        literal_null_expr(),
    ))
}

pub fn filter_in_strings(path: &[&str], values: &[&str]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::In,
        values.iter().copied().map(literal_string_expr).collect(),
    ))
}

pub fn filter_in_ints(path: &[&str], values: &[i64]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::In,
        values.iter().copied().map(literal_int_expr).collect(),
    ))
}

pub fn filter_in_floats(path: &[&str], values: &[f64]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::In,
        values.iter().copied().map(literal_float_expr).collect(),
    ))
}

pub fn filter_in_bools(path: &[&str], values: &[bool]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::In,
        values.iter().copied().map(literal_bool_expr).collect(),
    ))
}

pub fn filter_not_in_strings(path: &[&str], values: &[&str]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::NotIn,
        values.iter().copied().map(literal_string_expr).collect(),
    ))
}

pub fn filter_in_empty(path: &[&str]) -> Expr {
    Expr::In(InExpr::new(path_expr(path), InOp::In, vec![]))
}

pub fn filter_in_null(path: &[&str]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::In,
        vec![literal_null_expr()],
    ))
}

pub fn filter_in_path_item(path: &[&str], item_path: &[&str]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::In,
        vec![path_expr(item_path)],
    ))
}
