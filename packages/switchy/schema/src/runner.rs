//! # Migration Runner
//!
//! The migration runner is the core execution engine for running database migrations.
//! It supports multiple execution strategies and provides hooks for customization.
//!
//! ## Basic Usage
//!
//! ```rust,no_run
//! use switchy_schema::runner::{MigrationRunner, ExecutionStrategy};
//! use switchy_schema::discovery::code::CodeMigrationSource;
//! use switchy_database::Database;
//!
//! # async fn example(db: &dyn Database) -> switchy_schema::Result<()> {
//! // Create a migration source
//! let source = CodeMigrationSource::new();
//!
//! // Create and configure the runner
//! let runner = MigrationRunner::new(Box::new(source))
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

use crate::{Result, migration::MigrationSource, version::VersionTracker};
use std::collections::BTreeMap;
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
        migration_map: &'b BTreeMap<String, Box<dyn crate::migration::Migration<'a> + 'a>>,
    ) -> BTreeMap<String, &'b Box<dyn crate::migration::Migration<'a> + 'a>> {
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
        async fn test_apply_strategy_all() {
            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            let mut migration_map = BTreeMap::new();
            let migration1 = Box::new(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Box<dyn crate::migration::Migration<'static> + 'static>;
            let migration2 = Box::new(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Box<dyn crate::migration::Migration<'static> + 'static>;

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
            let migration1 = Box::new(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Box<dyn crate::migration::Migration<'static> + 'static>;
            let migration2 = Box::new(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Box<dyn crate::migration::Migration<'static> + 'static>;

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
            let migration1 = Box::new(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Box<dyn crate::migration::Migration<'static> + 'static>;
            let migration2 = Box::new(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ))
                as Box<dyn crate::migration::Migration<'static> + 'static>;

            migration_map.insert("001_test".to_string(), migration1);
            migration_map.insert("002_test".to_string(), migration2);

            let result = runner.apply_strategy(&migration_map);
            assert_eq!(result.len(), 1);
            assert!(result.contains_key("001_test"));
        }
    }
}
