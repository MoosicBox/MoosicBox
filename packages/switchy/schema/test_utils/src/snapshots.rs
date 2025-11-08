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
use std::{future::Future, pin::Pin};

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

#[cfg(feature = "snapshots")]
type SetupFn = Box<
    dyn for<'a> Fn(
            &'a dyn Database,
        ) -> Pin<
            Box<dyn Future<Output = std::result::Result<(), DatabaseError>> + Send + 'a>,
        > + Send
        + Sync,
>;
#[cfg(feature = "snapshots")]
type VerificationFn = Box<
    dyn for<'a> Fn(
            &'a dyn Database,
        ) -> Pin<
            Box<dyn Future<Output = std::result::Result<(), DatabaseError>> + Send + 'a>,
        > + Send
        + Sync,
>;

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
    data_samples: Option<std::collections::BTreeMap<String, Vec<serde_json::Value>>>,
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
    migrations_dir: Option<PathBuf>,
    assert_schema: bool,
    assert_sequence: bool,
    expected_tables: Vec<String>, // NEW: Tables to inspect for schema capture
    redact_timestamps: bool,
    redact_auto_ids: bool,
    redact_paths: bool,
    assert_data: bool,
    data_samples: std::collections::BTreeMap<String, usize>,
    setup_fn: Option<SetupFn>,
    verification_fn: Option<VerificationFn>,
    db: Option<Box<dyn Database>>,
    migrations_table_name: Option<String>,
}

impl MigrationSnapshotTest {
    /// Create a new migration snapshot test
    #[must_use]
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            // No default migrations directory
            migrations_dir: None,
            assert_schema: true,
            assert_sequence: true,
            expected_tables: Vec::new(), // Empty by default
            redact_timestamps: true,
            redact_auto_ids: true,
            redact_paths: true,
            assert_data: false,
            data_samples: std::collections::BTreeMap::new(),
            setup_fn: None,
            verification_fn: None,
            db: None,
            migrations_table_name: None,
        }
    }

    /// Set the directory containing migration files to test
    #[must_use]
    pub fn migrations_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.migrations_dir = Some(path.into());
        self
    }

    /// Enable or disable schema assertion in snapshot
    #[must_use]
    pub const fn assert_schema(mut self, enabled: bool) -> Self {
        self.assert_schema = enabled;
        self
    }

    /// Enable or disable migration sequence assertion in snapshot
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

    /// Configure data assertion
    #[must_use]
    pub const fn assert_data(mut self, enabled: bool) -> Self {
        self.assert_data = enabled;
        self
    }

    /// Add data samples for a specific table
    #[must_use]
    pub fn with_data_samples(mut self, table: &str, count: usize) -> Self {
        self.data_samples.insert(table.to_string(), count);
        self
    }

    /// Add setup function to run before migrations
    #[must_use]
    #[cfg(feature = "snapshots")]
    pub fn with_setup<F>(mut self, f: F) -> Self
    where
        F: for<'a> Fn(
                &'a dyn Database,
            ) -> Pin<
                Box<dyn Future<Output = std::result::Result<(), DatabaseError>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        self.setup_fn = Some(Box::new(f));
        self
    }

    /// Add verification function to run after migrations
    #[must_use]
    #[cfg(feature = "snapshots")]
    pub fn with_verification<F>(mut self, f: F) -> Self
    where
        F: for<'a> Fn(
                &'a dyn Database,
            ) -> Pin<
                Box<dyn Future<Output = std::result::Result<(), DatabaseError>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        self.verification_fn = Some(Box::new(f));
        self
    }

    /// Use an existing database instance instead of creating a new one
    /// This allows integration with `MigrationTestBuilder` or other test scenarios
    #[must_use]
    pub fn with_database(mut self, db: Box<dyn Database>) -> Self {
        self.db = Some(db);
        self
    }

    /// Set custom migrations table name (defaults to `__switchy_migrations`)
    #[must_use]
    pub fn with_migrations_table(mut self, table_name: impl Into<String>) -> Self {
        self.migrations_table_name = Some(table_name.into());
        self
    }

    /// Auto-discover tables by parsing migration files (future enhancement)
    #[must_use]
    pub const fn auto_discover_tables(self) -> Self {
        // Will be implemented to parse CREATE TABLE from migration files
        self
    }

    /// Full integration with existing test builder for complex scenarios
    #[must_use]
    #[cfg(feature = "snapshots")]
    pub fn with_test_builder(self, _builder: crate::MigrationTestBuilder<'_>) -> Self {
        // Integration bridges the two systems
        // Note: This would store the builder for execution during run()
        // For now, we maintain the placeholder pattern but document the integration point
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

    /// Capture data samples with type-aware conversion
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if data sampling fails
    #[cfg(feature = "snapshots")]
    async fn capture_data_samples(
        &self,
        db: &dyn Database,
    ) -> Result<std::collections::BTreeMap<String, Vec<serde_json::Value>>> {
        let mut samples = std::collections::BTreeMap::new();

        for (table, &count) in &self.data_samples {
            // Use Database query builder instead of raw SQL
            let query = db.select(table).limit(count);

            let rows = query.execute(db).await?;

            let sample_data: Vec<serde_json::Value> = rows
                .into_iter()
                .map(row_to_json) // Using our conversion function
                .collect();

            samples.insert(table.clone(), sample_data);
        }

        Ok(samples)
    }

    /// Create a test database using existing utilities
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if database creation fails
    #[cfg(feature = "snapshots")]
    async fn create_test_database(&self) -> Result<Box<dyn Database>> {
        log::debug!("Creating test database");
        // Use existing test_utils helper (SQLite in-memory)
        // This database persists for the entire test lifecycle
        let db = crate::create_empty_in_memory()
            .await
            .map_err(TestError::from)?;
        Ok(db)
    }

    /// Load migrations from directory that need to be applied
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if migration loading fails
    #[cfg(feature = "snapshots")]
    async fn load_migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
        if let Some(ref migrations_dir) = self.migrations_dir {
            if migrations_dir.exists() {
                log::debug!(
                    "Loading migrations from directory: {}",
                    migrations_dir.display()
                );

                let source = DirectoryMigrationSource::from_path(migrations_dir.clone());
                let migrations = source.migrations().await?;

                log::debug!("Loaded {} migrations from directory", migrations.len());
                return Ok(migrations);
            }

            log::debug!(
                "Migrations directory does not exist: {}",
                migrations_dir.display()
            );
        } else {
            log::debug!("No migrations directory configured");
        }

        Ok(vec![])
    }

    /// Get the sequence of already applied migrations from the database
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if querying fails
    #[cfg(feature = "snapshots")]
    async fn get_migration_sequence(&self, db: &dyn Database) -> Result<Vec<String>> {
        use switchy_schema::{migration::MigrationStatus, version::VersionTracker};

        let tracker = self
            .migrations_table_name
            .as_ref()
            .map_or_else(VersionTracker::new, |table_name| {
                VersionTracker::with_table_name(table_name.clone())
            });

        // Just call it directly - it handles missing table gracefully
        let ids = tracker
            .get_applied_migration_ids(db, MigrationStatus::Completed)
            .await
            .map_err(SnapshotError::Migration)?;

        log::debug!("Found {} applied migrations in database", ids.len());
        Ok(ids)
    }

    /// Run the snapshot test
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if test execution fails
    #[cfg(feature = "snapshots")]
    #[allow(clippy::cognitive_complexity)]
    pub async fn run(mut self) -> Result<()> {
        // Use provided database or create a new one
        let db = if let Some(db) = self.db.take() {
            db
        } else {
            self.create_test_database().await?
        };
        let db = &*db;

        // Load migrations from directory (these will be applied to the DB)
        let migrations_to_apply = self.load_migrations().await?;

        // Execute setup function if provided
        if let Some(setup_fn) = &self.setup_fn {
            log::debug!("run: executing setup function");
            setup_fn(db).await?;
        } else {
            log::debug!("run: no setup function provided");
        }

        // Execute migrations - fail fast on any error
        if migrations_to_apply.is_empty() {
            log::debug!("run: no new migrations to apply");
        } else {
            log::debug!("run: executing {} migrations", migrations_to_apply.len());
            let source = VecMigrationSource::new(migrations_to_apply.clone());
            let runner = MigrationRunner::new(Box::new(source));

            // Any migration error will propagate and fail the test
            runner.run(db).await?;
        }

        // Execute verification function if provided
        if let Some(verification_fn) = &self.verification_fn {
            log::debug!("run: executing verification function");
            verification_fn(db).await?;
        } else {
            log::debug!("run: no verification function provided");
        }

        // Capture results based on configuration
        let schema = if self.assert_schema {
            log::debug!("run: capturing schema");
            Some(self.capture_schema(db).await?)
        } else {
            log::debug!("run: no schema capture");
            None
        };

        // Get the sequence of already applied migrations AFTER running new ones
        let sequence = if self.assert_sequence {
            log::debug!("run: capturing migration sequence");
            self.get_migration_sequence(db).await?
        } else {
            log::debug!("run: no migration sequence capture");
            vec![]
        };
        log::debug!("run: migration sequence: {sequence:?}");

        let data_samples = if self.assert_data {
            log::debug!("run: capturing data samples");
            Some(self.capture_data_samples(db).await?)
        } else {
            log::debug!("run: no data samples capture");
            None
        };

        let snapshot = MigrationSnapshot {
            test_name: self.test_name.clone(),
            migration_sequence: sequence,
            schema,
            data_samples,
        };

        log::debug!("run: snapshot={snapshot:?}");

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
        DatabaseValue::Int8(i) | DatabaseValue::Int8Opt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::UInt8(i) | DatabaseValue::UInt8Opt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::Int16(i) | DatabaseValue::Int16Opt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::UInt16(i) | DatabaseValue::UInt16Opt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::Int32(i) | DatabaseValue::Int32Opt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::UInt32(i) | DatabaseValue::UInt32Opt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::Int64(i) | DatabaseValue::Int64Opt(Some(i)) => {
            serde_json::Value::Number(i.into())
        }
        DatabaseValue::UInt64(u) | DatabaseValue::UInt64Opt(Some(u)) => {
            serde_json::Value::Number(u.into())
        }
        DatabaseValue::Real64(f) | DatabaseValue::Real64Opt(Some(f)) => {
            serde_json::Number::from_f64(f)
                .map_or(serde_json::Value::Null, serde_json::Value::Number)
        }
        DatabaseValue::Real32(f) | DatabaseValue::Real32Opt(Some(f)) => {
            serde_json::Number::from_f64(f64::from(f))
                .map_or(serde_json::Value::Null, serde_json::Value::Number)
        }
        DatabaseValue::Null
        | DatabaseValue::StringOpt(None)
        | DatabaseValue::BoolOpt(None)
        | DatabaseValue::Int8Opt(None)
        | DatabaseValue::UInt8Opt(None)
        | DatabaseValue::Int16Opt(None)
        | DatabaseValue::UInt16Opt(None)
        | DatabaseValue::Int32Opt(None)
        | DatabaseValue::UInt32Opt(None)
        | DatabaseValue::Int64Opt(None)
        | DatabaseValue::UInt64Opt(None)
        | DatabaseValue::Real64Opt(None)
        | DatabaseValue::Real32Opt(None) => serde_json::Value::Null,
        DatabaseValue::DateTime(dt) => serde_json::Value::String(dt.to_string()),
        DatabaseValue::NowPlus(interval) => {
            serde_json::Value::String(format!("NOW + {interval:?}"))
        }
        DatabaseValue::Now => serde_json::Value::String("NOW".to_string()),
        #[cfg(feature = "decimal")]
        DatabaseValue::Decimal(d) | DatabaseValue::DecimalOpt(Some(d)) => {
            serde_json::Value::String(d.to_string())
        }
        #[cfg(feature = "decimal")]
        DatabaseValue::DecimalOpt(None) => serde_json::Value::Null,
        #[cfg(feature = "uuid")]
        DatabaseValue::Uuid(d) | DatabaseValue::UuidOpt(Some(d)) => {
            serde_json::Value::String(d.to_string())
        }
        #[cfg(feature = "uuid")]
        DatabaseValue::UuidOpt(None) => serde_json::Value::Null,
    }
}
