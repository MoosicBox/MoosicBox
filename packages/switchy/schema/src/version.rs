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
///
/// Contains complete metadata about a migration execution including
/// timing information, status, and checksums for validation.
#[derive(Debug, Clone)]
pub struct MigrationRecord {
    /// Unique migration identifier
    pub id: String,
    /// Timestamp when migration execution started
    pub run_on: NaiveDateTime,
    /// Timestamp when migration completed or failed (None for in-progress migrations)
    pub finished_on: Option<NaiveDateTime>,
    /// Current execution status (`in_progress`, completed, or failed)
    pub status: MigrationStatus,
    /// Error message if migration failed (None otherwise)
    pub failure_reason: Option<String>,
    /// Hex-encoded SHA-256 checksum of the up migration (64 characters)
    pub up_checksum: String,
    /// Hex-encoded SHA-256 checksum of the down migration (64 characters)
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
    /// Create a new version tracker with the default table name
    ///
    /// Uses `__switchy_migrations` as the tracking table name.
    #[must_use]
    pub fn new() -> Self {
        Self {
            table_name: DEFAULT_MIGRATIONS_TABLE.to_string(),
        }
    }

    /// Create a new version tracker with a custom table name
    ///
    /// Use this when you need to separate migration tracking for different
    /// applications or avoid naming conflicts.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_table_name(table_name: impl Into<String>) -> Self {
        Self {
            table_name: table_name.into(),
        }
    }

    /// Get the name of the migration tracking table
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

    /// Drop the migrations tracking table
    ///
    /// This is a destructive operation that removes all migration history including:
    /// * Migration execution status (completed, failed, in-progress)
    /// * Execution timestamps (`run_on`, `finished_on`)
    /// * Failure reasons and error messages
    /// * Stored checksums for validation
    ///
    /// # Use Cases
    ///
    /// * Recovering from a corrupted migration tracking table
    /// * Fixing schema mismatches between table structure and code
    /// * Completely resetting migration history (combined with `mark_all_migrations_completed`)
    ///
    /// # Errors
    ///
    /// * If the table drop operation fails
    pub async fn drop_table(&self, db: &dyn Database) -> Result<()> {
        use switchy_database::schema::drop_table;

        drop_table(&self.table_name)
            .if_exists(true)
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

    /// Get all successfully applied migration IDs in chronological order
    ///
    /// Returns migration IDs for all completed migrations, ordered by run time (oldest first).
    /// This method is used by the migration runner to determine rollback order and track applied state.
    ///
    /// This method will return an empty list if the migrations table does not exist.
    ///
    /// # Errors
    ///
    /// * If the database query fails
    pub async fn get_applied_migration_ids(
        &self,
        db: &dyn Database,
        status: impl Into<Option<MigrationStatus>>,
    ) -> Result<Vec<String>> {
        // Check if the migrations table exists first using the schema feature
        if db.get_table_info(&self.table_name).await?.is_none() {
            // Table doesn't exist yet - return empty list (fresh database)
            return Ok(vec![]);
        }

        // Table exists, proceed with query
        let results = db.select(&self.table_name).columns(&["id"]);

        let results = if let Some(status) = status.into() {
            results.filter(Box::new(where_eq("status", status.to_string())))
        } else {
            results
        };

        let results = results.execute(db).await?;

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
    /// This method will return an empty list if the migrations table does not exist.
    ///
    /// # Errors
    ///
    /// * If the database query fails
    /// * If record parsing fails
    pub async fn get_applied_migrations(
        &self,
        db: &dyn Database,
        status: impl Into<Option<MigrationStatus>>,
    ) -> Result<Vec<MigrationRecord>> {
        // Check if the migrations table exists first using the schema feature
        if db.get_table_info(&self.table_name).await?.is_none() {
            // Table doesn't exist yet - return empty list (fresh database)
            return Ok(vec![]);
        }

        let results = db.select(&self.table_name).columns(&[
            "id",
            "run_on",
            "finished_on",
            "status",
            "failure_reason",
            "up_checksum",
            "down_checksum",
        ]);

        let results = if let Some(status) = status.into() {
            results.filter(Box::new(where_eq("status", status.to_string())))
        } else {
            results
        };

        let results = results.execute(db).await?;

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

    #[test_log::test(switchy_async::test)]
    async fn test_remove_migration_record_success() {
        // Create in-memory database
        let db = switchy_schema_test_utils::create_empty_in_memory()
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

    #[test_log::test(switchy_async::test)]
    async fn test_remove_migration_record_nonexistent() {
        // Create in-memory database
        let db = switchy_schema_test_utils::create_empty_in_memory()
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

    #[test_log::test(switchy_async::test)]
    async fn test_get_dirty_migrations_filters_correctly() {
        // Create in-memory database
        let db = switchy_schema_test_utils::create_empty_in_memory()
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

    #[test_log::test(switchy_async::test)]
    async fn test_checksum_validation() {
        // Create in-memory database
        let db = switchy_schema_test_utils::create_empty_in_memory()
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

    #[test_log::test(switchy_async::test)]
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

    #[test_log::test(switchy_async::test)]
    async fn test_migration_record_fields() {
        // Create in-memory database
        let db = switchy_schema_test_utils::create_empty_in_memory()
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

    #[test_log::test(switchy_async::test)]
    async fn test_get_applied_migrations_no_table() {
        // Test graceful handling when migrations table doesn't exist
        let db = switchy_schema_test_utils::create_empty_in_memory()
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        // DON'T create the table - test fresh database scenario

        // Should return empty list, not error
        let migrations = tracker
            .get_applied_migrations(&*db, None)
            .await
            .expect("Should not error when table doesn't exist");

        assert_eq!(
            migrations.len(),
            0,
            "Should return empty list for fresh database"
        );

        // Also test with specific status filter
        let completed_migrations = tracker
            .get_applied_migrations(&*db, MigrationStatus::Completed)
            .await
            .expect("Should not error even with status filter");

        assert_eq!(completed_migrations.len(), 0);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_applied_migrations_empty_table() {
        let db = switchy_schema_test_utils::create_empty_in_memory()
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        // Create table but don't add any migrations
        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Test with no status filter
        let migrations = tracker
            .get_applied_migrations(&*db, None)
            .await
            .expect("Should succeed with empty table");

        assert_eq!(
            migrations.len(),
            0,
            "Should return empty list for empty table"
        );

        // Test with status filter
        let completed = tracker
            .get_applied_migrations(&*db, MigrationStatus::Completed)
            .await
            .expect("Should succeed with status filter");

        assert_eq!(completed.len(), 0);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_applied_migrations_with_data() {
        let db = switchy_schema_test_utils::create_empty_in_memory()
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Add some completed migrations
        tracker.record_migration(&*db, "001_initial").await.unwrap();
        tracker
            .record_migration(&*db, "002_add_users")
            .await
            .unwrap();
        tracker
            .record_migration(&*db, "003_add_posts")
            .await
            .unwrap();

        // Get all migrations (no status filter)
        let all_migrations = tracker
            .get_applied_migrations(&*db, None)
            .await
            .expect("Should return migrations");

        assert_eq!(all_migrations.len(), 3, "Should return all 3 migrations");

        // Verify they're all completed and have the right IDs
        let ids: Vec<&str> = all_migrations.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["001_initial", "002_add_users", "003_add_posts"]);
        assert!(
            all_migrations
                .iter()
                .all(|m| m.status == MigrationStatus::Completed)
        );

        // Get only completed migrations (should be the same)
        let completed = tracker
            .get_applied_migrations(&*db, MigrationStatus::Completed)
            .await
            .expect("Should return migrations");

        assert_eq!(completed.len(), 3);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_applied_migrations_mixed_status() {
        let db = switchy_schema_test_utils::create_empty_in_memory()
            .await
            .expect("Failed to create test database");

        let tracker = VersionTracker::new();

        tracker
            .ensure_table_exists(&*db)
            .await
            .expect("Failed to create version table");

        // Add completed migration
        tracker
            .record_migration(&*db, "001_completed")
            .await
            .unwrap();

        // Add in-progress migration
        let checksum = bytes::Bytes::from(vec![0u8; 32]);
        tracker
            .record_migration_started(&*db, "002_in_progress", &checksum, &checksum)
            .await
            .unwrap();

        // Add failed migration
        tracker
            .record_migration_started(&*db, "003_failed", &checksum, &checksum)
            .await
            .unwrap();
        tracker
            .update_migration_status(
                &*db,
                "003_failed",
                MigrationStatus::Failed,
                Some("Test error".to_string()),
            )
            .await
            .unwrap();

        // Test getting all migrations
        let all = tracker
            .get_applied_migrations(&*db, None)
            .await
            .expect("Should return all migrations");

        assert_eq!(all.len(), 3, "Should return all 3 migrations");

        // Test filtering by status
        let completed = tracker
            .get_applied_migrations(&*db, MigrationStatus::Completed)
            .await
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, "001_completed");

        let in_progress = tracker
            .get_applied_migrations(&*db, MigrationStatus::InProgress)
            .await
            .unwrap();
        assert_eq!(in_progress.len(), 1);
        assert_eq!(in_progress[0].id, "002_in_progress");

        let failed = tracker
            .get_applied_migrations(&*db, MigrationStatus::Failed)
            .await
            .unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, "003_failed");
        assert_eq!(failed[0].failure_reason, Some("Test error".to_string()));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_get_applied_migrations_custom_table_name() {
        let db = switchy_schema_test_utils::create_empty_in_memory()
            .await
            .expect("Failed to create test database");

        // Use custom table name
        let tracker = VersionTracker::with_table_name("__custom_migrations");

        // Test without table existing
        let empty = tracker
            .get_applied_migrations(&*db, None)
            .await
            .expect("Should handle missing custom table");
        assert_eq!(empty.len(), 0);

        // Create custom table and add migration
        tracker.ensure_table_exists(&*db).await.unwrap();
        tracker.record_migration(&*db, "custom_001").await.unwrap();

        // Verify it works with custom table
        let migrations = tracker.get_applied_migrations(&*db, None).await.unwrap();

        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].id, "custom_001");
    }
}
