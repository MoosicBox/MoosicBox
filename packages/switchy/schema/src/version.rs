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
    query::{FilterableQuery, where_eq, where_not_eq},
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
    pub up_checksum: String,
    pub down_checksum: String,
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
            up_checksum: self
                .to_value("up_checksum")
                .map_err(|e| ParseError::ConvertType(format!("up_checksum: {e}")))?,
            down_checksum: self
                .to_value("down_checksum")
                .map_err(|e| ParseError::ConvertType(format!("down_checksum: {e}")))?,
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
                name: "up_checksum".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::VarChar(64),
                default: None,
            })
            .column(Column {
                name: "down_checksum".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::VarChar(64),
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
        // Use default checksums (32 zero bytes each) for direct completed migrations
        let up_checksum_hex = hex::encode(vec![0u8; 32]);
        let down_checksum_hex = hex::encode(vec![0u8; 32]);

        db.insert(&self.table_name)
            .value("id", migration_id)
            .value("status", MigrationStatus::Completed.to_string())
            .value("finished_on", DatabaseValue::Now)
            .value("failure_reason", DatabaseValue::Null)
            .value("up_checksum", up_checksum_hex)
            .value("down_checksum", down_checksum_hex)
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
    /// * If the checksum is not exactly 32 bytes
    /// * If the database insert fails
    pub async fn record_migration_started(
        &self,
        db: &dyn Database,
        migration_id: &str,
        up_checksum: &bytes::Bytes,
        down_checksum: &bytes::Bytes,
    ) -> Result<()> {
        // Validate both checksums are exactly 32 bytes
        if up_checksum.len() != 32 {
            return Err(crate::MigrationError::InvalidChecksum(format!(
                "Expected 32 bytes for up_checksum, got {}",
                up_checksum.len()
            )));
        }
        if down_checksum.len() != 32 {
            return Err(crate::MigrationError::InvalidChecksum(format!(
                "Expected 32 bytes for down_checksum, got {}",
                down_checksum.len()
            )));
        }

        // Convert to lowercase hex strings (always 64 chars each)
        let up_checksum_hex = hex::encode(up_checksum);
        let down_checksum_hex = hex::encode(down_checksum);

        db.insert(&self.table_name)
            .value("id", migration_id)
            .value("status", MigrationStatus::InProgress.to_string())
            .value("finished_on", DatabaseValue::Null)
            .value("failure_reason", DatabaseValue::Null)
            .value("up_checksum", up_checksum_hex)
            .value("down_checksum", down_checksum_hex)
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
            .columns(&[
                "id",
                "run_on",
                "finished_on",
                "status",
                "failure_reason",
                "up_checksum",
                "down_checksum",
            ])
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
            .columns(&[
                "id",
                "run_on",
                "finished_on",
                "status",
                "failure_reason",
                "up_checksum",
                "down_checksum",
            ])
            .filter(Box::new(where_not_eq(
                "status",
                MigrationStatus::Completed.to_string(),
            )))
            .execute(db)
            .await?;

        let dirty_migrations = results
            .into_iter()
            .map(|row| {
                row.to_value_type().map_err(|e| {
                    crate::MigrationError::Validation(format!("Row conversion failed: {e}"))
                })
            })
            .collect::<Result<Vec<MigrationRecord>>>()?;

        Ok(dirty_migrations)
    }

    /// Get all successfully applied migrations in chronological order
    ///
    /// Returns migration IDs for all completed migrations, ordered by run time (oldest first).
    /// This method is used by the migration runner to determine rollback order and track applied state.
    ///
    /// # Errors
    ///
    /// * If the database query fails
    pub async fn get_applied_migrations(&self, db: &dyn Database) -> Result<Vec<String>> {
        let results = db
            .select(&self.table_name)
            .columns(&["id"])
            .filter(Box::new(where_eq(
                "status",
                MigrationStatus::Completed.to_string(),
            )))
            .execute(db)
            .await?;

        let migration_ids: Vec<String> = results
            .into_iter()
            .map(|row| {
                row.to_value("id").map_err(|e| {
                    crate::MigrationError::Validation(format!(
                        "Failed to extract migration ID: {e}"
                    ))
                })
            })
            .collect::<Result<Vec<String>>>()?;

        // Return in chronological order (oldest first) - database returns in insertion order
        Ok(migration_ids)
    }

    /// Get all successfully applied migrations with full record details
    ///
    /// Returns complete migration records for all completed migrations, ordered by run time (oldest first).
    /// This method is used by checksum validation to access stored checksums for comparison.
    ///
    /// # Errors
    ///
    /// * If the database query fails
    /// * If record parsing fails
    pub async fn list_applied_migrations(&self, db: &dyn Database) -> Result<Vec<MigrationRecord>> {
        let results = db
            .select(&self.table_name)
            .columns(&[
                "id",
                "run_on",
                "finished_on",
                "status",
                "failure_reason",
                "up_checksum",
                "down_checksum",
            ])
            .filter(Box::new(where_eq(
                "status",
                MigrationStatus::Completed.to_string(),
            )))
            .execute(db)
            .await?;

        let migration_records: Vec<MigrationRecord> = results
            .into_iter()
            .map(|row| {
                row.to_value_type().map_err(|e| {
                    crate::MigrationError::Validation(format!(
                        "Failed to parse migration record: {e}"
                    ))
                })
            })
            .collect::<Result<Vec<MigrationRecord>>>()?;

        // Return in chronological order (oldest first) - database returns in insertion order
        Ok(migration_records)
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

    /// Alias for `remove_migration_record` for backward compatibility
    ///
    /// # Errors
    ///
    /// * If the database delete fails
    pub async fn remove_migration(&self, db: &dyn Database, migration_id: &str) -> Result<()> {
        self.remove_migration_record(db, migration_id).await?;
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

        let up_checksum = bytes::Bytes::from(vec![0u8; 32]);
        let down_checksum = bytes::Bytes::from(vec![0u8; 32]);
        tracker
            .record_migration_started(&*db, "in_progress_migration", &up_checksum, &down_checksum)
            .await
            .expect("Failed to record in-progress migration");

        tracker
            .record_migration_started(&*db, "failed_migration", &up_checksum, &down_checksum)
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
    async fn test_checksum_validation() {
        // Create in-memory database
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::with_table_name("__test_checksum_validation");

        // Ensure table exists
        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Test valid 32-byte checksums
        let valid_checksum = bytes::Bytes::from(vec![0u8; 32]);
        let result = tracker
            .record_migration_started(&*db, "valid_migration", &valid_checksum, &valid_checksum)
            .await;
        assert!(result.is_ok(), "Valid 32-byte checksums should be accepted");

        // Test invalid checksum (too short)
        let invalid_checksum_short = bytes::Bytes::from(vec![0u8; 16]);
        let result = tracker
            .record_migration_started(
                &*db,
                "invalid_migration_short",
                &invalid_checksum_short,
                &valid_checksum,
            )
            .await;
        assert!(result.is_err(), "16-byte checksum should be rejected");
        match result.unwrap_err() {
            crate::MigrationError::InvalidChecksum(msg) => {
                assert!(msg.contains("Expected 32 bytes for up_checksum, got 16"));
            }
            _ => panic!("Expected InvalidChecksum error"),
        }

        // Test invalid checksum (too long)
        let invalid_checksum_long = bytes::Bytes::from(vec![0u8; 64]);
        let result = tracker
            .record_migration_started(
                &*db,
                "invalid_migration_long",
                &invalid_checksum_long,
                &valid_checksum,
            )
            .await;
        assert!(result.is_err(), "64-byte checksum should be rejected");
        match result.unwrap_err() {
            crate::MigrationError::InvalidChecksum(msg) => {
                assert!(msg.contains("Expected 32 bytes for up_checksum, got 64"));
            }
            _ => panic!("Expected InvalidChecksum error"),
        }
    }

    #[tokio::test]
    async fn test_hex_encoding() {
        // Test that 32 bytes always produces 64-character hex string
        let checksum = vec![0u8; 32];
        let hex_string = hex::encode(&checksum);
        assert_eq!(
            hex_string.len(),
            64,
            "32 bytes should produce 64-character hex string"
        );
        assert_eq!(
            hex_string,
            "0".repeat(64),
            "Zero bytes should produce all zeros"
        );

        // Test with non-zero bytes
        let checksum = vec![255u8; 32];
        let hex_string = hex::encode(&checksum);
        assert_eq!(
            hex_string.len(),
            64,
            "32 bytes should produce 64-character hex string"
        );
        assert_eq!(
            hex_string,
            "ff".repeat(32),
            "255 bytes should produce all 'ff'"
        );
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
        let checksum = bytes::Bytes::from(vec![0u8; 32]);
        tracker
            .record_migration_started(&*db, "test_migration", &checksum, &checksum)
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
