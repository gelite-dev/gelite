extern crate alloc;

use crate::{SQLiteAffinity, plan_initial_schema};
use alloc::vec;
use alloc::vec::Vec;
use schema::{
    Field, LinkField, ObjectType, ScalarField, ScalarType, SchemaCatalog, SingleCardinality,
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
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
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
    assert_eq!(plan.metadata_tables()[2].columns().len(), 8);

    let columns = plan.metadata_tables()[2].columns();
    assert_eq!(columns[0].name(), "field_id");
    assert_eq!(columns[0].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[0].is_nullable(), false);
    assert_eq!(columns[0].is_primary_key(), true);
    assert_eq!(columns[0].is_unique(), true);

    assert_eq!(columns[1].name(), "object_id");
    assert_eq!(columns[1].affinity(), SQLiteAffinity::Text);
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
    assert_eq!(columns[6].affinity(), SQLiteAffinity::Text);
    assert_eq!(columns[6].is_nullable(), true);
    assert_eq!(columns[6].is_primary_key(), false);
    assert_eq!(columns[6].is_unique(), false);

    assert_eq!(columns[7].name(), "is_implicit");
    assert_eq!(columns[7].affinity(), SQLiteAffinity::Integer);
    assert_eq!(columns[7].is_nullable(), false);
    assert_eq!(columns[7].is_primary_key(), false);
    assert_eq!(columns[7].is_unique(), false);
}

#[test]
fn initial_schema_plan_defines_catalog_fields_object_foreign_key() {
    let catalog = SchemaCatalog::try_new(vec![]).unwrap();
    let plan = plan_initial_schema(&catalog);

    let catalog_fields = &plan.metadata_tables()[2];
    assert_eq!(catalog_fields.name(), "_engine_catalog_fields");
    assert_eq!(catalog_fields.foreign_keys().len(), 1);

    let foreign_key = &catalog_fields.foreign_keys()[0];
    assert_eq!(foreign_key.column_name(), "object_id");
    assert_eq!(foreign_key.target_table(), "_engine_catalog_objects");
    assert_eq!(foreign_key.target_column(), "object_id");
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
