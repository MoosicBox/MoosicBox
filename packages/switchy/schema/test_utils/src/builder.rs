#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Migration test builder for complex migration testing scenarios
//!
//! This module provides the `MigrationTestBuilder` for testing migrations with
//! data insertion at specific points in the migration sequence. This is particularly
//! useful for testing data migration scenarios where existing data needs to be
//! transformed by subsequent migrations.
//!
//! ## Default Behavior
//!
//! Migrations persist after execution (no rollback) to allow tests to work with
//! the migrated schema. Use `.with_rollback()` to explicitly enable rollback
//! behavior for tests that need to verify migration reversibility.
//!
//! ## Common Usage Patterns
//!
//! ### Integration Testing (Default)
//! ```rust,no_run
//! use switchy_schema_test_utils::MigrationTestBuilder;
//! use std::sync::Arc;
//!
//! # async fn example(migrations: Vec<Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>, db: &dyn switchy_database::Database) -> Result<(), Box<dyn std::error::Error>> {
//! MigrationTestBuilder::new(migrations)
//!     .with_table_name("__test_migrations")
//!     .run(db)
//!     .await?;
//! // Schema persists for testing
//! # Ok(())
//! # }
//! ```
//!
//! ### Migration Reversibility Testing
//! ```rust,no_run
//! use switchy_schema_test_utils::MigrationTestBuilder;
//! use std::sync::Arc;
//!
//! # async fn example(migrations: Vec<Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>, db: &dyn switchy_database::Database) -> Result<(), Box<dyn std::error::Error>> {
//! MigrationTestBuilder::new(migrations)
//!     .with_rollback()  // Explicitly enable rollback
//!     .run(db)
//!     .await?;
//! // Schema is rolled back after execution
//! # Ok(())
//! # }
//! ```
//!
//! ### Data Migration Testing
//! ```rust,no_run
//! use switchy_schema_test_utils::MigrationTestBuilder;
//! use std::sync::Arc;
//!
//! # async fn example(migrations: Vec<Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>, db: &dyn switchy_database::Database) -> Result<(), Box<dyn std::error::Error>> {
//! MigrationTestBuilder::new(migrations)
//!     .with_data_before("migration_id", |db| {
//!         Box::pin(async move {
//!             // Insert test data before migration
//!             Ok(())
//!         })
//!     })
//!     .with_data_after("migration_id", |db| {
//!         Box::pin(async move {
//!             // Verify data after migration
//!             Ok(())
//!         })
//!     })
//!     .run(db)
//!     .await?;
//! # Ok(())
//! # }
//! ```

use std::{future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use switchy_database::{Database, DatabaseError};
use switchy_schema::{
    migration::{Migration, MigrationSource},
    runner::MigrationRunner,
    version::{DEFAULT_MIGRATIONS_TABLE, VersionTracker},
};

use crate::TestError;

/// Builder for complex migration test scenarios with breakpoints
///
/// Allows running migrations with data insertion at specific points,
/// useful for testing data transformations during migrations.
///
/// By default, migrations persist after execution. Use `.with_rollback()`
/// to enable cleanup after test completion.
pub struct MigrationTestBuilder<'a> {
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
    breakpoints: Vec<Breakpoint<'a>>,
    initial_setup: Option<SetupFn<'a>>,
    with_rollback: bool,
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
    #[must_use]
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
    ///
    /// Migrations will persist by default (no rollback).
    #[must_use]
    pub fn new(migrations: Vec<Arc<dyn Migration<'a> + 'a>>) -> Self {
        Self {
            migrations,
            breakpoints: Vec::new(),
            initial_setup: None,
            with_rollback: false,
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

    /// Enable rollback after migrations complete
    ///
    /// By default, migrations persist to allow testing with the migrated schema.
    /// Use this method to enable rollback for testing migration reversibility.
    #[must_use]
    pub const fn with_rollback(mut self) -> Self {
        self.with_rollback = true;
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
    /// Runs migrations with any configured breakpoints and data insertions.
    /// By default, the migrated schema persists. Use `.with_rollback()` to
    /// enable cleanup after execution.
    ///
    /// # Errors
    ///
    /// * If initial setup fails
    /// * If a migration ID in breakpoints is not found in the migration list
    /// * If any migration execution fails
    /// * If any breakpoint action fails
    /// * If rollback fails (when explicitly enabled)
    pub async fn run(self, db: &dyn Database) -> Result<(), TestError> {
        use std::collections::BTreeMap;

        // Extract fields to avoid borrow checker issues
        let migrations = self.migrations;
        let breakpoints = self.breakpoints;
        let initial_setup = self.initial_setup;
        let with_rollback = self.with_rollback;
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
                Self::record_migration(db, DEFAULT_MIGRATIONS_TABLE, target_migration.id()).await?;
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
        if with_rollback {
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
        // Create the migration table if it doesn't exist
        let version_tracker = VersionTracker::with_table_name(table_name);
        version_tracker.ensure_table_exists(db).await?;
        version_tracker.record_migration(db, migration_id).await?;

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
    #[test_log::test(switchy_async::test)]
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

        // With default behavior, tables should persist
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
    #[test_log::test(switchy_async::test)]
    async fn test_migration_test_builder_default_persistence() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_test",
            "CREATE TABLE test_table (id INTEGER)",
            Some("DROP TABLE test_table"),
        )) as Arc<dyn Migration<'static> + 'static>];

        MigrationTestBuilder::new(migrations)
            .run(&*db)
            .await
            .unwrap();

        // Table should still exist since migrations persist by default
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
    #[test_log::test(switchy_async::test)]
    async fn test_migration_test_builder_custom_table_name() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_test",
            "CREATE TABLE test_table (id INTEGER)",
            Some("DROP TABLE test_table"),
        )) as Arc<dyn Migration<'static> + 'static>];

        MigrationTestBuilder::new(migrations)
            .with_table_name("__custom_migrations")
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
    #[test_log::test(switchy_async::test)]
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
    #[test_log::test(switchy_async::test)]
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
    #[test_log::test(switchy_async::test)]
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
    #[test_log::test(switchy_async::test)]
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
    #[test_log::test(switchy_async::test)]
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
    #[test_log::test(switchy_async::test)]
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
            .with_rollback() // Explicitly enable rollback
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
    #[test_log::test(switchy_async::test)]
    async fn test_multiple_before_breakpoints_same_migration() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![
            Arc::new(TestMigration::new(
                "001_create_users",
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                Some("DROP TABLE users"),
            )) as Arc<dyn Migration<'static> + 'static>,
            Arc::new(TestMigration::new(
                "002_create_posts",
                "CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT)",
                Some("DROP TABLE posts"),
            )) as Arc<dyn Migration<'static> + 'static>,
        ];

        // Multiple data_before for the same migration should execute in order
        MigrationTestBuilder::new(migrations)
            .with_data_after("001_create_users", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO users (name) VALUES ('First')")
                        .await?;
                    Ok(())
                })
            })
            .with_data_after("001_create_users", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO users (name) VALUES ('Second')")
                        .await?;
                    Ok(())
                })
            })
            .run(&*db)
            .await
            .unwrap();

        // Verify both insertions happened
        let result = query::select("users")
            .columns(&["name"])
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(switchy_async::test)]
    async fn test_multiple_after_breakpoints_same_migration() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>];

        // Multiple data_after for the same migration
        MigrationTestBuilder::new(migrations)
            .with_data_after("001_create_users", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO users (name) VALUES ('First')")
                        .await?;
                    Ok(())
                })
            })
            .with_data_after("001_create_users", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO users (name) VALUES ('Second')")
                        .await?;
                    Ok(())
                })
            })
            .with_data_after("001_create_users", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO users (name) VALUES ('Third')")
                        .await?;
                    Ok(())
                })
            })
            .run(&*db)
            .await
            .unwrap();

        // Verify all three insertions happened
        let result = query::select("users")
            .columns(&["name"])
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 3);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(switchy_async::test)]
    async fn test_empty_migrations_with_initial_setup() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![];

        // Should handle empty migrations list with initial setup
        MigrationTestBuilder::new(migrations)
            .with_initial_setup(|db| {
                Box::pin(async move {
                    db.exec_raw("CREATE TABLE test (id INTEGER)").await?;
                    Ok(())
                })
            })
            .run(&*db)
            .await
            .unwrap();

        // Verify initial setup ran
        let result = query::select("sqlite_master")
            .columns(&["name"])
            .where_eq("type", "table")
            .where_eq("name", "test")
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(switchy_async::test)]
    async fn test_builder_with_failing_initial_setup() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>];

        // Initial setup that fails
        let result = MigrationTestBuilder::new(migrations)
            .with_initial_setup(|db| {
                Box::pin(async move {
                    db.exec_raw("INVALID SQL STATEMENT").await?;
                    Ok(())
                })
            })
            .run(&*db)
            .await;

        assert!(result.is_err());
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(switchy_async::test)]
    async fn test_builder_with_failing_breakpoint_action() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>];

        // Breakpoint action that fails
        let result = MigrationTestBuilder::new(migrations)
            .with_data_after("001_create_users", |db| {
                Box::pin(async move {
                    db.exec_raw("INSERT INTO nonexistent_table VALUES (1)")
                        .await?;
                    Ok(())
                })
            })
            .run(&*db)
            .await;

        assert!(result.is_err());
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(switchy_async::test)]
    async fn test_breakpoint_timing_both_before_and_after() {
        let db = crate::create_empty_in_memory().await.unwrap();

        let migrations = vec![Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, counter INTEGER DEFAULT 0)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>];

        // Test both before and after for the same migration
        MigrationTestBuilder::new(migrations)
            .with_data_before("001_create_users", |db| {
                Box::pin(async move {
                    // Before: create a temp table
                    db.exec_raw("CREATE TABLE temp (value INTEGER)").await?;
                    db.exec_raw("INSERT INTO temp VALUES (1)").await?;
                    Ok(())
                })
            })
            .with_data_after("001_create_users", |db| {
                Box::pin(async move {
                    // After: insert into the newly created users table
                    db.exec_raw("INSERT INTO users (name, counter) VALUES ('test', (SELECT value FROM temp))")
                        .await?;
                    Ok(())
                })
            })
            .run(&*db)
            .await
            .unwrap();

        // Verify the complex interaction worked
        let result = query::select("users")
            .columns(&["name", "counter"])
            .execute(db.as_ref())
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].get("counter").unwrap().as_i64().unwrap(), 1);
    }

    #[cfg(feature = "sqlite")]
    #[test_log::test(switchy_async::test)]
    async fn test_vec_migration_source_new() {
        // Test the VecMigrationSource constructor
        let migration = Arc::new(TestMigration::new(
            "001_test",
            "CREATE TABLE test (id INTEGER)",
            Some("DROP TABLE test"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let source = VecMigrationSource::new(vec![migration]);
        let migrations = source.migrations().await.unwrap();

        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].id(), "001_test");
    }
}
