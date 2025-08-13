use crate::{Result, migration::Migration, migration::MigrationSource};
use async_trait::async_trait;
use include_dir::Dir;

/// Migration implementation for embedded migrations using `include_dir`
pub struct EmbeddedMigration {
    id: String,
    up_content: String,
    down_content: Option<String>,
}

impl EmbeddedMigration {
    #[must_use]
    pub const fn new(id: String, up_content: String, down_content: Option<String>) -> Self {
        Self {
            id,
            up_content,
            down_content,
        }
    }
}

#[async_trait]
impl Migration for EmbeddedMigration {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn switchy_database::Database) -> Result<()> {
        db.exec_raw(&self.up_content).await?;
        Ok(())
    }

    async fn down(&self, db: &dyn switchy_database::Database) -> Result<()> {
        if let Some(down_sql) = &self.down_content {
            db.exec_raw(down_sql).await?;
        }
        Ok(())
    }
}

/// Migration source for embedded migrations using `include_dir`
pub struct EmbeddedMigrationSource {
    #[allow(dead_code)] // Will be used when actual discovery is implemented
    migrations_dir: &'static Dir<'static>,
}

impl EmbeddedMigrationSource {
    #[must_use]
    pub const fn new(migrations_dir: &'static Dir<'static>) -> Self {
        Self { migrations_dir }
    }
}

#[async_trait]
impl MigrationSource for EmbeddedMigrationSource {
    async fn migrations(&self) -> Result<Vec<Box<dyn Migration>>> {
        let migrations: Vec<Box<dyn Migration>> = Vec::new();

        // TODO: Implement actual migration discovery from include_dir
        // This is a placeholder implementation

        Ok(migrations)
    }
}
