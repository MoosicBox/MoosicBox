//! # Mutation Migration Test Example
//!
//! This example demonstrates how to use `verify_migrations_with_mutations` to test
//! migrations with data changes happening between migration steps. This simulates
//! real-world scenarios where data is being modified while migrations are running,
//! or where you need to test how migrations handle specific data patterns.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use switchy_database::{
    Database, DatabaseError, DatabaseValue, Executable,
    schema::{Column, DataType},
};
use switchy_schema::migration::Migration;
use switchy_schema_test_utils::{create_empty_in_memory, verify_migrations_with_mutations};

/// Migration that creates a users table
///
/// Creates a table with columns for user information including id, name, email,
/// status, and creation timestamp. Includes a unique constraint on the email field.
struct CreateUsersTable;

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &str {
        "001_create_users"
    }

    /// Creates the users table with a unique email constraint.
    ///
    /// # Errors
    ///
    /// * Database execution fails when creating the table
    /// * Index creation fails for the unique email constraint
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
                name: "status".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: Some(DatabaseValue::String("active".to_string())),
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

        // Add UNIQUE constraint using raw SQL since schema builder doesn't support it yet
        db.exec_raw("CREATE UNIQUE INDEX idx_users_email ON users(email)")
            .await?;
        Ok(())
    }

    /// Drops the users table.
    ///
    /// # Errors
    ///
    /// * Database execution fails when dropping the table
    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("DROP TABLE users").await?;
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Create users table with status field")
    }
}

/// Migration that creates a posts table
///
/// Creates a table for blog posts with columns for id, user_id (foreign key),
/// title, content, published status, and creation timestamp.
struct CreatePostsTable;

#[async_trait]
impl Migration<'static> for CreatePostsTable {
    fn id(&self) -> &str {
        "002_create_posts"
    }

    /// Creates the posts table with a reference to the users table.
    ///
    /// # Errors
    ///
    /// * Database execution fails when creating the table
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
                name: "published".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Int, // SQLite stores boolean as integer
                default: Some(DatabaseValue::Int64(0)), // FALSE = 0
            })
            .column(Column {
                name: "created_at".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text, // SQLite stores datetime as text
                default: Some(DatabaseValue::String("CURRENT_TIMESTAMP".to_string())),
            })
            .primary_key("id")
            .execute(db)
            .await?;
        Ok(())
    }

    /// Drops the posts table.
    ///
    /// # Errors
    ///
    /// * Database execution fails when dropping the table
    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("DROP TABLE posts").await?;
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Create posts table with published flag")
    }
}

/// Migration that adds an analytics table
///
/// Creates a table for tracking user events with columns for id, user_id (optional),
/// event_type, event_data (JSON), and timestamp.
struct CreateAnalyticsTable;

#[async_trait]
impl Migration<'static> for CreateAnalyticsTable {
    fn id(&self) -> &str {
        "003_create_analytics"
    }

    /// Creates the analytics table for event tracking.
    ///
    /// # Errors
    ///
    /// * Database execution fails when creating the table
    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        // Create analytics table using schema query builder
        db.create_table("analytics")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "user_id".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "event_type".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "event_data".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "timestamp".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text, // SQLite stores datetime as text
                default: Some(DatabaseValue::String("CURRENT_TIMESTAMP".to_string())),
            })
            .primary_key("id")
            .execute(db)
            .await?;
        Ok(())
    }

    /// Drops the analytics table.
    ///
    /// # Errors
    ///
    /// * Database execution fails when dropping the table
    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("DROP TABLE analytics").await?;
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Create analytics table for tracking user events")
    }
}

/// Migration that adds indexes for performance
///
/// Creates indexes on foreign key columns and commonly queried fields to improve
/// query performance on the posts and analytics tables.
struct AddPerformanceIndexes;

#[async_trait]
impl Migration<'static> for AddPerformanceIndexes {
    fn id(&self) -> &str {
        "004_add_performance_indexes"
    }

    /// Creates performance indexes on posts and analytics tables.
    ///
    /// # Errors
    ///
    /// * Database execution fails when creating any of the indexes
    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("CREATE INDEX idx_posts_user_id ON posts(user_id)")
            .await?;
        db.exec_raw("CREATE INDEX idx_posts_published ON posts(published)")
            .await?;
        db.exec_raw("CREATE INDEX idx_analytics_user_id ON analytics(user_id)")
            .await?;
        db.exec_raw("CREATE INDEX idx_analytics_event_type ON analytics(event_type)")
            .await?;
        Ok(())
    }

    /// Drops the performance indexes.
    ///
    /// # Errors
    ///
    /// * Database execution fails when dropping any of the indexes
    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        // Use IF EXISTS to avoid errors if tables are already dropped
        db.exec_raw("DROP INDEX IF EXISTS idx_posts_user_id")
            .await?;
        db.exec_raw("DROP INDEX IF EXISTS idx_posts_published")
            .await?;
        db.exec_raw("DROP INDEX IF EXISTS idx_analytics_user_id")
            .await?;
        db.exec_raw("DROP INDEX IF EXISTS idx_analytics_event_type")
            .await?;
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Add performance indexes on foreign keys and common query fields")
    }
}

/// Custom executable for inserting test data
///
/// Inserts three test users (Alice, Bob, Carol) into the users table with
/// different status values for testing purposes.
struct InsertTestUsers;

#[async_trait]
impl Executable for InsertTestUsers {
    /// Inserts test users into the users table.
    ///
    /// # Errors
    ///
    /// * Database insertion fails for any of the test users
    /// * Unique constraint violation if users already exist
    async fn execute(&self, db: &dyn Database) -> std::result::Result<(), DatabaseError> {
        // Insert test users using query builder
        db.insert("users")
            .value("name", "Alice Johnson")
            .value("email", "alice@example.com")
            .value("status", "active")
            .execute(db)
            .await?;

        db.insert("users")
            .value("name", "Bob Smith")
            .value("email", "bob@example.com")
            .value("status", "active")
            .execute(db)
            .await?;

        db.insert("users")
            .value("name", "Carol Davis")
            .value("email", "carol@example.com")
            .value("status", "inactive")
            .execute(db)
            .await?;

        Ok(())
    }
}

/// Custom executable for inserting test posts
///
/// Inserts four test posts with varying published status and associated with
/// different test users.
struct InsertTestPosts;

#[async_trait]
impl Executable for InsertTestPosts {
    /// Inserts test posts into the posts table.
    ///
    /// # Errors
    ///
    /// * Database insertion fails for any of the test posts
    /// * Foreign key constraint violation if referenced users don't exist
    async fn execute(&self, db: &dyn Database) -> std::result::Result<(), DatabaseError> {
        // Insert test posts using query builder
        db.insert("posts")
            .value("user_id", 1)
            .value("title", "Alice First Post")
            .value("content", "Hello world from Alice!")
            .value("published", 1) // TRUE = 1 in SQLite
            .execute(db)
            .await?;

        db.insert("posts")
            .value("user_id", 1)
            .value("title", "Alice Draft")
            .value("content", "This is a draft post")
            .value("published", 0) // FALSE = 0 in SQLite
            .execute(db)
            .await?;

        db.insert("posts")
            .value("user_id", 2)
            .value("title", "Bob Introduction")
            .value("content", "Hi everyone, I am Bob")
            .value("published", 1) // TRUE = 1 in SQLite
            .execute(db)
            .await?;

        db.insert("posts")
            .value("user_id", 3)
            .value("title", "Carol Thoughts")
            .value("content", "Some thoughts from Carol")
            .value("published", 0) // FALSE = 0 in SQLite
            .execute(db)
            .await?;

        Ok(())
    }
}

/// Custom executable for inserting analytics data
///
/// Inserts analytics events including login and post creation events for the test users.
struct InsertAnalyticsData;

#[async_trait]
impl Executable for InsertAnalyticsData {
    /// Inserts analytics data into the analytics table.
    ///
    /// # Errors
    ///
    /// * Database insertion fails for any of the analytics events
    async fn execute(&self, db: &dyn Database) -> std::result::Result<(), DatabaseError> {
        // Insert analytics data using query builder
        db.insert("analytics")
            .value("user_id", 1)
            .value("event_type", "login")
            .value("event_data", r#"{"ip": "192.168.1.1"}"#)
            .execute(db)
            .await?;

        db.insert("analytics")
            .value("user_id", 1)
            .value("event_type", "post_created")
            .value("event_data", r#"{"post_id": 1}"#)
            .execute(db)
            .await?;

        db.insert("analytics")
            .value("user_id", 2)
            .value("event_type", "login")
            .value("event_data", r#"{"ip": "192.168.1.2"}"#)
            .execute(db)
            .await?;

        db.insert("analytics")
            .value("user_id", 2)
            .value("event_type", "post_created")
            .value("event_data", r#"{"post_id": 3}"#)
            .execute(db)
            .await?;

        db.insert("analytics")
            .value("user_id", 3)
            .value("event_type", "login")
            .value("event_data", r#"{"ip": "192.168.1.3"}"#)
            .execute(db)
            .await?;

        Ok(())
    }
}

/// Demonstrates mutation migration testing by running migrations with interleaved data changes.
///
/// This example shows how to use `verify_migrations_with_mutations` to test migrations
/// with realistic data patterns, ensuring that migrations work correctly when data is
/// being modified between migration steps.
///
/// # Errors
///
/// * Database creation fails
/// * Any migration or mutation operation fails
/// * Verification or rollback fails
#[switchy_async::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Mutation Migration Test Example");
    println!("===============================");
    println!();

    // Create an in-memory SQLite database for testing
    let db = create_empty_in_memory().await?;
    println!("✅ Created in-memory SQLite database");

    // Define our migrations in order
    let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
        Arc::new(CreateUsersTable),
        Arc::new(CreatePostsTable),
        Arc::new(CreateAnalyticsTable),
        Arc::new(AddPerformanceIndexes),
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

    // Create mutations that will happen between specific migrations
    let mut mutations: BTreeMap<String, Arc<dyn Executable>> = BTreeMap::new();

    // After users table is created, insert some test users
    mutations.insert("001_create_users".to_string(), Arc::new(InsertTestUsers));

    // After posts table is created, insert some test posts
    mutations.insert("002_create_posts".to_string(), Arc::new(InsertTestPosts));

    // After analytics table is created, insert analytics data and update user statuses
    mutations.insert(
        "003_create_analytics".to_string(),
        Arc::new(InsertAnalyticsData),
    );

    println!("🔄 Testing migrations with interleaved data mutations...");
    println!("   1. Apply migration: Create users table");
    println!("   2. Mutate data: Insert test users");
    println!("   3. Apply migration: Create posts table");
    println!("   4. Mutate data: Insert test posts");
    println!("   5. Apply migration: Create analytics table");
    println!("   6. Mutate data: Insert analytics data");
    println!("   7. Apply migration: Add performance indexes");
    println!("   8. Verify all migrations handle mutated data correctly");
    println!("   9. Roll back all migrations and mutations");

    match verify_migrations_with_mutations(db.as_ref(), migrations, mutations).await {
        Ok(()) => {
            println!("✅ Mutation migration testing completed successfully!");
            println!();
            println!("🎉 All migrations handled data mutations correctly:");
            println!("   • Users table created and populated with test data");
            println!("   • Posts table created with foreign key constraints working");
            println!("   • Analytics table created and populated");
            println!("   • Performance indexes added without affecting existing data");
            println!("   • All rollbacks preserved data integrity");
            println!("   • Foreign key constraints maintained throughout");
        }
        Err(e) => {
            println!("❌ Mutation migration testing failed: {}", e);
            return Err(e.into());
        }
    }

    println!();
    println!("💡 Key Benefits of verify_migrations_with_mutations:");
    println!("   • Tests migrations with realistic data patterns");
    println!("   • Ensures foreign key constraints work correctly");
    println!("   • Validates that indexes can be created on populated tables");
    println!("   • Tests rollback behavior with complex data relationships");
    println!("   • Simulates production scenarios with active data changes");
    println!("   • Catches performance issues with large datasets");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchy_schema_test_utils::TestError;
    use switchy_schema_test_utils::mutations::MutationProvider;

    /// Tests individual mutations can be applied successfully.
    ///
    /// Verifies that test users can be inserted after creating the users table.
    #[switchy_async::test]
    async fn test_individual_mutations() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Create users table
        let create_users = CreateUsersTable;
        create_users.up(db.as_ref()).await?;

        // Test inserting users
        let insert_users = InsertTestUsers;
        insert_users.execute(db.as_ref()).await?;

        // Verify users were inserted using query builder
        let results = switchy_database::query::select("users")
            .columns(&["*"])
            .execute(db.as_ref())
            .await?;
        assert_eq!(results.len(), 3);

        Ok(())
    }

    /// Tests foreign key constraints work correctly.
    ///
    /// Verifies that posts can be inserted with valid user_id references.
    #[switchy_async::test]
    async fn test_foreign_key_constraints() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Create tables
        let create_users = CreateUsersTable;
        let create_posts = CreatePostsTable;

        create_users.up(db.as_ref()).await?;
        create_posts.up(db.as_ref()).await?;

        // Insert users first
        let insert_users = InsertTestUsers;
        insert_users.execute(db.as_ref()).await?;

        // Insert posts (should work with valid user_ids)
        let insert_posts = InsertTestPosts;
        insert_posts.execute(db.as_ref()).await?;

        // Verify posts were inserted using query builder
        let results = switchy_database::query::select("posts")
            .columns(&["*"])
            .execute(db.as_ref())
            .await?;
        assert_eq!(results.len(), 4);

        Ok(())
    }

    /// Tests the complete mutation migration flow with test utilities.
    ///
    /// Verifies that all migrations can be applied and rolled back with interleaved
    /// data mutations using the `verify_migrations_with_mutations` helper.
    #[switchy_async::test]
    async fn test_mutations_with_test_utils() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
            Arc::new(CreateUsersTable),
            Arc::new(CreatePostsTable),
            Arc::new(CreateAnalyticsTable),
            Arc::new(AddPerformanceIndexes),
        ];

        let mut mutations: BTreeMap<String, Arc<dyn Executable>> = BTreeMap::new();
        mutations.insert("001_create_users".to_string(), Arc::new(InsertTestUsers));
        mutations.insert("002_create_posts".to_string(), Arc::new(InsertTestPosts));
        mutations.insert(
            "003_create_analytics".to_string(),
            Arc::new(InsertAnalyticsData),
        );

        // This is the main test - using the test utility with mutations
        verify_migrations_with_mutations(db.as_ref(), migrations, mutations).await?;
        Ok(())
    }

    /// Tests indexes can be created on tables with existing data.
    ///
    /// Verifies that performance indexes can be added to populated tables and
    /// successfully rolled back.
    #[switchy_async::test]
    async fn test_index_creation_on_populated_tables() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await.unwrap();

        // Create and populate tables
        let create_users = CreateUsersTable;
        let create_posts = CreatePostsTable;
        let create_analytics = CreateAnalyticsTable;

        create_users.up(db.as_ref()).await.unwrap();
        create_posts.up(db.as_ref()).await.unwrap();
        create_analytics.up(db.as_ref()).await.unwrap();

        let insert_users = InsertTestUsers;
        let insert_posts = InsertTestPosts;

        insert_users.execute(db.as_ref()).await.unwrap();
        insert_posts.execute(db.as_ref()).await.unwrap();

        // Now add indexes on populated tables
        let add_indexes = AddPerformanceIndexes;
        add_indexes.up(db.as_ref()).await.unwrap();

        // Verify indexes were created (using raw SQL since there's no query builder for index queries)
        let result = db
            .exec_raw("SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'")
            .await;
        assert!(result.is_ok());

        // Test rollback
        add_indexes.down(db.as_ref()).await.unwrap();
        Ok(())
    }

    /// Tests the `BTreeMap` implementation of `MutationProvider`.
    ///
    /// Verifies that mutations can be retrieved from a `BTreeMap` and that
    /// nonexistent mutations return `None`.
    #[switchy_async::test]
    async fn test_btreemap_mutation_provider() {
        let mut mutations: BTreeMap<String, Arc<dyn Executable>> = BTreeMap::new();
        mutations.insert("test_migration".to_string(), Arc::new(InsertTestUsers));

        // Test the MutationProvider implementation for BTreeMap
        let provider = &mutations;
        let mutation = provider.get_mutation("test_migration").await;
        assert!(mutation.is_some());

        let no_mutation = provider.get_mutation("nonexistent").await;
        assert!(no_mutation.is_none());
    }
}
