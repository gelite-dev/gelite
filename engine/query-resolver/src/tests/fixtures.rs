use alloc::string::String;
use alloc::vec;
use query_ast::{CompareExpr, CompareOp, Expr, Literal, Path, PathStep};
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
    Expr::Path(Path::new(
        path.iter().copied().map(PathStep::new).collect(),
    ))
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
