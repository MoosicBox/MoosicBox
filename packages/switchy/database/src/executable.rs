//! Trait for executing database operations
//!
//! This module provides the [`Executable`] trait that allows various types of SQL operations
//! (raw SQL strings, query builders, schema statements) to be executed against a database
//! through a unified interface.
//!
//! # Supported Types
//!
//! The `Executable` trait is implemented for:
//! * **Raw SQL**: `String` and `&str` execute as raw SQL statements
//! * **Query builders**: `InsertStatement`, `UpdateStatement`, `DeleteStatement`, `UpsertStatement`
//! * **Schema statements**: `CreateTableStatement`, `DropTableStatement`, `CreateIndexStatement`,
//!   `DropIndexStatement`, `AlterTableStatement`
//!
//! # Example
//!
//! ```rust,ignore
//! use switchy_database::{Database, Executable};
//!
//! async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
//!     // Execute raw SQL
//!     "CREATE TABLE temp (id INT)".execute(db).await?;
//!
//!     // Execute query builder
//!     db.insert("temp")
//!         .value("id", 1)
//!         .execute(db)
//!         .await?;
//!
//!     Ok(())
//! }
//! ```

use crate::{Database, DatabaseError};
use async_trait::async_trait;

/// Trait for types that can be executed against a database
///
/// This trait provides a unified interface for executing different types of SQL operations,
/// whether they are raw SQL strings or structured query builders.
#[async_trait]
pub trait Executable: Send + Sync {
    /// Execute this operation against the database
    ///
    /// # Errors
    ///
    /// * If database execution fails
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError>;
}

/// Implement `Executable` for String (raw SQL)
#[async_trait]
impl Executable for String {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_raw(self).await
    }
}

/// Implement `Executable` for &str (raw SQL)
#[async_trait]
impl Executable for &str {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_raw(self).await
    }
}

/// Implement `Executable` for `CreateTableStatement`
#[cfg(feature = "schema")]
#[async_trait]
impl Executable for crate::schema::CreateTableStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_create_table(self).await
    }
}

/// Implement `Executable` for `DropTableStatement`
#[cfg(feature = "schema")]
#[async_trait]
impl Executable for crate::schema::DropTableStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_drop_table(self).await
    }
}

/// Implement `Executable` for `CreateIndexStatement`
#[cfg(feature = "schema")]
#[async_trait]
impl Executable for crate::schema::CreateIndexStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_create_index(self).await
    }
}

/// Implement `Executable` for `DropIndexStatement`
#[cfg(feature = "schema")]
#[async_trait]
impl Executable for crate::schema::DropIndexStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_drop_index(self).await
    }
}

/// Implement `Executable` for `AlterTableStatement`
#[cfg(feature = "schema")]
#[async_trait]
impl Executable for crate::schema::AlterTableStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_alter_table(self).await
    }
}

/// Implement `Executable` for `InsertStatement`
#[async_trait]
impl Executable for crate::query::InsertStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_insert(self).await.map(|_| ())
    }
}

/// Implement `Executable` for `UpdateStatement`
#[async_trait]
impl Executable for crate::query::UpdateStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_update(self).await.map(|_| ())
    }
}

/// Implement `Executable` for `DeleteStatement`
#[async_trait]
impl Executable for crate::query::DeleteStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_delete(self).await.map(|_| ())
    }
}

/// Implement `Executable` for `UpsertStatement`
#[async_trait]
impl Executable for crate::query::UpsertStatement<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_upsert(self).await.map(|_| ())
    }
}
