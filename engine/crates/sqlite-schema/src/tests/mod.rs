extern crate alloc;

use crate::plan_initial_schema;
use alloc::vec;
use alloc::vec::Vec;
use schema::SchemaCatalog;

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
