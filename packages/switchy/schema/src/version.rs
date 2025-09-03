//! # Version Tracking
//!
//! This module provides migration state tracking functionality, managing
//! which migrations have been applied to a database. The version tracker
//! maintains a table in the database to record migration history.
//!
//! ## Core Functionality
//!
//! * **Migration tracking**: Records which migrations have been applied
//! * **Custom table names**: Configurable migration tracking table
//! * **Chronological ordering**: Tracks when migrations were applied
//! * **Rollback support**: Can remove migration records during rollback
//! * **Database agnostic**: Works with any supported database
//!
//! ## Default Behavior
//!
//! By default, the version tracker uses a table named `__switchy_migrations`
//! with the following schema:
//!
//! * `id` (TEXT): The migration identifier
//! * `run_on` (DATETIME): When the migration was applied
//!
//! ## Usage
//!
//! ```rust,no_run
//! use switchy_schema::version::VersionTracker;
//!
//! # async fn example(db: &dyn switchy_database::Database) -> switchy_schema::Result<()> {
//! // Use default table name
//! let tracker = VersionTracker::new();
//!
//! // Or use custom table name
//! let tracker = VersionTracker::with_table_name("my_migrations".to_string());
//!
//! // Ensure the tracking table exists
//! tracker.ensure_table_exists(db).await?;
//!
//! // Check if a migration is applied
//! let is_applied = tracker.is_migration_applied(db, "001_create_users").await?;
//!
//! if !is_applied {
//!     // Record a migration as applied
//!     tracker.record_migration(db, "001_create_users").await?;
//! }
//! # Ok(())
//! # }
//! ```

use crate::Result;
use switchy_database::{
    Database, DatabaseValue,
    query::FilterableQuery,
    schema::{Column, DataType},
};

/// Default name for the migration tracking table
pub const DEFAULT_MIGRATIONS_TABLE: &str = "__switchy_migrations";

/// Tracks migration state in the database
///
/// The `VersionTracker` maintains a table in the database that records
/// which migrations have been applied and when. This enables the migration
/// system to:
///
/// * Skip already-applied migrations
/// * Provide rollback functionality
/// * List migration history
/// * Prevent duplicate migration execution
///
/// ## Table Schema
///
/// The tracking table contains:
/// * `id` (TEXT): Unique migration identifier
/// * `run_on` (DATETIME): Timestamp when migration was applied
///
/// ## Custom Table Names
///
/// You can use a custom table name for migration tracking, which is useful
/// when you need to:
/// * Separate different application migrations
/// * Avoid naming conflicts
/// * Follow specific naming conventions
pub struct VersionTracker {
    table_name: String,
}

impl VersionTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            table_name: DEFAULT_MIGRATIONS_TABLE.to_string(),
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_table_name(table_name: impl Into<String>) -> Self {
        Self {
            table_name: table_name.into(),
        }
    }

    #[must_use]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Ensure the migrations tracking table exists
    ///
    /// # Errors
    ///
    /// * If the table creation fails
    pub async fn ensure_table_exists(&self, db: &dyn Database) -> Result<()> {
        db.create_table(&self.table_name)
            .if_not_exists(true)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "run_on".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::DateTime,
                default: Some(DatabaseValue::Now),
            })
            .execute(db)
            .await?;

        Ok(())
    }

    /// Check if a migration has been applied
    ///
    /// # Errors
    ///
    /// * If the database query fails
    pub async fn is_migration_applied(
        &self,
        db: &dyn Database,
        migration_id: &str,
    ) -> Result<bool> {
        let results = db
            .select(&self.table_name)
            .columns(&["id"])
            .where_eq("id", migration_id)
            .execute(db)
            .await?;

        Ok(!results.is_empty())
    }

    /// Record a migration as completed
    ///
    /// # Errors
    ///
    /// * If the database insert fails
    pub async fn record_migration(&self, db: &dyn Database, migration_id: &str) -> Result<()> {
        db.insert(&self.table_name)
            .value("id", migration_id)
            .execute(db)
            .await?;

        Ok(())
    }

    /// Get all applied migrations in reverse chronological order (most recent first)
    ///
    /// # Errors
    ///
    /// * If the database query fails
    pub async fn get_applied_migrations(&self, db: &dyn Database) -> Result<Vec<String>> {
        let results = db
            .select(&self.table_name)
            .columns(&["id"])
            .sort("run_on", switchy_database::query::SortDirection::Desc)
            .execute(db)
            .await?;

        let migration_ids = results
            .into_iter()
            .filter_map(|row| {
                row.get("id")
                    .and_then(|value| value.as_str().map(std::string::ToString::to_string))
            })
            .collect();

        Ok(migration_ids)
    }

    /// Remove a migration record (used during rollback)
    ///
    /// # Errors
    ///
    /// * If the database delete fails
    pub async fn remove_migration(&self, db: &dyn Database, migration_id: &str) -> Result<()> {
        db.delete(&self.table_name)
            .where_eq("id", migration_id)
            .execute(db)
            .await?;

        Ok(())
    }
}

impl Default for VersionTracker {
    fn default() -> Self {
        Self::new()
    }
}
