use schema_model::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};
use sqlite_query_sqlgen::{SQLiteBindValue, render_select};

fn blog_catalog() -> SchemaCatalog {
    SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "email",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::new("author", "User", Cardinality::Required)),
            ],
        ),
    ])
    .expect("blog catalog should be valid")
}

fn render_query(source: &str) -> sqlite_query_sqlgen::SQLiteSelectStatement {
    let ast = query_parser::parse_select(source).expect("query should parse");
    let ir = query_resolver::resolve_select(&blog_catalog(), &ast).expect("query should resolve");
    let plan = sqlite_query_plan::plan_select(&ir);

    render_select(&plan)
}

#[test]
fn select_pipeline_can_render_in_filter_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .title in ["Draft", "Published"] order by .title asc limit 20"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root WHERE root.title IN (?, ?) ORDER BY root.title ASC LIMIT 20"
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
fn select_pipeline_can_render_not_in_filter_through_single_link_from_query_text() {
    let statement = render_query(
        r#"select Post { title } filter .author.email not in ["blocked@example.com"]"#,
    );

    assert_eq!(
        statement.sql(),
        "SELECT root.title FROM post AS root INNER JOIN user AS author ON root.author_id = author.id WHERE author.email NOT IN (?)"
    );
    assert_eq!(
        statement.bind_values(),
        &[SQLiteBindValue::String("blocked@example.com".to_string())]
    );
}
