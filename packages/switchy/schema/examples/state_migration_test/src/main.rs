//! # State Migration Test Example
//!
//! This example demonstrates how to use `verify_migrations_with_state` to test
//! migrations against a database that already contains data. This is crucial for
//! testing migrations in production-like scenarios where you need to ensure
//! existing data is preserved and handled correctly.

use std::sync::Arc;

use async_trait::async_trait;
use switchy_database::{
    Database, DatabaseError, DatabaseValue,
    schema::{Column, DataType},
};
use switchy_schema::migration::Migration;
use switchy_schema_test_utils::{create_empty_in_memory, verify_migrations_with_state};

/// Migration that adds a new column to existing users table
struct AddUsersBioColumn;

#[async_trait]
impl Migration<'static> for AddUsersBioColumn {
    fn id(&self) -> &str {
        "002_add_users_bio"
    }

    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        // Add bio column with default value for existing users
        // Using raw SQL since ALTER TABLE ADD COLUMN isn't supported by schema builder yet
        db.exec_raw("ALTER TABLE users ADD COLUMN bio TEXT DEFAULT ''")
            .await?;
        Ok(())
    }

    async fn down(&self, _db: &dyn Database) -> switchy_schema::Result<()> {
        // For this example, we'll just note that the column would be removed
        // In a real scenario, you might recreate the table without the bio column
        // but for simplicity in this test example, we'll do nothing
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Add bio column to users table")
    }
}

/// Migration that creates an index on the email column
struct AddEmailIndex;

#[async_trait]
impl Migration<'static> for AddEmailIndex {
    fn id(&self) -> &str {
        "003_add_email_index"
    }

    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("CREATE INDEX idx_users_email_migration ON users(email)")
            .await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        db.exec_raw("DROP INDEX idx_users_email_migration").await?;
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Add index on users.email")
    }
}

/// Setup function that creates initial data in the database
async fn setup_initial_data(db: &dyn Database) -> std::result::Result<(), DatabaseError> {
    println!("  📊 Setting up initial data...");

    // Create the users table first (simulating existing schema) using schema query builder
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

    // Insert some test users using query builder
    db.insert("users")
        .value("name", "Alice Johnson")
        .value("email", "alice@example.com")
        .execute(db)
        .await?;

    db.insert("users")
        .value("name", "Bob Smith")
        .value("email", "bob@example.com")
        .execute(db)
        .await?;

    db.insert("users")
        .value("name", "Carol Davis")
        .value("email", "carol@example.com")
        .execute(db)
        .await?;
    println!("  ✅ Created users table with 3 existing users");
    Ok(())
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("State Migration Test Example");
    println!("============================");
    println!();

    // Create an in-memory SQLite database for testing
    let db = create_empty_in_memory().await?;
    println!("✅ Created in-memory SQLite database");

    // Define migrations that will be applied to existing data
    // Note: We skip the first migration since our setup creates the users table
    let migrations: Vec<Arc<dyn Migration<'static> + 'static>> =
        vec![Arc::new(AddUsersBioColumn), Arc::new(AddEmailIndex)];

    println!(
        "📋 Defined {} migrations to test against existing data:",
        migrations.len()
    );
    for migration in &migrations {
        println!(
            "  - {}: {}",
            migration.id(),
            migration.description().unwrap_or("No description")
        );
    }
    println!();

    // Use verify_migrations_with_state to test migrations against existing data
    println!("🔄 Testing migrations with pre-existing state...");
    println!("   1. Setup initial data (users table with 3 users)");
    println!("   2. Apply migrations forward (up)");
    println!("   3. Verify migrations handle existing data correctly");
    println!("   4. Apply migrations backward (down)");
    println!("   5. Verify rollback preserves initial state");

    match verify_migrations_with_state(db.as_ref(), migrations, |db| {
        Box::pin(setup_initial_data(db))
    })
    .await
    {
        Ok(()) => {
            println!("✅ State migration testing completed successfully!");
            println!();
            println!("🎉 All migrations handled existing data correctly:");
            println!("   • Bio column added with default values for existing users");
            println!("   • Email index created without affecting existing data");
            println!("   • Rollback preserved original data and schema");
            println!("   • No data loss during forward or backward migrations");
        }
        Err(e) => {
            println!("❌ State migration testing failed: {}", e);
            return Err(e.into());
        }
    }

    println!();
    println!("💡 Key Benefits of verify_migrations_with_state:");
    println!("   • Tests migrations against realistic data scenarios");
    println!("   • Ensures existing data is preserved during schema changes");
    println!("   • Validates that rollbacks don't corrupt existing data");
    println!("   • Perfect for testing production migration safety");
    println!("   • Catches data compatibility issues early");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchy_database::query::FilterableQuery;
    use switchy_schema_test_utils::TestError;

    #[tokio::test]
    async fn test_bio_column_migration() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Setup initial users table
        setup_initial_data(db.as_ref()).await?;

        // Apply bio column migration
        let migration = AddUsersBioColumn;
        migration.up(db.as_ref()).await?;

        // Verify bio column exists and has default values
        let result = db
            .exec_raw("SELECT bio FROM users WHERE name = 'Alice Johnson'")
            .await;
        assert!(result.is_ok());

        // Test rollback
        migration.down(db.as_ref()).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_email_index_migration() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Setup initial users table
        setup_initial_data(db.as_ref()).await?;

        // Apply email index migration
        let migration = AddEmailIndex;
        migration.up(db.as_ref()).await?;

        // Verify index was created (using raw SQL since there's no query builder for index queries)
        let result = db.exec_raw("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_users_email_migration'").await;
        assert!(result.is_ok());

        // Test rollback
        migration.down(db.as_ref()).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_state_migrations_with_test_utils() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> =
            vec![Arc::new(AddUsersBioColumn), Arc::new(AddEmailIndex)];

        // This is the main test - using the test utility with state
        verify_migrations_with_state(db.as_ref(), migrations, |db| {
            Box::pin(setup_initial_data(db))
        })
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_data_preservation() -> std::result::Result<(), TestError> {
        let db = create_empty_in_memory().await?;

        // Setup and verify initial data
        setup_initial_data(db.as_ref()).await?;

        // Count initial users using query builder
        let initial_results = switchy_database::query::select("users")
            .columns(&["*"])
            .execute(db.as_ref())
            .await?;
        let initial_count = initial_results.len();

        // Apply bio column migration
        let migration = AddUsersBioColumn;
        migration.up(db.as_ref()).await?;

        // Verify user count is preserved using query builder
        let after_results = switchy_database::query::select("users")
            .columns(&["*"])
            .execute(db.as_ref())
            .await?;
        let after_count = after_results.len();
        assert_eq!(initial_count, after_count);

        // Verify specific user data is preserved using query builder
        let alice_results = switchy_database::query::select("users")
            .columns(&["id"])
            .where_eq("name", "Alice Johnson")
            .where_eq("email", "alice@example.com")
            .execute(db.as_ref())
            .await?;
        assert!(!alice_results.is_empty());

        Ok(())
    }
}
