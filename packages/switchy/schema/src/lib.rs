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
}

/// Result type alias for migration operations
///
/// Most functions in this crate return this type for consistent error handling.
pub type Result<T> = std::result::Result<T, MigrationError>;
