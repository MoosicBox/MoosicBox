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
//! use switchy_schema::migration::{Migration, MigrationSource};
//! use async_trait::async_trait;
//!
//! struct MyMigrationSource {
//!     migrations: Vec<Box<dyn Migration<'static> + 'static>>,
//! }
//!
//! #[async_trait]
//! impl MigrationSource<'static> for MyMigrationSource {
//!     async fn migrations(&self) -> switchy_schema::Result<Vec<Box<dyn Migration<'static> + 'static>>> {
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
use async_trait::async_trait;
use switchy_database::Database;

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

    fn depends_on(&self) -> Vec<&str> {
        Vec::new()
    }

    fn supported_databases(&self) -> Vec<&str> {
        vec!["sqlite", "postgres", "mysql"]
    }
}

#[async_trait]
pub trait MigrationSource<'a>: Send + Sync {
    async fn migrations(&self) -> Result<Vec<Box<dyn Migration<'a> + 'a>>>;
}
