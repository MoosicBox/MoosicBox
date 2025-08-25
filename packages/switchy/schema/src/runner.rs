//! # Migration Runner
//!
//! The migration runner is the core execution engine for running database migrations.
//! It supports multiple execution strategies and provides hooks for customization.
//!
//! ## Basic Usage
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use switchy_schema::runner::{MigrationRunner, ExecutionStrategy, RollbackStrategy};
//! use switchy_schema::migration::{Migration, MigrationSource};
//! use switchy_database::Database;
//!
//! # async fn example(db: &dyn Database) -> switchy_schema::Result<()> {
//! // Create a mock migration source for demonstration
//! struct MockSource;
//!
//! #[async_trait::async_trait]
//! impl MigrationSource<'static> for MockSource {
//!     async fn migrations(&self) -> switchy_schema::Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
//!         Ok(vec![])
//!     }
//! }
//!
//! // Create and configure the runner
//! let runner = MigrationRunner::new(Box::new(MockSource))
//!     .with_strategy(ExecutionStrategy::All);
//!
//! // Run migrations
//! runner.run(db).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Execution Strategies
//!
//! - `ExecutionStrategy::All` - Run all pending migrations
//! - `ExecutionStrategy::UpTo(id)` - Run migrations up to a specific ID
//! - `ExecutionStrategy::Steps(n)` - Run a specific number of migrations
//! - `ExecutionStrategy::DryRun` - Validate without executing
//!
//! ## Specialized Constructors
//!
//! ```rust,no_run
//! use switchy_schema::runner::MigrationRunner;
//!
//! // For embedded migrations (requires actual directory)
//! # #[cfg(feature = "embedded")]
//! # {
//! # use include_dir::include_dir;
//! # static MIGRATIONS: include_dir::Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/test_migrations");
//! # let runner = MigrationRunner::new_embedded(&MIGRATIONS);
//! # }
//!
//! // For directory-based migrations
//! # #[cfg(feature = "directory")]
//! # let runner = MigrationRunner::new_directory("./migrations");
//!
//! // For code-based migrations
//! # #[cfg(feature = "code")]
//! # let runner = MigrationRunner::new_code();
//! ```
//!
//! ## Custom Table Names
//!
//! ```rust,no_run
//! use switchy_schema::runner::MigrationRunner;
//!
//! # #[cfg(feature = "embedded")]
//! # {
//! # use include_dir::include_dir;
//! # static MIGRATIONS: include_dir::Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/test_migrations");
//! // Use a custom table name for migration tracking
//! let runner = MigrationRunner::new_embedded(&MIGRATIONS)
//!     .with_table_name("my_custom_migrations");
//! # }
//! ```

use std::{collections::BTreeMap, sync::Arc};

use crate::{Result, migration::MigrationSource, version::VersionTracker};
use switchy_database::Database;

#[cfg(feature = "embedded")]
use include_dir;

/// Execution strategy for migrations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStrategy {
    /// Run all pending migrations
    All,
    /// Run migrations up to a specific migration ID
    UpTo(String),
    /// Run a specific number of migrations
    Steps(usize),
    /// Dry run - validate migrations without executing
    DryRun,
}

/// Rollback strategy for determining which migrations to roll back
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackStrategy {
    /// Roll back the most recent migration
    Last,
    /// Roll back to (but not including) a specific migration ID
    DownTo(String),
    /// Roll back N migrations
    Steps(usize),
    /// Roll back all applied migrations
    All,
}

/// Migration hooks for customizing execution behavior
#[allow(clippy::type_complexity)]
#[derive(Default)]
pub struct MigrationHooks {
    /// Called before each migration
    pub before_migration: Option<Box<dyn Fn(&str) + Send + Sync>>,
    /// Called after each successful migration
    pub after_migration: Option<Box<dyn Fn(&str) + Send + Sync>>,
    /// Called when a migration fails
    pub on_error: Option<Box<dyn Fn(&str, &crate::MigrationError) + Send + Sync>>,
}

/// Migration runner with configurable execution strategies
pub struct MigrationRunner<'a> {
    source: Box<dyn MigrationSource<'a> + 'a>,
    version_tracker: VersionTracker,
    strategy: ExecutionStrategy,
    hooks: MigrationHooks,
    dry_run: bool,
}

impl<'a> MigrationRunner<'a> {
    /// Create a new migration runner with the given source
    #[must_use]
    pub fn new(source: Box<dyn MigrationSource<'a> + 'a>) -> Self {
        Self {
            source,
            version_tracker: VersionTracker::new(),
            strategy: ExecutionStrategy::All,
            hooks: MigrationHooks::default(),
            dry_run: false,
        }
    }

    /// Set the version tracker (for custom table names)
    #[must_use]
    pub fn with_version_tracker(mut self, version_tracker: VersionTracker) -> Self {
        self.version_tracker = version_tracker;
        self
    }

    /// Set a custom migration table name
    #[must_use]
    pub fn with_table_name(mut self, table_name: impl Into<String>) -> Self {
        self.version_tracker = VersionTracker::with_table_name(table_name.into());
        self
    }

    /// Set the execution strategy
    #[must_use]
    pub fn with_strategy(mut self, strategy: ExecutionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set migration hooks
    #[must_use]
    pub fn with_hooks(mut self, hooks: MigrationHooks) -> Self {
        self.hooks = hooks;
        self
    }

    /// Enable dry run mode
    #[must_use]
    pub const fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Create a runner for embedded migrations
    #[cfg(feature = "embedded")]
    #[must_use]
    pub fn new_embedded(dir: &'static include_dir::Dir<'static>) -> MigrationRunner<'static> {
        use crate::discovery::embedded::EmbeddedMigrationSource;
        let source = EmbeddedMigrationSource::new(dir);
        MigrationRunner::new(Box::new(source))
    }

    /// Create a runner for directory-based migrations
    #[cfg(feature = "directory")]
    #[must_use]
    pub fn new_directory<P: AsRef<std::path::Path>>(path: P) -> MigrationRunner<'static> {
        use crate::discovery::directory::DirectoryMigrationSource;
        let source = DirectoryMigrationSource::from_path(path.as_ref().to_path_buf());
        MigrationRunner::new(Box::new(source))
    }

    /// Create a runner for code-based migrations
    #[cfg(feature = "code")]
    #[must_use]
    pub fn new_code() -> MigrationRunner<'static> {
        use crate::discovery::code::CodeMigrationSource;
        let source = CodeMigrationSource::new();
        MigrationRunner::new(Box::new(source))
    }

    /// Run migrations according to the configured strategy
    ///
    /// # Errors
    ///
    /// * If the migrations table fails to be created
    /// * If fails to select existing ran migrations
    /// * If fails to insert new migration runs
    /// * If migration execution fails
    ///
    /// # Limitations
    ///
    /// * Transaction support is not available in `switchy_database` yet
    /// * Each migration runs independently without transaction isolation
    pub async fn run(&self, db: &dyn Database) -> Result<()> {
        // Ensure the version tracking table exists
        self.version_tracker.ensure_table_exists(db).await?;

        // Get all migrations from the source
        let migrations = self.source.migrations().await?;

        // Create a BTreeMap for deterministic ordering by migration ID
        let mut migration_map = BTreeMap::new();
        for migration in migrations {
            migration_map.insert(migration.id().to_string(), migration);
        }

        // Apply execution strategy
        let migrations_to_run = self.apply_strategy(&migration_map);

        // Execute migrations
        for (migration_id, migration) in migrations_to_run {
            // Check if migration has already been run
            if self
                .version_tracker
                .is_migration_applied(db, &migration_id)
                .await?
            {
                continue;
            }

            // Call before hook
            if let Some(ref hook) = self.hooks.before_migration {
                hook(&migration_id);
            }

            // Execute migration (unless dry run)
            if !self.dry_run {
                match migration.up(db).await {
                    Ok(()) => {
                        // Record migration as completed
                        self.version_tracker
                            .record_migration(db, &migration_id)
                            .await?;

                        // Call after hook
                        if let Some(ref hook) = self.hooks.after_migration {
                            hook(&migration_id);
                        }
                    }
                    Err(e) => {
                        // Call error hook
                        if let Some(ref hook) = self.hooks.on_error {
                            hook(&migration_id, &e);
                        }

                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Apply the execution strategy to filter migrations
    #[allow(clippy::borrowed_box)]
    fn apply_strategy<'b>(
        &self,
        migration_map: &'b BTreeMap<String, Arc<dyn crate::migration::Migration<'a> + 'a>>,
    ) -> BTreeMap<String, &'b Arc<dyn crate::migration::Migration<'a> + 'a>> {
        let mut result = BTreeMap::new();

        match &self.strategy {
            ExecutionStrategy::All => {
                for (id, migration) in migration_map {
                    result.insert(id.clone(), migration);
                }
            }
            ExecutionStrategy::UpTo(target_id) => {
                for (id, migration) in migration_map {
                    result.insert(id.clone(), migration);
                    if id == target_id {
                        break;
                    }
                }
            }
            ExecutionStrategy::Steps(max_steps) => {
                for (id, migration) in migration_map.iter().take(*max_steps) {
                    result.insert(id.clone(), migration);
                }
            }
            ExecutionStrategy::DryRun => {
                // DryRun is handled by the dry_run flag, include all migrations
                for (id, migration) in migration_map {
                    result.insert(id.clone(), migration);
                }
            }
        }

        result
    }

    /// Roll back migrations according to the specified strategy
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection
    /// * `strategy` - Rollback strategy to determine which migrations to roll back
    ///
    /// # Errors
    ///
    /// * If database operations fail
    /// * If a migration's `down()` method fails
    /// * If a migration doesn't have a `down()` method when validation is enabled
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use switchy_schema::runner::{MigrationRunner, RollbackStrategy};
    /// # use std::sync::Arc;
    /// # use switchy_schema::migration::{Migration, MigrationSource};
    /// # use switchy_database::Database;
    /// # async fn example(db: &dyn Database) -> Result<(), switchy_schema::MigrationError> {
    /// # // Create a mock migration source for demonstration
    /// # struct MockSource;
    /// # #[async_trait::async_trait]
    /// # impl MigrationSource<'static> for MockSource {
    /// #     async fn migrations(&self) -> switchy_schema::Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
    /// #         Ok(vec![])
    /// #     }
    /// # }
    /// let runner = MigrationRunner::new(Box::new(MockSource));
    ///
    /// // Roll back the last migration
    /// runner.rollback(db, RollbackStrategy::Last).await?;
    ///
    /// // Roll back 3 migrations
    /// runner.rollback(db, RollbackStrategy::Steps(3)).await?;
    ///
    /// // Roll back to a specific migration (not including it)
    /// runner.rollback(db, RollbackStrategy::DownTo("20240101_initial".to_string())).await?;
    /// # Ok(())
    /// # }    /// ```
    pub async fn rollback(&self, db: &dyn Database, strategy: RollbackStrategy) -> Result<()> {
        // Ensure migrations table exists
        self.version_tracker.ensure_table_exists(db).await?;

        // Get all applied migrations in reverse chronological order
        let applied_migrations = self.version_tracker.get_applied_migrations(db).await?;

        // Determine which migrations to roll back based on strategy
        let migrations_to_rollback = match strategy {
            RollbackStrategy::Last => {
                if applied_migrations.is_empty() {
                    Vec::new()
                } else {
                    vec![applied_migrations[0].clone()]
                }
            }
            RollbackStrategy::Steps(n) => applied_migrations.into_iter().take(n).collect(),
            RollbackStrategy::DownTo(target_id) => {
                let mut result = Vec::new();
                for migration_id in applied_migrations {
                    if migration_id == target_id {
                        break;
                    }
                    result.push(migration_id);
                }
                result
            }
            RollbackStrategy::All => applied_migrations,
        };

        // If no migrations to rollback, return early
        if migrations_to_rollback.is_empty() {
            return Ok(());
        }

        // Get all available migrations
        let mut migration_map = std::collections::BTreeMap::new();
        for migration in self.source.migrations().await? {
            migration_map.insert(migration.id().to_string(), migration);
        }

        // Roll back each migration
        for migration_id in migrations_to_rollback {
            // Find the migration
            let migration = migration_map.get(&migration_id).ok_or_else(|| {
                crate::MigrationError::Execution(format!(
                    "Migration '{migration_id}' not found in migration source"
                ))
            })?;

            // Call before hook
            if let Some(ref hook) = self.hooks.before_migration {
                hook(&migration_id);
            }

            // Execute rollback (unless dry run)
            if !self.dry_run {
                match migration.down(db).await {
                    Ok(()) => {
                        // Remove migration record
                        self.version_tracker
                            .remove_migration(db, &migration_id)
                            .await?;

                        // Call after hook
                        if let Some(ref hook) = self.hooks.after_migration {
                            hook(&migration_id);
                        }
                    }
                    Err(e) => {
                        // Call error hook
                        if let Some(ref hook) = self.hooks.on_error {
                            hook(&migration_id, &e);
                        }

                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    /// List available migrations with their applied status
    ///
    /// Returns a list of all available migrations from the source with information
    /// about which ones have been applied to the database.
    ///
    /// # Errors
    ///
    /// * If migration discovery fails
    /// * If database query for applied migrations fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use switchy_schema::runner::MigrationRunner;
    /// use switchy_database::Database;
    ///
    /// # async fn example(runner: &MigrationRunner<'_>, db: &dyn Database) -> switchy_schema::Result<()> {
    /// let migration_info = runner.list_migrations(db).await?;
    ///
    /// for info in migration_info {
    ///     if info.applied {
    ///         println!("✓ {} - {}", info.id, info.description.unwrap_or_default());
    ///     } else {
    ///         println!("○ {} - {}", info.id, info.description.unwrap_or_default());
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_migrations(
        &self,
        db: &dyn Database,
    ) -> Result<Vec<crate::migration::MigrationInfo>> {
        // Ensure migrations table exists to avoid errors when querying applied migrations
        self.version_tracker.ensure_table_exists(db).await?;

        // Get all available migrations from the source
        let mut migrations = self.source.list().await?;

        // Get applied migration IDs
        let applied_migrations = self.version_tracker.get_applied_migrations(db).await?;
        let applied_set: std::collections::HashSet<String> =
            applied_migrations.into_iter().collect();

        // Update applied status for each migration
        for migration in &mut migrations {
            migration.applied = applied_set.contains(&migration.id);
        }

        // Sort by migration ID for consistent ordering
        migrations.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(migrations)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "code")]
    mod code {
        use super::super::*;
        use crate::discovery::code::{CodeMigration, CodeMigrationSource};
        use crate::version::DEFAULT_MIGRATIONS_TABLE;
        use switchy_database::Executable;

        #[tokio::test]
        async fn test_migration_runner_creation() {
            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // Test that runner is created with default values
            assert!(matches!(runner.strategy, ExecutionStrategy::All));
            assert!(!runner.dry_run);
            assert_eq!(
                runner.version_tracker.table_name(),
                DEFAULT_MIGRATIONS_TABLE
            );
        }

        #[tokio::test]
        async fn test_execution_strategy_configuration() {
            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source))
                .with_strategy(ExecutionStrategy::UpTo("test_migration".to_string()))
                .dry_run();

            assert!(matches!(runner.strategy, ExecutionStrategy::UpTo(_)));
            assert!(runner.dry_run);
        }

        #[tokio::test]
        async fn test_custom_table_name() {
            let source = CodeMigrationSource::new();
            let custom_table_name = "my_custom_migrations";

            // Test with_table_name convenience method
            let runner = MigrationRunner::new(Box::new(source)).with_table_name(custom_table_name);

            assert_eq!(runner.version_tracker.table_name(), custom_table_name);

            // Test with_version_tracker method
            let source2 = CodeMigrationSource::new();
            let version_tracker = VersionTracker::with_table_name(custom_table_name.to_string());
            let runner2 =
                MigrationRunner::new(Box::new(source2)).with_version_tracker(version_tracker);

            assert_eq!(runner2.version_tracker.table_name(), custom_table_name);
        }

        #[tokio::test]
        async fn test_apply_strategy_all() {
            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            let mut migration_map = BTreeMap::new();
            let migration1 = Arc::new(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Arc<dyn crate::migration::Migration<'static> + 'static>;
            let migration2 = Arc::new(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Arc<dyn crate::migration::Migration<'static> + 'static>;

            migration_map.insert("001_test".to_string(), migration1);
            migration_map.insert("002_test".to_string(), migration2);

            let result = runner.apply_strategy(&migration_map);
            assert_eq!(result.len(), 2);
            assert!(result.contains_key("001_test"));
            assert!(result.contains_key("002_test"));
        }

        #[tokio::test]
        async fn test_apply_strategy_up_to() {
            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source))
                .with_strategy(ExecutionStrategy::UpTo("001_test".to_string()));

            let mut migration_map = BTreeMap::new();
            let migration1 = Arc::new(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Arc<dyn crate::migration::Migration<'static> + 'static>;
            let migration2 = Arc::new(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Arc<dyn crate::migration::Migration<'static> + 'static>;

            migration_map.insert("001_test".to_string(), migration1);
            migration_map.insert("002_test".to_string(), migration2);

            let result = runner.apply_strategy(&migration_map);
            assert_eq!(result.len(), 1);
            assert!(result.contains_key("001_test"));
            assert!(!result.contains_key("002_test"));
        }

        #[tokio::test]
        async fn test_apply_strategy_steps() {
            let source = CodeMigrationSource::new();
            let runner =
                MigrationRunner::new(Box::new(source)).with_strategy(ExecutionStrategy::Steps(1));

            let mut migration_map = BTreeMap::new();
            let migration1 = Arc::new(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Arc<dyn crate::migration::Migration<'static> + 'static>;
            let migration2 = Arc::new(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Arc<dyn crate::migration::Migration<'static> + 'static>;

            migration_map.insert("001_test".to_string(), migration1);
            migration_map.insert("002_test".to_string(), migration2);

            let result = runner.apply_strategy(&migration_map);
            assert_eq!(result.len(), 1);
            assert!(result.contains_key("001_test"));
        }

        #[test]
        fn test_rollback_strategy_creation() {
            // Test RollbackStrategy enum variants
            let last = RollbackStrategy::Last;
            let down_to = RollbackStrategy::DownTo("migration_001".to_string());
            let steps = RollbackStrategy::Steps(3);
            let all = RollbackStrategy::All;

            // Test Debug and PartialEq implementations
            assert_eq!(last, RollbackStrategy::Last);
            assert_eq!(
                down_to,
                RollbackStrategy::DownTo("migration_001".to_string())
            );
            assert_eq!(steps, RollbackStrategy::Steps(3));
            assert_eq!(all, RollbackStrategy::All);

            // Test that different strategies are not equal
            assert_ne!(last, steps);
            assert_ne!(down_to, all);
        }

        #[test]
        fn test_rollback_strategy_logic() {
            // Test the logic for determining migrations to rollback
            let applied_migrations = vec![
                "003_latest".to_string(),
                "002_middle".to_string(),
                "001_initial".to_string(),
            ];

            // Test Last strategy
            let last_result = match RollbackStrategy::Last {
                RollbackStrategy::Last => {
                    if applied_migrations.is_empty() {
                        Vec::new()
                    } else {
                        vec![applied_migrations[0].clone()]
                    }
                }
                _ => unreachable!(),
            };
            assert_eq!(last_result, vec!["003_latest".to_string()]);

            // Test Steps strategy
            let steps_result = match RollbackStrategy::Steps(2) {
                RollbackStrategy::Steps(n) => applied_migrations
                    .clone()
                    .into_iter()
                    .take(n)
                    .collect::<Vec<_>>(),
                _ => unreachable!(),
            };
            assert_eq!(
                steps_result,
                vec!["003_latest".to_string(), "002_middle".to_string()]
            );

            // Test DownTo strategy
            let down_to_result = match RollbackStrategy::DownTo("001_initial".to_string()) {
                RollbackStrategy::DownTo(target_id) => {
                    let mut result = Vec::new();
                    for migration_id in &applied_migrations {
                        if migration_id == &target_id {
                            break;
                        }
                        result.push(migration_id.clone());
                    }
                    result
                }
                _ => unreachable!(),
            };
            assert_eq!(
                down_to_result,
                vec!["003_latest".to_string(), "002_middle".to_string()]
            );

            // Test All strategy
            let all_result = match RollbackStrategy::All {
                RollbackStrategy::All => applied_migrations.clone(),
                _ => unreachable!(),
            };
            assert_eq!(all_result, applied_migrations);
        }

        #[test]
        fn test_rollback_edge_cases() {
            // Test empty applied migrations
            let empty_migrations: Vec<String> = vec![];

            let last_empty = match RollbackStrategy::Last {
                RollbackStrategy::Last => {
                    if empty_migrations.is_empty() {
                        Vec::new()
                    } else {
                        vec![empty_migrations[0].clone()]
                    }
                }
                _ => unreachable!(),
            };
            assert!(last_empty.is_empty());

            // Test DownTo with non-existent target
            let applied = vec!["002_test".to_string(), "001_test".to_string()];
            let down_to_missing = match RollbackStrategy::DownTo("999_missing".to_string()) {
                RollbackStrategy::DownTo(target_id) => {
                    let mut result = Vec::new();
                    for migration_id in &applied {
                        if migration_id == &target_id {
                            break;
                        }
                        result.push(migration_id.clone());
                    }
                    result
                }
                _ => unreachable!(),
            };
            // Should rollback all migrations since target not found
            assert_eq!(down_to_missing, applied);

            // Test Steps with more steps than available migrations
            let steps_overflow = match RollbackStrategy::Steps(10) {
                RollbackStrategy::Steps(n) => {
                    applied.clone().into_iter().take(n).collect::<Vec<_>>()
                }
                _ => unreachable!(),
            };
            // Should only rollback available migrations
            assert_eq!(steps_overflow, applied);
        }

        #[tokio::test]
        async fn test_custom_table_name_integration() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration source with a simple migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test_custom_table".to_string(),
                Box::new("CREATE TABLE test_table (id INTEGER PRIMARY KEY);".to_string())
                    as Box<dyn Executable>,
                Some(Box::new("DROP TABLE test_table;".to_string()) as Box<dyn Executable>),
            ));

            let custom_table_name = "custom_migration_tracker";

            // Create runner with custom table name
            let runner = MigrationRunner::new(Box::new(source)).with_table_name(custom_table_name);

            // Run migrations
            runner.run(&*db).await.expect("Migration should succeed");

            // Verify custom table was created and used
            let results = db
                .select(custom_table_name)
                .columns(&["id"])
                .execute(&*db)
                .await
                .expect("Should be able to query custom migration table");

            assert_eq!(results.len(), 1);
            assert_eq!(
                results[0].get("id").unwrap().as_str().unwrap(),
                "001_test_custom_table"
            );

            // Verify the actual test table was created
            let test_results = db
                .select("test_table")
                .columns(&["id"])
                .execute(&*db)
                .await
                .expect("Test table should exist");

            assert_eq!(test_results.len(), 0); // Empty table is fine
        }

        #[tokio::test]
        async fn test_list_migrations_empty_source() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create empty migration source
            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // List migrations should return empty list
            let migrations = runner
                .list_migrations(&*db)
                .await
                .expect("List should succeed");
            assert_eq!(migrations.len(), 0);
        }

        #[tokio::test]
        async fn test_list_migrations_with_applied_status() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration source with test migrations
            let mut source = CodeMigrationSource::new();

            // Add migrations in non-alphabetical order to test sorting
            source.add_migration(CodeMigration::new(
                "002_second".to_string(),
                Box::new("CREATE TABLE second (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            source.add_migration(CodeMigration::new(
                "001_first".to_string(),
                Box::new("CREATE TABLE first (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            source.add_migration(CodeMigration::new(
                "003_third".to_string(),
                Box::new("CREATE TABLE third (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Initially, no migrations should be applied
            let initial_list = runner
                .list_migrations(&*db)
                .await
                .expect("List should succeed");
            assert_eq!(initial_list.len(), 3);

            // Verify sorting by ID
            assert_eq!(initial_list[0].id, "001_first");
            assert_eq!(initial_list[1].id, "002_second");
            assert_eq!(initial_list[2].id, "003_third");

            // All should be unapplied initially
            for info in &initial_list {
                assert!(
                    !info.applied,
                    "Migration {} should not be applied initially",
                    info.id
                );
            }

            // Apply first two migrations
            let partial_runner =
                runner.with_strategy(ExecutionStrategy::UpTo("002_second".to_string()));
            partial_runner
                .run(&*db)
                .await
                .expect("Migrations should succeed");

            // Create a new runner for listing (since the previous one was moved)
            let mut source_for_listing = CodeMigrationSource::new();
            source_for_listing.add_migration(CodeMigration::new(
                "002_second".to_string(),
                Box::new("CREATE TABLE second (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source_for_listing.add_migration(CodeMigration::new(
                "001_first".to_string(),
                Box::new("CREATE TABLE first (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source_for_listing.add_migration(CodeMigration::new(
                "003_third".to_string(),
                Box::new("CREATE TABLE third (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            let listing_runner = MigrationRunner::new(Box::new(source_for_listing));

            // List again to check applied status
            let updated_list = listing_runner
                .list_migrations(&*db)
                .await
                .expect("List should succeed");
            assert_eq!(updated_list.len(), 3);

            // Check applied status
            assert!(updated_list[0].applied, "001_first should be applied");
            assert!(updated_list[1].applied, "002_second should be applied");
            assert!(!updated_list[2].applied, "003_third should not be applied");
        }
    }
}
