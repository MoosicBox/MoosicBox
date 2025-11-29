//! # Basic Migration Test Example
//!
//! This example demonstrates how to use `verify_migrations_full_cycle` to test
//! migrations from a fresh database state. This is the most common testing pattern
//! for verifying that migrations work correctly both forward and backward.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::Arc;

use async_trait::async_trait;
use switchy_database::{
    Database, DatabaseValue,
    schema::{Column, DataType},
};
use switchy_schema::migration::Migration;
use switchy_schema_test_utils::{create_empty_in_memory, verify_migrations_full_cycle};

/// Example migration that creates a users table
struct CreateUsersTable;

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &'static str {
        "001_create_users"
    }

    /// Applies the migration forward, creating the users table with columns for id, name, email, and `created_at`.
    ///
    /// # Errors
    ///
    /// * Returns an error if table creation fails
    /// * Returns an error if unique index creation fails
    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        // Create users table using schema query builder
        db.create_table("users")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "email".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "created_at".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text, // SQLite stores datetime as text
                default: Some(DatabaseValue::String("CURRENT_TIMESTAMP".to_string())),
            })
            .primary_key("id")
            // Note: UNIQUE constraint on email will be handled by raw SQL for now
            .execute(db)
            .await?;

        // Add UNIQUE constraint and index using raw SQL since schema builder doesn't support it yet
        db.exec_raw("CREATE UNIQUE INDEX idx_users_email ON users(email)")
            .await?;
        Ok(())
    }

    /// Rolls back the migration, dropping the users table and its index.
    ///
    /// # Errors
    ///
    /// * Returns an error if index drop fails
    /// * Returns an error if table drop fails
    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        // Drop index first, then table
        db.exec_raw("DROP INDEX IF EXISTS idx_users_email").await?;
        db.exec_raw("DROP TABLE users").await?;
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Create users table with basic fields")
    }
}

/// Example migration that adds a status column to users
struct AddUsersStatusColumn;

#[async_trait]
impl Migration<'static> for AddUsersStatusColumn {
    fn id(&self) -> &'static str {
        "002_add_users_status"
    }

    /// Applies the migration forward, adding a status column with default value 'active' to the users table.
    ///
    /// # Errors
    ///
    /// * Returns an error if the ALTER TABLE statement fails
    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        // Add status column using raw SQL since ALTER TABLE ADD COLUMN
        // isn't supported by the schema builder yet
        db.exec_raw("ALTER TABLE users ADD COLUMN status TEXT DEFAULT 'active'")
            .await?;
        Ok(())
    }

    /// Rolls back the migration.
    ///
    /// Note: In this example, the down migration does nothing because the status column
    /// removal is not critical for testing, and the table will be dropped by the previous
    /// migration's down method anyway during the full cycle test.
    ///
    /// # Errors
    ///
    /// This implementation never returns an error.
    async fn down(&self, _db: &dyn Database) -> switchy_schema::Result<()> {
        // For this example, we'll just note that the column would be removed
        // In a real scenario, you might recreate the table without the status column
        // but for simplicity in this test example, we'll do nothing
        // since the table will be dropped by the previous migration anyway
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Add status column to users table")
    }
}

/// Example migration that adds a posts table with foreign key
struct CreatePostsTable;

#[async_trait]
impl Migration<'static> for CreatePostsTable {
    fn id(&self) -> &'static str {
        "003_create_posts"
    }

    /// Applies the migration forward, creating the posts table with a foreign key to users.
    ///
    /// # Errors
    ///
    /// * Returns an error if table creation fails
    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        // Create posts table using schema query builder
        db.create_table("posts")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "user_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "title".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "content".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "created_at".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text, // SQLite stores datetime as text
                default: Some(DatabaseValue::String("CURRENT_TIMESTAMP".to_string())),
            })
            .primary_key("id")
            .foreign_key(("user_id", "users(id)"))
            .execute(db)
            .await?;
        Ok(())
    }

    /// Rolls back the migration, dropping the posts table.
    ///
    /// # Errors
    ///
    /// * Returns an error if table drop fails
    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("DROP TABLE posts").await?;
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Create posts table with foreign key to users")
    }
}

/// Demonstrates testing database migrations using `verify_migrations_full_cycle`.
///
/// This example creates three migrations (users table, status column, posts table)
/// and uses `verify_migrations_full_cycle` to verify they work correctly in both
/// forward (up) and backward (down) directions.
///
/// # Errors
///
/// * Returns an error if database creation fails
/// * Returns an error if any migration fails during the full cycle test
#[switchy_async::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Basic Migration Test Example");
    println!("============================");
    println!();

    // Create an in-memory SQLite database for testing
    let db = create_empty_in_memory().await?;
    println!("✅ Created in-memory SQLite database");

    // Define our migrations in the order they should be applied
    let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
        Arc::new(CreateUsersTable),
        Arc::new(AddUsersStatusColumn),
        Arc::new(CreatePostsTable),
    ];

    println!("📋 Defined {} migrations:", migrations.len());
    for migration in &migrations {
        println!(
            "  - {}: {}",
            migration.id(),
            migration.description().unwrap_or("No description")
        );
    }
    println!();

    // Use verify_migrations_full_cycle to test the complete migration lifecycle
    println!("🔄 Testing full migration cycle...");
    println!("   1. Apply all migrations forward (up)");
    println!("   2. Verify no errors during forward migration");
    println!("   3. Apply all migrations backward (down)");
    println!("   4. Verify database returns to initial state");

    match verify_migrations_full_cycle(db.as_ref(), migrations).await {
        Ok(()) => {
            println!("✅ Full migration cycle completed successfully!");
            println!();
            println!("🎉 All migrations work correctly:");
            println!("   • Forward migrations create tables and indexes properly");
            println!("   • Backward migrations clean up all changes");
            println!("   • Database returns to initial empty state");
        }
        Err(e) => {
            println!("❌ Migration cycle failed: {e}");
            return Err(e.into());
        }
    }

    println!();
    println!("💡 Key Benefits of verify_migrations_full_cycle:");
    println!("   • Tests both up and down migrations");
    println!("   • Ensures migrations are reversible");
    println!("   • Catches migration ordering issues");
    println!("   • Verifies clean rollback behavior");
    println!("   • Perfect for CI/CD pipeline testing");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchy_schema_test_utils::TestError;

    #[switchy_async::test]
    async fn test_individual_migrations() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Test each migration individually
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        // Verify table was created using query builder
        let result = switchy_database::query::select("users")
            .columns(&["id"])
            .limit(0)
            .execute(db.as_ref())
            .await;
        assert!(result.is_ok());

        create_users.down(db.as_ref()).await?;
        Ok(())
    }

    #[switchy_async::test]
    async fn test_migration_descriptions() {
        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
            Arc::new(CreateUsersTable),
            Arc::new(AddUsersStatusColumn),
            Arc::new(CreatePostsTable),
        ];

        assert_eq!(migrations[0].id(), "001_create_users");
        assert_eq!(migrations[1].id(), "002_add_users_status");
        assert_eq!(migrations[2].id(), "003_create_posts");

        assert!(migrations[0].description().is_some());
        assert!(migrations[1].description().is_some());
        assert!(migrations[2].description().is_some());
    }

    #[switchy_async::test]
    async fn test_full_cycle_with_test_utils() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
            Arc::new(CreateUsersTable),
            Arc::new(AddUsersStatusColumn),
            Arc::new(CreatePostsTable),
        ];

        // This is the main test - using the test utility
        verify_migrations_full_cycle(db.as_ref(), migrations).await?;
        Ok(())
    }
}
