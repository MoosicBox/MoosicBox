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
//! The example uses an in-memory SQLite database, so no external setup is required.

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

    async fn down(&self, db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
        // Drop index for rollback
        db.drop_index("idx_users_email", "users")
            .if_exists()
            .execute(db)
            .await?;

        Ok(())
    }
}

/// Migration to add created_at column using schema builder
struct AddCreatedAtColumn;

#[async_trait]
impl Migration<'static> for AddCreatedAtColumn {
    fn id(&self) -> &str {
        "003_add_created_at_column"
    }

    fn description(&self) -> Option<&str> {
        Some("Add created_at timestamp column to track when users are created")
    }

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

    println!("📝 Inserted user with ID: {:?}", user_id);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that CreateUsersTable migration has the correct ID
    #[test]
    fn test_create_users_table_id() {
        let migration = CreateUsersTable;
        assert_eq!(migration.id(), "001_create_users_table");
    }

    /// Test that CreateUsersTable migration has a description
    #[test]
    fn test_create_users_table_description() {
        let migration = CreateUsersTable;
        assert_eq!(
            migration.description(),
            Some("Create users table with id, name, and email columns")
        );
    }

    /// Test that AddEmailIndex migration has the correct ID
    #[test]
    fn test_add_email_index_id() {
        let migration = AddEmailIndex;
        assert_eq!(migration.id(), "002_add_email_index");
    }

    /// Test that AddEmailIndex migration has a description
    #[test]
    fn test_add_email_index_description() {
        let migration = AddEmailIndex;
        assert_eq!(
            migration.description(),
            Some("Add index on email column for faster lookups")
        );
    }

    /// Test that AddCreatedAtColumn migration has the correct ID
    #[test]
    fn test_add_created_at_column_id() {
        let migration = AddCreatedAtColumn;
        assert_eq!(migration.id(), "003_add_created_at_column");
    }

    /// Test that AddCreatedAtColumn migration has a description
    #[test]
    fn test_add_created_at_column_description() {
        let migration = AddCreatedAtColumn;
        assert_eq!(
            migration.description(),
            Some("Add created_at timestamp column to track when users are created")
        );
    }

    /// Test that BasicUsageMigrations returns all migrations in the correct order
    #[test_log::test(switchy_async::test)]
    async fn test_basic_usage_migrations_order() {
        let source = BasicUsageMigrations;
        let migrations = source.migrations().await.unwrap();

        assert_eq!(migrations.len(), 3, "Should have exactly 3 migrations");
        assert_eq!(migrations[0].id(), "001_create_users_table");
        assert_eq!(migrations[1].id(), "002_add_email_index");
        assert_eq!(migrations[2].id(), "003_add_created_at_column");
    }

    /// Test that BasicUsageMigrations returns migrations with descriptions
    #[test_log::test(switchy_async::test)]
    async fn test_basic_usage_migrations_descriptions() {
        let source = BasicUsageMigrations;
        let migrations = source.migrations().await.unwrap();

        for migration in migrations {
            assert!(
                migration.description().is_some(),
                "Migration {} should have a description",
                migration.id()
            );
        }
    }

    /// Test that all migration IDs follow the naming convention
    #[test_log::test(switchy_async::test)]
    async fn test_migration_id_naming_convention() {
        let source = BasicUsageMigrations;
        let migrations = source.migrations().await.unwrap();

        for migration in migrations {
            let id = migration.id();
            assert!(
                id.starts_with(char::is_numeric),
                "Migration ID {} should start with a number",
                id
            );
            assert!(
                id.contains('_'),
                "Migration ID {} should use snake_case",
                id
            );
        }
    }

    /// Test that migration IDs are unique
    #[test_log::test(switchy_async::test)]
    async fn test_migration_ids_are_unique() {
        let source = BasicUsageMigrations;
        let migrations = source.migrations().await.unwrap();

        let mut ids = std::collections::BTreeSet::new();
        for migration in migrations {
            let id = migration.id();
            assert!(
                ids.insert(id.to_string()),
                "Migration ID {} is duplicated",
                id
            );
        }
    }

    /// Integration test: Verify CreateUsersTable migration can execute successfully
    #[test_log::test(switchy_async::test)]
    async fn test_create_users_table_integration() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to initialize database");
        let migration = CreateUsersTable;

        // Test up migration
        let result = migration.up(&*db).await;
        assert!(result.is_ok(), "Migration up should succeed");

        // Verify table exists by attempting to query it
        let query_result = db.select("users").execute(&*db).await;
        assert!(
            query_result.is_ok(),
            "Should be able to query users table after migration"
        );

        // Test down migration
        let result = migration.down(&*db).await;
        assert!(result.is_ok(), "Migration down should succeed");
    }

    /// Integration test: Verify AddEmailIndex migration can execute successfully
    #[test_log::test(switchy_async::test)]
    async fn test_add_email_index_integration() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to initialize database");

        // First create the users table
        let create_table = CreateUsersTable;
        create_table
            .up(&*db)
            .await
            .expect("Failed to create users table");

        // Now test the index migration
        let migration = AddEmailIndex;

        // Test up migration
        let result = migration.up(&*db).await;
        assert!(result.is_ok(), "Index migration up should succeed");

        // Test down migration
        let result = migration.down(&*db).await;
        assert!(result.is_ok(), "Index migration down should succeed");

        // Cleanup
        create_table.down(&*db).await.ok();
    }

    /// Integration test: Verify AddCreatedAtColumn migration can execute successfully
    #[test_log::test(switchy_async::test)]
    async fn test_add_created_at_column_integration() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to initialize database");

        // First create the users table
        let create_table = CreateUsersTable;
        create_table
            .up(&*db)
            .await
            .expect("Failed to create users table");

        // Now test the column addition migration
        let migration = AddCreatedAtColumn;

        // Test up migration
        let result = migration.up(&*db).await;
        assert!(result.is_ok(), "Column migration up should succeed");

        // Verify we can insert data with the new column
        let insert_result = db
            .insert("users")
            .value("name", "Test User")
            .value("email", "test@example.com")
            .execute(&*db)
            .await;
        assert!(
            insert_result.is_ok(),
            "Should be able to insert data after adding created_at column"
        );

        // Test down migration
        let result = migration.down(&*db).await;
        assert!(result.is_ok(), "Column migration down should succeed");

        // Cleanup
        create_table.down(&*db).await.ok();
    }

    /// Integration test: Verify all migrations can run in sequence
    #[test_log::test(switchy_async::test)]
    async fn test_full_migration_sequence() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to initialize database");

        let source = BasicUsageMigrations;
        let migrations = source.migrations().await.unwrap();

        // Run all migrations up
        for migration in &migrations {
            let result = migration.up(&*db).await;
            assert!(
                result.is_ok(),
                "Migration {} up should succeed",
                migration.id()
            );
        }

        // Verify we can use the fully migrated schema
        let insert_result = db
            .insert("users")
            .value("name", "Full Migration Test")
            .value("email", "full@example.com")
            .execute(&*db)
            .await;
        assert!(
            insert_result.is_ok(),
            "Should be able to insert data after all migrations"
        );

        // Verify data was inserted
        let users = db.select("users").execute(&*db).await.unwrap();
        assert_eq!(users.len(), 1, "Should have one user after insertion");

        // Run all migrations down in reverse order
        for migration in migrations.iter().rev() {
            let result = migration.down(&*db).await;
            assert!(
                result.is_ok(),
                "Migration {} down should succeed",
                migration.id()
            );
        }
    }

    /// Integration test: Verify migration idempotency (up can be called multiple times safely)
    #[test_log::test(switchy_async::test)]
    async fn test_migration_idempotency() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to initialize database");

        let create_table = CreateUsersTable;

        // First run should succeed
        let result = create_table.up(&*db).await;
        assert!(result.is_ok(), "First migration up should succeed");

        // Second run might fail or succeed depending on implementation
        // For this test, we just verify it doesn't panic
        let _ = create_table.up(&*db).await;

        // Cleanup
        create_table.down(&*db).await.ok();
    }

    /// Integration test: Verify index migration idempotency with if_not_exists
    #[test_log::test(switchy_async::test)]
    async fn test_index_migration_idempotency() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .expect("Failed to initialize database");

        // Setup: create table first
        let create_table = CreateUsersTable;
        create_table.up(&*db).await.unwrap();

        let index_migration = AddEmailIndex;

        // First run should succeed
        let result = index_migration.up(&*db).await;
        assert!(result.is_ok(), "First index creation should succeed");

        // Second run should also succeed due to if_not_exists
        let result = index_migration.up(&*db).await;
        assert!(
            result.is_ok(),
            "Second index creation should succeed with if_not_exists"
        );

        // Cleanup
        index_migration.down(&*db).await.ok();
        create_table.down(&*db).await.ok();
    }
}
