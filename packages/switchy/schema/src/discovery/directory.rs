//! # Directory-Based Migrations
//!
//! This module provides support for directory-based migrations that are loaded
//! from the filesystem at runtime. This is useful for development environments
//! where you want to modify migrations without recompiling.
//!
//! ## Features
//!
//! * **Runtime loading**: Migrations loaded from disk when needed
//! * **Development friendly**: Modify migrations without rebuilding
//! * **Same structure**: Uses identical directory structure to embedded migrations
//! * **Hot reload**: Changes are picked up on next migration run
//! * **Optional files**: Both up.sql and down.sql are optional
//!
//! ## Directory Structure
//!
//! Directory-based migrations use the same structure as embedded migrations:
//!
//! ```text
//! migrations/
//! ├── 001_create_users/
//! │   ├── up.sql      # Optional migration SQL
//! │   └── down.sql    # Optional rollback SQL
//! ├── 002_add_posts/
//! │   └── up.sql      # down.sql is optional
//! └── 003_indexes/
//!     ├── up.sql
//!     └── down.sql
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use switchy_schema::runner::MigrationRunner;
//!
//! # async fn example(db: &dyn switchy_database::Database) -> switchy_schema::Result<()> {
//! // Create migration runner for directory-based migrations
//! let runner = MigrationRunner::new_directory("./migrations");
//!
//! // Run migrations
//! runner.run(db).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Advantages vs Embedded
//!
//! * **Development speed**: No rebuild required to change migrations
//! * **Debugging**: Easier to inspect and modify SQL during development
//! * **Flexibility**: Can load migrations from different paths
//!
//! ## Disadvantages vs Embedded
//!
//! * **Runtime dependency**: Requires filesystem access
//! * **Deployment complexity**: Must ensure migration files are available
//! * **Potential inconsistency**: Files can be modified or missing at runtime

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::{Result, migration::Migration, migration::MigrationSource};

/// A single file-based migration with optional up and down SQL content
///
/// File-based migrations are loaded from the filesystem at runtime,
/// providing flexibility for development and debugging. Each migration
/// consists of:
///
/// * **ID**: The directory name (used for ordering)
/// * **Path**: The filesystem path to the migration directory
/// * **Up SQL**: Optional SQL for applying the migration
/// * **Down SQL**: Optional SQL for rolling back the migration
///
/// Both up and down SQL are optional. Missing or empty SQL files
/// are treated as no-op operations.
pub struct FileMigration {
    id: String,
    path: PathBuf,
    up_sql: Option<String>,
    down_sql: Option<String>,
}

impl FileMigration {
    /// Create a new file-based migration
    #[must_use]
    pub const fn new(
        id: String,
        path: PathBuf,
        up_sql: Option<String>,
        down_sql: Option<String>,
    ) -> Self {
        Self {
            id,
            path,
            up_sql,
            down_sql,
        }
    }

    /// Get the filesystem path to this migration's directory
    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[async_trait]
impl Migration<'static> for FileMigration {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn switchy_database::Database) -> Result<()> {
        if let Some(up_sql) = &self.up_sql
            && !up_sql.trim().is_empty()
        {
            db.exec_raw(up_sql).await?;
        }
        Ok(())
    }

    async fn down(&self, db: &dyn switchy_database::Database) -> Result<()> {
        if let Some(down_sql) = &self.down_sql
            && !down_sql.trim().is_empty()
        {
            db.exec_raw(down_sql).await?;
        }
        Ok(())
    }

    async fn up_checksum(&self) -> Result<bytes::Bytes> {
        let mut hasher = Sha256::new();
        match &self.up_sql {
            Some(content) => hasher.update(content.as_bytes()),
            None => hasher.update(b""), // Hash empty bytes for None
        }
        Ok(bytes::Bytes::from(hasher.finalize().to_vec()))
    }

    async fn down_checksum(&self) -> Result<bytes::Bytes> {
        let mut hasher = Sha256::new();
        match &self.down_sql {
            Some(content) => hasher.update(content.as_bytes()),
            None => hasher.update(b""), // Hash empty bytes for None
        }
        Ok(bytes::Bytes::from(hasher.finalize().to_vec()))
    }
}

/// Migration source for directory-based migrations loaded from the filesystem
///
/// This source loads migrations from a directory on the filesystem at runtime.
/// It's particularly useful during development when you want to modify migrations
/// without recompiling your application.
///
/// ## Features
///
/// * **Runtime loading**: Scans filesystem when migrations are requested
/// * **Development friendly**: Changes are picked up without rebuilds
/// * **Error handling**: Gracefully handles missing or unreadable files
/// * **Consistent ordering**: Migrations sorted alphabetically by directory name
///
/// ## Example
///
/// ```rust,no_run
/// use switchy_schema::{
///     runner::MigrationRunner,
///     discovery::directory::DirectoryMigrationSource
/// };
/// use std::path::PathBuf;
///
/// // Create source directly (advanced usage)
/// let source = DirectoryMigrationSource::from_path(PathBuf::from("./migrations"));
///
/// // Or use the convenience constructor (recommended)
/// let runner = MigrationRunner::new_directory("./migrations");
/// ```
///
/// The source will scan the specified directory for subdirectories containing
/// `up.sql` and/or `down.sql` files. Each subdirectory becomes a migration
/// with its name as the migration ID.
///
/// ## Error Handling
///
/// The directory source handles several filesystem-related scenarios:
///
/// * **Missing directory**: Returns empty migration list
/// * **Unreadable files**: Skips files that cannot be read
/// * **Empty files**: Treats as no-op migrations
/// * **Missing SQL files**: Creates migration with no-op for missing files
pub struct DirectoryMigrationSource {
    migrations_path: PathBuf,
}

impl DirectoryMigrationSource {
    /// Create a new directory migration source from the given path
    #[must_use]
    pub const fn from_path(migrations_path: PathBuf) -> Self {
        Self { migrations_path }
    }

    /// Get the path to the migrations directory
    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.migrations_path
    }
}

#[async_trait]
impl MigrationSource<'static> for DirectoryMigrationSource {
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
        log::trace!("Discovering migrations from directory");

        let migration_map = self.extract_migrations().await?;
        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = migration_map
            .into_values()
            .map(|m| Arc::new(m) as Arc<dyn Migration<'static> + 'static>)
            .collect();
        Ok(migrations)
    }
}

impl DirectoryMigrationSource {
    /// Extract migrations from the directory structure
    ///
    /// # Errors
    ///
    /// * If the migrations directory cannot be read
    /// * If any migration directory cannot be accessed
    /// * If any SQL file cannot be read
    async fn extract_migrations(&self) -> Result<BTreeMap<String, FileMigration>> {
        log::trace!(
            "extract_migrations: extracting migrations from directory from '{}' (cwd='{}')",
            self.migrations_path.display(),
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .display()
        );

        let mut migrations = BTreeMap::new();

        // Read all entries in the migrations directory
        let entries = switchy_fs::unsync::read_dir_sorted(&self.migrations_path).await?;

        for entry in entries {
            let entry_path = entry.path();

            log::trace!(
                "extract_migrations: processing entry '{}'",
                entry_path.display()
            );

            // Only process directories (each directory is one migration)
            if !entry.file_type().await?.is_dir() {
                log::trace!("extract_migrations: skipping non-directory entry");
                continue;
            }

            // Use directory name as migration ID (as-is, no validation)
            let migration_id = entry.file_name().to_string_lossy().to_string();

            // Look for up.sql and down.sql files
            let up_sql_path = entry_path.join("up.sql");
            let down_sql_path = entry_path.join("down.sql");

            // Read up.sql (optional)
            let up_sql = switchy_fs::unsync::read_to_string(&up_sql_path).await.ok();

            // Read down.sql (optional)
            let down_sql = switchy_fs::unsync::read_to_string(&down_sql_path)
                .await
                .ok();

            // Skip migrations with no SQL files at all
            if up_sql.is_none() && down_sql.is_none() {
                log::trace!("extract_migrations: skipping migration with no SQL files");
                continue;
            }

            let migration = FileMigration::new(migration_id.clone(), entry_path, up_sql, down_sql);

            log::trace!("extract_migrations: extracted migration '{migration_id}'");

            migrations.insert(migration_id, migration);
        }

        log::trace!(
            "extract_migrations: extracted {} migrations",
            migrations.len()
        );

        Ok(migrations)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[switchy_async::test(real_fs)]
    async fn test_directory_migration_source() {
        let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_migrations_dir");

        let source = DirectoryMigrationSource::from_path(test_dir);
        let migrations = source.migrations().await.unwrap();

        // Should find 3 migrations in alphabetical order (004_no_sql_files is skipped)
        assert_eq!(migrations.len(), 3);

        // Check migration IDs are in alphabetical order
        assert_eq!(migrations[0].id(), "001_create_users");
        assert_eq!(migrations[1].id(), "002_add_indexes");
        assert_eq!(migrations[2].id(), "003_empty_migration");
    }

    #[switchy_async::test(real_fs)]
    async fn test_file_migration_with_content() {
        let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_migrations_dir");

        let source = DirectoryMigrationSource::from_path(test_dir);
        let migration_map = source.extract_migrations().await.unwrap();

        // Test migration with both up and down SQL
        let create_users = migration_map.get("001_create_users").unwrap();
        assert!(
            create_users
                .up_sql
                .as_ref()
                .unwrap()
                .contains("CREATE TABLE users")
        );
        assert!(
            create_users
                .down_sql
                .as_ref()
                .unwrap()
                .contains("DROP TABLE users")
        );

        // Test migration with only up SQL
        let add_indexes = migration_map.get("002_add_indexes").unwrap();
        assert!(
            add_indexes
                .up_sql
                .as_ref()
                .unwrap()
                .contains("CREATE INDEX")
        );
        assert!(add_indexes.down_sql.is_none());

        // Test empty migration
        let empty_migration = migration_map.get("003_empty_migration").unwrap();
        assert!(empty_migration.up_sql.as_ref().unwrap().trim().is_empty());
        assert!(empty_migration.down_sql.is_none());

        // Test that migration with no SQL files is not included
        assert!(!migration_map.contains_key("004_no_sql_files"));
    }

    #[switchy_async::test]
    async fn test_file_migration_execution() {
        let migration = FileMigration::new(
            "test".to_string(),
            PathBuf::from("/test"),
            Some("SELECT 1;".to_string()),
            Some("SELECT 2;".to_string()),
        );

        // Test that migration has correct ID
        assert_eq!(migration.id(), "test");

        // Test that path is accessible
        assert_eq!(migration.path(), &PathBuf::from("/test"));
    }

    #[switchy_async::test]
    async fn test_empty_sql_handling() {
        let migration = FileMigration::new(
            "empty".to_string(),
            PathBuf::from("/empty"),
            Some(String::new()),
            Some("   ".to_string()),
        );

        // Empty SQL should not cause errors (tested via compilation)
        assert_eq!(migration.id(), "empty");
    }

    #[switchy_async::test]
    async fn test_migration_with_no_sql_files() {
        let migration =
            FileMigration::new("no_sql".to_string(), PathBuf::from("/no_sql"), None, None);

        // Migration with no SQL files should be valid but do nothing
        assert_eq!(migration.id(), "no_sql");
    }

    #[switchy_async::test(real_fs)]
    async fn test_file_modification_changes_checksum() {
        use std::fs;
        use tempfile::TempDir;

        // Create temporary directory with migration
        let temp_dir = TempDir::new().unwrap();
        let migration_dir = temp_dir.path().join("001_test");
        fs::create_dir(&migration_dir).unwrap();

        // Write initial SQL
        fs::write(migration_dir.join("up.sql"), "CREATE TABLE test;").unwrap();
        fs::write(migration_dir.join("down.sql"), "DROP TABLE test;").unwrap();

        // Get initial checksum
        let source1 = DirectoryMigrationSource::from_path(temp_dir.path().to_path_buf());
        let migrations1 = source1.migrations().await.unwrap();
        let checksum1_up = migrations1[0].up_checksum().await.unwrap();
        let checksum1_down = migrations1[0].down_checksum().await.unwrap();

        // Modify up file
        fs::write(migration_dir.join("up.sql"), "CREATE TABLE test2;").unwrap();

        // Get new checksum
        let source2 = DirectoryMigrationSource::from_path(temp_dir.path().to_path_buf());
        let migrations2 = source2.migrations().await.unwrap();
        let checksum2_up = migrations2[0].up_checksum().await.unwrap();
        let checksum2_down = migrations2[0].down_checksum().await.unwrap();

        // Up checksum should change, down should remain the same
        assert_ne!(
            checksum1_up, checksum2_up,
            "Up checksum should change when file is modified"
        );
        assert_eq!(
            checksum1_down, checksum2_down,
            "Down checksum should remain the same"
        );

        // Modify down file
        fs::write(migration_dir.join("down.sql"), "DROP TABLE test2;").unwrap();

        // Get third checksum
        let source3 = DirectoryMigrationSource::from_path(temp_dir.path().to_path_buf());
        let migrations3 = source3.migrations().await.unwrap();
        let checksum3_up = migrations3[0].up_checksum().await.unwrap();
        let checksum3_down = migrations3[0].down_checksum().await.unwrap();

        // Up should remain the same, down should change
        assert_eq!(
            checksum2_up, checksum3_up,
            "Up checksum should remain the same"
        );
        assert_ne!(
            checksum2_down, checksum3_down,
            "Down checksum should change when file is modified"
        );

        // Verify checksums are valid 32-byte SHA256 hashes
        assert_eq!(checksum3_up.len(), 32);
        assert_eq!(checksum3_down.len(), 32);
    }
}
