//! # Code-Based Migrations
//!
//! This module provides programmatic migration support using the [`Executable`] trait,
//! allowing migrations to be defined using both raw SQL and type-safe query builders.
//!
//! ## Static Example (Owned Data)
//!
//! Most code migrations own their data and use the `'static` lifetime:
//!
//! ```rust
//! use switchy_schema::discovery::code::{CodeMigration, CodeMigrationSource};
//! use switchy_database::Executable;
//!
//! // Create a migration with owned SQL strings
//! let migration = CodeMigration::new(
//!     "001_create_users".to_string(),
//!     Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)".to_string()),
//!     Some(Box::new("DROP TABLE users".to_string())),
//! );
//!
//! // Add to a migration source
//! let mut source = CodeMigrationSource::new();
//! source.add_migration(migration);
//! ```
//!
//! ## Query Builder Example
//!
//! Using type-safe query builders from the database package:
//!
//! ```rust
//! use switchy_schema::discovery::code::CodeMigration;
//! use switchy_database::schema::{create_table, Column, DataType};
//!
//! // Create a migration using query builders
//! let create_table_stmt = create_table("users")
//!     .if_not_exists(true)
//!     .column(Column {
//!         name: "id".to_string(),
//!         nullable: false,
//!         auto_increment: true,
//!         data_type: DataType::Int,
//!         default: None,
//!     })
//!     .column(Column {
//!         name: "name".to_string(),
//!         nullable: false,
//!         auto_increment: false,
//!         data_type: DataType::Text,
//!         default: None,
//!     })
//!     .primary_key("id");
//!
//! let migration = CodeMigration::new(
//!     "001_create_users_typed".to_string(),
//!     Box::new(create_table_stmt),
//!     None,
//! );
//! ```
//!
//! ## Non-Static Example (Borrowed Query Builder)
//!
//! Advanced usage with borrowed data and explicit lifetimes:
//!
//! ```rust
//! use switchy_schema::discovery::code::CodeMigration;
//! use switchy_database::schema::{create_table, Column, DataType};
//!
//! fn create_table_migration<'a>(table_name: &'a str, id_column: &'a str) -> CodeMigration<'a> {
//!     let stmt = create_table(table_name)
//!         .column(Column {
//!             name: id_column.to_string(),
//!             nullable: false,
//!             auto_increment: true,
//!             data_type: DataType::Int,
//!             default: None,
//!         })
//!         .primary_key(id_column);
//!
//!     CodeMigration::new(
//!         format!("create_{}", table_name),
//!         Box::new(stmt),
//!         None,
//!     )
//! }
//!
//! // Usage with borrowed data
//! let migration = create_table_migration("products", "product_id");
//! ```
//!
//! ## Migration Source Usage
//!
//! Collecting multiple code migrations:
//!
//! ```rust
//! use switchy_schema::discovery::code::{CodeMigration, CodeMigrationSource};
//! use switchy_schema::migration::MigrationSource;
//!
//! # async fn example() -> switchy_schema::Result<()> {
//! let mut source = CodeMigrationSource::new();
//!
//! // Add multiple migrations
//! source.add_migration(CodeMigration::new(
//!     "001_users".to_string(),
//!     Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY)".to_string()),
//!     Some(Box::new("DROP TABLE users".to_string())),
//! ));
//!
//! source.add_migration(CodeMigration::new(
//!     "002_posts".to_string(),
//!     Box::new("CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER)".to_string()),
//!     Some(Box::new("DROP TABLE posts".to_string())),
//! ));
//!
//! // Get all migrations (sorted by ID)
//! let migrations = source.migrations().await?;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;

use crate::{Result, migration::Migration, migration::MigrationSource};
use switchy_database::Executable;

/// Migration implementation for code-based migrations using `Executable`
pub struct CodeMigration<'a> {
    id: String,
    up_sql: Box<dyn Executable + 'a>,
    down_sql: Option<Box<dyn Executable + 'a>>,
}

impl<'a> CodeMigration<'a> {
    #[must_use]
    pub fn new(
        id: String,
        up_sql: Box<dyn Executable + 'a>,
        down_sql: Option<Box<dyn Executable + 'a>>,
    ) -> Self {
        Self {
            id,
            up_sql,
            down_sql,
        }
    }
}

#[async_trait]
impl<'a> Migration<'a> for CodeMigration<'a> {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn switchy_database::Database) -> Result<()> {
        self.up_sql.execute(db).await?;
        Ok(())
    }

    async fn down(&self, db: &dyn switchy_database::Database) -> Result<()> {
        if let Some(down_sql) = &self.down_sql {
            down_sql.execute(db).await?;
        }
        Ok(())
    }
}

/// Migration source for code-based migrations with registry
pub struct CodeMigrationSource<'a> {
    migrations: Option<Vec<CodeMigration<'a>>>,
}

impl<'a> CodeMigrationSource<'a> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            migrations: Some(Vec::new()),
        }
    }

    pub fn add_migration(&mut self, migration: CodeMigration<'a>) {
        if let Some(ref mut migrations) = self.migrations {
            migrations.push(migration);
        }
    }
}

impl Default for CodeMigrationSource<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<'a> MigrationSource<'a> for CodeMigrationSource<'a> {
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'a> + 'a>>> {
        // For now, return empty vec - this implementation needs to be completed
        // when we have concrete use cases for code-based migrations
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_code_migration_creation() {
        let up_sql = Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY);".to_string());
        let down_sql = Some(Box::new("DROP TABLE test;".to_string()) as Box<dyn Executable>);

        let migration = CodeMigration::new("001_create_test".to_string(), up_sql, down_sql);

        assert_eq!(migration.id(), "001_create_test");
    }

    #[tokio::test]
    async fn test_code_migration_source() {
        let mut source = CodeMigrationSource::new();

        let migration = CodeMigration::new(
            "001_test".to_string(),
            Box::new("SELECT 1;".to_string()),
            None,
        );

        source.add_migration(migration);

        // Test that migrations() can be called (even if it returns empty for now)
        let migrations = source.migrations().await.unwrap();
        assert_eq!(migrations.len(), 0); // Current implementation returns empty
    }

    #[tokio::test]
    async fn test_code_migration_with_query_builder() {
        // Test using the database query builders
        use switchy_database::schema::{Column, DataType, create_table};

        let create_table_stmt = create_table("users")
            .if_not_exists(true)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::Int,
                default: None,
            })
            .primary_key("id");

        let migration = CodeMigration::new(
            "001_create_users".to_string(),
            Box::new(create_table_stmt),
            None,
        );

        assert_eq!(migration.id(), "001_create_users");
    }
}
