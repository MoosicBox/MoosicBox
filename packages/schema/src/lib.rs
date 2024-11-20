#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use thiserror::Error;

#[cfg(feature = "postgres")]
pub const POSTGRES_CONFIG_MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!("migrations/server/config/postgres");

#[cfg(feature = "sqlite")]
pub const SQLITE_CONFIG_MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!("migrations/server/config/sqlite");

#[cfg(feature = "postgres")]
pub const POSTGRES_LIBRARY_MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!("migrations/server/library/postgres");

#[cfg(feature = "sqlite")]
pub const SQLITE_LIBRARY_MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!("migrations/server/library/sqlite");

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error("Diesel migration error: {0:?}")]
    DieselMigration(Box<dyn std::error::Error + Send + Sync>),
}

/// # Panics
///
/// * If the db connection fails to establish
///
/// # Errors
///
/// * If the migrations fail to run
#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub fn migrate_config(database_url: &str) -> Result<(), MigrateError> {
    use diesel::Connection as _;
    use diesel_migrations::MigrationHarness as _;

    #[cfg(feature = "postgres")]
    {
        log::debug!("migrate_config: running postgres migrations");
        let mut conn = diesel::PgConnection::establish(database_url).unwrap();
        conn.run_pending_migrations(POSTGRES_CONFIG_MIGRATIONS)
            .map_err(MigrateError::DieselMigration)?;
        log::debug!("migrate_config: finished running postgres migrations");
    }

    #[cfg(feature = "sqlite")]
    {
        log::debug!("migrate_config: running sqlite migrations");
        let mut conn = diesel::SqliteConnection::establish(database_url).unwrap();
        conn.run_pending_migrations(SQLITE_CONFIG_MIGRATIONS)
            .map_err(MigrateError::DieselMigration)?;
        log::debug!("migrate_config: finished running sqlite migrations");
    }

    Ok(())
}

/// # Panics
///
/// * If the db connection fails to establish
///
/// # Errors
///
/// * If the migrations fail to run
#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub fn migrate_library(database_url: &str) -> Result<(), MigrateError> {
    use diesel::Connection as _;
    use diesel_migrations::MigrationHarness as _;

    #[cfg(feature = "postgres")]
    {
        log::debug!("migrate_library: running postgres migrations");
        let mut conn = diesel::PgConnection::establish(database_url).unwrap();
        conn.run_pending_migrations(POSTGRES_LIBRARY_MIGRATIONS)
            .map_err(MigrateError::DieselMigration)?;
        log::debug!("migrate_library: finished running postgres migrations");
    }

    #[cfg(feature = "sqlite")]
    {
        log::debug!("migrate_library: running sqlite migrations");
        let mut conn = diesel::SqliteConnection::establish(database_url).unwrap();
        conn.run_pending_migrations(SQLITE_LIBRARY_MIGRATIONS)
            .map_err(MigrateError::DieselMigration)?;
        log::debug!("migrate_library: finished running sqlite migrations");
    }

    Ok(())
}
