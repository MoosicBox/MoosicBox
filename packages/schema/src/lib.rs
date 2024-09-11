#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use thiserror::Error;

#[cfg(feature = "postgres")]
pub const POSTGRES_LIBRARY_MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!("../../migrations/server/postgres");

#[cfg(feature = "sqlite")]
pub const SQLITE_LIBRARY_MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!("../../migrations/server/sqlite");

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("Diesel migration error: {0:?}")]
    DieselMigration(Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub fn migrate_library(database_url: &str) -> Result<(), MigrateError> {
    use diesel::Connection as _;
    use diesel_migrations::MigrationHarness as _;

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
