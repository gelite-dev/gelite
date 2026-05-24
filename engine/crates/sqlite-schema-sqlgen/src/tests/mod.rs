extern crate alloc;

use crate::{render_create_index, render_create_table, render_insert};
use alloc::vec;
use schema::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality,
};
use sqlite_schema::{
    SQLiteIndexPlan, SQLiteValuePlan, plan_catalog_object_inserts, plan_initial_schema,
};

#[test]
fn render_create_table_for_catalog_fields_uses_composite_primary_key() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);
    let catalog_fields = &plan.metadata_tables()[2];

    let sql = render_create_table(catalog_fields);

    assert_eq!(
        sql,
        "CREATE TABLE _engine_catalog_fields (object_id INTEGER NOT NULL, field_id INTEGER NOT NULL, name TEXT NOT NULL, field_kind TEXT NOT NULL, cardinality TEXT NOT NULL, scalar_type TEXT NULL, target_object_id INTEGER NULL, is_implicit INTEGER NOT NULL, is_unique INTEGER NOT NULL, PRIMARY KEY (object_id, field_id), FOREIGN KEY (object_id) REFERENCES _engine_catalog_objects(object_id), FOREIGN KEY (target_object_id) REFERENCES _engine_catalog_objects(object_id))"
    );
}

#[test]
fn render_create_index_for_single_link_foreign_key_index() {
    let catalog = SchemaCatalog::try_new(vec![
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
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let index = &plan.indexes()[0];

    let sql = render_create_index(index);

    assert_eq!(sql, "CREATE INDEX post__author_id_idx ON post (author_id)");
}

#[test]
fn render_create_unique_index_uses_create_unique_index() {
    let index = SQLiteIndexPlan::new("user__email_idx", "user", vec!["email".into()], true);

    let sql = render_create_index(&index);

    assert_eq!(sql, "CREATE UNIQUE INDEX user__email_idx ON user (email)");
}

#[test]
fn render_catalog_object_insert_uses_placeholders() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new("User", vec![])]).unwrap();

    let plan = plan_initial_schema(&catalog);
    let inserts = plan_catalog_object_inserts(&plan);
    let rendered = render_insert(&inserts[0]);

    assert_eq!(
        rendered.sql(),
        "INSERT INTO _engine_catalog_objects (object_id, name) VALUES (?, ?)"
    );
    assert_eq!(
        rendered.values(),
        [
            SQLiteValuePlan::Integer(1),
            SQLiteValuePlan::Text("User".into()),
        ]
    )
}
