#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use diesel::Connection as _;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use thiserror::Error;

#[cfg(feature = "postgres")]
pub const POSTGRES_LIBRARY_MIGRATIONS: EmbeddedMigrations =
    embed_migrations!("../../migrations/server/postgres");

#[cfg(feature = "sqlite")]
pub const SQLITE_LIBRARY_MIGRATIONS: EmbeddedMigrations =
    embed_migrations!("../../migrations/server/sqlite");

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("Diesel migration error: {0:?}")]
    DieselMigration(Box<dyn std::error::Error + Send + Sync>),
}

pub fn migrate_library(database_url: &str) -> Result<(), MigrateError> {
    #[cfg(feature = "postgres")]
    {
        let mut conn = diesel::PgConnection::establish(database_url).unwrap();
        conn.run_pending_migrations(POSTGRES_LIBRARY_MIGRATIONS)
            .map_err(MigrateError::DieselMigration)?;
    }

    #[cfg(feature = "sqlite")]
    {
        let mut conn = diesel::SqliteConnection::establish(database_url).unwrap();
        conn.run_pending_migrations(SQLITE_LIBRARY_MIGRATIONS)
            .map_err(MigrateError::DieselMigration)?;
    }

    Ok(())
}
