#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::unnecessary_literal_bound)]

//! Basic usage example for `switchy_schema` demonstrating type-safe database migrations.
//!
//! This example showcases the core features of `switchy_schema`:
//!
//! * Creating tables with type-safe schema builders
//! * Adding indexes to improve query performance
//! * Altering tables to add new columns
//! * Running migrations with automatic tracking
//! * Checking migration status and history
//!
//! The example creates a simple `users` table with migrations that:
//!
//! 1. Create the initial table structure with `id`, `name`, and `email` columns
//! 2. Add an index on the `email` column for faster lookups
//! 3. Add a `created_at` timestamp column with a default value
//!
//! # Running the Example
//!
//! ```bash
//! cargo run --package basic_usage
//! ```
//!
//! The example uses an in-memory `SQLite` database, so no external setup is required.

use async_trait::async_trait;
use std::sync::Arc;
use switchy_database::schema::{Column, DataType};
use switchy_database::{Database, DatabaseValue};
use switchy_schema::migration::{Migration, MigrationSource};
use switchy_schema::runner::MigrationRunner;

/// Migration to create users table with proper schema builder
struct CreateUsersTable;

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &str {
        "001_create_users_table"
    }

    fn description(&self) -> Option<&str> {
        Some("Create users table with id, name, and email columns")
    }

    /// Creates the users table with id, name, and email columns.
    ///
    /// # Errors
    ///
    /// * Database connection errors
    /// * SQL syntax errors in table creation
    /// * Constraint violations if table already exists
    async fn up(&self, db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
        // Create users table using type-safe schema builder
        db.create_table("users")
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                data_type: DataType::VarChar(255),
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "email".to_string(),
                data_type: DataType::VarChar(255),
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await?;

        Ok(())
    }

    /// Drops the users table for migration rollback.
    ///
    /// # Errors
    ///
    /// * Database connection errors
    /// * SQL execution errors when dropping the table
    async fn down(&self, db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
        // Drop users table for rollback
        db.drop_table("users").if_exists(true).execute(db).await?;

        Ok(())
    }
}

/// Migration to add email index using schema builder
struct AddEmailIndex;

#[async_trait]
impl Migration<'static> for AddEmailIndex {
    fn id(&self) -> &str {
        "002_add_email_index"
    }

    fn description(&self) -> Option<&str> {
        Some("Add index on email column for faster lookups")
    }

    /// Creates an index on the `email` column for improved query performance.
    ///
    /// # Errors
    ///
    /// * Database connection errors
    /// * SQL syntax errors in index creation
    /// * Index creation failures (e.g., if column doesn't exist)
    async fn up(&self, db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
        // Create index using type-safe schema builder
        db.create_index("idx_users_email")
            .table("users")
            .column("email")
            .if_not_exists(true)
            .execute(db)
            .await?;

        Ok(())
    }

    /// Drops the `email` index for migration rollback.
    ///
    /// # Errors
    ///
    /// * Database connection errors
    /// * SQL execution errors when dropping the index
    async fn down(&self, db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
        // Drop index for rollback
        db.drop_index("idx_users_email", "users")
            .if_exists()
            .execute(db)
            .await?;

        Ok(())
    }
}

/// Migration to add `created_at` column using schema builder
struct AddCreatedAtColumn;

#[async_trait]
impl Migration<'static> for AddCreatedAtColumn {
    fn id(&self) -> &str {
        "003_add_created_at_column"
    }

    fn description(&self) -> Option<&str> {
        Some("Add created_at timestamp column to track when users are created")
    }

    /// Adds a `created_at` timestamp column with a default value of `NOW()`.
    ///
    /// # Errors
    ///
    /// * Database connection errors
    /// * SQL syntax errors in ALTER TABLE statement
    /// * Column addition failures (e.g., if table doesn't exist)
    async fn up(&self, db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
        // Add column using type-safe schema builder
        db.alter_table("users")
            .add_column(
                "created_at".to_string(),
                DataType::DateTime,
                false,
                Some(DatabaseValue::Now),
            )
            .execute(db)
            .await?;

        Ok(())
    }

    /// Drops the `created_at` column for migration rollback.
    ///
    /// # Errors
    ///
    /// * Database connection errors
    /// * SQL execution errors when dropping the column
    async fn down(&self, db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
        // Drop column for rollback
        db.alter_table("users")
            .drop_column("created_at".to_string())
            .execute(db)
            .await?;

        Ok(())
    }
}

/// Create migration source with our migrations
struct BasicUsageMigrations;

#[async_trait]
impl MigrationSource<'static> for BasicUsageMigrations {
    /// Returns the ordered list of migrations for this example.
    ///
    /// # Errors
    ///
    /// * Currently never returns errors, but trait requires Result for extensibility
    async fn migrations(
        &self,
    ) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>, switchy_schema::MigrationError> {
        Ok(vec![
            Arc::new(CreateUsersTable),
            Arc::new(AddEmailIndex),
            Arc::new(AddCreatedAtColumn),
        ])
    }
}

/// Runs the basic usage example demonstrating `switchy_schema` features.
///
/// This example creates an in-memory `SQLite` database, runs migrations to create
/// a users table with indexes, inserts test data, and displays migration status.
///
/// # Errors
///
/// * Database initialization failures
/// * Migration execution errors
/// * SQL query execution errors
/// * Data retrieval failures
///
/// # Panics
///
/// * When retrieving database values if the expected column types don't match
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see migration progress
    env_logger::init();

    // Setup database connection (SQLite in-memory for demo)
    let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
    let db = &*db;

    println!("🚀 Starting Basic Usage Example");
    println!("================================");

    // Create migration source with our migrations
    let source = BasicUsageMigrations;

    // Create migration runner
    let runner =
        MigrationRunner::new(Box::new(source)).with_table_name("__example_migrations".to_string());

    // Check migration status before running
    println!("\n📋 Checking migration status...");
    let migration_info = runner.list_migrations(db).await?;

    for info in &migration_info {
        let status = if info.applied {
            "✅ Applied"
        } else {
            "❌ Pending"
        };
        let description = info.description.as_deref().unwrap_or("No description");
        println!("  {} - {} {}", info.id, description, status);
    }

    // Run migrations
    println!("\n🔧 Running migrations...");
    runner.run(db).await?;
    println!("✅ All migrations completed successfully!");

    // Verify schema with some test data
    println!("\n🧪 Verifying schema with test data...");

    // Insert test user
    let user_id = db
        .insert("users")
        .value("name", "Alice Johnson")
        .value("email", "alice@example.com")
        .execute(db)
        .await?;

    println!("📝 Inserted user with ID: {user_id:?}");

    // Query users to verify structure
    let users = db.select("users").execute(db).await?;

    for user in &users {
        println!(
            "👤 User: {} - {} (created: {})",
            user.get("id").unwrap().as_i64().unwrap(),
            user.get("name").unwrap().as_str().unwrap(),
            user.get("email").unwrap().as_str().unwrap_or("None"),
        );
    }

    // Check final migration status
    println!("\n📊 Final migration status:");
    let final_status = runner.list_migrations(db).await?;

    for info in &final_status {
        let status = if info.applied {
            "✅ Applied"
        } else {
            "❌ Pending"
        };
        let description = info.description.as_deref().unwrap_or("No description");
        println!("  {} - {} {}", info.id, description, status);
    }

    println!("\n🎉 Basic usage example completed successfully!");

    // Optional rollback demonstration (commented out)
    /*
    println!("\n🔄 Rollback demonstration (optional):");
    println!("Uncommenting this will rollback the last migration...");

    runner.rollback(db, switchy_schema::runner::RollbackStrategy::Steps(1)).await?;
    println!("✅ Rollback completed - created_at column removed");

    let post_rollback_users = db.select("users").execute(db).await?;
    println!("📊 Users after rollback: {} found", post_rollback_users.len());
    */

    Ok(())
}
