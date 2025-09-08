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
use switchy_database::Database;

/// Information about a migration, including its current status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationInfo {
    /// Migration ID
    pub id: String,
    /// Migration description if available
    pub description: Option<String>,
    /// Whether this migration has been applied
    pub applied: bool,
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
    type Err = crate::MigrationError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(crate::MigrationError::Validation(format!(
                "Invalid migration status: '{s}'. Valid values are: in_progress, completed, failed"
            ))),
        }
    }
}

#[async_trait]
pub trait Migration<'a>: Send + Sync + 'a {
    fn id(&self) -> &str;

    async fn up(&self, db: &dyn Database) -> Result<()>;

    async fn down(&self, _db: &dyn Database) -> Result<()> {
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        None
    }

    fn supported_databases(&self) -> Vec<&str> {
        vec!["sqlite", "postgres", "mysql"]
    }
}

#[async_trait]
pub trait MigrationSource<'a>: Send + Sync {
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

    #[test]
    fn test_migration_info_creation() {
        let info = MigrationInfo {
            id: "001_test_migration".to_string(),
            description: Some("Test migration".to_string()),
            applied: false,
        };

        assert_eq!(info.id, "001_test_migration");
        assert_eq!(info.description, Some("Test migration".to_string()));
        assert!(!info.applied);
    }

    #[tokio::test]
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
}
