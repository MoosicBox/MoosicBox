use crate::Result;
use async_trait::async_trait;
use switchy_database::Database;

#[async_trait]
pub trait Migration: Send + Sync {
    fn id(&self) -> &str;

    async fn up(&self, db: &dyn Database) -> Result<()>;

    async fn down(&self, _db: &dyn Database) -> Result<()> {
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        None
    }

    fn depends_on(&self) -> Vec<&str> {
        Vec::new()
    }

    fn supported_databases(&self) -> Vec<&str> {
        vec!["sqlite", "postgres", "mysql"]
    }
}

#[async_trait]
pub trait MigrationSource: Send + Sync {
    async fn migrations(&self) -> Result<Vec<Box<dyn Migration>>>;
}
