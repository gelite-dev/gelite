extern crate alloc;

use crate::{SQLiteAffinity, SQLiteCatalogFieldKind, plan_initial_schema};
use alloc::vec;
use alloc::vec::Vec;
use schema::{
    Cardinality, Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog,
    SingleCardinality, Uniqueness,
};

#[test]
fn initial_schema_plan_creates_metadata_tables() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();

    let plan = plan_initial_schema(&catalog);

    let table_names = plan
        .metadata_tables()
        .iter()
        .map(|table| table.name())
        .collect::<Vec<_>>();

    assert_eq!(
        table_names,
        vec![
            "_engine_schema_versions",
            "_engine_catalog_objects",
            "_engine_catalog_fields",
        ]
    );
}

#[test]
fn initial_schema_plan_defines_catalog_objects_metadata_table() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    assert_eq!(plan.metadata_tables()[1].name(), "_engine_catalog_objects");
    assert_eq!(plan.metadata_tables()[1].columns().len(), 2);

    let columns = plan.metadata_tables()[1].columns();
    assert_eq!(columns[0].name(), "object_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[0].is_nullable(), false);
    assert_eq!(columns[0].is_primary_key(), true);
    assert_eq!(columns[0].is_unique(), true);

    assert_eq!(columns[1].name(), "name");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), false);
    assert_eq!(columns[1].is_primary_key(), false);
    assert_eq!(columns[1].is_unique(), true);
}

#[test]
fn initial_schema_plan_defines_schema_versions_metadata_table() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    assert_eq!(plan.metadata_tables()[0].name(), "_engine_schema_versions");
    assert_eq!(plan.metadata_tables()[0].columns().len(), 4);

    let columns = plan.metadata_tables()[0].columns();
    assert_eq!(columns[0].name(), "version_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[0].is_nullable(), false);
    assert_eq!(columns[0].is_primary_key(), true);
    assert_eq!(columns[0].is_unique(), true);

    assert_eq!(columns[1].name(), "checksum");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), false);
    assert_eq!(columns[1].is_primary_key(), false);
    assert_eq!(columns[1].is_unique(), false);

    assert_eq!(columns[2].name(), "applied_at");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[2].is_nullable(), false);
    assert_eq!(columns[2].is_primary_key(), false);
    assert_eq!(columns[2].is_unique(), false);

    assert_eq!(columns[3].name(), "schema_snapshot");
    assert_eq!(columns[3].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[3].is_nullable(), false);
    assert_eq!(columns[3].is_primary_key(), false);
    assert_eq!(columns[3].is_unique(), false);
}

#[test]
fn initial_schema_plan_defines_catalog_fields_metadata_table() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    assert_eq!(plan.metadata_tables()[2].name(), "_engine_catalog_fields");
    assert_eq!(plan.metadata_tables()[2].columns().len(), 9);

    let columns = plan.metadata_tables()[2].columns();
    assert_eq!(columns[0].name(), "object_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[0].is_nullable(), false);
    assert_eq!(columns[0].is_primary_key(), false);
    assert_eq!(columns[0].is_unique(), false);

    assert_eq!(columns[1].name(), "field_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[1].is_nullable(), false);
    assert_eq!(columns[1].is_primary_key(), false);
    assert_eq!(columns[1].is_unique(), false);

    assert_eq!(columns[2].name(), "name");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[2].is_nullable(), false);
    assert_eq!(columns[2].is_primary_key(), false);
    assert_eq!(columns[2].is_unique(), false);

    assert_eq!(columns[3].name(), "field_kind");
    assert_eq!(columns[3].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[3].is_nullable(), false);
    assert_eq!(columns[3].is_primary_key(), false);
    assert_eq!(columns[3].is_unique(), false);

    assert_eq!(columns[4].name(), "cardinality");
    assert_eq!(columns[4].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[4].is_nullable(), false);
    assert_eq!(columns[4].is_primary_key(), false);
    assert_eq!(columns[4].is_unique(), false);

    assert_eq!(columns[5].name(), "scalar_type");
    assert_eq!(columns[5].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[5].is_nullable(), true);
    assert_eq!(columns[5].is_primary_key(), false);
    assert_eq!(columns[5].is_unique(), false);

    assert_eq!(columns[6].name(), "target_object_id");
    assert_eq!(columns[6].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[6].is_nullable(), true);
    assert_eq!(columns[6].is_primary_key(), false);
    assert_eq!(columns[6].is_unique(), false);

    assert_eq!(columns[7].name(), "is_implicit");
    assert_eq!(columns[7].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[7].is_nullable(), false);
    assert_eq!(columns[7].is_primary_key(), false);
    assert_eq!(columns[7].is_unique(), false);

    assert_eq!(columns[8].name(), "is_unique");
    assert_eq!(columns[8].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[8].is_nullable(), false);
    assert_eq!(columns[8].is_primary_key(), false);
    assert_eq!(columns[8].is_unique(), false);

    let primary_key = plan.metadata_tables()[2].primary_key().unwrap();
    assert_eq!(primary_key.column_names().len(), 2);
    assert_eq!(primary_key.column_names()[0], "object_id");
    assert_eq!(primary_key.column_names()[1], "field_id");
}

#[test]
fn initial_schema_plan_defines_catalog_fields_foreign_keys() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    let catalog_fields = &plan.metadata_tables()[2];
    assert_eq!(catalog_fields.name(), "_engine_catalog_fields");
    assert_eq!(catalog_fields.foreign_keys().len(), 2);

    let object_foreign_key = &catalog_fields.foreign_keys()[0];
    assert_eq!(object_foreign_key.column_name(), "object_id");
    assert_eq!(object_foreign_key.target_table(), "_engine_catalog_objects");
    assert_eq!(object_foreign_key.target_column(), "object_id");

    let target_object_foreign_key = &catalog_fields.foreign_keys()[1];
    assert_eq!(target_object_foreign_key.column_name(), "target_object_id");
    assert_eq!(
        target_object_foreign_key.target_table(),
        "_engine_catalog_objects"
    );
    assert_eq!(target_object_foreign_key.target_column(), "object_id");
}

#[test]
fn initial_schema_plan_creates_object_table_for_scalar_fields() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "User",
        vec![
            Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            )),
            Field::Scalar(ScalarField::new(
                "age",
                ScalarType::Int64,
                SingleCardinality::Optional,
            )),
        ],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    assert_eq!(plan.object_tables().len(), 1);

    let user = &plan.object_tables()[0];
    assert_eq!(user.name(), "user");

    let columns = user.columns();
    assert_eq!(columns[0].name(), "id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[0].is_nullable(), false);
    assert_eq!(columns[0].is_primary_key(), true);

    assert_eq!(columns[1].name(), "name");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), false);

    assert_eq!(columns[2].name(), "age");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[2].is_nullable(), true);
}

#[test]
fn initial_schema_plan_maps_all_scalar_types_to_sqlite_affinities() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "ScalarSample",
        vec![
            Field::Scalar(ScalarField::new(
                "str_field",
                ScalarType::Str,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "int64_field",
                ScalarType::Int64,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "float64_field",
                ScalarType::Float64,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "bool_field",
                ScalarType::Bool,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "uuid_field",
                ScalarType::Uuid,
                SingleCardinality::Optional,
            )),
            Field::Scalar(ScalarField::new(
                "datetime_field",
                ScalarType::DateTime,
                SingleCardinality::Optional,
            )),
        ],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let columns = plan.object_tables()[0].columns();

    let expected_affinities = [
        ("id", SQLiteAffinity::Text),
        ("str_field", SQLiteAffinity::Text),
        ("int64_field", SQLiteAffinity::Integer),
        ("float64_field", SQLiteAffinity::Real),
        ("bool_field", SQLiteAffinity::Integer),
        ("uuid_field", SQLiteAffinity::Text),
        ("datetime_field", SQLiteAffinity::Text),
    ];

    for (index, (expected_name, expected_affinity)) in expected_affinities.iter().enumerate() {
        assert_eq!(columns[index].name(), *expected_name);
        assert_eq!(columns[index].affinity(), *expected_affinity);
    }
}

#[test]
fn initial_schema_plan_creates_required_single_link_foreign_key_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
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
                Field::Link(LinkField::new(
                    "author",
                    "User",
                    schema::Cardinality::Required,
                )),
            ],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let post = &plan.object_tables()[1];
    assert_eq!(post.name(), "post");

    let columns = post.columns();
    assert_eq!(columns[0].name(), "id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[0].is_nullable(), false);
    assert_eq!(columns[0].is_primary_key(), true);

    assert_eq!(columns[1].name(), "title");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), false);
    assert_eq!(columns[1].is_primary_key(), false);

    assert_eq!(columns[2].name(), "author_id");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[2].is_nullable(), false);
    assert_eq!(columns[2].is_primary_key(), false);

    assert_eq!(post.foreign_keys().len(), 1);

    let foreign_key = &post.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "author_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
}

#[test]
fn initial_schema_plan_creates_optional_single_link_foreign_key_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Link(LinkField::new(
                "author",
                "User",
                schema::Cardinality::Optional,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let post = &plan.object_tables()[1];
    assert_eq!(post.name(), "post");

    let columns = post.columns();
    assert_eq!(columns[1].name(), "author_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), true);
    assert_eq!(columns[1].is_primary_key(), false);

    assert_eq!(post.foreign_keys().len(), 1);

    let foreign_key = &post.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "author_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
}

#[test]
fn schema_scalar_field_can_be_marked_unique() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "User",
        vec![Field::Scalar(ScalarField::with_uniqueness(
            "email",
            ScalarType::Str,
            SingleCardinality::Required,
            Uniqueness::Unique,
        ))],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    assert_eq!(plan.object_tables().len(), 1);

    let user = &plan.object_tables()[0];
    assert_eq!(user.name(), "user");

    let columns = user.columns();
    assert_eq!(columns[0].name(), "id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[0].is_nullable(), false);
    assert_eq!(columns[0].is_primary_key(), true);
    assert_eq!(columns[0].is_unique(), true);

    assert_eq!(columns[1].name(), "email");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), false);
    assert_eq!(columns[1].is_unique(), true);
}

#[test]
fn schema_scalar_field_new_is_not_unique_by_default() {
    let field = ScalarField::new("name", ScalarType::Str, SingleCardinality::Required);

    assert_eq!(field.uniqueness(), Uniqueness::NotUnique);
    assert!(!field.is_unique());
}

#[test]
fn initial_schema_plan_allows_optional_unique_scalar_field() {
    let catalog = SchemaCatalog::try_new(vec![ObjectType::new(
        "User",
        vec![Field::Scalar(ScalarField::with_uniqueness(
            "nickname",
            ScalarType::Str,
            SingleCardinality::Optional,
            Uniqueness::Unique,
        ))],
    )])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    assert_eq!(plan.object_tables().len(), 1);

    let user = &plan.object_tables()[0];
    assert_eq!(user.name(), "user");

    let columns = user.columns();
    assert_eq!(columns[1].name(), "nickname");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), true);
    assert_eq!(columns[1].is_unique(), true);
}

#[test]
fn initial_schema_plan_marks_required_unique_single_link_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Profile",
            vec![Field::Link(LinkField::with_uniqueness(
                "user",
                "User",
                schema::Cardinality::Required,
                Uniqueness::Unique,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);

    let profile = &plan.object_tables()[1];
    assert_eq!(profile.name(), "profile");

    let columns = profile.columns();
    assert_eq!(columns[1].name(), "user_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), false);
    assert_eq!(columns[1].is_primary_key(), false);
    assert_eq!(columns[1].is_unique(), true);

    assert_eq!(profile.foreign_keys().len(), 1);

    let foreign_key = &profile.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "user_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
}

#[test]
fn initial_schema_plan_marks_optional_unique_single_link_column() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Scalar(ScalarField::new(
                "name",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
        ObjectType::new(
            "Profile",
            vec![Field::Link(LinkField::with_uniqueness(
                "user",
                "User",
                schema::Cardinality::Optional,
                Uniqueness::Unique,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);

    let profile = &plan.object_tables()[1];
    assert_eq!(profile.name(), "profile");

    let columns = profile.columns();
    assert_eq!(columns[1].name(), "user_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), true);
    assert_eq!(columns[1].is_primary_key(), false);
    assert_eq!(columns[1].is_unique(), true);

    assert_eq!(profile.foreign_keys().len(), 1);

    let foreign_key = &profile.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "user_id");
    assert_eq!(foreign_key.target_table(), "user");
    assert_eq!(foreign_key.target_column(), "id");
}

#[test]
fn initial_schema_plan_creates_multi_link_join_table() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Link(LinkField::new(
                "posts",
                "Post",
                Cardinality::Many,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);

    let relation_tables = plan.relation_tables();
    assert_eq!(relation_tables.len(), 1);

    let user_posts = &relation_tables[0];
    assert_eq!(user_posts.name(), "user__posts");

    let columns = user_posts.columns();
    assert_eq!(columns[0].name(), "source_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[0].is_nullable(), false);

    assert_eq!(columns[1].name(), "target_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[1].is_nullable(), false);

    assert_eq!(columns[2].name(), "position");
    assert_eq!(columns[2].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[2].is_nullable(), true);

    let primary_key = user_posts
        .primary_key()
        .expect("join table should have primary key");
    assert_eq!(primary_key.column_names(), &["source_id", "target_id"]);

    let foreign_keys = user_posts.foreign_keys();
    assert_eq!(foreign_keys.len(), 2);

    assert_eq!(foreign_keys[0].column_name(), "source_id");
    assert_eq!(foreign_keys[0].target_table(), "user");
    assert_eq!(foreign_keys[0].target_column(), "id");

    assert_eq!(foreign_keys[1].column_name(), "target_id");
    assert_eq!(foreign_keys[1].target_table(), "post");
    assert_eq!(foreign_keys[1].target_column(), "id");
}

#[test]
fn initial_schema_plan_records_catalog_object_rows() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![Field::Link(LinkField::new(
                "posts",
                "Post",
                Cardinality::Many,
            ))],
        ),
        ObjectType::new(
            "Post",
            vec![Field::Scalar(ScalarField::new(
                "title",
                ScalarType::Str,
                SingleCardinality::Required,
            ))],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let rows = plan.catalog_object_rows();

    assert_eq!(rows.len(), 2);

    assert_eq!(rows[0].object_id(), 1);
    assert_eq!(rows[0].name(), "User");

    assert_eq!(rows[1].object_id(), 2);
    assert_eq!(rows[1].name(), "Post");
}

#[test]
fn initial_schema_plan_records_catalog_field_rows() {
    let catalog = SchemaCatalog::try_new(vec![
        ObjectType::new(
            "User",
            vec![
                Field::Scalar(ScalarField::with_uniqueness(
                    "email",
                    ScalarType::Str,
                    SingleCardinality::Required,
                    Uniqueness::Unique,
                )),
                Field::Link(LinkField::new("posts", "Post", Cardinality::Many)),
            ],
        ),
        ObjectType::new(
            "Post",
            vec![
                Field::Scalar(ScalarField::new(
                    "title",
                    ScalarType::Str,
                    SingleCardinality::Required,
                )),
                Field::Link(LinkField::with_uniqueness(
                    "author",
                    "User",
                    Cardinality::Required,
                    Uniqueness::Unique,
                )),
            ],
        ),
    ])
    .unwrap();

    let plan = plan_initial_schema(&catalog);
    let rows = plan.catalog_field_rows();

    assert_eq!(rows.len(), 6);

    assert_eq!(rows[0].object_id(), 1);
    assert_eq!(rows[0].field_id(), 1);
    assert_eq!(rows[0].name(), "id");
    assert_eq!(rows[0].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[0].cardinality(), Cardinality::Required);
    assert_eq!(rows[0].scalar_type(), Some(ScalarType::Uuid));
    assert_eq!(rows[0].target_object_id(), None);
    assert_eq!(rows[0].is_implicit(), true);
    assert_eq!(rows[0].is_unique(), false);

    assert_eq!(rows[1].object_id(), 1);
    assert_eq!(rows[1].field_id(), 2);
    assert_eq!(rows[1].name(), "email");
    assert_eq!(rows[1].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[1].cardinality(), Cardinality::Required);
    assert_eq!(rows[1].scalar_type(), Some(ScalarType::Str));
    assert_eq!(rows[1].target_object_id(), None);
    assert_eq!(rows[1].is_implicit(), false);
    assert_eq!(rows[1].is_unique(), true);

    assert_eq!(rows[2].object_id(), 1);
    assert_eq!(rows[2].field_id(), 3);
    assert_eq!(rows[2].name(), "posts");
    assert_eq!(rows[2].field_kind(), SQLiteCatalogFieldKind::Link);
    assert_eq!(rows[2].cardinality(), Cardinality::Many);
    assert_eq!(rows[2].scalar_type(), None);
    assert_eq!(rows[2].target_object_id(), Some(2));
    assert_eq!(rows[2].is_implicit(), false);
    assert_eq!(rows[2].is_unique(), false);

    assert_eq!(rows[3].object_id(), 2);
    assert_eq!(rows[3].field_id(), 1);
    assert_eq!(rows[3].name(), "id");
    assert_eq!(rows[3].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[3].cardinality(), Cardinality::Required);
    assert_eq!(rows[3].scalar_type(), Some(ScalarType::Uuid));
    assert_eq!(rows[3].target_object_id(), None);
    assert_eq!(rows[3].is_implicit(), true);
    assert_eq!(rows[3].is_unique(), false);

    assert_eq!(rows[4].object_id(), 2);
    assert_eq!(rows[4].field_id(), 2);
    assert_eq!(rows[4].name(), "title");
    assert_eq!(rows[4].field_kind(), SQLiteCatalogFieldKind::Scalar);
    assert_eq!(rows[4].cardinality(), Cardinality::Required);
    assert_eq!(rows[4].scalar_type(), Some(ScalarType::Str));
    assert_eq!(rows[4].target_object_id(), None);
    assert_eq!(rows[4].is_implicit(), false);
    assert_eq!(rows[4].is_unique(), false);

    assert_eq!(rows[5].object_id(), 2);
    assert_eq!(rows[5].field_id(), 3);
    assert_eq!(rows[5].name(), "author");
    assert_eq!(rows[5].field_kind(), SQLiteCatalogFieldKind::Link);
    assert_eq!(rows[5].cardinality(), Cardinality::Required);
    assert_eq!(rows[5].scalar_type(), None);
    assert_eq!(rows[5].target_object_id(), Some(1));
    assert_eq!(rows[5].is_implicit(), false);
    assert_eq!(rows[5].is_unique(), true);
}
