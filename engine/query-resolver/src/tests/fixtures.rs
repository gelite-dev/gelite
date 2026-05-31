use alloc::string::String;
use alloc::vec;
use query_ast::{CompareExpr, CompareOp, Expr, InExpr, InOp, Literal, Path, PathStep};
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

pub fn post_with_author_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                schema_model::SingleCardinality::Required,
            ))],
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

pub fn literal_null_expr() -> Expr {
    Expr::Literal(Literal::Null)
}

pub fn filter_eq_string(path: &[&str], value: &str) -> Expr {
    Expr::Compare(CompareExpr::new(
        path_expr(path),
        CompareOp::Eq,
        literal_string_expr(value),
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

pub fn filter_in_strings(path: &[&str], values: &[&str]) -> Expr {
    Expr::In(InExpr::new(
        path_expr(path),
        InOp::In,
        values.iter().copied().map(literal_string_expr).collect(),
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
