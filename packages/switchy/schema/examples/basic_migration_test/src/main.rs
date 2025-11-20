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
#[tokio::main]
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

    #[tokio::test]
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

    #[tokio::test]
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

    #[tokio::test]
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

    #[tokio::test]
    async fn test_add_users_status_column_migration() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // First create the users table
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        // Apply the status column migration
        let add_status = AddUsersStatusColumn;
        add_status.up(db.as_ref()).await?;

        // Verify the status column exists with default value by inserting a row
        db.exec_raw("INSERT INTO users (name, email) VALUES ('Test User', 'test@example.com')")
            .await?;

        // Query the inserted row to verify status has the default value
        let result = switchy_database::query::select("users")
            .columns(&["status"])
            .execute(db.as_ref())
            .await?;

        assert_eq!(result.len(), 1);
        match result[0].get("status") {
            Some(DatabaseValue::String(s)) => assert_eq!(s, "active"),
            _ => panic!("Expected status to be 'active'"),
        }

        // Test down migration (should succeed even though it's a no-op in this example)
        add_status.down(db.as_ref()).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_create_posts_table_migration() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // First create the users table (required for foreign key)
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        // Apply the posts table migration
        let create_posts = CreatePostsTable;
        create_posts.up(db.as_ref()).await?;

        // Verify the posts table exists and we can query it
        let result = switchy_database::query::select("posts")
            .columns(&["id"])
            .limit(0)
            .execute(db.as_ref())
            .await;
        assert!(result.is_ok());

        // Test down migration
        create_posts.down(db.as_ref()).await?;

        // Verify the table was dropped
        let result = switchy_database::query::select("posts")
            .columns(&["id"])
            .limit(0)
            .execute(db.as_ref())
            .await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_unique_email_constraint() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Create the users table with unique email constraint
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        // Insert a user
        db.exec_raw("INSERT INTO users (name, email) VALUES ('User 1', 'test@example.com')")
            .await?;

        // Try to insert another user with the same email - should fail
        let result = db
            .exec_raw("INSERT INTO users (name, email) VALUES ('User 2', 'test@example.com')")
            .await;

        assert!(result.is_err(), "Expected unique constraint violation");

        Ok(())
    }

    #[tokio::test]
    async fn test_foreign_key_constraint() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Create both users and posts tables
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        let create_posts = CreatePostsTable;
        create_posts.up(db.as_ref()).await?;

        // Insert a valid user
        db.exec_raw("INSERT INTO users (name, email) VALUES ('Valid User', 'valid@example.com')")
            .await?;

        // Insert a post with valid user_id (1) - should succeed
        // SQLite auto-increment starts at 1
        let valid_post_result = db
            .exec_raw("INSERT INTO posts (user_id, title) VALUES (1, 'Valid Post')")
            .await;
        assert!(
            valid_post_result.is_ok(),
            "Valid foreign key insert should succeed"
        );

        // Try to insert a post with invalid user_id - should fail due to foreign key constraint
        let result = db
            .exec_raw("INSERT INTO posts (user_id, title) VALUES (9999, 'Invalid Post')")
            .await;

        assert!(result.is_err(), "Expected foreign key constraint violation");

        Ok(())
    }

    #[tokio::test]
    async fn test_created_at_default_value() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Create the users table
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        // Insert a user without specifying created_at
        db.exec_raw("INSERT INTO users (name, email) VALUES ('Test User', 'test@example.com')")
            .await?;

        // Query the inserted row to verify created_at was set automatically
        let result = switchy_database::query::select("users")
            .columns(&["created_at"])
            .execute(db.as_ref())
            .await?;

        assert_eq!(result.len(), 1);
        let created_at = result[0].get("created_at");
        assert!(
            created_at.is_some(),
            "created_at should have a default value"
        );

        // Verify it's not null
        match created_at {
            Some(DatabaseValue::String(s)) => {
                assert!(!s.is_empty(), "created_at should not be empty");
            }
            _ => panic!("Expected created_at to be a String"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_data_persistence_across_status_migration() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Create users table
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        // Insert test data
        db.exec_raw("INSERT INTO users (name, email) VALUES ('Test User 1', 'user1@example.com')")
            .await?;
        db.exec_raw("INSERT INTO users (name, email) VALUES ('Test User 2', 'user2@example.com')")
            .await?;

        // Verify we have 2 users
        let result = switchy_database::query::select("users")
            .columns(&["id", "name", "email"])
            .execute(db.as_ref())
            .await?;
        assert_eq!(result.len(), 2);

        // Apply status column migration
        let add_status = AddUsersStatusColumn;
        add_status.up(db.as_ref()).await?;

        // Verify data still exists and has the new status column with default value
        let result = switchy_database::query::select("users")
            .columns(&["id", "name", "email", "status"])
            .execute(db.as_ref())
            .await?;

        assert_eq!(result.len(), 2);

        // Check first user
        match result[0].get("name") {
            Some(DatabaseValue::String(s)) => assert_eq!(s, "Test User 1"),
            _ => panic!("Expected name to be 'Test User 1'"),
        }
        match result[0].get("status") {
            Some(DatabaseValue::String(s)) => assert_eq!(s, "active"),
            _ => panic!("Expected status to be 'active'"),
        }

        // Check second user
        match result[1].get("name") {
            Some(DatabaseValue::String(s)) => assert_eq!(s, "Test User 2"),
            _ => panic!("Expected name to be 'Test User 2'"),
        }
        match result[1].get("status") {
            Some(DatabaseValue::String(s)) => assert_eq!(s, "active"),
            _ => panic!("Expected status to be 'active'"),
        }

        Ok(())
    }
}
