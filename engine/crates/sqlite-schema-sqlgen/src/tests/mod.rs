extern crate alloc;

use crate::render_create_table;
use alloc::vec;
use schema::SchemaCatalog;
use sqlite_schema::plan_initial_schema;

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
