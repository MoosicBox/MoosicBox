//! Snapshot testing utilities for migration verification using JSON format
//!
//! This module provides utilities for capturing and comparing database schemas
//! and migration results using insta's snapshot testing with JSON serialization.
//! JSON is used for its wide compatibility, active maintenance, and human readability
//! when pretty-printed.

use crate::TestError;
use std::path::PathBuf;
use switchy_database::{Database, DatabaseError};
use switchy_schema::MigrationError;

#[cfg(feature = "snapshots")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "snapshots")]
use std::collections::BTreeMap;
#[cfg(feature = "snapshots")]
use switchy_database::schema::{ColumnInfo as DbColumnInfo, TableInfo};
#[cfg(feature = "snapshots")]
use switchy_database::{DatabaseValue, Row};

/// Error type for snapshot testing operations
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// Database operation failed
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    /// Migration operation failed
    #[error("Migration error: {0}")]
    Migration(#[from] MigrationError),

    /// IO operation failed
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Snapshot validation failed
    #[error("Snapshot validation failed: {0}")]
    Validation(String),

    /// Test utilities error
    #[error("Test error: {0}")]
    Test(#[from] TestError),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for snapshot operations
pub type Result<T> = std::result::Result<T, SnapshotError>;

/// Snapshot structure for migration state verification
/// Note: This structure will grow in later phases.
/// Breaking changes to snapshot structure are acceptable during development.
/// Regenerate snapshots with `cargo insta review` when structure changes.
#[cfg(feature = "snapshots")]
#[derive(Debug, Serialize, Deserialize)]
struct MigrationSnapshot {
    test_name: String,
    migration_sequence: Vec<String>,
    schema: Option<DatabaseSchema>,
}

/// Complete database schema structure for snapshot storage
#[cfg(feature = "snapshots")]
#[derive(Debug, Serialize, Deserialize)]
struct DatabaseSchema {
    tables: BTreeMap<String, TableSchema>,
}

/// Table schema structure with columns and indexes
#[cfg(feature = "snapshots")]
#[derive(Debug, Serialize, Deserialize)]
struct TableSchema {
    columns: Vec<ColumnInfo>,
    indexes: Vec<String>,
}

/// Column information for schema capture
#[cfg(feature = "snapshots")]
#[derive(Debug, Serialize, Deserialize)]
struct ColumnInfo {
    name: String,
    data_type: String,
    nullable: bool,
    default_value: Option<String>,
    primary_key: bool,
}

/// Placeholder for snapshot testing functionality
/// Full implementation will come in Phase 11.4.2+
pub struct SnapshotTester {
    // Implementation to follow in subsequent phases
}

/// Migration snapshot test struct for verifying database schema changes
pub struct MigrationSnapshotTest {
    test_name: String,
    migrations_dir: PathBuf,
    assert_schema: bool,
    assert_sequence: bool,
    expected_tables: Vec<String>, // NEW: Tables to inspect for schema capture
}

impl MigrationSnapshotTest {
    /// Create a new migration snapshot test
    #[must_use]
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            // Points to dedicated snapshot test migrations
            migrations_dir: PathBuf::from("./test-resources/snapshot-migrations/minimal"),
            assert_schema: true,
            assert_sequence: true,
            expected_tables: Vec::new(), // Empty by default
        }
    }

    #[must_use]
    pub fn migrations_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.migrations_dir = path.into();
        self
    }

    #[must_use]
    pub const fn assert_schema(mut self, enabled: bool) -> Self {
        self.assert_schema = enabled;
        self
    }

    #[must_use]
    pub const fn assert_sequence(mut self, enabled: bool) -> Self {
        self.assert_sequence = enabled;
        self
    }

    /// Configure which tables to inspect for schema capture
    #[must_use]
    pub fn expected_tables(mut self, tables: Vec<String>) -> Self {
        self.expected_tables = tables;
        self
    }

    /// Auto-discover tables by parsing migration files (future enhancement)
    #[must_use]
    pub const fn auto_discover_tables(self) -> Self {
        // Will be implemented to parse CREATE TABLE from migration files
        self
    }

    /// Optionally integrate with existing test builder for complex scenarios
    #[must_use]
    pub fn with_test_builder(self, _builder: crate::MigrationTestBuilder<'_>) -> Self {
        // Will be implemented in later phases
        self
    }

    /// Capture database schema using Phase 16 table introspection API
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if schema capture fails
    #[cfg(feature = "snapshots")]
    async fn capture_schema(&self, db: &dyn Database) -> Result<DatabaseSchema> {
        let mut schema = DatabaseSchema {
            tables: BTreeMap::new(),
        };

        // Use Phase 16 table introspection API to get schema information
        for table_name in &self.expected_tables {
            if let Some(table_info) = db.get_table_info(table_name).await? {
                // Convert Phase 16 TableInfo to our snapshot types
                let columns = table_info
                    .columns
                    .into_values()
                    .map(|col| ColumnInfo {
                        name: col.name,
                        data_type: format!("{:?}", col.data_type), // Convert DataType enum to string
                        nullable: col.nullable,
                        default_value: col.default_value.map(|v| format!("{v:?}")),
                        primary_key: col.is_primary_key,
                    })
                    .collect();

                let indexes = table_info
                    .indexes
                    .into_values()
                    .map(|idx| idx.name)
                    .collect();

                schema
                    .tables
                    .insert(table_name.clone(), TableSchema { columns, indexes });
            }
        }

        Ok(schema)
    }

    /// Auto-discover tables from migrations if `expected_tables` is empty
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if table discovery fails
    #[cfg(feature = "snapshots")]
    #[allow(unused, clippy::unused_async)] // Future enhancement
    async fn discover_tables_from_migrations(&self) -> Result<Vec<String>> {
        // TODO: Parse migration files in migrations_dir to find CREATE TABLE statements
        // For now, return empty vec - this would be implemented in a future enhancement
        Ok(vec![])
    }

    /// Create a test database using existing utilities
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if database creation fails
    #[cfg(feature = "snapshots")]
    async fn create_test_database(&self) -> Result<Box<dyn Database>> {
        // Use existing test_utils helper (SQLite in-memory)
        // This database persists for the entire test lifecycle
        let db = crate::create_empty_in_memory()
            .await
            .map_err(TestError::from)?;
        Ok(db)
    }

    /// Run the snapshot test
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if test execution fails
    #[cfg(feature = "snapshots")]
    pub async fn run(self) -> Result<()> {
        // Create SQLite database - persists for entire test
        let db = self.create_test_database().await?;

        // Verify database works
        db.exec_raw("SELECT 1").await?;

        // Capture schema if expected_tables is specified
        let schema = if self.expected_tables.is_empty() {
            None
        } else {
            Some(self.capture_schema(&*db).await?)
        };

        // Create snapshot with database info
        let snapshot = MigrationSnapshot {
            test_name: self.test_name.clone(),
            migration_sequence: vec![], // No migrations yet
            schema,
        };

        insta::assert_json_snapshot!(self.test_name, snapshot);
        Ok(())
    }

    /// Run the snapshot test (non-snapshots version)
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if test execution fails
    #[cfg(not(feature = "snapshots"))]
    pub fn run(self) -> Result<()> {
        // Still minimal but uses configuration
        println!("Test: {}", self.test_name);
        println!("Migrations: {}", self.migrations_dir.display());
        println!(
            "Schema: {}, Sequence: {}",
            self.assert_schema, self.assert_sequence
        );

        Ok(())
    }
}

// Conversion functions from Phase 16 types to snapshot types
#[cfg(feature = "snapshots")]
#[allow(unused)]
fn table_info_to_schema(info: TableInfo) -> TableSchema {
    TableSchema {
        columns: info
            .columns
            .into_values()
            .map(db_column_info_to_column_info)
            .collect(),
        indexes: info.indexes.into_values().map(|idx| idx.name).collect(),
    }
}

#[cfg(feature = "snapshots")]
fn db_column_info_to_column_info(col: DbColumnInfo) -> ColumnInfo {
    ColumnInfo {
        name: col.name,
        data_type: format!("{:?}", col.data_type), // Convert DataType enum to string
        nullable: col.nullable,
        default_value: col.default_value.map(|v| format!("{v:?}")),
        primary_key: col.is_primary_key,
    }
}

// JSON Conversion functions for Row and DatabaseValue types
#[cfg(feature = "snapshots")]
#[allow(unused)]
fn row_to_json(row: Row) -> serde_json::Value {
    let map: serde_json::Map<String, serde_json::Value> = row
        .columns
        .into_iter()
        .map(|(k, v)| (k, database_value_to_json(v)))
        .collect();
    serde_json::Value::Object(map)
}

#[cfg(feature = "snapshots")]
#[allow(unused)]
fn database_value_to_json(value: DatabaseValue) -> serde_json::Value {
    match value {
        DatabaseValue::String(s) | DatabaseValue::StringOpt(Some(s)) => {
            serde_json::Value::String(s)
        }
        DatabaseValue::Bool(b) | DatabaseValue::BoolOpt(Some(b)) => serde_json::Value::Bool(b),
        DatabaseValue::Number(i) | DatabaseValue::NumberOpt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::UNumber(u) | DatabaseValue::UNumberOpt(Some(u)) => {
            serde_json::Value::Number(u.into())
        }
        DatabaseValue::Real(f) | DatabaseValue::RealOpt(Some(f)) => serde_json::Number::from_f64(f)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        DatabaseValue::Null
        | DatabaseValue::StringOpt(None)
        | DatabaseValue::BoolOpt(None)
        | DatabaseValue::NumberOpt(None)
        | DatabaseValue::UNumberOpt(None)
        | DatabaseValue::RealOpt(None) => serde_json::Value::Null,
        DatabaseValue::DateTime(dt) => serde_json::Value::String(dt.to_string()),
        DatabaseValue::NowAdd(s) => serde_json::Value::String(format!("NOW + {s}")),
        DatabaseValue::Now => serde_json::Value::String("NOW".to_string()),
    }
}
