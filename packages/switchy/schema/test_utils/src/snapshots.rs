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
use insta::Settings;
#[cfg(feature = "snapshots")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "snapshots")]
use std::collections::BTreeMap;
#[cfg(feature = "snapshots")]
use std::sync::Arc;

#[cfg(feature = "snapshots")]
use switchy_database::schema::{ColumnInfo as DbColumnInfo, TableInfo};
#[cfg(feature = "snapshots")]
use switchy_database::{DatabaseValue, Row};
#[cfg(feature = "snapshots")]
use switchy_schema::discovery::directory::DirectoryMigrationSource;
#[cfg(feature = "snapshots")]
use switchy_schema::migration::{Migration, MigrationSource};
#[cfg(feature = "snapshots")]
use switchy_schema::runner::MigrationRunner;

// VecMigrationSource helper for test utilities
#[cfg(feature = "snapshots")]
struct VecMigrationSource<'a> {
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
}

#[cfg(feature = "snapshots")]
impl<'a> VecMigrationSource<'a> {
    fn new(migrations: Vec<Arc<dyn Migration<'a> + 'a>>) -> Self {
        Self { migrations }
    }
}

#[cfg(feature = "snapshots")]
#[async_trait::async_trait]
impl<'a> MigrationSource<'a> for VecMigrationSource<'a> {
    async fn migrations(&self) -> switchy_schema::Result<Vec<Arc<dyn Migration<'a> + 'a>>> {
        Ok(self.migrations.clone())
    }
}

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
#[allow(clippy::struct_excessive_bools)]
pub struct MigrationSnapshotTest {
    test_name: String,
    migrations_dir: PathBuf,
    assert_schema: bool,
    assert_sequence: bool,
    expected_tables: Vec<String>, // NEW: Tables to inspect for schema capture
    redact_timestamps: bool,
    redact_auto_ids: bool,
    redact_paths: bool,
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
            redact_timestamps: true,
            redact_auto_ids: true,
            redact_paths: true,
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

    /// Configure timestamp redaction
    #[must_use]
    pub const fn redact_timestamps(mut self, enabled: bool) -> Self {
        self.redact_timestamps = enabled;
        self
    }

    /// Configure auto-ID redaction
    #[must_use]
    pub const fn redact_auto_ids(mut self, enabled: bool) -> Self {
        self.redact_auto_ids = enabled;
        self
    }

    /// Configure path redaction
    #[must_use]
    pub const fn redact_paths(mut self, enabled: bool) -> Self {
        self.redact_paths = enabled;
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

    /// Load migrations from directory with error handling
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if migration loading fails
    #[cfg(feature = "snapshots")]
    async fn load_migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
        // Fail with clear error for missing directory (catches configuration mistakes)
        if !self.migrations_dir.exists() {
            return Err(SnapshotError::Validation(format!(
                "Migrations directory does not exist: {}",
                self.migrations_dir.display()
            )));
        }

        let source = DirectoryMigrationSource::from_path(self.migrations_dir.clone());
        let migrations = source.migrations().await?;
        Ok(migrations)
    }

    /// Run the snapshot test
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if test execution fails
    #[cfg(feature = "snapshots")]
    pub async fn run(self) -> Result<()> {
        let db = self.create_test_database().await?;
        let migrations = self.load_migrations().await?;

        // Execute migrations - fail fast on any error
        if !migrations.is_empty() {
            let source = VecMigrationSource::new(migrations.clone());
            let runner = MigrationRunner::new(Box::new(source));

            // Any migration error will propagate and fail the test
            runner.run(db.as_ref()).await?;
        }

        // Capture results based on configuration
        let schema = if self.assert_schema {
            Some(self.capture_schema(db.as_ref()).await?)
        } else {
            None
        };

        let sequence = if self.assert_sequence {
            migrations.iter().map(|m| m.id().to_string()).collect()
        } else {
            vec![]
        };

        let snapshot = MigrationSnapshot {
            test_name: self.test_name.clone(),
            migration_sequence: sequence,
            schema,
        };

        // Apply redactions using insta's Settings with precise patterns
        let mut settings = Settings::clone_current();

        if self.redact_timestamps {
            // Precise timestamp patterns for different formats
            settings.add_filter(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}", "[TIMESTAMP]");
            settings.add_filter(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}", "[TIMESTAMP]");
            settings.add_filter(r"\d{4}-\d{2}-\d{2}", "[DATE]");
        }

        if self.redact_auto_ids {
            // JSON-specific patterns for different ID fields
            settings.add_filter(r#""id": \d+"#, r#""id": "[ID]""#);
            settings.add_filter(r#""user_id": \d+"#, r#""user_id": "[USER_ID]""#);
            settings.add_filter(r#""post_id": \d+"#, r#""post_id": "[POST_ID]""#);
            settings.add_filter(r#""(\w+_id)": \d+"#, r#""$1": "[FK_ID]""#);
        }

        if self.redact_paths {
            // Unix and Windows path patterns
            settings.add_filter(r"/[\w/.-]+", "[PATH]");
            settings.add_filter(r"[A-Z]:\\[\w\\.-]+", "[PATH]");
        }

        settings.bind(|| {
            insta::assert_json_snapshot!(self.test_name, snapshot);
        });

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
