use crate::{Result, migration::Migration, migration::MigrationSource};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

type MigrationFn = Box<
    dyn Fn(&dyn switchy_database::Database) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Migration implementation for code-based migrations
pub struct CodeMigration {
    id: String,
    up_fn: MigrationFn,
    down_fn: Option<MigrationFn>,
}

impl CodeMigration {
    #[must_use]
    pub fn new(id: String, up_fn: MigrationFn, down_fn: Option<MigrationFn>) -> Self {
        Self { id, up_fn, down_fn }
    }
}

#[async_trait]
impl Migration for CodeMigration {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn switchy_database::Database) -> Result<()> {
        (self.up_fn)(db).await
    }

    async fn down(&self, db: &dyn switchy_database::Database) -> Result<()> {
        if let Some(down_fn) = &self.down_fn {
            down_fn(db).await
        } else {
            Ok(())
        }
    }
}

/// Migration source for code-based migrations with registry
pub struct CodeMigrationSource {
    migrations: BTreeMap<String, Box<dyn Migration>>,
}

impl CodeMigrationSource {
    #[must_use]
    pub fn new() -> Self {
        Self {
            migrations: BTreeMap::new(),
        }
    }

    pub fn add_migration(&mut self, migration: Box<dyn Migration>) {
        let id = migration.id().to_string();
        self.migrations.insert(id, migration);
    }
}

impl Default for CodeMigrationSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MigrationSource for CodeMigrationSource {
    async fn migrations(&self) -> Result<Vec<Box<dyn Migration>>> {
        // TODO: Implement proper migration cloning or reference handling
        // For now, return empty vec as placeholder
        Ok(Vec::new())
    }
}
