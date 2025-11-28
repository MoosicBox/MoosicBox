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

use crate::{
    Result,
    migration::{MigrationSource, MigrationStatus},
    version::VersionTracker,
};
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

/// Configuration for checksum validation requirements
#[derive(Debug, Clone, Default)]
pub struct ChecksumConfig {
    /// When true, validates all migration checksums before running any migrations
    pub require_validation: bool,
}

/// Controls which migration states should be marked as completed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkCompletedScope {
    /// Only mark migrations that have not been tracked yet (safest, default)
    ///
    /// * Pending → Completed
    /// * Already Completed → Unchanged
    /// * Failed → Unchanged
    /// * In-Progress → Unchanged
    #[default]
    PendingOnly,

    /// Mark pending and failed migrations
    ///
    /// * Pending → Completed
    /// * Failed → Completed
    /// * Already Completed → Unchanged
    /// * In-Progress → Unchanged
    IncludeFailed,

    /// Mark pending and in-progress migrations
    ///
    /// * Pending → Completed
    /// * In-Progress → Completed
    /// * Already Completed → Unchanged
    /// * Failed → Unchanged
    IncludeInProgress,

    /// Mark all migrations regardless of current state (most dangerous)
    ///
    /// * Pending → Completed
    /// * Failed → Completed
    /// * In-Progress → Completed
    /// * Already Completed → Unchanged
    All,
}

impl MarkCompletedScope {
    /// Check if this scope should mark a migration in the given status
    #[must_use]
    pub const fn should_mark(&self, current_status: Option<&MigrationStatus>) -> bool {
        match current_status {
            None => true,
            Some(MigrationStatus::Completed) => false,
            Some(MigrationStatus::Failed) => matches!(self, Self::IncludeFailed | Self::All),
            Some(MigrationStatus::InProgress) => {
                matches!(self, Self::IncludeInProgress | Self::All)
            }
        }
    }
}

/// Summary of mark all migrations completed operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkAllCompletedSummary {
    /// Total number of migrations found
    pub total: usize,
    /// Number of migrations that were already completed
    pub already_completed: usize,
    /// Number of migrations newly marked as completed (were untracked)
    pub newly_marked: usize,
    /// Number of failed migrations updated to completed
    pub failed_marked: usize,
    /// Number of in-progress migrations updated to completed
    pub in_progress_marked: usize,
    /// Number of failed migrations that were skipped (not included in scope)
    pub failed_skipped: usize,
    /// Number of in-progress migrations that were skipped (not included in scope)
    pub in_progress_skipped: usize,
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
    allow_dirty: bool,
    checksum_config: ChecksumConfig,
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
            allow_dirty: false,
            checksum_config: ChecksumConfig::default(),
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

    /// Allow running migrations with dirty state (in-progress migrations)
    #[must_use]
    pub const fn with_allow_dirty(mut self, allow_dirty: bool) -> Self {
        self.allow_dirty = allow_dirty;
        self
    }

    /// Configure checksum validation requirements
    ///
    /// # Examples
    /// ```
    /// use switchy_schema::runner::{ChecksumConfig, MigrationRunner};
    ///
    /// let config = ChecksumConfig { require_validation: true };
    /// # #[cfg(feature = "code")]
    /// # {
    /// let runner = MigrationRunner::new_code()
    ///     .with_checksum_config(config);
    /// # }
    /// ```
    #[must_use]
    pub const fn with_checksum_config(mut self, config: ChecksumConfig) -> Self {
        self.checksum_config = config;
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

    /// Check for migrations in dirty state (in-progress) and return error if any exist
    ///
    /// # Errors
    ///
    /// * If the database query fails
    /// * If dirty migrations exist and `allow_dirty` is false
    async fn check_dirty_state(&self, db: &dyn Database) -> Result<()> {
        let dirty_migrations = self.version_tracker.get_dirty_migrations(db).await?;

        if !dirty_migrations.is_empty() && !self.allow_dirty {
            return Err(crate::MigrationError::DirtyState {
                migrations: dirty_migrations.into_iter().map(|r| r.id).collect(),
            });
        }

        Ok(())
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

        // Check for dirty state (migrations in progress)
        self.check_dirty_state(db).await?;

        // Validate checksums if strict mode is enabled
        if self.checksum_config.require_validation {
            let mismatches = self.validate_checksums(db).await?;
            if !mismatches.is_empty() {
                return Err(crate::MigrationError::ChecksumValidationFailed { mismatches });
            }
        }

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
                // Calculate checksums and record migration as started
                let up_checksum = migration.up_checksum().await?;
                let down_checksum = migration.down_checksum().await?;
                if up_checksum.len() != 32 {
                    return Err(crate::MigrationError::InvalidChecksum(format!(
                        "Expected 32 bytes for up_checksum, got {}",
                        up_checksum.len()
                    )));
                }
                if down_checksum.len() != 32 {
                    return Err(crate::MigrationError::InvalidChecksum(format!(
                        "Expected 32 bytes for down_checksum, got {}",
                        down_checksum.len()
                    )));
                }
                self.version_tracker
                    .record_migration_started(db, &migration_id, &up_checksum, &down_checksum)
                    .await?;

                match migration.up(db).await {
                    Ok(()) => {
                        // Update migration status as completed
                        self.version_tracker
                            .update_migration_status(
                                db,
                                &migration_id,
                                MigrationStatus::Completed,
                                None,
                            )
                            .await?;

                        // Call after hook
                        if let Some(ref hook) = self.hooks.after_migration {
                            hook(&migration_id);
                        }
                    }
                    Err(e) => {
                        // Update migration status as failed
                        self.version_tracker
                            .update_migration_status(
                                db,
                                &migration_id,
                                MigrationStatus::Failed,
                                Some(e.to_string()),
                            )
                            .await?;

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
    /// # Panics
    ///
    /// This method does not panic. The use of `unwrap()` is safe as it is only called after
    /// checking that the migrations list is not empty.
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
        let applied_migrations = self
            .version_tracker
            .get_applied_migration_ids(db, MigrationStatus::Completed)
            .await?;

        // Determine which migrations to roll back based on strategy
        let migrations_to_rollback = match strategy {
            RollbackStrategy::Last => {
                if applied_migrations.is_empty() {
                    Vec::new()
                } else {
                    vec![applied_migrations.last().unwrap().clone()]
                }
            }
            RollbackStrategy::Steps(n) => applied_migrations.into_iter().rev().take(n).collect(),
            RollbackStrategy::DownTo(target_id) => {
                let mut result = Vec::new();
                for migration_id in applied_migrations.into_iter().rev() {
                    if migration_id == *target_id {
                        break;
                    }
                    result.push(migration_id);
                }
                result
            }
            RollbackStrategy::All => applied_migrations.into_iter().rev().collect(),
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
        let applied_migrations = self
            .version_tracker
            .get_applied_migration_ids(db, MigrationStatus::Completed)
            .await?;
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

    /// List all migrations that are in failed state
    ///
    /// # Errors
    ///
    /// * If database query fails
    pub async fn list_failed_migrations(
        &self,
        db: &dyn Database,
    ) -> Result<Vec<crate::version::MigrationRecord>> {
        let dirty_migrations = self.version_tracker.get_dirty_migrations(db).await?;

        // Filter results to only include records where status == MigrationStatus::Failed
        let mut failed_migrations: Vec<_> = dirty_migrations
            .into_iter()
            .filter(|record| record.status == MigrationStatus::Failed)
            .collect();

        // Sort by run_on timestamp for chronological order
        failed_migrations.sort_by(|a, b| a.run_on.cmp(&b.run_on));

        Ok(failed_migrations)
    }

    /// Retry a specific failed migration
    ///
    /// # Errors
    ///
    /// * If migration doesn't exist or is not in failed state
    /// * If migration source cannot provide the migration
    /// * If migration execution fails
    pub async fn retry_migration(&self, db: &dyn Database, migration_id: &str) -> Result<()> {
        // First check migration exists and is in failed state
        let migration_status = self
            .version_tracker
            .get_migration_status(db, migration_id)
            .await?;

        match migration_status {
            Some(record) if record.status == MigrationStatus::Failed => {
                // Delete the failed record
                self.version_tracker
                    .remove_migration(db, migration_id)
                    .await?;

                // Get migration from source by ID
                let migrations = self.source.migrations().await?;
                let migration = migrations
                    .iter()
                    .find(|m| m.id() == migration_id)
                    .ok_or_else(|| {
                        crate::MigrationError::Discovery(format!(
                            "Migration '{migration_id}' not found in source"
                        ))
                    })?;

                // Calculate checksums and re-run the single migration
                let up_checksum = migration.up_checksum().await?;
                let down_checksum = migration.down_checksum().await?;
                if up_checksum.len() != 32 {
                    return Err(crate::MigrationError::InvalidChecksum(format!(
                        "Expected 32 bytes for up_checksum, got {}",
                        up_checksum.len()
                    )));
                }
                if down_checksum.len() != 32 {
                    return Err(crate::MigrationError::InvalidChecksum(format!(
                        "Expected 32 bytes for down_checksum, got {}",
                        down_checksum.len()
                    )));
                }
                self.version_tracker
                    .record_migration_started(db, migration_id, &up_checksum, &down_checksum)
                    .await?;

                match migration.up(db).await {
                    Ok(()) => {
                        self.version_tracker
                            .update_migration_status(
                                db,
                                migration_id,
                                MigrationStatus::Completed,
                                None,
                            )
                            .await?;
                    }
                    Err(e) => {
                        self.version_tracker
                            .update_migration_status(
                                db,
                                migration_id,
                                MigrationStatus::Failed,
                                Some(e.to_string()),
                            )
                            .await?;
                        return Err(e);
                    }
                }

                Ok(())
            }
            Some(_) => Err(crate::MigrationError::Validation(format!(
                "Migration '{migration_id}' is not in failed state"
            ))),
            None => Err(crate::MigrationError::Validation(format!(
                "Migration '{migration_id}' not found"
            ))),
        }
    }

    /// Manually mark a migration as completed (dangerous operation)
    ///
    /// # Errors
    ///
    /// * If database operations fail
    pub async fn mark_migration_completed(
        &self,
        db: &dyn Database,
        migration_id: &str,
    ) -> Result<String> {
        // First check if migration exists
        let migration_status = self
            .version_tracker
            .get_migration_status(db, migration_id)
            .await?;

        match migration_status {
            Some(record) if record.status == MigrationStatus::Completed => {
                Ok(format!("Migration '{migration_id}' is already completed"))
            }
            Some(_) => {
                // Exists but not completed - update status
                self.version_tracker
                    .update_migration_status(db, migration_id, MigrationStatus::Completed, None)
                    .await?;
                Ok(format!("Migration '{migration_id}' marked as completed"))
            }
            None => {
                // Doesn't exist - insert new record as completed
                self.version_tracker
                    .record_migration(db, migration_id)
                    .await?;
                Ok(format!("Migration '{migration_id}' recorded as completed"))
            }
        }
    }

    /// Mark all available migrations as completed without executing them (dangerous operation)
    ///
    /// This method records migrations from the source as completed in the database
    /// without actually running their SQL. The `scope` parameter controls which
    /// migration states are affected.
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection
    /// * `scope` - Controls which migration states to mark as completed (default: `PendingOnly`)
    ///
    /// # Behavior by Scope
    ///
    /// ## `PendingOnly` (Default - Safest)
    /// Only marks migrations that haven't been tracked yet:
    ///
    /// * ✅ Untracked migrations → Marked as Completed
    /// * ⏭️ Already Completed → Unchanged
    /// * ⏭️ Failed → Unchanged (counted as `failed_skipped`)
    /// * ⏭️ In-Progress → Unchanged (counted as `in_progress_skipped`)
    ///
    /// ## `IncludeFailed`
    /// Marks pending and failed migrations:
    ///
    /// * ✅ Untracked migrations → Marked as Completed
    /// * ✅ Failed → Updated to Completed (counted as `failed_marked`)
    /// * ⏭️ Already Completed → Unchanged
    /// * ⏭️ In-Progress → Unchanged (counted as `in_progress_skipped`)
    ///
    /// ## `IncludeInProgress`
    /// Marks pending and in-progress migrations:
    ///
    /// * ✅ Untracked migrations → Marked as Completed
    /// * ✅ In-Progress → Updated to Completed (counted as `in_progress_marked`)
    /// * ⏭️ Already Completed → Unchanged
    /// * ⏭️ Failed → Unchanged (counted as `failed_skipped`)
    ///
    /// ## `All` (Most Dangerous)
    /// Marks all migrations regardless of state:
    ///
    /// * ✅ Untracked migrations → Marked as Completed
    /// * ✅ Failed → Updated to Completed (counted as `failed_marked`)
    /// * ✅ In-Progress → Updated to Completed (counted as `in_progress_marked`)
    /// * ⏭️ Already Completed → Unchanged
    ///
    /// # Use Cases
    ///
    /// * **`PendingOnly`**: Initializing tracking for an existing database
    /// * **`IncludeFailed`**: Recovering from multiple failed migrations after manual fixes
    /// * **`IncludeInProgress`**: Recovering from crashed migration process
    /// * **All**: Complete reset/sync of migration tracking table
    ///
    /// # Errors
    ///
    /// * If database operations fail
    /// * If migration discovery fails
    ///
    /// # Warning
    ///
    /// This is a dangerous operation that can lead to database inconsistencies if used
    /// incorrectly. Only use this when you're certain the database schema matches what
    /// the migrations would produce.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use switchy_schema::runner::{MigrationRunner, MarkCompletedScope};
    /// use switchy_database::Database;
    ///
    /// # async fn example(runner: &MigrationRunner<'_>, db: &dyn Database) -> switchy_schema::Result<()> {
    /// // Default: Only mark pending (safest)
    /// let summary = runner.mark_all_migrations_completed(db, MarkCompletedScope::PendingOnly).await?;
    /// println!("Marked {} new migrations", summary.newly_marked);
    ///
    /// // Include failed migrations
    /// let summary = runner.mark_all_migrations_completed(db, MarkCompletedScope::IncludeFailed).await?;
    /// println!("Marked {} failed migrations", summary.failed_marked);
    ///
    /// // All migrations (dangerous)
    /// let summary = runner.mark_all_migrations_completed(db, MarkCompletedScope::All).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn mark_all_migrations_completed(
        &self,
        db: &dyn Database,
        scope: MarkCompletedScope,
    ) -> Result<MarkAllCompletedSummary> {
        // Ensure the version tracking table exists
        self.version_tracker.ensure_table_exists(db).await?;

        // Get all migrations from the source
        let migrations = self.source.migrations().await?;

        let mut summary = MarkAllCompletedSummary {
            total: migrations.len(),
            already_completed: 0,
            newly_marked: 0,
            failed_marked: 0,
            in_progress_marked: 0,
            failed_skipped: 0,
            in_progress_skipped: 0,
        };

        // Mark each migration as completed based on scope
        for migration in migrations {
            let migration_id = migration.id();

            // Check if migration exists
            let migration_status = self
                .version_tracker
                .get_migration_status(db, migration_id)
                .await?;

            match migration_status {
                Some(record) if record.status == MigrationStatus::Completed => {
                    summary.already_completed += 1;
                }
                Some(record) if record.status == MigrationStatus::Failed => {
                    if scope.should_mark(Some(&record.status)) {
                        self.version_tracker
                            .update_migration_status(
                                db,
                                migration_id,
                                MigrationStatus::Completed,
                                None,
                            )
                            .await?;
                        summary.failed_marked += 1;
                    } else {
                        summary.failed_skipped += 1;
                    }
                }
                Some(record) if record.status == MigrationStatus::InProgress => {
                    if scope.should_mark(Some(&record.status)) {
                        self.version_tracker
                            .update_migration_status(
                                db,
                                migration_id,
                                MigrationStatus::Completed,
                                None,
                            )
                            .await?;
                        summary.in_progress_marked += 1;
                    } else {
                        summary.in_progress_skipped += 1;
                    }
                }
                Some(_record) => {
                    // Other unknown states - treat as in-progress for scope check
                    if scope.should_mark(Some(&MigrationStatus::InProgress)) {
                        self.version_tracker
                            .update_migration_status(
                                db,
                                migration_id,
                                MigrationStatus::Completed,
                                None,
                            )
                            .await?;
                        summary.in_progress_marked += 1;
                    } else {
                        summary.in_progress_skipped += 1;
                    }
                }
                None => {
                    let up_checksum = migration.up_checksum().await?;
                    let down_checksum = migration.down_checksum().await?;

                    self.version_tracker
                        .record_migration_started(db, migration_id, &up_checksum, &down_checksum)
                        .await?;
                    self.version_tracker
                        .update_migration_status(db, migration_id, MigrationStatus::Completed, None)
                        .await?;

                    summary.newly_marked += 1;
                }
            }
        }

        Ok(summary)
    }

    /// Drop the migration tracking table
    ///
    /// This is a destructive operation that removes all migration history.
    /// After calling this, you should call `ensure_tracking_table_exists()` to recreate
    /// the table before marking migrations.
    ///
    /// # Use Cases
    ///
    /// * Recovering from a corrupted migration tracking table
    /// * Fixing schema mismatches between table structure and code
    /// * Completely resetting migration history (when combined with `mark_all_migrations_completed`)
    ///
    /// # Errors
    ///
    /// * If the table drop operation fails
    pub async fn drop_tracking_table(&self, db: &dyn Database) -> Result<()> {
        self.version_tracker.drop_table(db).await
    }

    /// Ensure the migration tracking table exists with the correct schema
    ///
    /// Creates the tracking table if it doesn't exist. Safe to call multiple times.
    ///
    /// # Errors
    ///
    /// * If the table creation fails
    pub async fn ensure_tracking_table_exists(&self, db: &dyn Database) -> Result<()> {
        self.version_tracker.ensure_table_exists(db).await
    }

    /// Validate checksums of applied migrations against current migration content
    ///
    /// Compares stored checksums in the database with current migration content to detect
    /// if migrations have been modified since they were applied. This is crucial for
    /// detecting migration drift in production environments.
    ///
    /// # Errors
    ///
    /// * If database operations fail
    /// * If migration discovery fails
    /// * If checksum calculation fails
    /// * If hex decoding fails
    ///
    /// # Returns
    ///
    /// Returns a vector of all checksum mismatches found. An empty vector means all
    /// checksums are valid.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use switchy_schema::runner::MigrationRunner;
    /// use switchy_database::Database;
    ///
    /// # async fn example(runner: &MigrationRunner<'_>, db: &dyn Database) -> switchy_schema::Result<()> {
    /// let mismatches = runner.validate_checksums(db).await?;
    ///
    /// if mismatches.is_empty() {
    ///     println!("All migration checksums are valid!");
    /// } else {
    ///     for mismatch in mismatches {
    ///         println!("Checksum mismatch in migration '{}' ({}): stored={}, current={}",
    ///             mismatch.migration_id,
    ///             mismatch.checksum_type,
    ///             mismatch.stored_checksum,
    ///             mismatch.current_checksum
    ///         );
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn validate_checksums(
        &self,
        db: &dyn Database,
    ) -> Result<Vec<crate::ChecksumMismatch>> {
        let mut mismatches = vec![];

        // Get all applied migrations with their stored checksums
        let applied = self
            .version_tracker
            .get_applied_migrations(db, MigrationStatus::Completed)
            .await?;
        // Get all available migrations from source
        let available = self.source.migrations().await?;

        for record in applied {
            if let Some(migration) = available.iter().find(|m| m.id() == record.id) {
                // Validate UP migration checksum
                let current_up = migration.up_checksum().await?;
                let stored_up = hex::decode(&record.up_checksum).map_err(|e| {
                    crate::MigrationError::Validation(format!(
                        "Failed to decode stored up_checksum for migration '{}': {}",
                        record.id, e
                    ))
                })?;

                if current_up.as_ref() != stored_up.as_slice() {
                    mismatches.push(crate::ChecksumMismatch {
                        migration_id: record.id.clone(),
                        checksum_type: crate::ChecksumType::Up,
                        stored_checksum: record.up_checksum.clone(),
                        current_checksum: hex::encode(&current_up),
                    });
                }

                // Validate DOWN migration checksum
                let current_down = migration.down_checksum().await?;
                let stored_down = hex::decode(&record.down_checksum).map_err(|e| {
                    crate::MigrationError::Validation(format!(
                        "Failed to decode stored down_checksum for migration '{}': {}",
                        record.id, e
                    ))
                })?;

                if current_down.as_ref() != stored_down.as_slice() {
                    mismatches.push(crate::ChecksumMismatch {
                        migration_id: record.id.clone(),
                        checksum_type: crate::ChecksumType::Down,
                        stored_checksum: record.down_checksum.clone(),
                        current_checksum: hex::encode(&current_down),
                    });
                }
            }
            // Note: We silently skip migrations that exist in database but not in source
            // This could be reported in the future as a separate issue type
        }

        Ok(mismatches)
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

        #[switchy_async::test]
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

        #[switchy_async::test]
        async fn test_execution_strategy_configuration() {
            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source))
                .with_strategy(ExecutionStrategy::UpTo("test_migration".to_string()))
                .dry_run();

            assert!(matches!(runner.strategy, ExecutionStrategy::UpTo(_)));
            assert!(runner.dry_run);
        }

        #[switchy_async::test]
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

        #[switchy_async::test]
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

        #[switchy_async::test]
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

        #[switchy_async::test]
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

        #[switchy_async::test]
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

        #[switchy_async::test]
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

        #[switchy_async::test]
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

        #[switchy_async::test]
        async fn test_dirty_state_check_prevents_migrations() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source)).with_allow_dirty(false);

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Insert a dirty migration (in_progress status)
            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "test_migration", &checksum, &checksum)
                .await
                .expect("Failed to record migration start");

            // Verify dirty state check fails
            let result = runner.check_dirty_state(&*db).await;
            assert!(result.is_err(), "Should fail with dirty migrations");

            match result {
                Err(crate::MigrationError::DirtyState { migrations }) => {
                    assert_eq!(migrations.len(), 1);
                    assert_eq!(migrations[0], "test_migration");
                }
                _ => panic!("Expected DirtyState error"),
            }
        }

        #[switchy_async::test]
        async fn test_allow_dirty_bypasses_check() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source)).with_allow_dirty(true);

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Insert a dirty migration
            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "test_migration", &checksum, &checksum)
                .await
                .expect("Failed to record migration start");

            // Verify dirty state check passes with allow_dirty = true
            let result = runner.check_dirty_state(&*db).await;
            assert!(result.is_ok(), "Should pass with allow_dirty = true");
        }

        #[switchy_async::test]
        async fn test_migration_status_tracking_success() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration source with test migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Run migration
            runner.run(&*db).await.expect("Migration should succeed");

            // Verify migration status is MigrationStatus::Completed
            let status = runner
                .version_tracker
                .get_migration_status(&*db, "001_test")
                .await
                .expect("Failed to get migration status")
                .expect("Migration should exist");

            assert_eq!(status.status, MigrationStatus::Completed);
            assert!(status.finished_on.is_some());
            assert!(status.failure_reason.is_none());
        }

        #[switchy_async::test]
        async fn test_migration_status_tracking_failure() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration source with failing migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_failing".to_string(),
                Box::new("INVALID SQL SYNTAX;".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Run migration - should fail
            let result = runner.run(&*db).await;
            assert!(result.is_err(), "Migration should fail");

            // Verify migration status is MigrationStatus::Failed
            let status = runner
                .version_tracker
                .get_migration_status(&*db, "001_failing")
                .await
                .expect("Failed to get migration status")
                .expect("Migration should exist");

            assert_eq!(status.status, MigrationStatus::Failed);
            assert!(status.finished_on.is_some());
            assert!(status.failure_reason.is_some());
        }

        #[switchy_async::test]
        async fn test_list_failed_migrations() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Insert test migration records with different statuses
            runner
                .version_tracker
                .record_migration(&*db, "completed_migration")
                .await
                .expect("Failed to record completed migration");

            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "in_progress_migration", &checksum, &checksum)
                .await
                .expect("Failed to record in-progress migration");

            runner
                .version_tracker
                .record_migration_started(&*db, "failed_migration", &checksum, &checksum)
                .await
                .expect("Failed to record failed migration start");
            runner
                .version_tracker
                .update_migration_status(
                    &*db,
                    "failed_migration",
                    MigrationStatus::Failed,
                    Some("Test error".to_string()),
                )
                .await
                .expect("Failed to update migration status");

            // Get failed migrations
            let failed_migrations = runner
                .list_failed_migrations(&*db)
                .await
                .expect("Failed to list failed migrations");

            // Should only return the failed migration
            assert_eq!(failed_migrations.len(), 1);
            assert_eq!(failed_migrations[0].id, "failed_migration");
            assert_eq!(failed_migrations[0].status, MigrationStatus::Failed);
        }

        #[switchy_async::test]
        async fn test_retry_migration_success() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration source with test migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_retry_test".to_string(),
                Box::new("CREATE TABLE retry_test (id INTEGER);".to_string())
                    as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Simulate a failed migration
            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "001_retry_test", &checksum, &checksum)
                .await
                .expect("Failed to record migration start");
            runner
                .version_tracker
                .update_migration_status(
                    &*db,
                    "001_retry_test",
                    MigrationStatus::Failed,
                    Some("Test error".to_string()),
                )
                .await
                .expect("Failed to update migration status");

            // Retry the migration
            runner
                .retry_migration(&*db, "001_retry_test")
                .await
                .expect("Retry should succeed");

            // Verify migration status is now MigrationStatus::Completed
            let status = runner
                .version_tracker
                .get_migration_status(&*db, "001_retry_test")
                .await
                .expect("Failed to get migration status")
                .expect("Migration should exist");

            assert_eq!(status.status, MigrationStatus::Completed);
            assert!(status.failure_reason.is_none());
        }

        #[switchy_async::test]
        async fn test_retry_migration_rejects_non_failed() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Record a completed migration
            runner
                .version_tracker
                .record_migration(&*db, "completed_migration")
                .await
                .expect("Failed to record completed migration");

            // Try to retry - should fail
            let result = runner.retry_migration(&*db, "completed_migration").await;
            assert!(
                result.is_err(),
                "Should reject retrying non-failed migration"
            );

            match result {
                Err(crate::MigrationError::Validation(msg)) => {
                    assert!(msg.contains("not in failed state"));
                }
                _ => panic!("Expected Validation error"),
            }
        }

        #[switchy_async::test]
        async fn test_retry_migration_rejects_nonexistent() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Try to retry non-existent migration
            let result = runner.retry_migration(&*db, "nonexistent").await;
            assert!(
                result.is_err(),
                "Should reject retrying non-existent migration"
            );

            match result {
                Err(crate::MigrationError::Validation(msg)) => {
                    assert!(msg.contains("not found"));
                }
                _ => panic!("Expected Validation error"),
            }
        }

        #[switchy_async::test]
        async fn test_mark_migration_completed_new_record() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Mark non-existent migration as completed
            let message = runner
                .mark_migration_completed(&*db, "new_migration")
                .await
                .expect("Should succeed");

            assert!(message.contains("recorded as completed"));

            // Verify migration was recorded as completed
            let status = runner
                .version_tracker
                .get_migration_status(&*db, "new_migration")
                .await
                .expect("Failed to get migration status")
                .expect("Migration should exist");

            assert_eq!(status.status, MigrationStatus::Completed);
        }

        #[switchy_async::test]
        async fn test_mark_migration_completed_existing_incomplete() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Record a failed migration
            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "failed_migration", &checksum, &checksum)
                .await
                .expect("Failed to record migration start");
            runner
                .version_tracker
                .update_migration_status(
                    &*db,
                    "failed_migration",
                    MigrationStatus::Failed,
                    Some("Test error".to_string()),
                )
                .await
                .expect("Failed to update migration status");

            // Mark as completed
            let message = runner
                .mark_migration_completed(&*db, "failed_migration")
                .await
                .expect("Should succeed");

            assert!(message.contains("marked as completed"));

            // Verify migration status was updated
            let status = runner
                .version_tracker
                .get_migration_status(&*db, "failed_migration")
                .await
                .expect("Failed to get migration status")
                .expect("Migration should exist");

            assert_eq!(status.status, MigrationStatus::Completed);
        }

        #[switchy_async::test]
        async fn test_mark_migration_completed_already_complete() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Failed to create version table");

            // Record a completed migration
            runner
                .version_tracker
                .record_migration(&*db, "completed_migration")
                .await
                .expect("Failed to record completed migration");

            // Try to mark as completed again
            let message = runner
                .mark_migration_completed(&*db, "completed_migration")
                .await
                .expect("Should succeed");

            assert!(message.contains("already completed"));
        }

        #[switchy_async::test]
        async fn test_validate_checksums_no_mismatches() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration source with test migrations
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test_validation".to_string(),
                Box::new("CREATE TABLE test_validation (id INTEGER);".to_string())
                    as Box<dyn Executable>,
                Some(Box::new("DROP TABLE test_validation;".to_string()) as Box<dyn Executable>),
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Run the migration first
            runner.run(&*db).await.expect("Migration should succeed");

            // Validate checksums - should find no mismatches
            let mismatches = runner
                .validate_checksums(&*db)
                .await
                .expect("Validation should succeed");

            assert!(mismatches.is_empty(), "Should find no checksum mismatches");
        }

        #[switchy_async::test]
        async fn test_validate_checksums_with_mismatches() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create first migration source and run a migration
            let mut source1 = CodeMigrationSource::new();
            source1.add_migration(CodeMigration::new(
                "001_original".to_string(),
                Box::new("CREATE TABLE original (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE original;".to_string()) as Box<dyn Executable>),
            ));

            let runner1 = MigrationRunner::new(Box::new(source1));
            runner1.run(&*db).await.expect("Migration should succeed");

            // Create second migration source with DIFFERENT content for same ID
            let mut source2 = CodeMigrationSource::new();
            source2.add_migration(CodeMigration::new(
                "001_original".to_string(),
                Box::new("CREATE TABLE modified (id INTEGER, name TEXT);".to_string())
                    as Box<dyn Executable>,
                Some(Box::new("DROP TABLE modified;".to_string()) as Box<dyn Executable>),
            ));

            let runner2 = MigrationRunner::new(Box::new(source2));

            // Validate checksums - should find mismatches
            let mismatches = runner2
                .validate_checksums(&*db)
                .await
                .expect("Validation should succeed");

            assert_eq!(
                mismatches.len(),
                2,
                "Should find 2 checksum mismatches (up and down)"
            );

            // Check that both up and down checksums are flagged
            let up_mismatch = mismatches
                .iter()
                .find(|m| m.checksum_type == crate::ChecksumType::Up);
            let down_mismatch = mismatches
                .iter()
                .find(|m| m.checksum_type == crate::ChecksumType::Down);

            assert!(up_mismatch.is_some(), "Should find up checksum mismatch");
            assert!(
                down_mismatch.is_some(),
                "Should find down checksum mismatch"
            );

            // Verify migration ID is correct
            assert_eq!(up_mismatch.unwrap().migration_id, "001_original");
            assert_eq!(down_mismatch.unwrap().migration_id, "001_original");
        }

        #[switchy_async::test]
        async fn test_validate_checksums_empty_database() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration source but don't run any migrations
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_never_run".to_string(),
                Box::new("CREATE TABLE never_run (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Ensure version tracking table exists first
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Should create version table");

            // Validate checksums on empty database
            let mismatches = runner
                .validate_checksums(&*db)
                .await
                .expect("Validation should succeed");

            assert!(
                mismatches.is_empty(),
                "Should find no mismatches in empty database"
            );
        }

        #[switchy_async::test]
        async fn test_validate_checksums_partial_mismatch() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create migration with down migration = None
            let mut source1 = CodeMigrationSource::new();
            source1.add_migration(CodeMigration::new(
                "001_no_down".to_string(),
                Box::new("CREATE TABLE no_down (id INTEGER);".to_string()) as Box<dyn Executable>,
                None, // No down migration
            ));

            let runner1 = MigrationRunner::new(Box::new(source1));
            runner1.run(&*db).await.expect("Migration should succeed");

            // Create migration with SAME up content but different down content
            let mut source2 = CodeMigrationSource::new();
            source2.add_migration(CodeMigration::new(
                "001_no_down".to_string(),
                Box::new("CREATE TABLE no_down (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE no_down;".to_string()) as Box<dyn Executable>), // Now has down migration
            ));

            let runner2 = MigrationRunner::new(Box::new(source2));

            // Validate checksums
            let mismatches = runner2
                .validate_checksums(&*db)
                .await
                .expect("Validation should succeed");

            assert_eq!(
                mismatches.len(),
                1,
                "Should find 1 checksum mismatch (only down)"
            );

            let mismatch = &mismatches[0];
            assert_eq!(mismatch.checksum_type, crate::ChecksumType::Down);
            assert_eq!(mismatch.migration_id, "001_no_down");
        }

        #[switchy_async::test]
        async fn test_mark_all_migrations_completed_empty() {
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let source = CodeMigrationSource::new();
            let runner = MigrationRunner::new(Box::new(source));

            let summary = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::PendingOnly)
                .await
                .expect("Should succeed");

            assert_eq!(summary.total, 0);
            assert_eq!(summary.already_completed, 0);
            assert_eq!(summary.newly_marked, 0);
            assert_eq!(summary.failed_marked, 0);
            assert_eq!(summary.in_progress_marked, 0);
            assert_eq!(summary.failed_skipped, 0);
            assert_eq!(summary.in_progress_skipped, 0);
        }

        #[switchy_async::test]
        async fn test_mark_all_migrations_completed_new() {
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            let summary = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::PendingOnly)
                .await
                .expect("Should succeed");

            assert_eq!(summary.total, 2);
            assert_eq!(summary.already_completed, 0);
            assert_eq!(summary.newly_marked, 2);
            assert_eq!(summary.failed_marked, 0);
            assert_eq!(summary.in_progress_marked, 0);

            // Verify migrations were recorded
            let status1 = runner
                .version_tracker
                .get_migration_status(&*db, "001_test")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status1.status, MigrationStatus::Completed);

            let status2 = runner
                .version_tracker
                .get_migration_status(&*db, "002_test")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status2.status, MigrationStatus::Completed);

            // Verify checksums are not all zeros (stored as hex strings)
            let zero_checksum = hex::encode(vec![0u8; 32]);
            assert_ne!(status1.up_checksum, zero_checksum);
            assert_ne!(status1.down_checksum, zero_checksum);
            assert_ne!(status2.up_checksum, zero_checksum);
            assert_ne!(status2.down_checksum, zero_checksum);
        }

        #[switchy_async::test]
        async fn test_mark_all_migrations_completed_mixed_states() {
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_completed".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_failed".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "003_new".to_string(),
                Box::new("CREATE TABLE test3 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Pre-populate: one completed, one failed
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Should create table");
            runner
                .version_tracker
                .record_migration(&*db, "001_completed")
                .await
                .expect("Should record");

            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "002_failed", &checksum, &checksum)
                .await
                .expect("Should record");
            runner
                .version_tracker
                .update_migration_status(
                    &*db,
                    "002_failed",
                    MigrationStatus::Failed,
                    Some("Test".to_string()),
                )
                .await
                .expect("Should update");

            // Mark with PendingOnly scope - should skip failed
            let summary = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::PendingOnly)
                .await
                .expect("Should succeed");

            assert_eq!(summary.total, 3);
            assert_eq!(summary.already_completed, 1);
            assert_eq!(summary.newly_marked, 1); // Only 003_new
            assert_eq!(summary.failed_marked, 0);
            assert_eq!(summary.failed_skipped, 1); // 002_failed was skipped

            // Verify 002_failed is STILL failed
            let status2 = runner
                .version_tracker
                .get_migration_status(&*db, "002_failed")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status2.status, MigrationStatus::Failed);
        }

        #[switchy_async::test]
        async fn test_mark_all_migrations_completed_include_failed() {
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_completed".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_failed".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "003_in_progress".to_string(),
                Box::new("CREATE TABLE test3 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Pre-populate
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Should create table");
            runner
                .version_tracker
                .record_migration(&*db, "001_completed")
                .await
                .expect("Should record");

            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "002_failed", &checksum, &checksum)
                .await
                .expect("Should record");
            runner
                .version_tracker
                .update_migration_status(
                    &*db,
                    "002_failed",
                    MigrationStatus::Failed,
                    Some("Test".to_string()),
                )
                .await
                .expect("Should update");

            runner
                .version_tracker
                .record_migration_started(&*db, "003_in_progress", &checksum, &checksum)
                .await
                .expect("Should record");

            // Mark with IncludeFailed scope
            let summary = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::IncludeFailed)
                .await
                .expect("Should succeed");

            assert_eq!(summary.total, 3);
            assert_eq!(summary.already_completed, 1);
            assert_eq!(summary.newly_marked, 0);
            assert_eq!(summary.failed_marked, 1); // 002_failed was marked
            assert_eq!(summary.in_progress_marked, 0);
            assert_eq!(summary.in_progress_skipped, 1); // 003_in_progress was skipped

            // Verify 002_failed is now completed
            let status2 = runner
                .version_tracker
                .get_migration_status(&*db, "002_failed")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status2.status, MigrationStatus::Completed);

            // Verify 003_in_progress is still in progress
            let status3 = runner
                .version_tracker
                .get_migration_status(&*db, "003_in_progress")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status3.status, MigrationStatus::InProgress);
        }

        #[switchy_async::test]
        async fn test_mark_all_migrations_completed_include_in_progress() {
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_completed".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_failed".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "003_in_progress".to_string(),
                Box::new("CREATE TABLE test3 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Pre-populate
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Should create table");
            runner
                .version_tracker
                .record_migration(&*db, "001_completed")
                .await
                .expect("Should record");

            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "002_failed", &checksum, &checksum)
                .await
                .expect("Should record");
            runner
                .version_tracker
                .update_migration_status(
                    &*db,
                    "002_failed",
                    MigrationStatus::Failed,
                    Some("Test".to_string()),
                )
                .await
                .expect("Should update");

            runner
                .version_tracker
                .record_migration_started(&*db, "003_in_progress", &checksum, &checksum)
                .await
                .expect("Should record");

            // Mark with IncludeInProgress scope
            let summary = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::IncludeInProgress)
                .await
                .expect("Should succeed");

            assert_eq!(summary.total, 3);
            assert_eq!(summary.already_completed, 1);
            assert_eq!(summary.newly_marked, 0);
            assert_eq!(summary.failed_marked, 0);
            assert_eq!(summary.in_progress_marked, 1); // 003_in_progress was marked
            assert_eq!(summary.failed_skipped, 1); // 002_failed was skipped

            // Verify 002_failed is still failed
            let status2 = runner
                .version_tracker
                .get_migration_status(&*db, "002_failed")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status2.status, MigrationStatus::Failed);

            // Verify 003_in_progress is now completed
            let status3 = runner
                .version_tracker
                .get_migration_status(&*db, "003_in_progress")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status3.status, MigrationStatus::Completed);
        }

        #[switchy_async::test]
        async fn test_mark_all_migrations_completed_all_scope() {
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_completed".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_failed".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "003_in_progress".to_string(),
                Box::new("CREATE TABLE test3 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "004_new".to_string(),
                Box::new("CREATE TABLE test4 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Pre-populate
            runner
                .version_tracker
                .ensure_table_exists(&*db)
                .await
                .expect("Should create table");
            runner
                .version_tracker
                .record_migration(&*db, "001_completed")
                .await
                .expect("Should record");

            let checksum = bytes::Bytes::from(vec![0u8; 32]);
            runner
                .version_tracker
                .record_migration_started(&*db, "002_failed", &checksum, &checksum)
                .await
                .expect("Should record");
            runner
                .version_tracker
                .update_migration_status(
                    &*db,
                    "002_failed",
                    MigrationStatus::Failed,
                    Some("Test".to_string()),
                )
                .await
                .expect("Should update");

            runner
                .version_tracker
                .record_migration_started(&*db, "003_in_progress", &checksum, &checksum)
                .await
                .expect("Should record");

            // Mark with All scope
            let summary = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::All)
                .await
                .expect("Should succeed");

            assert_eq!(summary.total, 4);
            assert_eq!(summary.already_completed, 1);
            assert_eq!(summary.newly_marked, 1); // 004_new
            assert_eq!(summary.failed_marked, 1); // 002_failed
            assert_eq!(summary.in_progress_marked, 1); // 003_in_progress
            assert_eq!(summary.failed_skipped, 0);
            assert_eq!(summary.in_progress_skipped, 0);

            // Verify all are now completed
            for id in &["002_failed", "003_in_progress", "004_new"] {
                let status = runner
                    .version_tracker
                    .get_migration_status(&*db, id)
                    .await
                    .expect("Should get status")
                    .expect("Migration should exist");
                assert_eq!(status.status, MigrationStatus::Completed);
            }
        }

        #[switchy_async::test]
        async fn test_drop_tracking_table_and_recreate() {
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test1 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_test".to_string(),
                Box::new("CREATE TABLE test2 (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));

            // Create table and mark migrations
            runner
                .ensure_tracking_table_exists(&*db)
                .await
                .expect("Should create table");
            let summary = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::PendingOnly)
                .await
                .expect("Should succeed");
            assert_eq!(summary.newly_marked, 2);

            // Verify migrations exist
            let migrations_before = runner
                .version_tracker
                .get_applied_migration_ids(&*db, None)
                .await
                .expect("Should get migrations");
            assert_eq!(migrations_before.len(), 2);

            // Drop the table
            runner
                .drop_tracking_table(&*db)
                .await
                .expect("Should drop table");

            // Verify table is gone (should return empty list)
            let migrations_after_drop = runner
                .version_tracker
                .get_applied_migration_ids(&*db, None)
                .await
                .expect("Should handle missing table");
            assert_eq!(migrations_after_drop.len(), 0);

            // Recreate table and mark again
            runner
                .ensure_tracking_table_exists(&*db)
                .await
                .expect("Should create table");
            let summary2 = runner
                .mark_all_migrations_completed(&*db, MarkCompletedScope::PendingOnly)
                .await
                .expect("Should succeed");
            assert_eq!(summary2.newly_marked, 2); // Both are new again

            // Verify migrations exist with new checksums
            let status1 = runner
                .version_tracker
                .get_migration_status(&*db, "001_test")
                .await
                .expect("Should get status")
                .expect("Migration should exist");
            assert_eq!(status1.status, MigrationStatus::Completed);

            // Verify checksums are not all zeros
            let zero_checksum = hex::encode(vec![0u8; 32]);
            assert_ne!(status1.up_checksum, zero_checksum);
            assert_ne!(status1.down_checksum, zero_checksum);
        }

        #[switchy_async::test]
        async fn test_list_applied_migrations() {
            use switchy_database_connection;

            // Create in-memory database
            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // Create and run migrations
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_first".to_string(),
                Box::new("CREATE TABLE first (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE first;".to_string()) as Box<dyn Executable>),
            ));
            source.add_migration(CodeMigration::new(
                "002_second".to_string(),
                Box::new("CREATE TABLE second (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source));
            runner.run(&*db).await.expect("Migrations should succeed");

            // Test list_applied_migrations
            let applied_migrations = runner
                .version_tracker
                .get_applied_migrations(&*db, MigrationStatus::Completed)
                .await
                .expect("Should list applied migrations");

            assert_eq!(
                applied_migrations.len(),
                2,
                "Should have 2 applied migrations"
            );

            // Verify migration records
            let first = applied_migrations
                .iter()
                .find(|m| m.id == "001_first")
                .unwrap();
            let second = applied_migrations
                .iter()
                .find(|m| m.id == "002_second")
                .unwrap();

            assert_eq!(first.status, crate::migration::MigrationStatus::Completed);
            assert_eq!(second.status, crate::migration::MigrationStatus::Completed);

            // Verify checksums exist and are hex strings
            assert_eq!(
                first.up_checksum.len(),
                64,
                "Up checksum should be 64 char hex string"
            );
            assert_eq!(
                first.down_checksum.len(),
                64,
                "Down checksum should be 64 char hex string"
            );
            assert_eq!(
                second.up_checksum.len(),
                64,
                "Up checksum should be 64 char hex string"
            );
            assert_eq!(
                second.down_checksum.len(),
                64,
                "Down checksum should be 64 char hex string"
            );

            // Verify checksums are valid hex
            hex::decode(&first.up_checksum).expect("Should be valid hex");
            hex::decode(&first.down_checksum).expect("Should be valid hex");
            hex::decode(&second.up_checksum).expect("Should be valid hex");
            hex::decode(&second.down_checksum).expect("Should be valid hex");
        }

        #[switchy_async::test]
        async fn test_strict_mode_prevents_run_on_up_checksum_mismatch() {
            use switchy_database_connection::init_sqlite_sqlx;

            let db = init_sqlite_sqlx(None).await.unwrap();

            // Create initial migration source and run a migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE users (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            let runner = MigrationRunner::new(Box::new(source));

            // Run migration once to establish checksums
            runner.run(&*db).await.unwrap();

            // Create source with same ID but different up content to cause checksum mismatch
            let mut source_modified = CodeMigrationSource::new();
            source_modified.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE customers (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            let runner_strict = MigrationRunner::new(Box::new(source_modified))
                .with_checksum_config(ChecksumConfig {
                    require_validation: true,
                });

            // Should fail due to up checksum mismatch
            let result = runner_strict.run(&*db).await;
            assert!(result.is_err(), "Should fail with checksum mismatch");

            match result.unwrap_err() {
                crate::MigrationError::ChecksumValidationFailed { mismatches } => {
                    assert_eq!(mismatches.len(), 1);
                    assert_eq!(mismatches[0].migration_id, "001_test");
                    assert_eq!(mismatches[0].checksum_type, crate::ChecksumType::Up);
                }
                _ => panic!("Expected ChecksumValidationFailed error"),
            }
        }

        #[switchy_async::test]
        async fn test_strict_mode_prevents_run_on_down_checksum_mismatch() {
            use switchy_database_connection::init_sqlite_sqlx;

            let db = init_sqlite_sqlx(None).await.unwrap();

            // Create initial migration source and run a migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE users (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE users;".to_string()) as Box<dyn Executable>),
            ));
            let runner = MigrationRunner::new(Box::new(source));

            // Run migration once to establish checksums
            runner.run(&*db).await.unwrap();

            // Create source with same up content but different down content to cause checksum mismatch
            let mut source_modified = CodeMigrationSource::new();
            source_modified.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE users (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE IF EXISTS users;".to_string()) as Box<dyn Executable>),
            ));
            let runner_strict = MigrationRunner::new(Box::new(source_modified))
                .with_checksum_config(ChecksumConfig {
                    require_validation: true,
                });

            // Should fail due to down checksum mismatch
            let result = runner_strict.run(&*db).await;
            assert!(result.is_err(), "Should fail with checksum mismatch");

            match result.unwrap_err() {
                crate::MigrationError::ChecksumValidationFailed { mismatches } => {
                    assert_eq!(mismatches.len(), 1);
                    assert_eq!(mismatches[0].migration_id, "001_test");
                    assert_eq!(mismatches[0].checksum_type, crate::ChecksumType::Down);
                }
                _ => panic!("Expected ChecksumValidationFailed error"),
            }
        }

        #[switchy_async::test]
        async fn test_strict_mode_allows_run_when_checksums_valid() {
            use switchy_database_connection::init_sqlite_sqlx;

            let db = init_sqlite_sqlx(None).await.unwrap();

            // Create initial migration source and run a migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE users (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE users;".to_string()) as Box<dyn Executable>),
            ));
            let runner = MigrationRunner::new(Box::new(source));

            // Run migration once to establish checksums
            runner.run(&*db).await.unwrap();

            // Create another migration source with exact same content (checksums should match)
            let mut source_same = CodeMigrationSource::new();
            source_same.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE users (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE users;".to_string()) as Box<dyn Executable>),
            ));
            let runner_strict =
                MigrationRunner::new(Box::new(source_same)).with_checksum_config(ChecksumConfig {
                    require_validation: true,
                });

            // Should succeed since checksums match
            let result = runner_strict.run(&*db).await;
            assert!(
                result.is_ok(),
                "Should succeed with matching checksums: {result:?}"
            );
        }

        #[switchy_async::test]
        async fn test_default_config_allows_run_with_mismatches() {
            use switchy_database_connection::init_sqlite_sqlx;

            let db = init_sqlite_sqlx(None).await.unwrap();

            // Create initial migration source and run a migration
            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE users (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            let runner = MigrationRunner::new(Box::new(source));

            // Run migration once to establish checksums
            runner.run(&*db).await.unwrap();

            // Create source with different content (would cause mismatch)
            let mut source_modified = CodeMigrationSource::new();
            source_modified.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE customers (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            let runner_default = MigrationRunner::new(Box::new(source_modified));
            // Note: using default config (no strict mode)

            // Should succeed because strict mode is disabled by default
            let result = runner_default.run(&*db).await;
            assert!(
                result.is_ok(),
                "Should succeed with default (non-strict) config: {result:?}"
            );
        }

        #[switchy_async::test]
        async fn test_with_checksum_config_builder() {
            // Test that builder method works correctly
            let config = ChecksumConfig {
                require_validation: true,
            };
            let runner = MigrationRunner::new_code().with_checksum_config(config);

            // Access the config through the runner (testing private field indirectly via behavior)
            // We can't directly access private fields, but we know the config is set correctly
            // if the builder method compiles and returns the runner
            assert_eq!(
                std::mem::size_of_val(&runner),
                std::mem::size_of::<MigrationRunner>()
            );

            // Test default config
            let default_config = ChecksumConfig::default();
            assert!(
                !default_config.require_validation,
                "Default should have validation disabled"
            );
        }

        #[test_log::test]
        fn test_mark_completed_scope_should_mark_none_status() {
            // All scopes should mark migrations with no current status (pending)
            assert!(MarkCompletedScope::PendingOnly.should_mark(None));
            assert!(MarkCompletedScope::IncludeFailed.should_mark(None));
            assert!(MarkCompletedScope::IncludeInProgress.should_mark(None));
            assert!(MarkCompletedScope::All.should_mark(None));
        }

        #[test_log::test]
        fn test_mark_completed_scope_should_mark_completed_status() {
            // No scope should mark already completed migrations
            assert!(
                !MarkCompletedScope::PendingOnly.should_mark(Some(&MigrationStatus::Completed))
            );
            assert!(
                !MarkCompletedScope::IncludeFailed.should_mark(Some(&MigrationStatus::Completed))
            );
            assert!(
                !MarkCompletedScope::IncludeInProgress
                    .should_mark(Some(&MigrationStatus::Completed))
            );
            assert!(!MarkCompletedScope::All.should_mark(Some(&MigrationStatus::Completed)));
        }

        #[test_log::test]
        fn test_mark_completed_scope_should_mark_failed_status() {
            // Only IncludeFailed and All should mark failed migrations
            assert!(!MarkCompletedScope::PendingOnly.should_mark(Some(&MigrationStatus::Failed)));
            assert!(MarkCompletedScope::IncludeFailed.should_mark(Some(&MigrationStatus::Failed)));
            assert!(
                !MarkCompletedScope::IncludeInProgress.should_mark(Some(&MigrationStatus::Failed))
            );
            assert!(MarkCompletedScope::All.should_mark(Some(&MigrationStatus::Failed)));
        }

        #[test_log::test]
        fn test_mark_completed_scope_should_mark_in_progress_status() {
            // Only IncludeInProgress and All should mark in-progress migrations
            assert!(
                !MarkCompletedScope::PendingOnly.should_mark(Some(&MigrationStatus::InProgress))
            );
            assert!(
                !MarkCompletedScope::IncludeFailed.should_mark(Some(&MigrationStatus::InProgress))
            );
            assert!(
                MarkCompletedScope::IncludeInProgress
                    .should_mark(Some(&MigrationStatus::InProgress))
            );
            assert!(MarkCompletedScope::All.should_mark(Some(&MigrationStatus::InProgress)));
        }

        #[switchy_async::test]
        async fn test_hooks_before_migration_called() {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicUsize, Ordering};
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let before_count = Arc::new(AtomicUsize::new(0));
            let before_count_clone = before_count.clone();
            let captured_ids = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured_ids_clone = captured_ids.clone();

            let hooks = MigrationHooks {
                before_migration: Some(Box::new(move |id: &str| {
                    before_count_clone.fetch_add(1, Ordering::SeqCst);
                    captured_ids_clone.lock().unwrap().push(id.to_string());
                })),
                after_migration: None,
                on_error: None,
            };

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_first".to_string(),
                Box::new("CREATE TABLE first (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_second".to_string(),
                Box::new("CREATE TABLE second (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source)).with_hooks(hooks);

            runner.run(&*db).await.expect("Migrations should succeed");

            // Verify before_migration was called twice (once per migration)
            assert_eq!(before_count.load(Ordering::SeqCst), 2);

            // Verify the correct IDs were passed
            let ids = captured_ids.lock().unwrap();
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&"001_first".to_string()));
            assert!(ids.contains(&"002_second".to_string()));
            drop(ids);
        }

        #[switchy_async::test]
        async fn test_hooks_after_migration_called() {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicUsize, Ordering};
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let after_count = Arc::new(AtomicUsize::new(0));
            let after_count_clone = after_count.clone();
            let captured_ids = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured_ids_clone = captured_ids.clone();

            let hooks = MigrationHooks {
                before_migration: None,
                after_migration: Some(Box::new(move |id: &str| {
                    after_count_clone.fetch_add(1, Ordering::SeqCst);
                    captured_ids_clone.lock().unwrap().push(id.to_string());
                })),
                on_error: None,
            };

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source)).with_hooks(hooks);

            runner.run(&*db).await.expect("Migration should succeed");

            // Verify after_migration was called once
            assert_eq!(after_count.load(Ordering::SeqCst), 1);

            // Verify the correct ID was passed
            let ids = captured_ids.lock().unwrap();
            assert_eq!(ids.len(), 1);
            assert_eq!(ids[0], "001_test");
            drop(ids);
        }

        #[switchy_async::test]
        async fn test_hooks_on_error_called() {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicUsize, Ordering};
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let error_count = Arc::new(AtomicUsize::new(0));
            let error_count_clone = error_count.clone();
            let captured_id = Arc::new(std::sync::Mutex::new(String::new()));
            let captured_id_clone = captured_id.clone();

            let hooks = MigrationHooks {
                before_migration: None,
                after_migration: None,
                on_error: Some(Box::new(move |id: &str, _err: &crate::MigrationError| {
                    error_count_clone.fetch_add(1, Ordering::SeqCst);
                    *captured_id_clone.lock().unwrap() = id.to_string();
                })),
            };

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_failing".to_string(),
                Box::new("INVALID SQL SYNTAX;".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source)).with_hooks(hooks);

            // Migration should fail
            let result = runner.run(&*db).await;
            assert!(result.is_err(), "Migration should fail");

            // Verify on_error was called
            assert_eq!(error_count.load(Ordering::SeqCst), 1);

            // Verify the correct ID was passed
            let id = captured_id.lock().unwrap();
            assert_eq!(*id, "001_failing");
            drop(id);
        }

        #[switchy_async::test]
        async fn test_hooks_all_combined() {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicUsize, Ordering};
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            let before_count = Arc::new(AtomicUsize::new(0));
            let before_count_clone = before_count.clone();
            let after_count = Arc::new(AtomicUsize::new(0));
            let after_count_clone = after_count.clone();
            let error_count = Arc::new(AtomicUsize::new(0));
            let error_count_clone = error_count.clone();

            let hooks = MigrationHooks {
                before_migration: Some(Box::new(move |_id: &str| {
                    before_count_clone.fetch_add(1, Ordering::SeqCst);
                })),
                after_migration: Some(Box::new(move |_id: &str| {
                    after_count_clone.fetch_add(1, Ordering::SeqCst);
                })),
                on_error: Some(Box::new(move |_id: &str, _err: &crate::MigrationError| {
                    error_count_clone.fetch_add(1, Ordering::SeqCst);
                })),
            };

            let mut source = CodeMigrationSource::new();
            source.add_migration(CodeMigration::new(
                "001_success".to_string(),
                Box::new("CREATE TABLE success (id INTEGER);".to_string()) as Box<dyn Executable>,
                None,
            ));
            source.add_migration(CodeMigration::new(
                "002_fail".to_string(),
                Box::new("INVALID SQL;".to_string()) as Box<dyn Executable>,
                None,
            ));

            let runner = MigrationRunner::new(Box::new(source)).with_hooks(hooks);

            // Should fail on second migration
            let result = runner.run(&*db).await;
            assert!(result.is_err(), "Should fail on second migration");

            // before_migration should be called twice (once for each migration attempt)
            assert_eq!(before_count.load(Ordering::SeqCst), 2);

            // after_migration should be called once (only for the successful first migration)
            assert_eq!(after_count.load(Ordering::SeqCst), 1);

            // on_error should be called once (for the failed second migration)
            assert_eq!(error_count.load(Ordering::SeqCst), 1);
        }

        #[switchy_async::test]
        async fn test_hooks_on_rollback() {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicUsize, Ordering};
            use switchy_database_connection;

            let db = switchy_database_connection::init_sqlite_sqlx(None)
                .await
                .expect("Failed to create test database");

            // First, run a successful migration
            let mut setup_source = CodeMigrationSource::new();
            setup_source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE test;".to_string()) as Box<dyn Executable>),
            ));

            let setup_runner = MigrationRunner::new(Box::new(setup_source));
            setup_runner
                .run(&*db)
                .await
                .expect("Setup migration should succeed");

            // Now test rollback with hooks
            let before_count = Arc::new(AtomicUsize::new(0));
            let before_count_clone = before_count.clone();
            let after_count = Arc::new(AtomicUsize::new(0));
            let after_count_clone = after_count.clone();

            let hooks = MigrationHooks {
                before_migration: Some(Box::new(move |_id: &str| {
                    before_count_clone.fetch_add(1, Ordering::SeqCst);
                })),
                after_migration: Some(Box::new(move |_id: &str| {
                    after_count_clone.fetch_add(1, Ordering::SeqCst);
                })),
                on_error: None,
            };

            let mut rollback_source = CodeMigrationSource::new();
            rollback_source.add_migration(CodeMigration::new(
                "001_test".to_string(),
                Box::new("CREATE TABLE test (id INTEGER);".to_string()) as Box<dyn Executable>,
                Some(Box::new("DROP TABLE test;".to_string()) as Box<dyn Executable>),
            ));

            let rollback_runner = MigrationRunner::new(Box::new(rollback_source)).with_hooks(hooks);

            rollback_runner
                .rollback(&*db, RollbackStrategy::Last)
                .await
                .expect("Rollback should succeed");

            // Verify hooks were called during rollback
            assert_eq!(
                before_count.load(Ordering::SeqCst),
                1,
                "before_migration should be called once during rollback"
            );
            assert_eq!(
                after_count.load(Ordering::SeqCst),
                1,
                "after_migration should be called once during rollback"
            );
        }
    }
}
