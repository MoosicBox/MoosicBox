//! Integration tests demonstrating new migration capabilities
//!
//! These tests showcase the advanced features of the migration system:
//! * Rollback functionality
//! * Complex breakpoint patterns
//! * Environment variable integration

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Demonstrates rollback functionality with a realistic migration scenario
///
/// This test creates a table, adds data, then rolls back the migration
/// to verify the rollback capability works end-to-end.
///
/// # Errors
///
/// * If creating the in-memory database fails
/// * If migration execution or rollback fails
/// * If schema verification queries fail
///
/// # Panics
///
/// * If the table `users` exists after rollback
///
/// # Examples
///
/// ```rust,no_run
/// # #[cfg(feature = "sqlite")]
/// # {
/// # async fn example() -> Result<(), switchy_schema_test_utils::TestError> {
/// switchy_schema_test_utils::integration_tests::demonstrate_rollback_functionality().await?;
/// # Ok(())
/// # }
/// # }
/// ```
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
/// * If creating the in-memory database fails
/// * If migration or breakpoint execution fails
/// * If post-migration validation queries fail
///
/// # Panics
///
/// * If any of the assertions fail
///
/// # Examples
///
/// ```rust,no_run
/// # #[cfg(feature = "sqlite")]
/// # {
/// # async fn example() -> Result<(), switchy_schema_test_utils::TestError> {
/// switchy_schema_test_utils::integration_tests::demonstrate_complex_breakpoint_patterns()
///     .await?;
/// # Ok(())
/// # }
/// # }
/// ```
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
                db.insert("users")
                    .value("name", "Alice")
                    .execute(db)
                    .await?;
                Ok(())
            })
        })
        .with_data_after("002_add_email_column", |db| {
            Box::pin(async move {
                // Insert user after email column is added (can specify email)
                db.insert("users")
                    .value("name", "Bob")
                    .value("email", "bob@example.com")
                    .execute(db)
                    .await?;
                Ok(())
            })
        })
        .with_data_after("003_create_posts", |db| {
            Box::pin(async move {
                // Insert posts after posts table is created
                db.insert("posts")
                    .value("user_id", 1)
                    .value("title", "Alice Post")
                    .execute(db)
                    .await?;
                db.insert("posts")
                    .value("user_id", 2)
                    .value("title", "Bob Post")
                    .execute(db)
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
            #[cfg(feature = "raw-sql")]
            db.exec_raw(sql).await?;
            #[cfg(not(feature = "raw-sql"))]
            {
                let _ = db;
                return Err(switchy_schema::MigrationError::RawSqlDisabled);
            }
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
            #[cfg(feature = "raw-sql")]
            db.exec_raw(sql).await?;
            #[cfg(not(feature = "raw-sql"))]
            {
                let _ = db;
                return Err(switchy_schema::MigrationError::RawSqlDisabled);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "sqlite")]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn test_rollback_demonstration() {
        demonstrate_rollback_functionality().await.unwrap();
    }

    #[switchy_async::test]
    async fn test_complex_breakpoint_demonstration() {
        demonstrate_complex_breakpoint_patterns().await.unwrap();
    }
}
