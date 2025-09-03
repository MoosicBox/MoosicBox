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

use crate::Result;
use chrono::NaiveDateTime;
use switchy_database::{
    Database, DatabaseValue,
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
    pub status: String,
    pub failure_reason: Option<String>,
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
                && status_str == "completed"
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
            .value("status", "completed")
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
            .value("status", "in_progress")
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
        status: &str,
        failure_reason: Option<String>,
    ) -> Result<()> {
        db.update(&self.table_name)
            .value("status", status)
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

        if let Some(row) = results.into_iter().next() {
            let id = row
                .get("id")
                .and_then(|v| v.as_str().map(ToString::to_string))
                .ok_or_else(|| crate::MigrationError::Validation("Missing id field".into()))?;

            let Some(DatabaseValue::DateTime(run_on)) = row.get("run_on") else {
                return Err(crate::MigrationError::Validation(
                    "Invalid run_on field".into(),
                ));
            };

            let finished_on = match row.get("finished_on") {
                Some(DatabaseValue::DateTime(dt)) => Some(dt),
                Some(DatabaseValue::Null) | None => None,
                _ => {
                    return Err(crate::MigrationError::Validation(
                        "Invalid finished_on field".into(),
                    ));
                }
            };

            let status = row
                .get("status")
                .and_then(|v| v.as_str().map(ToString::to_string))
                .ok_or_else(|| crate::MigrationError::Validation("Missing status field".into()))?;

            let failure_reason = match row.get("failure_reason") {
                Some(DatabaseValue::String(reason)) => Some(reason),
                Some(DatabaseValue::Null) | None => None,
                _ => {
                    return Err(crate::MigrationError::Validation(
                        "Invalid failure_reason field".into(),
                    ));
                }
            };

            Ok(Some(MigrationRecord {
                id,
                run_on,
                finished_on,
                status,
                failure_reason,
            }))
        } else {
            Ok(None)
        }
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
            .filter(Box::new(where_not_eq("status", "completed")))
            .sort("run_on", switchy_database::query::SortDirection::Asc)
            .execute(db)
            .await?;

        let mut records = Vec::new();
        for row in results {
            let id = row
                .get("id")
                .and_then(|v| v.as_str().map(ToString::to_string))
                .ok_or_else(|| crate::MigrationError::Validation("Missing id field".into()))?;

            let Some(DatabaseValue::DateTime(run_on)) = row.get("run_on") else {
                return Err(crate::MigrationError::Validation(
                    "Invalid run_on field".into(),
                ));
            };

            let finished_on = match row.get("finished_on") {
                Some(DatabaseValue::DateTime(dt)) => Some(dt),
                Some(DatabaseValue::Null) | None => None,
                _ => {
                    return Err(crate::MigrationError::Validation(
                        "Invalid finished_on field".into(),
                    ));
                }
            };

            let status = row
                .get("status")
                .and_then(|v| v.as_str().map(ToString::to_string))
                .ok_or_else(|| crate::MigrationError::Validation("Missing status field".into()))?;

            let failure_reason = match row.get("failure_reason") {
                Some(DatabaseValue::String(reason)) => Some(reason),
                Some(DatabaseValue::Null) | None => None,
                _ => {
                    return Err(crate::MigrationError::Validation(
                        "Invalid failure_reason field".into(),
                    ));
                }
            };

            records.push(MigrationRecord {
                id,
                run_on,
                finished_on,
                status,
                failure_reason,
            });
        }

        Ok(records)
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
                    if status_str == "completed" {
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
}

impl Default for VersionTracker {
    fn default() -> Self {
        Self::new()
    }
}
