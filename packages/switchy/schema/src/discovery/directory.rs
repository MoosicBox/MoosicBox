use crate::{Result, migration::Migration, migration::MigrationSource};
use async_trait::async_trait;
use std::path::PathBuf;

/// Migration implementation for file-based migrations
pub struct FileMigration {
    id: String,
    path: PathBuf,
    up_sql: String,
    down_sql: Option<String>,
}

impl FileMigration {
    #[must_use]
    pub const fn new(id: String, path: PathBuf, up_sql: String, down_sql: Option<String>) -> Self {
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
impl Migration for FileMigration {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn switchy_database::Database) -> Result<()> {
        db.exec_raw(&self.up_sql).await?;
        Ok(())
    }

    async fn down(&self, db: &dyn switchy_database::Database) -> Result<()> {
        if let Some(down_sql) = &self.down_sql {
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
impl MigrationSource for DirectoryMigrationSource {
    async fn migrations(&self) -> Result<Vec<Box<dyn Migration>>> {
        let migrations: Vec<Box<dyn Migration>> = Vec::new();

        // TODO: Implement actual directory scanning for migration files
        // Format: YYYY-MM-DD-HHMMSS_name/up.sql
        // down.sql is optional, metadata.toml is allowed
        // Empty migration files are treated as successful no-ops
        // Handle database-specific subdirectories

        Ok(migrations)
    }
}
