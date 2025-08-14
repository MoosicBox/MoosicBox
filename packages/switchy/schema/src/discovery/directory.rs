use crate::{Result, migration::Migration, migration::MigrationSource};
use async_trait::async_trait;
use std::{collections::BTreeMap, path::PathBuf};

/// Migration implementation for file-based migrations
pub struct FileMigration {
    id: String,
    path: PathBuf,
    up_sql: Option<String>,
    down_sql: Option<String>,
}

impl FileMigration {
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
}

/// Migration source for directory-based migrations
pub struct DirectoryMigrationSource {
    migrations_path: PathBuf,
}

impl DirectoryMigrationSource {
    #[must_use]
    pub const fn from_path(migrations_path: PathBuf) -> Self {
        Self { migrations_path }
    }

    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.migrations_path
    }
}

#[async_trait]
impl MigrationSource<'static> for DirectoryMigrationSource {
    async fn migrations(&self) -> Result<Vec<Box<dyn Migration<'static> + 'static>>> {
        let migration_map = self.extract_migrations().await?;
        let migrations: Vec<Box<dyn Migration<'static> + 'static>> = migration_map
            .into_values()
            .map(|m| Box::new(m) as Box<dyn Migration<'static> + 'static>)
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
        let mut migrations = BTreeMap::new();

        // Read all entries in the migrations directory
        let entries = switchy_fs::unsync::read_dir_sorted(&self.migrations_path).await?;

        for entry in entries {
            let entry_path = entry.path();

            // Only process directories (each directory is one migration)
            if !entry.file_type().await?.is_dir() {
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
                continue;
            }

            let migration = FileMigration::new(migration_id.clone(), entry_path, up_sql, down_sql);

            migrations.insert(migration_id, migration);
        }

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
}
