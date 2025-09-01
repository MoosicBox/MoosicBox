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

    runner.rollback(db, switchy_schema::RollbackStrategy::Steps(1)).await?;
    println!("✅ Rollback completed - created_at column removed");

    let post_rollback_users = db.select("users").execute(db).await?;
    println!("📊 Users after rollback: {} found", post_rollback_users.len());
    */

    Ok(())
}
