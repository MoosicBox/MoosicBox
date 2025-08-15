#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Migration test builder for complex migration testing scenarios
//!
//! This module provides the `MigrationTestBuilder` for testing migrations with
//! data insertion at specific points in the migration sequence. This is particularly
//! useful for testing data migration scenarios where existing data needs to be
//! transformed by subsequent migrations.

use std::{future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use switchy_database::{Database, DatabaseError};
use switchy_schema::{
    migration::{Migration, MigrationSource},
    runner::MigrationRunner,
};

use crate::TestError;

/// Builder for complex migration test scenarios with breakpoints
///
/// This builder allows you to:
/// * Run migrations up to specific points
/// * Insert data before or after specific migrations
/// * Set up initial database state
/// * Skip rollback for debugging
/// * Use custom migration table names
pub struct MigrationTestBuilder<'a> {
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
    breakpoints: Vec<Breakpoint<'a>>,
    initial_setup: Option<SetupFn<'a>>,
    skip_rollback: bool,
    table_name: Option<String>,
}

/// A breakpoint in the migration sequence where custom actions can be performed
struct Breakpoint<'a> {
    /// The migration ID to target
    migration_id: String,
    /// When to execute relative to the migration
    timing: BreakpointTiming,
    /// The action to perform at this breakpoint
    action: SetupFn<'a>,
}

/// When to execute a breakpoint action relative to a migration
#[derive(Debug, Clone, PartialEq, Eq)]
enum BreakpointTiming {
    /// Execute before the specified migration runs
    Before,
    /// Execute after the specified migration runs
    After,
}

type SetupFn<'a> = Box<
    dyn for<'db> FnOnce(
            &'db dyn Database,
        )
            -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'db>>
        + Send
        + 'a,
>;

/// Internal helper struct that wraps a Vec of migrations into a `MigrationSource`
struct VecMigrationSource<'a> {
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
}

impl<'a> VecMigrationSource<'a> {
    fn new(migrations: Vec<Arc<dyn Migration<'a> + 'a>>) -> Self {
        Self { migrations }
    }
}

#[async_trait]
impl<'a> MigrationSource<'a> for VecMigrationSource<'a> {
    async fn migrations(&self) -> switchy_schema::Result<Vec<Arc<dyn Migration<'a> + 'a>>> {
        Ok(self.migrations.clone()) // Cheap Arc cloning!
    }
}

impl<'a> MigrationTestBuilder<'a> {
    /// Create a new test builder with the given migrations
    #[must_use]
    pub fn new(migrations: Vec<Arc<dyn Migration<'a> + 'a>>) -> Self {
        Self {
            migrations,
            breakpoints: Vec::new(),
            initial_setup: None,
            skip_rollback: false,
            table_name: None,
        }
    }

    /// Set up initial database state before any migrations run
    #[must_use]
    pub fn with_initial_setup<F>(mut self, setup: F) -> Self
    where
        F: for<'db> FnOnce(
                &'db dyn Database,
            )
                -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'db>>
            + Send
            + 'a,
    {
        self.initial_setup = Some(Box::new(setup));
        self
    }

    /// Insert data BEFORE the specified migration runs
    #[must_use]
    pub fn with_data_before<F>(mut self, migration_id: &str, setup: F) -> Self
    where
        F: for<'db> FnOnce(
                &'db dyn Database,
            )
                -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'db>>
            + Send
            + 'a,
    {
        self.breakpoints.push(Breakpoint {
            migration_id: migration_id.to_string(),
            timing: BreakpointTiming::Before,
            action: Box::new(setup),
        });
        self
    }

    /// Insert data AFTER the specified migration runs
    #[must_use]
    pub fn with_data_after<F>(mut self, migration_id: &str, setup: F) -> Self
    where
        F: for<'db> FnOnce(
                &'db dyn Database,
            )
                -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'db>>
            + Send
            + 'a,
    {
        self.breakpoints.push(Breakpoint {
            migration_id: migration_id.to_string(),
            timing: BreakpointTiming::After,
            action: Box::new(setup),
        });
        self
    }

    /// Skip the rollback phase (useful for debugging)
    #[must_use]
    pub const fn skip_rollback(mut self) -> Self {
        self.skip_rollback = true;
        self
    }

    /// Use a custom migration table name
    #[must_use]
    pub fn with_table_name(mut self, table_name: impl Into<String>) -> Self {
        self.table_name = Some(table_name.into());
        self
    }

    /// Execute the test scenario
    ///
    /// # Errors
    ///
    /// * If initial setup fails
    /// * If a migration ID in breakpoints is not found in the migration list
    /// * If any migration execution fails
    /// * If any breakpoint action fails
    /// * If rollback fails (when not skipped)
    pub async fn run(self, db: &dyn Database) -> Result<(), TestError> {
        use std::collections::BTreeMap;

        // Extract fields to avoid borrow checker issues
        let migrations = self.migrations;
        let breakpoints = self.breakpoints;
        let initial_setup = self.initial_setup;
        let skip_rollback = self.skip_rollback;
        let table_name = self.table_name;

        // Step 1: Run initial setup if provided
        if let Some(setup) = initial_setup {
            setup(db).await?;
        }

        // Step 2: Group breakpoints by migration and sort by migration order
        let mut breakpoints_by_migration: BTreeMap<
            usize,
            (Vec<Breakpoint<'_>>, Vec<Breakpoint<'_>>),
        > = BTreeMap::new();

        for breakpoint in breakpoints {
            // Find the index of this migration in our migration list
            let migration_index = migrations
                .iter()
                .position(|m| m.id() == breakpoint.migration_id)
                .ok_or_else(|| {
                    TestError::Migration(switchy_schema::MigrationError::Validation(format!(
                        "Migration '{}' not found in migration list",
                        breakpoint.migration_id
                    )))
                })?;

            let entry = breakpoints_by_migration
                .entry(migration_index)
                .or_insert((Vec::new(), Vec::new()));
            match breakpoint.timing {
                BreakpointTiming::Before => entry.0.push(breakpoint),
                BreakpointTiming::After => entry.1.push(breakpoint),
            }
        }

        // Step 3: Execute migrations with breakpoints
        let mut current_migration_index = 0;

        for (breakpoint_migration_index, (before_breakpoints, after_breakpoints)) in
            breakpoints_by_migration
        {
            // Run migrations up to (but not including) the breakpoint migration
            if current_migration_index < breakpoint_migration_index {
                let migrations_to_run =
                    migrations[current_migration_index..breakpoint_migration_index].to_vec();
                if !migrations_to_run.is_empty() {
                    let source = VecMigrationSource::new(migrations_to_run);
                    let mut runner = MigrationRunner::new(Box::new(source));

                    if let Some(ref table_name) = table_name {
                        runner = runner.with_table_name(table_name.clone());
                    }

                    runner.run(db).await?;
                }
                current_migration_index = breakpoint_migration_index;
            }

            // Handle the breakpoint migration with before/after actions
            let target_migration = &migrations[breakpoint_migration_index];

            // Execute all "before" actions
            for breakpoint in before_breakpoints {
                (breakpoint.action)(db).await?;
            }

            // Run the migration
            target_migration
                .up(db)
                .await
                .map_err(TestError::Migration)?;

            // Execute all "after" actions
            for breakpoint in after_breakpoints {
                (breakpoint.action)(db).await?;
            }

            // Update migration tracking table manually since we ran the migration directly
            if let Some(ref table_name) = table_name {
                Self::record_migration(db, table_name, target_migration.id()).await?;
            } else {
                Self::record_migration(db, "__switchy_schema_migrations", target_migration.id())
                    .await?;
            }

            current_migration_index += 1;
        }

        // Step 4: Run any remaining migrations after the last breakpoint
        if current_migration_index < migrations.len() {
            let remaining_migrations = migrations[current_migration_index..].to_vec();
            let source = VecMigrationSource::new(remaining_migrations);
            let mut runner = MigrationRunner::new(Box::new(source));

            if let Some(ref table_name) = table_name {
                runner = runner.with_table_name(table_name.clone());
            }

            runner.run(db).await?;
        }

        // Step 5: Rollback all migrations unless skipped
        if !skip_rollback {
            let source = VecMigrationSource::new(migrations);
            let mut runner = MigrationRunner::new(Box::new(source));

            if let Some(ref table_name) = table_name {
                runner = runner.with_table_name(table_name.clone());
            }

            runner
                .rollback(db, switchy_schema::runner::RollbackStrategy::All)
                .await?;
        }

        Ok(())
    }

    /// Record a migration as completed in the migration tracking table
    async fn record_migration(
        db: &dyn Database,
        table_name: &str,
        migration_id: &str,
    ) -> Result<(), TestError> {
        use switchy_database::query;

        // Create the migration table if it doesn't exist
        let create_table_sql = format!(
            "CREATE TABLE IF NOT EXISTS {table_name} (
                id TEXT PRIMARY KEY,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        );
        db.exec_raw(&create_table_sql).await?;

        // Insert the migration record
        query::insert(table_name)
            .value("id", migration_id)
            .execute(db)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::{query, query::FilterableQuery};
    use switchy_schema::migration::Migration;

    // Mock migration for testing
    struct TestMigration {
        id: String,
        up_sql: String,
        down_sql: Option<String>,
    }

    impl TestMigration {
        fn new(id: &str, up_sql: &str, down_sql: Option<&str>) -> Self {
            Self {
                id: id.to_string(),
                up_sql: up_sql.to_string(),
                down_sql: down_sql.map(String::from),
            }
        }
    }

    #[async_trait]
    impl Migration<'static> for TestMigration {
        fn id(&self) -> &str {
            &self.id
        }

        async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
            db.exec_raw(&self.up_sql).await?;
            Ok(())
        }

        async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
            if let Some(ref down_sql) = self.down_sql {
                db.exec_raw(down_sql).await?;
            }
            Ok(())
        }

        fn description(&self) -> Option<&str> {
            None
        }
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_migration_test_builder_basic() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>];

        MigrationTestBuilder::new(migrations)
            .run(&*db)
            .await
            .unwrap();

        // After rollback, tables should not exist
        let result = query::select("sqlite_master")
            .columns(&["name"])
            .where_eq("type", "table")
            .where_eq("name", "users")
            .execute(db.as_ref())
            .await;
        assert!(result.is_err() || result.unwrap().is_empty());
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_migration_test_builder_skip_rollback() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_test",
            "CREATE TABLE test_table (id INTEGER)",
            Some("DROP TABLE test_table"),
        )) as Arc<dyn Migration<'static> + 'static>];

        MigrationTestBuilder::new(migrations)
            .skip_rollback()
            .run(&*db)
            .await
            .unwrap();

        // Table should still exist since we skipped rollback
        let result = query::select("sqlite_master")
            .columns(&["name"])
            .where_eq("type", "table")
            .where_eq("name", "test_table")
            .execute(db.as_ref())
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_migration_test_builder_custom_table_name() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_test",
            "CREATE TABLE test_table (id INTEGER)",
            Some("DROP TABLE test_table"),
        )) as Arc<dyn Migration<'static> + 'static>];

        MigrationTestBuilder::new(migrations)
            .with_table_name("__custom_migrations")
            .skip_rollback()
            .run(&*db)
            .await
            .unwrap();

        // Verify custom migration table was created
        let result = query::select("sqlite_master")
            .columns(&["name"])
            .where_eq("type", "table")
            .where_eq("name", "__custom_migrations")
            .execute(db.as_ref())
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_with_data_before_breakpoint() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![
            Arc::new(TestMigration::new(
                "001_create_users",
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                Some("DROP TABLE users"),
            )) as Arc<dyn Migration<'static> + 'static>,
            Arc::new(TestMigration::new(
                "002_add_email_column",
                "ALTER TABLE users ADD COLUMN email TEXT",
                Some("ALTER TABLE users DROP COLUMN email"),
            )) as Arc<dyn Migration<'static> + 'static>,
        ];

        MigrationTestBuilder::new(migrations)
            .with_data_before("002_add_email_column", |db| {
                Box::pin(async move {
                    // Insert data before the email column is added
                    db.exec_raw("INSERT INTO users (name) VALUES ('Alice')")
                        .await?;
                    Ok(())
                })
            })
            .skip_rollback()
            .run(&*db)
            .await
            .unwrap();

        // Verify the user was inserted and the email column was added
        let result = query::select("users")
            .columns(&["name", "email"])
            .where_eq("name", "Alice")
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        let row = &result[0];
        assert_eq!(row.get("name").unwrap().as_str().unwrap(), "Alice");
        // email column should exist but be NULL since it was added after the row was inserted
        assert_eq!(
            row.get("email").unwrap(),
            switchy_database::DatabaseValue::Null
        );
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_with_data_after_breakpoint() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![
            Arc::new(TestMigration::new(
                "001_create_users",
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                Some("DROP TABLE users"),
            )) as Arc<dyn Migration<'static> + 'static>,
            Arc::new(TestMigration::new(
                "002_add_email_column",
                "ALTER TABLE users ADD COLUMN email TEXT",
                Some("ALTER TABLE users DROP COLUMN email"),
            )) as Arc<dyn Migration<'static> + 'static>,
        ];

        MigrationTestBuilder::new(migrations)
            .with_data_after("002_add_email_column", |db| {
                Box::pin(async move {
                    // Insert data after the email column is added
                    db.exec_raw(
                        "INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')",
                    )
                    .await?;
                    Ok(())
                })
            })
            .skip_rollback()
            .run(&*db)
            .await
            .unwrap();

        // Verify the user was inserted with email data
        let result = query::select("users")
            .columns(&["name", "email"])
            .where_eq("name", "Bob")
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        let row = &result[0];
        assert_eq!(row.get("name").unwrap().as_str().unwrap(), "Bob");
        assert_eq!(
            row.get("email").unwrap().as_str().unwrap(),
            "bob@example.com"
        );
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_multiple_breakpoints_in_sequence() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![
            Arc::new(TestMigration::new(
                "001_create_users",
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                Some("DROP TABLE users"),
            )) as Arc<dyn Migration<'static> + 'static>,
            Arc::new(TestMigration::new(
                "002_add_email_column",
                "ALTER TABLE users ADD COLUMN email TEXT",
                Some("ALTER TABLE users DROP COLUMN email"),
            )) as Arc<dyn Migration<'static> + 'static>,
            Arc::new(TestMigration::new(
                "003_add_age_column",
                "ALTER TABLE users ADD COLUMN age INTEGER",
                Some("ALTER TABLE users DROP COLUMN age"),
            )) as Arc<dyn Migration<'static> + 'static>,
        ];

        MigrationTestBuilder::new(migrations)
            .with_data_before("002_add_email_column", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO users (name) VALUES ('Alice')")
                        .await?;
                    Ok(())
                })
            })
            .with_data_after("002_add_email_column", |db| {
                Box::pin(async move {
                    db.exec_raw(
                        "UPDATE users SET email = 'alice@example.com' WHERE name = 'Alice'",
                    )
                    .await?;
                    Ok(())
                })
            })
            .with_data_after("003_add_age_column", |db| {
                Box::pin(async move {
                    db.exec_raw("UPDATE users SET age = 30 WHERE name = 'Alice'")
                        .await?;
                    Ok(())
                })
            })
            .skip_rollback()
            .run(&*db)
            .await
            .unwrap();

        // Verify all data was inserted and updated correctly
        let result = query::select("users")
            .columns(&["name", "email", "age"])
            .where_eq("name", "Alice")
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        let row = &result[0];
        assert_eq!(row.get("name").unwrap().as_str().unwrap(), "Alice");
        assert_eq!(
            row.get("email").unwrap().as_str().unwrap(),
            "alice@example.com"
        );
        assert_eq!(row.get("age").unwrap().as_i64().unwrap(), 30);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_initial_setup_functionality() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>];

        MigrationTestBuilder::new(migrations)
            .with_initial_setup(|db| {
                Box::pin(async move {
                    // Create a temporary table for setup
                    db.exec_raw("CREATE TABLE temp_setup (value TEXT)").await?;
                    db.exec_raw("INSERT INTO temp_setup VALUES ('setup_complete')")
                        .await?;
                    Ok(())
                })
            })
            .skip_rollback()
            .run(&*db)
            .await
            .unwrap();

        // Verify initial setup ran
        let result = query::select("temp_setup")
            .columns(&["value"])
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        let row = &result[0];
        assert_eq!(
            row.get("value").unwrap().as_str().unwrap(),
            "setup_complete"
        );

        // Verify migration also ran
        let result = query::select("sqlite_master")
            .columns(&["name"])
            .where_eq("type", "table")
            .where_eq("name", "users")
            .execute(db.as_ref())
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_breakpoint_with_nonexistent_migration_id() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>];

        let result = MigrationTestBuilder::new(migrations)
            .with_data_before("999_nonexistent", |_db| Box::pin(async move { Ok(()) }))
            .run(&*db)
            .await;

        // Should return an error for non-existent migration
        assert!(result.is_err());
        let error_msg = format!("{:?}", result.unwrap_err());
        assert!(error_msg.contains("Migration '999_nonexistent' not found"));
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(tokio::test)]
    async fn test_rollback_works_with_breakpoints() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![
            Arc::new(TestMigration::new(
                "001_create_users",
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                Some("DROP TABLE users"),
            )) as Arc<dyn Migration<'static> + 'static>,
            Arc::new(TestMigration::new(
                "002_add_email_column",
                "ALTER TABLE users ADD COLUMN email TEXT",
                Some("ALTER TABLE users DROP COLUMN email"),
            )) as Arc<dyn Migration<'static> + 'static>,
        ];

        MigrationTestBuilder::new(migrations)
            .with_data_before("002_add_email_column", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO users (name) VALUES ('Alice')")
                        .await?;
                    Ok(())
                })
            })
            // Don't skip rollback - let it run
            .run(&*db)
            .await
            .unwrap();

        // After rollback, tables should not exist
        let result = query::select("sqlite_master")
            .columns(&["name"])
            .where_eq("type", "table")
            .where_eq("name", "users")
            .execute(db.as_ref())
            .await;
        assert!(result.is_err() || result.unwrap().is_empty());
    }
}
