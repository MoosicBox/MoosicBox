#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Test utilities for `switchy_schema` migration testing
//!
//! This crate provides comprehensive testing infrastructure for verifying migration
//! correctness and behavior. It supports testing migrations with fresh databases,
//! pre-seeded state, and interleaved mutations between migrations.
//!
//! ## Migration Test Builder
//!
//! The [`MigrationTestBuilder`] provides an ergonomic way to test complex migration
//! scenarios where you need to insert data at specific points in the migration sequence.
//! This is particularly useful for testing data migration scenarios.
//!
//! ### Basic Usage
//!
//! ```rust,no_run
//! # #[cfg(feature = "sqlite")]
//! # {
//! use switchy_schema_test_utils::{MigrationTestBuilder, create_empty_in_memory};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let db = create_empty_in_memory().await?;
//! let migrations = vec![/* your migrations */];
//!
//! MigrationTestBuilder::new(migrations)
//!     .with_table_name("__test_migrations")
//!     .run(&*db)
//!     .await?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ### Testing Data Migrations
//!
//! ```rust,no_run
//! # #[cfg(feature = "sqlite")]
//! # {
//! use switchy_schema_test_utils::MigrationTestBuilder;
//!
//! # async fn example(db: &dyn switchy_database::Database, migrations: Vec<std::sync::Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>) -> Result<(), Box<dyn std::error::Error>> {
//! // Test a data migration scenario
//! MigrationTestBuilder::new(migrations)
//!     .with_data_before(
//!         "002_migrate_user_data",
//!         |db| Box::pin(async move {
//!             // Insert old format data that migration will transform
//!             db.exec_raw("INSERT INTO old_users (name) VALUES ('test')").await
//!         })
//!     )
//!     .run(db)
//!     .await?;
//!
//! // Verify migration transformed data correctly
//! // Note: In real usage, you would use the query builder
//! // let users = query::select("new_users").columns(&["*"]).execute(db).await?;
//! // assert!(!users.is_empty());
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ### Multiple Breakpoints
//!
//! ```rust,no_run
//! # #[cfg(feature = "sqlite")]
//! # {
//! use switchy_schema_test_utils::MigrationTestBuilder;
//!
//! # async fn example(db: &dyn switchy_database::Database, migrations: Vec<std::sync::Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>) -> Result<(), Box<dyn std::error::Error>> {
//! MigrationTestBuilder::new(migrations)
//!     .with_data_after(
//!         "001_create_users",
//!         |db| Box::pin(async move {
//!             db.exec_raw("INSERT INTO users (name) VALUES ('test_user')").await
//!         })
//!     )
//!     .with_data_before(
//!         "003_migrate_posts",
//!         |db| Box::pin(async move {
//!             db.exec_raw("INSERT INTO old_posts (title, user_name) VALUES ('Test', 'test_user')").await
//!         })
//!     )
//!     .run(db)
//!     .await?;
//! # Ok(())
//! # }
//! # }
//! ```

use std::{future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use switchy_database::{Database, DatabaseError};
use switchy_schema::{
    MigrationError,
    migration::{Migration, MigrationSource},
    runner::{MigrationRunner, RollbackStrategy},
};

/// Re-export core types for convenience
pub use switchy_database;
pub use switchy_schema;

/// Re-export the migration test builder for convenience
#[cfg(feature = "sqlite")]
pub use builder::MigrationTestBuilder;

/// Mutation handling for advanced migration testing
pub mod mutations;

/// Test assertion helpers for database schema and migration verification
#[cfg(feature = "sqlite")]
pub mod assertions;

/// Migration test builder for complex testing scenarios
#[cfg(feature = "sqlite")]
pub mod builder;

/// Integration tests demonstrating new migration capabilities
pub mod integration_tests;

/// Snapshot testing utilities for migration verification
#[cfg(feature = "snapshots")]
pub mod snapshots;

/// Test error type that wraps existing errors from `switchy_schema` and `switchy_database`
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    /// Migration error
    #[error(transparent)]
    Migration(#[from] MigrationError),
    /// Database error
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Database connection initialization error
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    DatabaseInit(#[from] switchy_database_connection::InitSqliteSqlxDatabaseError),
}

// Re-export snapshot types when feature is enabled
#[cfg(feature = "snapshots")]
pub use snapshots::{
    MigrationSnapshotTest, Result as SnapshotResult, SnapshotError, SnapshotTester,
};

/// Feature-gated helper to create an empty in-memory `SQLite` database
///
/// # Errors
///
/// * If the `SQLite` database initialization fails
#[cfg(feature = "sqlite")]
pub async fn create_empty_in_memory()
-> Result<Box<dyn Database>, switchy_database_connection::InitSqliteSqlxDatabaseError> {
    // Create in-memory SQLite database using sqlx
    switchy_database_connection::init_sqlite_sqlx(None).await
}

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

/// Test migrations from fresh state - runs migrations forward then backward
///
/// This function creates a `MigrationRunner` internally and tests the full migration
/// cycle: applying all migrations forward, then rolling them all back.
///
/// # Arguments
///
/// * `db` - Database connection to test against
/// * `migrations` - Vector of migrations to test
///
/// # Errors
///
/// * If any migration fails during forward execution
/// * If any migration fails during rollback
/// * If database operations fail
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use switchy_schema_test_utils::{verify_migrations_full_cycle, TestError};
/// use switchy_schema::migration::Migration;
/// use switchy_database::Database;
///
/// # async fn example(db: &dyn Database, migrations: Vec<Arc<dyn Migration<'static> + 'static>>) -> Result<(), TestError> {
/// // Test a set of migrations
/// verify_migrations_full_cycle(db, migrations).await?;
/// # Ok(())
/// # }
/// ```
pub async fn verify_migrations_full_cycle<'a>(
    db: &dyn Database,
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
) -> Result<(), TestError> {
    // Create VecMigrationSource from provided migrations
    let source = VecMigrationSource::new(migrations);

    // Create MigrationRunner internally
    let runner = MigrationRunner::new(Box::new(source));

    // Run all migrations forward (up)
    runner.run(db).await?;

    // Run all migrations backward (down) using rollback functionality
    runner.rollback(db, RollbackStrategy::All).await?;

    Ok(())
}

/// Test migrations with pre-seeded state - runs setup, then migrations forward and backward
///
/// This function allows testing migrations against a database that already contains data.
/// It executes a setup closure first, then runs the full migration cycle.
///
/// # Arguments
///
/// * `db` - Database connection to test against
/// * `migrations` - Vector of migrations to test
/// * `setup` - Closure to populate initial database state
///
/// # Errors
///
/// * If the setup closure fails
/// * If any migration fails during forward execution
/// * If any migration fails during rollback
/// * If database operations fail
///
/// # Examples
///
/// ```rust,no_run
/// use std::sync::Arc;
/// use switchy_schema_test_utils::{verify_migrations_with_state, TestError};
/// use switchy_schema::migration::Migration;
/// use switchy_database::{Database, DatabaseError};
///
/// # async fn example(db: &dyn Database, migrations: Vec<Arc<dyn Migration<'static> + 'static>>) -> Result<(), TestError> {
/// // Test migrations with pre-existing data
/// verify_migrations_with_state(
///     db,
///     migrations,
///     |db| Box::pin(async move {
///         // Setup initial state
///         db.exec_raw("CREATE TABLE existing_table (id INTEGER)").await?;
///         db.exec_raw("INSERT INTO existing_table (id) VALUES (1)").await?;
///         Ok(())
///     })
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn verify_migrations_with_state<'a, F>(
    db: &dyn Database,
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
    setup: F,
) -> Result<(), TestError>
where
    F: for<'db> FnOnce(
        &'db dyn Database,
    )
        -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'db>>,
{
    // Execute setup closure to populate initial state
    setup(db).await?;

    // Create VecMigrationSource from provided migrations
    let source = VecMigrationSource::new(migrations);

    // Create MigrationRunner internally
    let runner = MigrationRunner::new(Box::new(source));

    // Run all migrations forward
    runner.run(db).await?;

    // Run all migrations backward using rollback functionality
    runner.rollback(db, RollbackStrategy::All).await?;

    Ok(())
}

/// Test migrations with data changes between migration steps
///
/// This function allows testing migrations with mutations (data changes) that occur
/// between specific migration steps. This verifies that migrations handle intermediate
/// state changes correctly and that rollback works with mutated data.
///
/// # Arguments
///
/// * `db` - Database connection to test against
/// * `migrations` - Vector of migrations to test
/// * `mutations` - Provider for mutations to execute between migrations
///
/// # Errors
///
/// * If any migration fails during forward execution
/// * If any mutation fails during execution
/// * If any migration fails during rollback
/// * If database operations fail
///
/// # Examples
///
/// ```rust,no_run
/// use std::{collections::BTreeMap, sync::Arc};
/// use switchy_schema_test_utils::{verify_migrations_with_mutations, TestError, mutations::MutationProvider};
/// use switchy_schema::migration::Migration;
/// use switchy_database::{Database, Executable};
///
/// # async fn example(db: &dyn Database, migrations: Vec<Arc<dyn Migration<'static> + 'static>>) -> Result<(), TestError> {
/// // Create mutations to execute between migrations
/// let mut mutation_map = BTreeMap::new();
/// mutation_map.insert(
///     "001_create_users".to_string(),
///     Arc::new("INSERT INTO users (name) VALUES ('test_user')".to_string()) as Arc<dyn Executable>
/// );
///
/// // Test migrations with mutations
/// verify_migrations_with_mutations(db, migrations, mutation_map).await?;
/// # Ok(())
/// # }
/// ```
pub async fn verify_migrations_with_mutations<'a, M>(
    db: &dyn Database,
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
    mutations: M,
) -> Result<(), TestError>
where
    M: mutations::MutationProvider,
{
    // Create VecMigrationSource from provided migrations
    let source = VecMigrationSource::new(migrations.clone());

    // Create MigrationRunner internally
    let runner = MigrationRunner::new(Box::new(source));

    // We need to run migrations one by one to execute mutations between them
    // First, get all migrations in order
    let mut migration_map = std::collections::BTreeMap::new();
    for migration in &migrations {
        migration_map.insert(migration.id().to_string(), Arc::clone(migration));
    }

    // Execute migrations one by one with mutations
    for (migration_id, migration) in &migration_map {
        // Run this single migration
        let single_migration_source = VecMigrationSource::new(vec![Arc::clone(migration)]);
        let single_runner = MigrationRunner::new(Box::new(single_migration_source));
        single_runner.run(db).await?;

        // Execute any mutation for this migration
        if let Some(mutation) = mutations.get_mutation(migration_id).await {
            mutation.execute(db).await?;
        }
    }

    // Now rollback all migrations
    runner.rollback(db, RollbackStrategy::All).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
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
            if let Some(down_sql) = &self.down_sql {
                db.exec_raw(down_sql).await?;
            }
            Ok(())
        }
    }

    #[switchy_async::test]
    async fn test_vec_migration_source() {
        let migration1 = Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let migration2 = Arc::new(TestMigration::new(
            "002_create_posts",
            "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER)",
            Some("DROP TABLE posts"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let test_migrations = vec![migration1, migration2];
        let source = VecMigrationSource::new(test_migrations.clone());

        // Test that migrations() returns the same migrations
        let result = source.migrations().await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id(), "001_create_users");
        assert_eq!(result[1].id(), "002_create_posts");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_full_cycle() {
        let db = create_empty_in_memory().await.unwrap();

        let migration1 = Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let migration2 = Arc::new(TestMigration::new(
            "002_create_posts",
            "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER, title TEXT)",
            Some("DROP TABLE posts"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let test_migrations = vec![migration1, migration2];

        // This should run migrations forward then backward without errors
        let result = verify_migrations_full_cycle(db.as_ref(), test_migrations).await;
        assert!(result.is_ok(), "Full cycle verification failed: {result:?}");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_with_state() {
        let db = create_empty_in_memory().await.unwrap();

        let migration1 = Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let migration2 = Arc::new(TestMigration::new(
            "002_add_email_column",
            "ALTER TABLE existing_data ADD COLUMN email TEXT",
            Some("ALTER TABLE existing_data DROP COLUMN email"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let test_migrations = vec![migration1, migration2];

        // Test with pre-seeded state
        let result = verify_migrations_with_state(db.as_ref(), test_migrations, |db| {
            Box::pin(async move {
                // Setup initial state
                db.exec_raw("CREATE TABLE existing_data (id INTEGER PRIMARY KEY, name TEXT)")
                    .await?;
                db.exec_raw("INSERT INTO existing_data (name) VALUES ('test')")
                    .await?;
                Ok(())
            })
        })
        .await;

        assert!(result.is_ok(), "With state verification failed: {result:?}");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_empty_list() {
        let db = create_empty_in_memory().await.unwrap();
        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = vec![];

        // Empty migration list should work fine
        let result = verify_migrations_full_cycle(db.as_ref(), migrations).await;
        assert!(result.is_ok(), "Empty migration list failed: {result:?}");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_single_migration() {
        let db = create_empty_in_memory().await.unwrap();

        let migration = Arc::new(TestMigration::new(
            "001_single_table",
            "CREATE TABLE single_table (id INTEGER PRIMARY KEY)",
            Some("DROP TABLE single_table"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let single_migration = vec![migration];

        // Single migration should work
        let result = verify_migrations_full_cycle(db.as_ref(), single_migration).await;
        assert!(result.is_ok(), "Single migration failed: {result:?}");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_with_mutations_btreemap() {
        let db = create_empty_in_memory().await.unwrap();

        let migration1 = Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let migration2 = Arc::new(TestMigration::new(
            "002_create_posts",
            "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER, title TEXT)",
            Some("DROP TABLE posts"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let test_migrations = vec![migration1, migration2];

        // Create mutations using BTreeMap
        let mut mutation_map = std::collections::BTreeMap::new();
        mutation_map.insert(
            "001_create_users".to_string(),
            Arc::new("INSERT INTO users (name) VALUES ('test_user')".to_string())
                as Arc<dyn switchy_database::Executable>,
        );

        // Test migrations with mutations
        let result =
            verify_migrations_with_mutations(db.as_ref(), test_migrations, mutation_map).await;
        assert!(result.is_ok(), "Mutations with BTreeMap failed: {result:?}");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_with_mutations_vec() {
        let db = create_empty_in_memory().await.unwrap();

        let migration1 = Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let test_migrations = vec![migration1];

        // Create mutations using Vec
        let mutations = vec![(
            "001_create_users".to_string(),
            Arc::new("INSERT INTO users (name) VALUES ('test_user')".to_string())
                as Arc<dyn switchy_database::Executable>,
        )];

        // Test migrations with mutations
        let result =
            verify_migrations_with_mutations(db.as_ref(), test_migrations, mutations).await;
        assert!(result.is_ok(), "Mutations with Vec failed: {result:?}");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_with_mutations_builder() {
        let db = create_empty_in_memory().await.unwrap();

        let migration1 = Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let test_migrations = vec![migration1];

        // Create mutations using builder pattern
        let mutations = crate::mutations::MutationBuilder::new()
            .add_mutation(
                "001_create_users",
                "INSERT INTO users (name) VALUES ('builder_user')",
            )
            .build();

        // Test migrations with mutations
        let result =
            verify_migrations_with_mutations(db.as_ref(), test_migrations, mutations).await;
        assert!(result.is_ok(), "Mutations with builder failed: {result:?}");
    }

    #[cfg(feature = "sqlite")]
    #[switchy_async::test]
    async fn test_verify_migrations_with_no_mutations() {
        let db = create_empty_in_memory().await.unwrap();

        let migration1 = Arc::new(TestMigration::new(
            "001_create_users",
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
            Some("DROP TABLE users"),
        )) as Arc<dyn Migration<'static> + 'static>;

        let test_migrations = vec![migration1];

        // Create empty mutations
        let mutations =
            std::collections::BTreeMap::<String, Arc<dyn switchy_database::Executable>>::new();

        // Test migrations with no mutations (should work like normal)
        let result =
            verify_migrations_with_mutations(db.as_ref(), test_migrations, mutations).await;
        assert!(
            result.is_ok(),
            "Migrations with no mutations failed: {result:?}"
        );
    }
}
