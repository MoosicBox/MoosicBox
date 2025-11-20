//! # Switchy Schema - Generic Database Migration System
//!
//! A flexible, generic database migration system that works with any database supported by
//! `switchy_database`. This library provides a clean API for managing database schema evolution
//! with support for multiple migration sources, rollback functionality, and comprehensive testing utilities.
//!
//! ## Core Features
//!
//! * **Multiple Migration Sources**: Directory-based, embedded, and code-based migrations
//! * **Database Agnostic**: Works with `SQLite`, `PostgreSQL`, `MySQL`, and more
//! * **Rollback Support**: Safe migration rollbacks with validation
//! * **Migration Listing**: View available migrations and their applied status
//! * **Comprehensive Testing**: Rich testing utilities for migration validation
//! * **Custom Table Names**: Configurable migration tracking table names
//!
//! ## Quick Start
//!
//! ### Basic Usage with Embedded Migrations
//!
//! ```rust,ignore
//! use switchy_schema::runner::MigrationRunner;
//! use include_dir::{Dir, include_dir};
//!
//! // Include migrations directory at compile time
//! static MIGRATIONS: Dir<'static> = include_dir!("migrations");
//!
//! # async fn example(db: &dyn switchy_database::Database) -> switchy_schema::Result<()> {
//! // Create and run migrations
//! let runner = MigrationRunner::new_embedded(&MIGRATIONS);
//! runner.run(db).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Listing Available Migrations
//!
//! ```rust,ignore
//! # use switchy_schema::runner::MigrationRunner;
//! # use include_dir::{Dir, include_dir};
//! # static MIGRATIONS: Dir<'static> = include_dir!("migrations");
//! # async fn example(db: &dyn switchy_database::Database) -> switchy_schema::Result<()> {
//! let runner = MigrationRunner::new_embedded(&MIGRATIONS);
//!
//! // Get migration status
//! let migrations = runner.list_migrations(db).await?;
//! for info in migrations {
//!     let status = if info.applied { "✓" } else { "○" };
//!     println!("{} {} - {}", status, info.id, info.description.unwrap_or_default());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Custom Configuration
//!
//! ```rust,ignore
//! use switchy_schema::runner::{MigrationRunner, ExecutionStrategy};
//! # use include_dir::{Dir, include_dir};
//! # static MIGRATIONS: Dir<'static> = include_dir!("migrations");
//!
//! # async fn example(db: &dyn switchy_database::Database) -> switchy_schema::Result<()> {
//! let runner = MigrationRunner::new_embedded(&MIGRATIONS)
//!     .with_strategy(ExecutionStrategy::UpTo("20240315_add_users".to_string()))
//!     .with_table_name("my_migrations");
//!
//! runner.run(db).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Architecture Overview
//!
//! The library is organized around several key concepts:
//!
//! * **[`migration`]**: Core migration traits and types
//! * **[`discovery`]**: Different ways to discover and load migrations
//! * **[`runner`]**: Execution engine for running migrations
//! * **[`version`]**: Migration state tracking and version management
//!
//! ## Migration Sources
//!
//! Choose the migration source that best fits your needs:
//!
//! ### Embedded Migrations (Recommended)
//!
//! Compile migrations into your binary for distribution:
//!
//! ```rust,ignore
//! use switchy_schema::runner::MigrationRunner;
//! use include_dir::{Dir, include_dir};
//!
//! static MIGRATIONS: Dir<'static> = include_dir!("migrations");
//! let runner = MigrationRunner::new_embedded(&MIGRATIONS);
//! ```
//!
//! ### Directory Migrations
//!
//! Load migrations from filesystem at runtime:
//!
//! ```rust,no_run
//! # #[cfg(feature = "directory")]
//! # {
//! use switchy_schema::runner::MigrationRunner;
//!
//! let runner = MigrationRunner::new_directory("./migrations");
//! # }
//! ```
//!
//! ### Code Migrations
//!
//! Define migrations programmatically in Rust:
//!
//! ```rust,no_run
//! # #[cfg(feature = "code")]
//! # {
//! use switchy_schema::{
//!     runner::MigrationRunner,
//!     discovery::code::{CodeMigration, CodeMigrationSource}
//! };
//!
//! let mut source = CodeMigrationSource::new();
//! source.add_migration(CodeMigration::new(
//!     "001_create_users".to_string(),
//!     Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)".to_string()),
//!     Some(Box::new("DROP TABLE users".to_string())),
//! ));
//!
//! let runner = MigrationRunner::new(Box::new(source));
//! # }
//! ```
//!
//! ## Testing Support
//!
//! The library provides testing utilities for migration validation:
//!
//! ```rust,no_run
//! # #[cfg(feature = "test-utils")]
//! # {
//! use switchy_schema::test_utils::TestMigrationBuilder;
//!
//! // Create a test migration builder
//! let builder = TestMigrationBuilder::new();
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod checksum_database;
pub mod digest;
pub mod discovery;
pub mod migration;
pub mod runner;
pub mod version;

#[cfg(feature = "validation")]
pub mod validation;

#[cfg(feature = "test-utils")]
pub mod test_utils;

use switchy_database::DatabaseError;
use thiserror::Error;

pub use checksum_database::{ChecksumDatabase, calculate_hash};
pub use digest::Digest;
pub use runner::{MarkAllCompletedSummary, MarkCompletedScope};

/// Detailed validation error information
///
/// This enum provides structured information about validation failures,
/// allowing callers to handle specific error cases appropriately.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// Migration not found in the database tracking table
    #[error("Migration '{id}' has not been run yet")]
    NotTracked { id: String },

    /// Migration exists but is in wrong state for operation
    #[error("Migration '{id}' is in {current:?} state, expected {expected:?}")]
    WrongState {
        id: String,
        current: migration::MigrationStatus,
        expected: migration::MigrationStatus,
    },

    /// Migration not found in migration source
    #[error("Migration '{id}' not found in migration source")]
    NotInSource { id: String },

    /// Migration already in target state
    #[error("Migration '{id}' is already {state:?}")]
    AlreadyInState {
        id: String,
        state: migration::MigrationStatus,
    },

    /// Invalid migration status string
    #[error(
        "Invalid migration status: '{value}'. Valid values are: in_progress, completed, failed"
    )]
    InvalidStatus { value: String },

    /// Generic validation error (for backward compatibility)
    #[error("{0}")]
    Generic(String),
}

impl From<String> for ValidationError {
    fn from(msg: String) -> Self {
        Self::Generic(msg)
    }
}

impl From<&str> for ValidationError {
    fn from(msg: &str) -> Self {
        Self::Generic(msg.to_string())
    }
}

/// Type of checksum that can have a mismatch
///
/// Used to distinguish between up migration and down migration checksum validation failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumType {
    /// Up migration checksum
    Up,
    /// Down migration checksum
    Down,
}

impl std::fmt::Display for ChecksumType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Up => write!(f, "up"),
            Self::Down => write!(f, "down"),
        }
    }
}

#[cfg(test)]
mod checksum_type_tests {
    use super::*;

    #[test_log::test]
    fn test_checksum_type_display() {
        assert_eq!(ChecksumType::Up.to_string(), "up");
        assert_eq!(ChecksumType::Down.to_string(), "down");
    }

    #[test_log::test]
    fn test_checksum_type_equality() {
        assert_eq!(ChecksumType::Up, ChecksumType::Up);
        assert_eq!(ChecksumType::Down, ChecksumType::Down);
        assert_ne!(ChecksumType::Up, ChecksumType::Down);
    }

    #[test_log::test]
    fn test_checksum_type_debug() {
        assert_eq!(format!("{:?}", ChecksumType::Up), "Up");
        assert_eq!(format!("{:?}", ChecksumType::Down), "Down");
    }

    #[test_log::test]
    fn test_checksum_type_copy() {
        let up = ChecksumType::Up;
        let up_copy = up;
        assert_eq!(up, up_copy); // Verifies Copy trait works

        let down = ChecksumType::Down;
        let down_copy = down;
        assert_eq!(down, down_copy);
    }

    #[test_log::test]
    fn test_checksum_type_clone() {
        let up = ChecksumType::Up;
        let up_clone = up;
        assert_eq!(up, up_clone);

        let down = ChecksumType::Down;
        let down_clone = down;
        assert_eq!(down, down_clone);
    }
}

/// Details about a checksum validation failure
///
/// Contains information about which migration failed validation,
/// which checksum type (up/down) failed, and the expected vs actual checksums.
#[derive(Debug, Clone)]
pub struct ChecksumMismatch {
    /// ID of the migration with checksum mismatch
    pub migration_id: String,
    /// Type of checksum that failed validation
    pub checksum_type: ChecksumType,
    /// Expected checksum (stored in database) as hex string
    pub stored_checksum: String,
    /// Actual checksum (calculated from current migration) as hex string
    pub current_checksum: String,
}

impl std::fmt::Display for ChecksumMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Migration '{}' {} checksum mismatch: stored={}, current={}",
            self.migration_id, self.checksum_type, self.stored_checksum, self.current_checksum
        )
    }
}

/// Errors that can occur during migration operations
///
/// This enum covers all possible error conditions in the migration system,
/// from database connectivity issues to migration-specific failures.
#[derive(Debug, Error)]
pub enum MigrationError {
    /// Database operation failed
    ///
    /// Wraps underlying database errors from `switchy_database`.
    #[error(transparent)]
    Database(#[from] DatabaseError),

    /// File system I/O error
    ///
    /// Occurs when reading migration files from the filesystem.
    #[error("IO error")]
    Io(#[from] std::io::Error),

    /// Migration discovery failed
    ///
    /// Occurs when the system cannot find or parse migration definitions.
    #[error("Migration discovery failed: {0}")]
    Discovery(String),

    /// Migration validation failed
    ///
    /// Occurs when migration content or structure is invalid.
    #[error("Migration validation failed: {0}")]
    Validation(String),

    /// Migration execution failed
    ///
    /// Occurs when a migration fails to execute successfully.
    #[error("Migration execution failed: {0}")]
    Execution(String),

    /// Dirty state detected
    ///
    /// Occurs when there are migrations in '`in_progress`' state that prevent new migrations from running.
    #[error("Dirty migration state detected. Migrations in progress: {}", migrations.join(", "))]
    DirtyState {
        /// List of migration IDs that are in dirty state
        migrations: Vec<String>,
    },

    /// Invalid checksum
    ///
    /// Occurs when a migration checksum is not exactly 32 bytes.
    #[error("Invalid checksum: {0}")]
    InvalidChecksum(String),

    /// Checksum validation failed
    ///
    /// Occurs when stored checksums don't match current migration content.
    /// Contains a list of all mismatched migrations for comprehensive reporting.
    #[error("Checksum validation failed: {} mismatch(es) found", mismatches.len())]
    ChecksumValidationFailed {
        /// List of all checksum mismatches found during validation
        mismatches: Vec<ChecksumMismatch>,
    },
}

impl From<moosicbox_json_utils::ParseError> for MigrationError {
    fn from(err: moosicbox_json_utils::ParseError) -> Self {
        Self::Validation(format!("Parse error: {err}"))
    }
}

/// Result type alias for migration operations
///
/// Most functions in this crate return this type for consistent error handling.
pub type Result<T> = std::result::Result<T, MigrationError>;

#[cfg(test)]
mod checksum_mismatch_tests {
    use super::*;

    #[test_log::test]
    fn test_checksum_mismatch_display() {
        let mismatch = ChecksumMismatch {
            migration_id: "001_create_users".to_string(),
            checksum_type: ChecksumType::Up,
            stored_checksum: "abc123".to_string(),
            current_checksum: "def456".to_string(),
        };

        let display = mismatch.to_string();
        assert!(display.contains("001_create_users"));
        assert!(display.contains("up"));
        assert!(display.contains("abc123"));
        assert!(display.contains("def456"));
        assert!(display.contains("mismatch"));
    }

    #[test_log::test]
    fn test_checksum_mismatch_down_type() {
        let mismatch = ChecksumMismatch {
            migration_id: "002_add_indexes".to_string(),
            checksum_type: ChecksumType::Down,
            stored_checksum: "stored_hash".to_string(),
            current_checksum: "current_hash".to_string(),
        };

        let display = mismatch.to_string();
        assert!(display.contains("002_add_indexes"));
        assert!(display.contains("down"));
        assert!(display.contains("stored_hash"));
        assert!(display.contains("current_hash"));
    }

    #[test_log::test]
    fn test_checksum_mismatch_debug() {
        let mismatch = ChecksumMismatch {
            migration_id: "test".to_string(),
            checksum_type: ChecksumType::Up,
            stored_checksum: "s".to_string(),
            current_checksum: "c".to_string(),
        };

        let debug = format!("{mismatch:?}");
        assert!(debug.contains("ChecksumMismatch"));
        assert!(debug.contains("test"));
    }

    #[test_log::test]
    fn test_checksum_mismatch_clone() {
        let original = ChecksumMismatch {
            migration_id: "test".to_string(),
            checksum_type: ChecksumType::Up,
            stored_checksum: "stored".to_string(),
            current_checksum: "current".to_string(),
        };

        let cloned = original.clone();
        assert_eq!(original.migration_id, cloned.migration_id);
        assert_eq!(original.checksum_type, cloned.checksum_type);
        assert_eq!(original.stored_checksum, cloned.stored_checksum);
        assert_eq!(original.current_checksum, cloned.current_checksum);
    }
}

#[cfg(test)]
mod validation_error_tests {
    use super::*;

    #[test_log::test]
    fn test_validation_error_not_tracked() {
        let err = ValidationError::NotTracked {
            id: "001_test".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("001_test"));
        assert!(display.contains("has not been run"));
    }

    #[test_log::test]
    fn test_validation_error_wrong_state() {
        let err = ValidationError::WrongState {
            id: "002_test".to_string(),
            current: migration::MigrationStatus::InProgress,
            expected: migration::MigrationStatus::Completed,
        };
        let display = err.to_string();
        assert!(display.contains("002_test"));
        assert!(display.contains("InProgress"));
        assert!(display.contains("Completed"));
    }

    #[test_log::test]
    fn test_validation_error_not_in_source() {
        let err = ValidationError::NotInSource {
            id: "003_test".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("003_test"));
        assert!(display.contains("not found in migration source"));
    }

    #[test_log::test]
    fn test_validation_error_already_in_state() {
        let err = ValidationError::AlreadyInState {
            id: "004_test".to_string(),
            state: migration::MigrationStatus::Completed,
        };
        let display = err.to_string();
        assert!(display.contains("004_test"));
        assert!(display.contains("already"));
        assert!(display.contains("Completed"));
    }

    #[test_log::test]
    fn test_validation_error_invalid_status() {
        let err = ValidationError::InvalidStatus {
            value: "unknown_status".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("unknown_status"));
        assert!(display.contains("Invalid migration status"));
        assert!(display.contains("in_progress"));
        assert!(display.contains("completed"));
        assert!(display.contains("failed"));
    }

    #[test_log::test]
    fn test_validation_error_generic() {
        let err = ValidationError::Generic("Custom error message".to_string());
        assert_eq!(err.to_string(), "Custom error message");
    }

    #[test_log::test]
    fn test_validation_error_from_string() {
        let err: ValidationError = "Test error".into();
        assert_eq!(err.to_string(), "Test error");

        let err: ValidationError = String::from("Another error").into();
        assert_eq!(err.to_string(), "Another error");
    }

    #[test_log::test]
    fn test_validation_error_debug() {
        let err = ValidationError::NotTracked {
            id: "test".to_string(),
        };
        let debug = format!("{err:?}");
        assert!(debug.contains("NotTracked"));
        assert!(debug.contains("test"));
    }
}

#[cfg(test)]
mod migration_error_tests {
    use super::*;
    use switchy_database::DatabaseError;

    #[test_log::test]
    fn test_migration_error_discovery() {
        let err = MigrationError::Discovery("Failed to find migrations".to_string());
        let display = err.to_string();
        assert!(display.contains("Migration discovery failed"));
        assert!(display.contains("Failed to find migrations"));
    }

    #[test_log::test]
    fn test_migration_error_validation() {
        let err = MigrationError::Validation("Invalid migration format".to_string());
        let display = err.to_string();
        assert!(display.contains("Migration validation failed"));
        assert!(display.contains("Invalid migration format"));
    }

    #[test_log::test]
    fn test_migration_error_execution() {
        let err = MigrationError::Execution("SQL execution failed".to_string());
        let display = err.to_string();
        assert!(display.contains("Migration execution failed"));
        assert!(display.contains("SQL execution failed"));
    }

    #[test_log::test]
    fn test_migration_error_dirty_state() {
        let err = MigrationError::DirtyState {
            migrations: vec!["001_first".to_string(), "002_second".to_string()],
        };
        let display = err.to_string();
        assert!(display.contains("Dirty migration state"));
        assert!(display.contains("001_first"));
        assert!(display.contains("002_second"));
    }

    #[test_log::test]
    fn test_migration_error_dirty_state_single() {
        let err = MigrationError::DirtyState {
            migrations: vec!["single_migration".to_string()],
        };
        let display = err.to_string();
        assert!(display.contains("single_migration"));
    }

    #[test_log::test]
    fn test_migration_error_invalid_checksum() {
        let err = MigrationError::InvalidChecksum("Checksum too short".to_string());
        let display = err.to_string();
        assert!(display.contains("Invalid checksum"));
        assert!(display.contains("Checksum too short"));
    }

    #[test_log::test]
    fn test_migration_error_checksum_validation_failed() {
        let mismatches = vec![
            ChecksumMismatch {
                migration_id: "001_test".to_string(),
                checksum_type: ChecksumType::Up,
                stored_checksum: "abc".to_string(),
                current_checksum: "def".to_string(),
            },
            ChecksumMismatch {
                migration_id: "002_test".to_string(),
                checksum_type: ChecksumType::Down,
                stored_checksum: "ghi".to_string(),
                current_checksum: "jkl".to_string(),
            },
        ];
        let err = MigrationError::ChecksumValidationFailed { mismatches };
        let display = err.to_string();
        assert!(display.contains("Checksum validation failed"));
        assert!(display.contains("2 mismatch(es)"));
    }

    #[test_log::test]
    fn test_migration_error_from_database_error() {
        let db_err = DatabaseError::TransactionFailed;
        let migration_err: MigrationError = db_err.into();
        let display = migration_err.to_string();
        assert!(display.contains("Transaction failed"));
    }

    #[test_log::test]
    fn test_migration_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let migration_err: MigrationError = io_err.into();
        let display = migration_err.to_string();
        assert!(display.contains("IO error"));
    }

    #[test_log::test]
    fn test_migration_error_from_parse_error() {
        let parse_err = moosicbox_json_utils::ParseError::ConvertType("test type".to_string());
        let migration_err: MigrationError = parse_err.into();
        let display = migration_err.to_string();
        assert!(display.contains("Parse error"));
        assert!(display.contains("test type"));
    }

    #[test_log::test]
    fn test_migration_error_debug() {
        let err = MigrationError::Discovery("test".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("Discovery"));
        assert!(debug.contains("test"));
    }
}
