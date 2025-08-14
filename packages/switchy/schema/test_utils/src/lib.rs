#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Test utilities for `switchy_schema` migration testing
//!
//! This crate provides comprehensive testing infrastructure for verifying migration
//! correctness and behavior. It supports testing migrations with fresh databases,
//! pre-seeded state, and interleaved mutations between migrations.

use switchy_database::DatabaseError;
use switchy_schema::MigrationError;

#[cfg(feature = "sqlite")]
use switchy_database::Database;

/// Re-export core types for convenience
pub use switchy_database;
pub use switchy_schema;

/// Test error type that wraps existing errors from `switchy_schema` and `switchy_database`
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    /// Migration error
    #[error(transparent)]
    Migration(#[from] MigrationError),
    /// Database error
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// Feature-gated helper to create an empty in-memory `SQLite` database
///
/// # Errors
///
/// * If the `SQLite` database initialization fails
#[cfg(feature = "sqlite")]
pub async fn create_empty_in_memory()
-> Result<Box<dyn Database>, switchy_database_connection::InitSqliteSqlxDatabaseError> {
    // Create in-memory SQLite database using sqlx
    switchy_database_connection::init_sqlite_sqlx(None).await
}
