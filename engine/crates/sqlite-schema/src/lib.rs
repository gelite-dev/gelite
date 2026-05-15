#![no_std]

//! SQLite schema planning for Gelite.
//!
//! This crate will map a validated `schema::SchemaCatalog` to SQLite object
//! tables, relation tables, metadata tables, indexes, and catalog metadata
//! rows. It should stay independent from SQLite connection execution until the
//! schema planning API is tested.

extern crate alloc;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use schema::{Cardinality, Field, ScalarType, SchemaCatalog};

const SCHEMA_VERSIONS_TABLE: &str = "_engine_schema_versions";
const CATALOG_OBJECTS_TABLE: &str = "_engine_catalog_objects";
const CATALOG_FIELDS_TABLE: &str = "_engine_catalog_fields";

/// SQLite-specific plan for the first schema application step.
///
/// This type is intentionally structured instead of storing raw DDL strings.
/// Tests can inspect table, column, and constraint decisions before a later
/// renderer turns the plan into `CREATE TABLE` statements.
pub struct SQLiteSchemaPlan {
    metadata_tables: Vec<SQLiteTablePlan>,
    object_tables: Vec<SQLiteTablePlan>,
}

impl SQLiteSchemaPlan {
    pub fn metadata_tables(&self) -> &[SQLiteTablePlan] {
        &self.metadata_tables
    }

    pub fn object_tables(&self) -> &[SQLiteTablePlan] {
        &self.object_tables
    }
}

/// Planned SQLite table definition before DDL rendering.
///
/// A table plan describes the physical table shape that should exist in
/// SQLite. It does not record whether the table came from engine metadata,
/// an object type, or a relation table; callers keep those groups separate in
/// the surrounding `SQLiteSchemaPlan`.
pub struct SQLiteTablePlan {
    name: String,
    columns: Vec<SQLiteColumnPlan>,
    foreign_keys: Vec<SQLiteForeignKeyPlan>,
}

impl SQLiteTablePlan {
    /// Creates a planned table with a deterministic table name and column list.
    pub fn new(name: impl Into<String>, columns: Vec<SQLiteColumnPlan>) -> Self {
        Self::new_with_foreign_keys(name, columns, Vec::new())
    }

    /// Creates a planned table with table-level foreign key constraints.
    pub fn new_with_foreign_keys(
        name: impl Into<String>,
        columns: Vec<SQLiteColumnPlan>,
        foreign_keys: Vec<SQLiteForeignKeyPlan>,
    ) -> Self {
        Self {
            name: name.into(),
            columns,
            foreign_keys,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn columns(&self) -> &[SQLiteColumnPlan] {
        &self.columns
    }

    pub fn foreign_keys(&self) -> &[SQLiteForeignKeyPlan] {
        &self.foreign_keys
    }
}

/// Builds the SQLite schema plan for applying a validated schema catalog to an
/// empty SQLite database.
pub fn plan_initial_schema(catalog: &SchemaCatalog) -> SQLiteSchemaPlan {
    let metadata_tables = vec![
        SQLiteTablePlan::new(
            SCHEMA_VERSIONS_TABLE.to_string(),
            vec![
                SQLiteColumnPlan::new(
                    "version_id".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    true,
                    true,
                ),
                SQLiteColumnPlan::new(
                    "checksum".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "applied_at".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "schema_snapshot".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
            ],
        ),
        SQLiteTablePlan::new(
            CATALOG_OBJECTS_TABLE.to_string(),
            vec![
                SQLiteColumnPlan::new(
                    "object_id".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    true,
                    true,
                ),
                SQLiteColumnPlan::new("name".to_string(), SQLiteAffinity::Text, false, false, true),
            ],
        ),
        SQLiteTablePlan::new_with_foreign_keys(
            CATALOG_FIELDS_TABLE.to_string(),
            vec![
                SQLiteColumnPlan::new(
                    "field_id".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    true,
                    true,
                ),
                SQLiteColumnPlan::new(
                    "object_id".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "name".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "field_kind".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "cardinality".to_string(),
                    SQLiteAffinity::Text,
                    false,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "scalar_type".to_string(),
                    SQLiteAffinity::Text,
                    true,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "target_object_id".to_string(),
                    SQLiteAffinity::Text,
                    true,
                    false,
                    false,
                ),
                SQLiteColumnPlan::new(
                    "is_implicit".to_string(),
                    SQLiteAffinity::Integer,
                    false,
                    false,
                    false,
                ),
            ],
            vec![SQLiteForeignKeyPlan::new(
                "object_id",
                CATALOG_OBJECTS_TABLE,
                "object_id",
            )],
        ),
    ];

    let object_tables = plan_objects(&catalog);

    SQLiteSchemaPlan {
        metadata_tables,
        object_tables,
    }
}

fn plan_objects(catalog: &SchemaCatalog) -> Vec<SQLiteTablePlan> {
    catalog
        .object_types()
        .iter()
        .map(|object_type| {
            let declared_fields = object_type.declared_fields();
            let mut columns = vec![SQLiteColumnPlan::new(
                "id",
                SQLiteAffinity::Text,
                false,
                true,
                true,
            )];

            columns.extend(declared_fields.iter().filter_map(|field| match field {
                Field::Scalar(scalar) => Some(SQLiteColumnPlan::new(
                    field.name(),
                    sqlite_affinity(scalar.scalar_type()),
                    field.cardinality() != Cardinality::Required,
                    false,
                    false,
                )),
                Field::Link(_) => Some(SQLiteColumnPlan::new(
                    format!("{}_id", field.name()),
                    SQLiteAffinity::Text,
                    field.cardinality() != Cardinality::Required,
                    false,
                    false,
                )),
            }));

            let foreign_keys = declared_fields
                .iter()
                .filter_map(|field| match field {
                    Field::Scalar(_) => None,
                    Field::Link(link) => Some(SQLiteForeignKeyPlan::new(
                        format!("{}_id", field.name()),
                        link.target_type_name().to_ascii_lowercase(),
                        "id",
                    )),
                })
                .collect();

            SQLiteTablePlan::new_with_foreign_keys(
                object_type.name().to_ascii_lowercase(),
                columns,
                foreign_keys,
            )
        })
        .collect()
}

fn sqlite_affinity(scalar_type: ScalarType) -> SQLiteAffinity {
    match scalar_type {
        ScalarType::Str => SQLiteAffinity::Text,
        ScalarType::Int64 => SQLiteAffinity::Integer,
        ScalarType::Float64 => SQLiteAffinity::Real,
        ScalarType::Bool => SQLiteAffinity::Integer,
        ScalarType::Uuid => SQLiteAffinity::Text,
        ScalarType::DateTime => SQLiteAffinity::Text,
    }
}

/// SQLite type affinity used by physical column plans.
///
/// This is not the same as `ScalarType`. Several semantic scalar types
/// can share one SQLite affinity, such as `bool` and `int64` both mapping to
/// `INTEGER` in the storage spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SQLiteAffinity {
    Text,
    Integer,
    Real,
}

/// Planned SQLite column definition before DDL rendering.
///
/// The booleans model the constraints currently needed by the metadata table
/// contract. Foreign keys are intentionally not part of this type; they should
/// be modeled as table-level plans once the first foreign-key test is added.
pub struct SQLiteColumnPlan {
    name: String,
    affinity: SQLiteAffinity,
    nullable: bool,
    primary_key: bool,
    unique: bool,
}

impl SQLiteColumnPlan {
    /// Creates a planned column with the constraints needed by the schema plan.
    pub fn new(
        name: impl Into<String>,
        affinity: SQLiteAffinity,
        nullable: bool,
        primary_key: bool,
        unique: bool,
    ) -> Self {
        Self {
            name: name.into(),
            affinity,
            nullable,
            primary_key,
            unique,
        }
    }

    pub fn affinity(&self) -> SQLiteAffinity {
        self.affinity
    }
    pub fn is_nullable(&self) -> bool {
        self.nullable
    }
    pub fn is_primary_key(&self) -> bool {
        self.primary_key
    }
    pub fn is_unique(&self) -> bool {
        self.unique
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Planned table-level foreign key before DDL rendering.
pub struct SQLiteForeignKeyPlan {
    column_name: String,
    target_table: String,
    target_column: String,
}

impl SQLiteForeignKeyPlan {
    pub fn new(
        column_name: impl Into<String>,
        target_table: impl Into<String>,
        target_column: impl Into<String>,
    ) -> Self {
        Self {
            column_name: column_name.into(),
            target_table: target_table.into(),
            target_column: target_column.into(),
        }
    }

    pub fn column_name(&self) -> &str {
        &self.column_name
    }

    pub fn target_table(&self) -> &str {
        &self.target_table
    }

    pub fn target_column(&self) -> &str {
        &self.target_column
    }
}

#[cfg(test)]
mod tests;
