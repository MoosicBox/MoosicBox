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
