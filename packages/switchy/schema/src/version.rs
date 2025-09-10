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
//! * `run_on` (DATETIME): When the migration was started
//! * `finished_on` (DATETIME): When the migration completed (NULL for in-progress)
//! * `status` (TEXT): Migration status ('`in_progress`', 'completed', 'failed')
//! * `failure_reason` (TEXT): Error message if migration failed (NULL otherwise)
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

use crate::{Result, migration::MigrationStatus};
use chrono::NaiveDateTime;
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType, database::ToValue};
use switchy_database::{
    Database, DatabaseValue, Row,
    query::{FilterableQuery, where_not_eq},
    schema::{Column, DataType},
};

/// Default name for the migration tracking table
pub const DEFAULT_MIGRATIONS_TABLE: &str = "__switchy_migrations";

/// Migration record information
#[derive(Debug, Clone)]
pub struct MigrationRecord {
    pub id: String,
    pub run_on: NaiveDateTime,
    pub finished_on: Option<NaiveDateTime>,
    pub status: MigrationStatus,
    pub failure_reason: Option<String>,
}

impl MissingValue<MigrationRecord> for &Row {}
impl MissingValue<MigrationStatus> for &Row {}

impl ToValueType<MigrationRecord> for &Row {
    fn to_value_type(self) -> std::result::Result<MigrationRecord, ParseError> {
        Ok(MigrationRecord {
            id: self
                .to_value("id")
                .map_err(|e| ParseError::ConvertType(format!("id: {e}")))?,
            run_on: self
                .to_value("run_on")
                .map_err(|e| ParseError::ConvertType(format!("run_on: {e}")))?,
            finished_on: self
                .to_value("finished_on")
                .map_err(|e| ParseError::ConvertType(format!("finished_on: {e}")))?,
            status: self
                .to_value("status")
                .map_err(|e| ParseError::ConvertType(format!("status: {e}")))?,
            failure_reason: self
                .to_value("failure_reason")
                .map_err(|e| ParseError::ConvertType(format!("failure_reason: {e}")))?,
        })
    }
}

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
/// * `run_on` (DATETIME): Timestamp when migration was started
/// * `finished_on` (DATETIME): Timestamp when migration completed (NULL for in-progress)
/// * `status` (TEXT): Migration status ('`in_progress`', 'completed', 'failed')
/// * `failure_reason` (TEXT): Error message if migration failed (NULL otherwise)
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
    /// **Breaking Change:** This method now creates a table with enhanced schema
    /// for error recovery tracking. If an old schema table exists, manual cleanup
    /// is required.
    ///
    /// # Errors
    ///
    /// * If the table creation fails
    pub async fn ensure_table_exists(&self, db: &dyn Database) -> Result<()> {
        // Create table with new enhanced schema
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
            .column(Column {
                name: "finished_on".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::DateTime,
                default: None,
            })
            .column(Column {
                name: "status".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "failure_reason".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .execute(db)
            .await?;

        Ok(())
    }

    /// Check if a migration has been applied successfully
    ///
    /// Returns true only if the migration has completed successfully (status = 'completed').
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
            .columns(&["id", "status"])
            .where_eq("id", migration_id)
            .execute(db)
            .await?;

        // Check if any result has status = 'completed'
        for row in &results {
            if let Some(status_value) = row.get("status")
                && let Some(status_str) = status_value.as_str()
                && status_str == MigrationStatus::Completed.to_string()
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Record a migration as completed
    ///
    /// This method records a migration as completed in the new schema.
    /// For migrations that need progress tracking, use `record_migration_started()`
    /// followed by `update_migration_status()`.
    ///
    /// # Errors
    ///
    /// * If the database insert fails
    pub async fn record_migration(&self, db: &dyn Database, migration_id: &str) -> Result<()> {
        db.insert(&self.table_name)
            .value("id", migration_id)
            .value("status", MigrationStatus::Completed.to_string())
            .value("finished_on", DatabaseValue::Now)
            .value("failure_reason", DatabaseValue::Null)
            .execute(db)
            .await?;

        Ok(())
    }

    /// Record a migration as started (in progress)
    ///
    /// This method records a migration in '`in_progress`' status and must be
    /// followed by a call to `update_migration_status()` when the migration
    /// completes or fails.
    ///
    /// # Errors
    ///
    /// * If the database insert fails
    pub async fn record_migration_started(
        &self,
        db: &dyn Database,
        migration_id: &str,
    ) -> Result<()> {
        db.insert(&self.table_name)
            .value("id", migration_id)
            .value("status", MigrationStatus::InProgress.to_string())
            .value("finished_on", DatabaseValue::Null)
            .value("failure_reason", DatabaseValue::Null)
            .execute(db)
            .await?;

        Ok(())
    }

    /// Update the status of a migration record
    ///
    /// This method updates a migration's status and sets the `finished_on` timestamp
    /// when the migration completes or fails.
    ///
    /// # Errors
    ///
    /// * If the database update fails
    pub async fn update_migration_status(
        &self,
        db: &dyn Database,
        migration_id: &str,
        status: MigrationStatus,
        failure_reason: Option<String>,
    ) -> Result<()> {
        db.update(&self.table_name)
            .value("status", status.to_string())
            .value("finished_on", DatabaseValue::Now)
            .value("failure_reason", DatabaseValue::StringOpt(failure_reason))
            .where_eq("id", migration_id)
            .execute(db)
            .await?;

        Ok(())
    }

    /// Get the status information for a specific migration
    ///
    /// Returns `None` if the migration has not been recorded.
    ///
    /// # Errors
    ///
    /// * If the database query fails
    pub async fn get_migration_status(
        &self,
        db: &dyn Database,
        migration_id: &str,
    ) -> Result<Option<MigrationRecord>> {
        let results = db
            .select(&self.table_name)
            .columns(&["id", "run_on", "finished_on", "status", "failure_reason"])
            .where_eq("id", migration_id)
            .execute(db)
            .await?;

        results
            .into_iter()
            .next()
            .map(|row| {
                row.to_value_type().map_err(|e| {
                    crate::MigrationError::Validation(format!("Row conversion failed: {e}"))
                })
            })
            .transpose()
    }

    /// Get migrations that are not in 'completed' status (dirty migrations)
    ///
    /// Returns migrations that are in '`in_progress`' or 'failed' status.
    /// These represent migrations that may have been interrupted or failed.
    ///
    /// # Errors
    ///
    /// * If the database query fails
    pub async fn get_dirty_migrations(&self, db: &dyn Database) -> Result<Vec<MigrationRecord>> {
        let results = db
            .select(&self.table_name)
            .columns(&["id", "run_on", "finished_on", "status", "failure_reason"])
            .filter(Box::new(where_not_eq(
                "status",
                MigrationStatus::Completed.to_string(),
            )))
            .sort("run_on", switchy_database::query::SortDirection::Asc)
            .execute(db)
            .await?;

        results
            .into_iter()
            .map(|row| {
                row.to_value_type().map_err(|e| {
                    crate::MigrationError::Validation(format!("Row conversion failed: {e}"))
                })
            })
            .collect()
    }

    /// Get all successfully applied migrations in reverse chronological order (most recent first)
    ///
    /// Returns only migrations with status = 'completed'.
    ///
    /// # Errors
    ///
    /// * If the database query fails
    pub async fn get_applied_migrations(&self, db: &dyn Database) -> Result<Vec<String>> {
        let results = db
            .select(&self.table_name)
            .columns(&["id", "status"])
            .sort("run_on", switchy_database::query::SortDirection::Desc)
            .execute(db)
            .await?;

        let migration_ids: Vec<String> = results
            .into_iter()
            .filter_map(|row| {
                let id = row
                    .get("id")
                    .and_then(|value| value.as_str().map(std::string::ToString::to_string));
                let status = row
                    .get("status")
                    .and_then(|value| value.as_str().map(ToString::to_string));

                // Only include completed migrations
                if let (Some(id_str), Some(status_str)) = (id, status) {
                    if status_str == MigrationStatus::Completed.to_string() {
                        Some(id_str)
                    } else {
                        None
                    }
                } else {
                    None
                }
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

    /// Remove a migration record from the tracking table
    ///
    /// This method deletes a migration record to enable retry functionality.
    ///
    /// # Errors
    ///
    /// * If the database delete fails
    /// * If migration doesn't exist (no error returned - idempotent operation)
    pub async fn remove_migration_record(
        &self,
        db: &dyn Database,
        migration_id: &str,
    ) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use switchy_database_connection;

    #[tokio::test]
    async fn test_remove_migration_record_success() {
        // Create in-memory database
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        // Ensure table exists
        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Record a migration
        tracker
            .record_migration(&*db, "test_migration")
            .await
            .expect("Failed to record migration");

        // Verify migration exists
        let status = tracker
            .get_migration_status(&*db, "test_migration")
            .await
            .expect("Failed to get migration status");
        assert!(status.is_some());

        // Remove the migration record
        tracker
            .remove_migration_record(&*db, "test_migration")
            .await
            .expect("Failed to remove migration record");

        // Verify migration no longer exists
        let status_after = tracker
            .get_migration_status(&*db, "test_migration")
            .await
            .expect("Failed to get migration status");
        assert!(status_after.is_none());
    }

    #[tokio::test]
    async fn test_remove_migration_record_nonexistent() {
        // Create in-memory database
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        // Ensure table exists
        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Try to remove non-existent migration - should be idempotent (no error)
        tracker
            .remove_migration_record(&*db, "nonexistent")
            .await
            .expect("Should succeed for non-existent migration (idempotent)");
    }

    #[tokio::test]
    async fn test_get_dirty_migrations_filters_correctly() {
        // Create in-memory database
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        // Ensure table exists
        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Record migrations with different statuses
        tracker
            .record_migration(&*db, "completed_migration")
            .await
            .expect("Failed to record completed migration");

        tracker
            .record_migration_started(&*db, "in_progress_migration")
            .await
            .expect("Failed to record in-progress migration");

        tracker
            .record_migration_started(&*db, "failed_migration")
            .await
            .expect("Failed to record failed migration start");
        tracker
            .update_migration_status(
                &*db,
                "failed_migration",
                MigrationStatus::Failed,
                Some("Test error".to_string()),
            )
            .await
            .expect("Failed to update migration status");

        // Get dirty migrations (should exclude completed)
        let dirty_migrations = tracker
            .get_dirty_migrations(&*db)
            .await
            .expect("Failed to get dirty migrations");

        // Should return in_progress and failed, but not completed
        assert_eq!(dirty_migrations.len(), 2);

        let migration_ids: Vec<&str> = dirty_migrations.iter().map(|r| r.id.as_str()).collect();
        assert!(migration_ids.contains(&"in_progress_migration"));
        assert!(migration_ids.contains(&"failed_migration"));
        assert!(!migration_ids.contains(&"completed_migration"));
    }

    #[tokio::test]
    async fn test_migration_record_fields() {
        // Create in-memory database
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        // Ensure table exists
        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Test failed migration with all fields
        tracker
            .record_migration_started(&*db, "test_migration")
            .await
            .expect("Failed to record migration start");

        tracker
            .update_migration_status(
                &*db,
                "test_migration",
                MigrationStatus::Failed,
                Some("Test error message".to_string()),
            )
            .await
            .expect("Failed to update migration status");

        // Get migration status and verify all fields
        let status = tracker
            .get_migration_status(&*db, "test_migration")
            .await
            .expect("Failed to get migration status")
            .expect("Migration should exist");

        assert_eq!(status.id, "test_migration");
        assert_eq!(status.status, MigrationStatus::Failed);
        assert!(status.run_on > chrono::NaiveDateTime::default());
        assert!(status.finished_on.is_some());
        assert_eq!(
            status.failure_reason,
            Some("Test error message".to_string())
        );
    }
}
