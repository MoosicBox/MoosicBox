//! Integration tests demonstrating new migration capabilities
//!
//! These tests showcase the advanced features of the migration system:
//! - Rollback functionality
//! - Complex breakpoint patterns
//! - Environment variable integration

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

/// Demonstrates rollback functionality with a realistic migration scenario
///
/// This test creates a table, adds data, then rolls back the migration
/// to verify the rollback capability works end-to-end.
///
/// # Errors
///
/// * `TestError` if database operations fail or rollback doesn't work correctly
///
/// # Panics
///
/// * If the table `users` exists after rollback
#[cfg(feature = "sqlite")]
pub async fn demonstrate_rollback_functionality() -> Result<(), crate::TestError> {
    use std::sync::Arc;

    use switchy_database::query::FilterableQuery as _;
    use switchy_schema::migration::Migration;

    use crate::{MigrationTestBuilder, create_empty_in_memory};

    let db = create_empty_in_memory().await?;

    // Create a simple migration that creates a table
    let migration: Arc<dyn Migration<'static> + 'static> = Arc::new(TestMigration {
        id: "001_create_users_table".to_string(),
        up_sql: Some("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)".to_string()),
        down_sql: Some("DROP TABLE users".to_string()),
    });

    // Run migration with rollback enabled
    MigrationTestBuilder::new(vec![migration])
        .with_rollback() // This is the key - enable rollback
        .run(&*db)
        .await?;

    // Verify table was created and then removed
    let tables = db
        .select("sqlite_master")
        .columns(&["name"])
        .where_eq("type", "table")
        .where_eq("name", "users")
        .execute(&*db)
        .await?;

    // Table should not exist after rollback
    assert!(tables.is_empty(), "Table should not exist after rollback");

    Ok(())
}

/// Demonstrates complex breakpoint patterns with multiple data insertions
///
/// This test shows how to insert data at multiple points during migration
/// execution, testing the flexibility of the breakpoint system.
///
/// # Errors
///
/// Returns `TestError` if database operations fail or breakpoints don't work correctly
///
/// # Panics
///
/// * If any of the assertions fail
#[cfg(feature = "sqlite")]
pub async fn demonstrate_complex_breakpoint_patterns() -> Result<(), crate::TestError> {
    use std::sync::Arc;

    use switchy_database::query::Expression as _;
    use switchy_schema::migration::Migration;

    use crate::{MigrationTestBuilder, create_empty_in_memory};

    let db = create_empty_in_memory().await?;

    let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![
        Arc::new(TestMigration {
            id: "001_create_users".to_string(),
            up_sql: Some(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)".to_string(),
            ),
            down_sql: Some("DROP TABLE users".to_string()),
        }),
        Arc::new(TestMigration {
            id: "002_add_email_column".to_string(),
            up_sql: Some("ALTER TABLE users ADD COLUMN email TEXT".to_string()),
            down_sql: Some("ALTER TABLE users DROP COLUMN email".to_string()),
        }),
        Arc::new(TestMigration {
            id: "003_create_posts".to_string(),
            up_sql: Some(
                "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER, title TEXT NOT NULL)"
                    .to_string(),
            ),
            down_sql: Some("DROP TABLE posts".to_string()),
        }),
    ];

    // Complex pattern: insert data before and after different migrations
    MigrationTestBuilder::new(migrations)
        .with_data_before("002_add_email_column", |db| {
            Box::pin(async move {
                // Insert user before email column is added (should get NULL for email)
                db.exec_raw("INSERT INTO users (name) VALUES ('Alice')")
                    .await?;
                Ok(())
            })
        })
        .with_data_after("002_add_email_column", |db| {
            Box::pin(async move {
                // Insert user after email column is added (can specify email)
                db.exec_raw("INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')")
                    .await?;
                Ok(())
            })
        })
        .with_data_after("003_create_posts", |db| {
            Box::pin(async move {
                // Insert posts after posts table is created
                db.exec_raw("INSERT INTO posts (user_id, title) VALUES (1, 'Alice Post')")
                    .await?;
                db.exec_raw("INSERT INTO posts (user_id, title) VALUES (2, 'Bob Post')")
                    .await?;
                Ok(())
            })
        })
        .run(&*db)
        .await?;

    // Verify the complex data insertion worked correctly
    let users = db
        .select("users")
        .columns(&["name", "email"])
        .execute(&*db)
        .await?;

    assert_eq!(users.len(), 2, "Should have 2 users");

    // Alice should have NULL email (inserted before column was added)
    let alice = &users[0];
    assert_eq!(alice.get("name").unwrap().as_str().unwrap(), "Alice");
    assert!(alice.get("email").unwrap().is_null());

    // Bob should have email (inserted after column was added)
    let bob = &users[1];
    assert_eq!(bob.get("name").unwrap().as_str().unwrap(), "Bob");
    assert_eq!(
        bob.get("email").unwrap().as_str().unwrap(),
        "bob@example.com"
    );

    let posts = db.select("posts").columns(&["title"]).execute(&*db).await?;

    assert_eq!(posts.len(), 2, "Should have 2 posts");

    Ok(())
}

/// Demonstrates environment variable integration with `moosicbox_schema`
///
/// This test verifies that the `MOOSICBOX_SKIP_MIGRATION_EXECUTION` environment
/// variable works correctly to skip migration execution while still populating
/// the migration tracking table with all migrations marked as completed.
///
/// Note: This function is only available in test builds since it requires
/// the `moosicbox_schema` crate which is a dev-dependency.
///
/// # Errors
///
/// Returns `TestError` if environment variable handling fails
///
/// # Panics
///
/// * If any of the assertions fail
#[cfg(all(test, feature = "sqlite"))]
pub async fn demonstrate_environment_variable_integration() -> Result<(), crate::TestError> {
    use switchy_database::query::FilterableQuery as _;

    use crate::create_empty_in_memory;

    let db = create_empty_in_memory().await?;

    // Set the environment variable to skip migration execution
    unsafe {
        std::env::set_var("MOOSICBOX_SKIP_MIGRATION_EXECUTION", "1");
    }

    // Call the actual moosicbox_schema migration functions
    // These should complete successfully but not actually run migrations
    // They should instead populate the migration table with all migrations marked as completed
    let result = moosicbox_schema::migrate_library(&*db).await;

    // Clean up environment variable
    unsafe {
        std::env::remove_var("MOOSICBOX_SKIP_MIGRATION_EXECUTION");
    }

    // Migration should succeed (not error) even though it was skipped
    assert!(
        result.is_ok(),
        "Migration should succeed when skipped via env var"
    );

    // Verify migration tracking table WAS created (even though migrations were skipped)
    let tables = db
        .select("sqlite_master")
        .columns(&["name"])
        .where_eq("type", "table")
        .where_eq("name", "__moosicbox_schema_migrations")
        .execute(&*db)
        .await?;

    // Migration table should exist since we now populate it even when skipping
    assert!(
        !tables.is_empty(),
        "Migration table should exist when migrations are skipped"
    );

    // Verify that migrations were recorded in the table
    let migration_records = db
        .select("__moosicbox_schema_migrations")
        .columns(&["id", "status"])
        .execute(&*db)
        .await?;

    // Should have migration records
    assert!(
        !migration_records.is_empty(),
        "Migration records should exist when skipped via env var"
    );

    // All migrations should be marked as completed
    for record in &migration_records {
        if let Some(status_value) = record.get("status") {
            let status = status_value.as_str();
            assert_eq!(
                status,
                Some("completed"),
                "All migrations should be marked as completed when skipped"
            );
        } else {
            panic!("Migration record missing status field");
        }
    }

    Ok(())
}

/// Simple test migration implementation for demonstrations
#[cfg(feature = "sqlite")]
struct TestMigration {
    id: String,
    up_sql: Option<String>,
    down_sql: Option<String>,
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl switchy_schema::migration::Migration<'static> for TestMigration {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(
        &self,
        db: &dyn switchy_database::Database,
    ) -> Result<(), switchy_schema::MigrationError> {
        if let Some(sql) = &self.up_sql
            && !sql.is_empty()
        {
            db.exec_raw(sql).await?;
        }
        Ok(())
    }

    async fn down(
        &self,
        db: &dyn switchy_database::Database,
    ) -> Result<(), switchy_schema::MigrationError> {
        if let Some(sql) = &self.down_sql
            && !sql.is_empty()
        {
            db.exec_raw(sql).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_rollback_demonstration() {
        demonstrate_rollback_functionality().await.unwrap();
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_complex_breakpoint_demonstration() {
        demonstrate_complex_breakpoint_patterns().await.unwrap();
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_environment_variable_demonstration() {
        demonstrate_environment_variable_integration()
            .await
            .unwrap();
    }
}
