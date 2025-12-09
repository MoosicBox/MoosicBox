//! # Migration Traits with Lifetime Support
//!
//! This module provides the core traits for database migrations with sophisticated
//! lifetime management, supporting both owned and borrowed data patterns.
//!
//! ## Lifetime Patterns
//!
//! ### Static Migrations (`'static`)
//! Most migrations own their data and use the `'static` lifetime:
//! - [`EmbeddedMigration`](crate::discovery::embedded::EmbeddedMigration) - Owns compiled-in bytes
//! - [`FileMigration`](crate::discovery::directory::FileMigration) - Owns loaded file content
//! - [`CodeMigration`](crate::discovery::code::CodeMigration) with owned SQL strings
//!
//! ```rust
//! use std::sync::Arc;
//! use switchy_schema::migration::Migration;
//! use switchy_database::Database;
//! use async_trait::async_trait;
//!
//! struct MyMigration {
//!     id: String,
//!     sql: String,
//! }
//!
//! #[async_trait]
//! impl Migration<'static> for MyMigration {
//!     fn id(&self) -> &str {
//!         &self.id
//!     }
//!
//!     async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
//!         db.exec_raw(&self.sql).await?;
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ### Borrowed Migrations (`'a`)
//! Advanced use cases can borrow data with explicit lifetimes:
//! - Query builders that reference external data
//! - Temporary migrations from configuration
//! - Migrations generated from borrowed schemas
//!
//! ```rust
//! use switchy_schema::migration::Migration;
//! use switchy_database::{Database, schema::CreateTableStatement};
//! use async_trait::async_trait;
//!
//! struct BorrowedMigration<'a> {
//!     id: String,
//!     create_stmt: &'a CreateTableStatement<'a>,
//! }
//!
//! #[async_trait]
//! impl<'a> Migration<'a> for BorrowedMigration<'a> {
//!     fn id(&self) -> &str {
//!         &self.id
//!     }
//!
//!     async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
//!         db.exec_create_table(self.create_stmt).await?;
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Migration Sources
//!
//! Migration sources provide collections of migrations and also use lifetime parameters:
//!
//! ```rust
//! use std::sync::Arc;
//! use switchy_schema::migration::{Migration, MigrationSource};
//! use async_trait::async_trait;
//!
//! struct MyMigrationSource {
//!     migrations: Vec<Arc<dyn Migration<'static> + 'static>>,
//! }
//!
//! #[async_trait]
//! impl MigrationSource<'static> for MyMigrationSource {
//!     async fn migrations(&self) -> switchy_schema::Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
//!         // Return owned migrations
//!         Ok(vec![])
//!     }
//! }
//! ```
//!
//! ## Best Practices
//!
//! - **Use `'static` for most cases** - This covers 99% of migration use cases
//! - **Use `'a` only when borrowing** - For advanced scenarios with borrowed data
//! - **Embedded and Directory migrations are always `'static`** - They own their data
//! - **Code migrations can be either** - Depending on whether they own or borrow data

use crate::Result;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDateTime;
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType};
use switchy_database::Database;
use switchy_database::DatabaseValue;

/// Information about a migration, including its current status
///
/// This struct provides a summary view of a migration's state, combining
/// information from the migration source (ID, description) with runtime
/// state from the database (applied status, timestamps, failure info).
///
/// Returned by [`MigrationSource::list`] and [`crate::runner::MigrationRunner::list_migrations`].
///
/// # Fields
///
/// * `id` - Unique migration identifier
/// * `description` - Optional human-readable description
/// * `applied` - Whether the migration has been successfully applied
/// * `status` - Detailed status (in-progress, completed, failed)
/// * `failure_reason` - Error message if migration failed
/// * `run_on` / `finished_on` - Execution timestamps
///
/// # Examples
///
/// ```rust
/// use switchy_schema::migration::MigrationInfo;
///
/// let info = MigrationInfo {
///     id: "001_create_users".to_string(),
///     description: Some("Create the users table".to_string()),
///     applied: true,
///     status: None,
///     failure_reason: None,
///     run_on: None,
///     finished_on: None,
/// };
///
/// if info.applied {
///     println!("✓ {} - {}", info.id, info.description.unwrap_or_default());
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationInfo {
    /// Migration ID
    pub id: String,
    /// Migration description if available
    pub description: Option<String>,
    /// Whether this migration has been applied
    pub applied: bool,
    /// Detailed status information (populated only when database is available)
    pub status: Option<MigrationStatus>,
    /// Error message if status == Failed
    pub failure_reason: Option<String>,
    /// When migration started
    pub run_on: Option<NaiveDateTime>,
    /// When migration completed/failed
    pub finished_on: Option<NaiveDateTime>,
}

/// Migration execution status
///
/// Represents the current state of a migration in the tracking table.
/// Migrations progress through these states during execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStatus {
    /// Migration is currently being executed
    InProgress,
    /// Migration completed successfully
    Completed,
    /// Migration execution failed
    Failed,
}

impl std::fmt::Display for MigrationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        };
        write!(f, "{status_str}")
    }
}

impl std::str::FromStr for MigrationStatus {
    type Err = ParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(ParseError::ConvertType(format!(
                "Invalid migration status: '{s}'. Valid values are: in_progress, completed, failed"
            ))),
        }
    }
}

impl MissingValue<MigrationStatus> for &DatabaseValue {}
impl MissingValue<MigrationStatus> for DatabaseValue {}

impl ToValueType<MigrationStatus> for &DatabaseValue {
    fn to_value_type(self) -> std::result::Result<MigrationStatus, ParseError> {
        let status_str = self
            .as_str()
            .ok_or_else(|| ParseError::ConvertType("MigrationStatus".into()))?;
        status_str
            .parse()
            .map_err(|_| ParseError::ConvertType("MigrationStatus".into()))
    }
}

impl ToValueType<MigrationStatus> for DatabaseValue {
    fn to_value_type(self) -> std::result::Result<MigrationStatus, ParseError> {
        (&self).to_value_type()
    }
}

#[async_trait]
pub trait Migration<'a>: Send + Sync + 'a {
    /// Get the unique identifier for this migration
    ///
    /// The ID is used to track which migrations have been applied and to determine
    /// migration order. IDs should be unique and sortable (e.g., timestamped).
    fn id(&self) -> &str;

    /// Execute the migration (forward direction)
    ///
    /// This method applies the migration changes to the database. It should be
    /// idempotent when possible, or the migration system will track execution to
    /// prevent duplicate runs.
    ///
    /// # Errors
    ///
    /// * Returns an error if the migration execution fails
    async fn up(&self, db: &dyn Database) -> Result<()>;

    /// Rollback the migration (reverse direction)
    ///
    /// This method reverses the changes made by `up()`. The default implementation
    /// does nothing, making the migration non-reversible.
    ///
    /// # Errors
    ///
    /// * Returns an error if the rollback fails
    async fn down(&self, _db: &dyn Database) -> Result<()> {
        Ok(())
    }

    /// Calculate the checksum for the up migration
    ///
    /// # Errors
    ///
    /// Returns an error if checksum calculation fails
    async fn up_checksum(&self) -> Result<bytes::Bytes> {
        // Default returns 32 zero bytes
        Ok(bytes::Bytes::from(vec![0u8; 32]))
    }

    /// Calculate the checksum for the down migration
    ///
    /// # Errors
    ///
    /// Returns an error if checksum calculation fails
    async fn down_checksum(&self) -> Result<bytes::Bytes> {
        // Default returns 32 zero bytes
        Ok(bytes::Bytes::from(vec![0u8; 32]))
    }

    /// Get a human-readable description of this migration
    ///
    /// This is used for display purposes when listing migrations. The default
    /// implementation returns `None`.
    fn description(&self) -> Option<&str> {
        None
    }

    /// Get the list of database types this migration supports
    ///
    /// The default implementation returns all supported databases. Override this
    /// to restrict a migration to specific database types.
    fn supported_databases(&self) -> Vec<&str> {
        vec!["sqlite", "postgres", "mysql"]
    }
}

#[async_trait]
pub trait MigrationSource<'a>: Send + Sync {
    /// Get all available migrations from this source
    ///
    /// Returns a vector of migration trait objects that can be executed.
    /// Migrations should be returned in the order they should be applied.
    ///
    /// # Errors
    ///
    /// * Returns an error if migrations cannot be discovered or loaded
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'a> + 'a>>>;

    /// List available migrations with their metadata
    ///
    /// Returns a list of migration information including ID, description, and status.
    /// The applied status is set to false by default - use `MigrationRunner::list()`
    /// to get the actual applied status from the database.
    ///
    /// # Errors
    ///
    /// * If migration discovery fails
    async fn list(&self) -> Result<Vec<MigrationInfo>> {
        let migrations = self.migrations().await?;
        Ok(migrations
            .into_iter()
            .map(|migration| MigrationInfo {
                id: migration.id().to_string(),
                description: migration.description().map(ToString::to_string),
                applied: false, // Default - actual status determined by MigrationRunner
                status: None,   // Populated only when database is available
                failure_reason: None,
                run_on: None,
                finished_on: None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use switchy_database::Database;

    // Mock migration for testing
    struct MockMigration {
        id: String,
        description: Option<String>,
    }

    #[async_trait]
    impl Migration<'static> for MockMigration {
        fn id(&self) -> &str {
            &self.id
        }

        async fn up(&self, _db: &dyn Database) -> Result<()> {
            Ok(())
        }

        fn description(&self) -> Option<&str> {
            self.description.as_deref()
        }
    }

    // Mock migration source for testing default list() implementation
    struct MockMigrationSource {
        migrations: Vec<Arc<dyn Migration<'static> + 'static>>,
    }

    #[async_trait]
    impl MigrationSource<'static> for MockMigrationSource {
        async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
            Ok(self.migrations.clone())
        }
    }

    #[test_log::test]
    fn test_migration_info_creation() {
        let info = MigrationInfo {
            id: "001_test_migration".to_string(),
            description: Some("Test migration".to_string()),
            applied: false,
            status: None,
            failure_reason: None,
            run_on: None,
            finished_on: None,
        };

        assert_eq!(info.id, "001_test_migration");
        assert_eq!(info.description, Some("Test migration".to_string()));
        assert!(!info.applied);
    }

    #[switchy_async::test]
    async fn test_default_list_implementation() {
        let migrations = vec![
            Arc::new(MockMigration {
                id: "001_first".to_string(),
                description: Some("First migration".to_string()),
            }) as Arc<dyn Migration<'static> + 'static>,
            Arc::new(MockMigration {
                id: "002_second".to_string(),
                description: None,
            }) as Arc<dyn Migration<'static> + 'static>,
        ];

        let source = MockMigrationSource {
            migrations: migrations.clone(),
        };

        let list = source.list().await.unwrap();

        assert_eq!(list.len(), 2);

        // First migration
        assert_eq!(list[0].id, "001_first");
        assert_eq!(list[0].description, Some("First migration".to_string()));
        assert!(!list[0].applied); // Default should be false

        // Second migration
        assert_eq!(list[1].id, "002_second");
        assert_eq!(list[1].description, None);
        assert!(!list[1].applied); // Default should be false
    }

    #[test_log::test]
    fn test_migration_status_display() {
        assert_eq!(MigrationStatus::InProgress.to_string(), "in_progress");
        assert_eq!(MigrationStatus::Completed.to_string(), "completed");
        assert_eq!(MigrationStatus::Failed.to_string(), "failed");
    }

    #[test_log::test]
    fn test_migration_status_from_str_valid() {
        use std::str::FromStr;

        assert_eq!(
            MigrationStatus::from_str("in_progress").unwrap(),
            MigrationStatus::InProgress
        );
        assert_eq!(
            MigrationStatus::from_str("completed").unwrap(),
            MigrationStatus::Completed
        );
        assert_eq!(
            MigrationStatus::from_str("failed").unwrap(),
            MigrationStatus::Failed
        );
    }

    #[test_log::test]
    fn test_migration_status_from_str_invalid() {
        use std::str::FromStr;

        let result = MigrationStatus::from_str("invalid_status");
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("Invalid migration status"));
        assert!(err_str.contains("invalid_status"));
        assert!(err_str.contains("in_progress"));
        assert!(err_str.contains("completed"));
        assert!(err_str.contains("failed"));
    }

    #[test_log::test]
    fn test_migration_status_from_str_case_sensitive() {
        use std::str::FromStr;

        // Should be case-sensitive
        assert!(MigrationStatus::from_str("COMPLETED").is_err());
        assert!(MigrationStatus::from_str("Completed").is_err());
        assert!(MigrationStatus::from_str("IN_PROGRESS").is_err());
        assert!(MigrationStatus::from_str("InProgress").is_err());
    }

    #[test_log::test]
    fn test_migration_status_equality() {
        assert_eq!(MigrationStatus::InProgress, MigrationStatus::InProgress);
        assert_eq!(MigrationStatus::Completed, MigrationStatus::Completed);
        assert_eq!(MigrationStatus::Failed, MigrationStatus::Failed);

        assert_ne!(MigrationStatus::InProgress, MigrationStatus::Completed);
        assert_ne!(MigrationStatus::InProgress, MigrationStatus::Failed);
        assert_ne!(MigrationStatus::Completed, MigrationStatus::Failed);
    }

    #[test_log::test]
    fn test_migration_status_copy() {
        let status = MigrationStatus::Completed;
        let status_copy = status;
        assert_eq!(status, status_copy);
    }

    #[test_log::test]
    fn test_migration_status_clone() {
        let status = MigrationStatus::Failed;
        let status_clone = status;
        assert_eq!(status, status_clone);
    }

    #[test_log::test]
    fn test_migration_info_equality() {
        let info1 = MigrationInfo {
            id: "001_test".to_string(),
            description: Some("Test".to_string()),
            applied: false,
            status: None,
            failure_reason: None,
            run_on: None,
            finished_on: None,
        };

        let info2 = MigrationInfo {
            id: "001_test".to_string(),
            description: Some("Test".to_string()),
            applied: false,
            status: None,
            failure_reason: None,
            run_on: None,
            finished_on: None,
        };

        assert_eq!(info1, info2);
    }

    #[test_log::test]
    fn test_migration_info_clone() {
        let original = MigrationInfo {
            id: "test".to_string(),
            description: Some("Description".to_string()),
            applied: true,
            status: Some(MigrationStatus::Completed),
            failure_reason: Some("Error".to_string()),
            run_on: None,
            finished_on: None,
        };

        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test_log::test]
    fn test_migration_info_debug() {
        let info = MigrationInfo {
            id: "test".to_string(),
            description: None,
            applied: false,
            status: Some(MigrationStatus::InProgress),
            failure_reason: None,
            run_on: None,
            finished_on: None,
        };

        let debug = format!("{info:?}");
        assert!(debug.contains("MigrationInfo"));
        assert!(debug.contains("test"));
        assert!(debug.contains("InProgress"));
    }

    #[switchy_async::test]
    async fn test_migration_default_checksums() {
        let migration = MockMigration {
            id: "test".to_string(),
            description: None,
        };

        // Default implementation returns 32 zero bytes
        let up_checksum = migration.up_checksum().await.unwrap();
        let down_checksum = migration.down_checksum().await.unwrap();

        assert_eq!(up_checksum.len(), 32);
        assert_eq!(down_checksum.len(), 32);
        assert_eq!(up_checksum, bytes::Bytes::from(vec![0u8; 32]));
        assert_eq!(down_checksum, bytes::Bytes::from(vec![0u8; 32]));
    }

    #[test_log::test]
    fn test_migration_default_supported_databases() {
        let migration = MockMigration {
            id: "test".to_string(),
            description: None,
        };

        let supported = migration.supported_databases();
        assert_eq!(supported.len(), 3);
        assert!(supported.contains(&"sqlite"));
        assert!(supported.contains(&"postgres"));
        assert!(supported.contains(&"mysql"));
    }
}
