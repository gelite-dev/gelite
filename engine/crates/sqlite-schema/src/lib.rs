#![no_std]

//! SQLite schema planning for Gelite.
//!
//! This crate will map a validated `schema::SchemaCatalog` to SQLite object
//! tables, relation tables, metadata tables, indexes, and catalog metadata
//! rows. It should stay independent from SQLite connection execution until the
//! schema planning API is tested.

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use schema::SchemaCatalog;

const SCHEMA_VERSIONS_TABLE: &str = "_engine_schema_versions";
const CATALOG_OBJECTS_TABLE: &str = "_engine_catalog_objects";
const CATALOG_FIELDS_TABLE: &str = "_engine_catalog_fields";

/// SQLite-specific plan for the first schema application step.
pub struct SQLiteSchemaPlan {
    metadata_tables: Vec<SQLiteTablePlan>,
}

impl SQLiteSchemaPlan {
    pub fn metadata_tables(&self) -> &[SQLiteTablePlan] {
        &self.metadata_tables
    }
}

/// Planned SQLite table definition before DDL rendering.
pub struct SQLiteTablePlan {
    name: String,
}

impl SQLiteTablePlan {
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Builds the SQLite schema plan for applying a validated schema catalog to an
/// empty SQLite database.
pub fn plan_initial_schema(_catalog: &SchemaCatalog) -> SQLiteSchemaPlan {
    let metadata_table_names = vec![
        SCHEMA_VERSIONS_TABLE,
        CATALOG_OBJECTS_TABLE,
        CATALOG_FIELDS_TABLE,
    ];
    let metadata_tables = metadata_table_names
        .iter()
        .map(|metadata_table_name| SQLiteTablePlan {
            name: metadata_table_name.to_string(),
        })
        .collect();

    SQLiteSchemaPlan { metadata_tables }
}

#[cfg(test)]
mod tests;
