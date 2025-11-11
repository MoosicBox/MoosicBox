//! Database schema migration management for `MoosicBox`.
//!
//! This crate provides migration functionality for `MoosicBox`'s `PostgreSQL` and `SQLite` databases,
//! supporting both configuration and library schemas. Migrations are embedded at compile-time
//! and executed using the `switchy_schema` migration framework.
//!
//! # Features
//!
//! * `postgres` - Enable `PostgreSQL` migration support
//! * `sqlite` - Enable `SQLite` migration support (enabled by default)
//!
//! # Environment Variables
//!
//! * `MOOSICBOX_SKIP_MIGRATION_EXECUTION` - Set to "1" to mark migrations as completed without executing
//! * `MOOSICBOX_DROP_MIGRATIONS_TABLE` - Set to "1" to drop the migrations tracking table before running
//!
//! # Examples
//!
//! ```rust,no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use moosicbox_schema::{migrate_library, migrate_config};
//!
//! // Connect to database
//! let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
//!
//! // Run configuration migrations
//! migrate_config(&*db).await?;
//!
//! // Run library migrations
//! migrate_library(&*db).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_database::DatabaseError;
use switchy_schema::{
    MigrationError as SwitchyMigrationError, discovery::code::CodeMigrationSource,
};
use thiserror::Error;

/// Error type for database migration operations
#[derive(Debug, Error)]
pub enum MigrateError {
    /// Database connection or query error
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Schema migration execution error
    #[error(transparent)]
    Schema(#[from] SwitchyMigrationError),
}

/// Check if migration execution should be skipped based on environment variable
#[cfg(any(feature = "postgres", feature = "sqlite"))]
fn should_skip_migrations() -> bool {
    switchy_env::var("MOOSICBOX_SKIP_MIGRATION_EXECUTION")
        .as_deref()
        .unwrap_or("0")
        == "1"
}

/// Check if migrations table should be dropped based on environment variable
#[cfg(any(feature = "postgres", feature = "sqlite"))]
fn should_drop_migrations_table() -> bool {
    switchy_env::var("MOOSICBOX_DROP_MIGRATIONS_TABLE")
        .as_deref()
        .unwrap_or("0")
        == "1"
}

/// Run `PostgreSQL` configuration migrations only
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(feature = "postgres")]
pub async fn migrate_config_postgres(
    db: &dyn switchy_database::Database,
) -> Result<(), MigrateError> {
    log::debug!("migrate_config_postgres: running postgres migrations");
    let source = postgres_config_migrations();
    let runner = switchy_schema::runner::MigrationRunner::new(Box::new(source))
        .with_table_name("__moosicbox_schema_migrations");

    if should_drop_migrations_table() {
        log::info!("migrate_config_postgres: dropping postgres migration table");
        runner.drop_tracking_table(db).await?;
    }

    if should_skip_migrations() {
        log::info!(
            "migrate_config_postgres: populating postgres migration table without execution due to MOOSICBOX_SKIP_MIGRATION_EXECUTION"
        );
        let summary = runner
            .mark_all_migrations_completed(db, switchy_schema::MarkCompletedScope::PendingOnly)
            .await?;
        log::info!(
            "migrate_config_postgres: marked {} migrations as completed ({} newly marked, {} failed skipped, {} in-progress skipped)",
            summary.total,
            summary.newly_marked,
            summary.failed_skipped,
            summary.in_progress_skipped
        );
    } else {
        runner.run(db).await?;
    }
    log::debug!("migrate_config_postgres: finished running postgres migrations");

    Ok(())
}

/// Run `SQLite` configuration migrations only
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(feature = "sqlite")]
pub async fn migrate_config_sqlite(
    db: &dyn switchy_database::Database,
) -> Result<(), MigrateError> {
    log::debug!("migrate_config_sqlite: running sqlite migrations");
    let source = sqlite_config_migrations();
    let runner = switchy_schema::runner::MigrationRunner::new(Box::new(source))
        .with_table_name("__moosicbox_schema_migrations");

    if should_drop_migrations_table() {
        log::info!("migrate_config_sqlite: dropping sqlite migration table");
        runner.drop_tracking_table(db).await?;
    }

    if should_skip_migrations() {
        log::info!(
            "migrate_config_sqlite: populating sqlite migration table without execution due to MOOSICBOX_SKIP_MIGRATION_EXECUTION"
        );
        let summary = runner
            .mark_all_migrations_completed(db, switchy_schema::MarkCompletedScope::PendingOnly)
            .await?;
        log::info!(
            "migrate_config_sqlite: marked {} migrations as completed ({} newly marked, {} failed skipped, {} in-progress skipped)",
            summary.total,
            summary.newly_marked,
            summary.failed_skipped,
            summary.in_progress_skipped
        );
    } else {
        runner.run(db).await?;
    }
    log::debug!("migrate_config_sqlite: finished running sqlite migrations");

    Ok(())
}

/// Run configuration migrations for both `PostgreSQL` and `SQLite` databases
///
/// This function attempts to run configuration migrations for both database types
/// if their respective features are enabled. `PostgreSQL` failures are logged but
/// don't prevent `SQLite` migrations from running.
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub async fn migrate_config(db: &dyn switchy_database::Database) -> Result<(), MigrateError> {
    #[cfg(feature = "postgres")]
    {
        if let Err(e) = migrate_config_postgres(db).await {
            log::warn!("migrate_config: postgres migrations failed, continuing: {e:?}");
        }
    }

    #[cfg(feature = "sqlite")]
    {
        migrate_config_sqlite(db).await?;
    }

    Ok(())
}

/// Run `PostgreSQL` library migrations only
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(feature = "postgres")]
pub async fn migrate_library_postgres(
    db: &dyn switchy_database::Database,
) -> Result<(), MigrateError> {
    migrate_library_postgres_until(db, None).await
}

/// Run `PostgreSQL` library migrations only until a specific migration
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(feature = "postgres")]
pub async fn migrate_library_postgres_until(
    db: &dyn switchy_database::Database,
    migration_name: Option<&str>,
) -> Result<(), MigrateError> {
    log::debug!("migrate_library_postgres: running postgres migrations");
    let source = postgres_library_migrations();
    let runner = switchy_schema::runner::MigrationRunner::new(Box::new(source))
        .with_table_name("__moosicbox_schema_migrations");

    if should_drop_migrations_table() {
        log::info!("migrate_library_postgres_until: dropping postgres migration table");
        runner.drop_tracking_table(db).await?;
    }

    if should_skip_migrations() {
        log::info!(
            "migrate_library_postgres: populating postgres migration table without execution due to MOOSICBOX_SKIP_MIGRATION_EXECUTION"
        );
        let summary = runner
            .mark_all_migrations_completed(db, switchy_schema::MarkCompletedScope::PendingOnly)
            .await?;
        log::info!(
            "migrate_library_postgres: marked {} migrations as completed ({} newly marked, {} failed skipped, {} in-progress skipped)",
            summary.total,
            summary.newly_marked,
            summary.failed_skipped,
            summary.in_progress_skipped
        );
    } else {
        let runner = if let Some(migration_name) = migration_name {
            runner.with_strategy(switchy_schema::runner::ExecutionStrategy::UpTo(
                migration_name.to_string(),
            ))
        } else {
            runner.with_strategy(switchy_schema::runner::ExecutionStrategy::All)
        };
        runner.run(db).await?;
    }
    log::debug!("migrate_library_postgres: finished running postgres migrations");

    Ok(())
}

/// Run `SQLite` library migrations only
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(feature = "sqlite")]
pub async fn migrate_library_sqlite(
    db: &dyn switchy_database::Database,
) -> Result<(), MigrateError> {
    migrate_library_sqlite_until(db, None).await
}

/// Run `SQLite` library migrations only until a specific migration
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(feature = "sqlite")]
pub async fn migrate_library_sqlite_until(
    db: &dyn switchy_database::Database,
    migration_name: Option<&str>,
) -> Result<(), MigrateError> {
    log::debug!("migrate_library_sqlite: running sqlite migrations");
    let source = sqlite_library_migrations();
    let runner = switchy_schema::runner::MigrationRunner::new(Box::new(source))
        .with_table_name("__moosicbox_schema_migrations");

    if should_drop_migrations_table() {
        log::info!("migrate_library_sqlite_until: dropping sqlite migration table");
        runner.drop_tracking_table(db).await?;
    }

    if should_skip_migrations() {
        log::info!(
            "migrate_library_sqlite: populating sqlite migration table without execution due to MOOSICBOX_SKIP_MIGRATION_EXECUTION"
        );
        let summary = runner
            .mark_all_migrations_completed(db, switchy_schema::MarkCompletedScope::PendingOnly)
            .await?;
        log::info!(
            "migrate_library_sqlite: marked {} migrations as completed ({} newly marked, {} failed skipped, {} in-progress skipped)",
            summary.total,
            summary.newly_marked,
            summary.failed_skipped,
            summary.in_progress_skipped
        );
    } else {
        let runner = if let Some(migration_name) = migration_name {
            runner.with_strategy(switchy_schema::runner::ExecutionStrategy::UpTo(
                migration_name.to_string(),
            ))
        } else {
            runner.with_strategy(switchy_schema::runner::ExecutionStrategy::All)
        };
        runner.run(db).await?;
    }
    log::debug!("migrate_library_sqlite: finished running sqlite migrations");

    Ok(())
}

/// Run library migrations for both `PostgreSQL` and `SQLite` databases
///
/// This function attempts to run all library migrations for both database types
/// if their respective features are enabled. `PostgreSQL` failures are logged but
/// don't prevent `SQLite` migrations from running.
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub async fn migrate_library(db: &dyn switchy_database::Database) -> Result<(), MigrateError> {
    migrate_library_until(db, None).await
}

/// Run library migrations up to a specific migration for both `PostgreSQL` and `SQLite`
///
/// This function attempts to run library migrations up to the specified migration name
/// for both database types if their respective features are enabled. If `migration_name`
/// is `None`, all migrations are run. `PostgreSQL` failures are logged but don't prevent
/// `SQLite` migrations from running.
///
/// # Errors
///
/// * `MigrateError::Database` - If database connection or query fails
/// * `MigrateError::Schema` - If migration execution fails
#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub async fn migrate_library_until(
    db: &dyn switchy_database::Database,
    migration_name: Option<&str>,
) -> Result<(), MigrateError> {
    #[cfg(feature = "postgres")]
    {
        if let Err(e) = migrate_library_postgres_until(db, migration_name).await {
            log::warn!("migrate_library: postgres migrations failed, continuing: {e:?}");
        }
    }

    #[cfg(feature = "sqlite")]
    {
        migrate_library_sqlite_until(db, migration_name).await?;
    }

    Ok(())
}

// Include migration directories at compile time
#[cfg(feature = "sqlite")]
static SQLITE_CONFIG_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/migrations/server/config/sqlite");

#[cfg(feature = "sqlite")]
static SQLITE_LIBRARY_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/migrations/server/library/sqlite");

#[cfg(feature = "postgres")]
static POSTGRES_CONFIG_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/migrations/server/config/postgres");

#[cfg(feature = "postgres")]
static POSTGRES_LIBRARY_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/migrations/server/library/postgres");

/// Load migrations from a directory
#[cfg(any(feature = "sqlite", feature = "postgres"))]
fn load_migrations_from_dir(dir: &include_dir::Dir) -> CodeMigrationSource<'static> {
    let mut source = CodeMigrationSource::new();

    // Get all migration directories and sort them by name (which includes timestamp)
    let mut migration_dirs: Vec<_> = dir
        .entries()
        .iter()
        .filter_map(|entry| entry.as_dir())
        .collect();

    migration_dirs.sort_by(|a, b| a.path().file_name().cmp(&b.path().file_name()));

    for migration_dir in migration_dirs {
        if let Some(dir_name) = migration_dir.path().file_name().and_then(|n| n.to_str()) {
            // Find up.sql and down.sql files by iterating through files
            let mut up_sql_content = None;
            let mut down_sql_content = None;

            for file in migration_dir.files() {
                if let Some(file_name) = file.path().file_name().and_then(|n| n.to_str()) {
                    if file_name == "up.sql" {
                        up_sql_content = file.contents_utf8();
                    } else if file_name == "down.sql" {
                        down_sql_content = file.contents_utf8();
                    }
                }
            }

            if let Some(up_sql) = up_sql_content {
                source.add_migration(switchy_schema::discovery::code::CodeMigration::new(
                    dir_name.to_string(),
                    Box::new(up_sql.to_string()) as Box<dyn switchy_database::Executable>,
                    down_sql_content
                        .map(|s| Box::new(s.to_string()) as Box<dyn switchy_database::Executable>),
                ));
            }
        }
    }

    source
}

#[cfg(feature = "sqlite")]
fn sqlite_config_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&SQLITE_CONFIG_MIGRATIONS_DIR)
}

#[cfg(feature = "sqlite")]
fn sqlite_library_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&SQLITE_LIBRARY_MIGRATIONS_DIR)
}

#[cfg(feature = "postgres")]
fn postgres_config_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&POSTGRES_CONFIG_MIGRATIONS_DIR)
}

#[cfg(feature = "postgres")]
fn postgres_library_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&POSTGRES_LIBRARY_MIGRATIONS_DIR)
}

// Test-only migration collection functions for use with MigrationTestBuilder

/// Get `SQLite` library migrations for testing
///
/// This function extracts the migrations from the internal migration source
/// and returns them as a Vec for use with test utilities like `MigrationTestBuilder`.
///
/// # Errors
///
/// * `MigrateError::Schema` - If the migration source fails to provide migrations
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_schema::get_sqlite_library_migrations;
/// use switchy_schema_test_utils::MigrationTestBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let migrations = get_sqlite_library_migrations().await?;
///
/// let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
///
/// // Use with MigrationTestBuilder
/// MigrationTestBuilder::new(migrations)
///     .with_table_name("__moosicbox_schema_migrations")
///     .run(&*db)
///     .await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "sqlite")]
pub async fn get_sqlite_library_migrations() -> Result<
    Vec<std::sync::Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>,
    MigrateError,
> {
    use switchy_schema::migration::MigrationSource;
    let source = sqlite_library_migrations();
    Ok(source.migrations().await?)
}

/// Get `SQLite` config migrations for testing
///
/// This function extracts the migrations from the internal migration source
/// and returns them as a Vec for use with test utilities like `MigrationTestBuilder`.
///
/// # Errors
///
/// * `MigrateError::Schema` - If the migration source fails to provide migrations
///
/// # Examples
///
/// ```rust,no_run
/// use moosicbox_schema::get_sqlite_config_migrations;
/// use switchy_schema_test_utils::MigrationTestBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let migrations = get_sqlite_config_migrations().await?;
///
/// let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
///
/// // Use with MigrationTestBuilder
/// MigrationTestBuilder::new(migrations)
///     .with_table_name("__moosicbox_schema_migrations")
///     .run(&*db)
///     .await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "sqlite")]
pub async fn get_sqlite_config_migrations() -> Result<
    Vec<std::sync::Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>,
    MigrateError,
> {
    use switchy_schema::migration::MigrationSource;
    let source = sqlite_config_migrations();
    Ok(source.migrations().await?)
}

/// Get `PostgreSQL` library migrations for testing
///
/// This function extracts the migrations from the internal migration source
/// and returns them as a Vec for use with test utilities like `MigrationTestBuilder`.
///
/// # Errors
///
/// * `MigrateError::Schema` - If the migration source fails to provide migrations
///
/// # Examples
///
/// ```rust,ignore
/// use moosicbox_schema::get_postgres_library_migrations;
/// use switchy_schema_test_utils::MigrationTestBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let migrations = get_postgres_library_migrations().await?;
///
/// let db = switchy_database_connection::init_postgres_sqlx(
///     "postgres://user:pass@localhost/test"
/// ).await?;
///
/// // Use with MigrationTestBuilder
/// MigrationTestBuilder::new(migrations)
///     .with_table_name("__moosicbox_schema_migrations")
///     .run(&*db)
///     .await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "postgres")]
pub async fn get_postgres_library_migrations() -> Result<
    Vec<std::sync::Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>,
    MigrateError,
> {
    use switchy_schema::migration::MigrationSource;
    let source = postgres_library_migrations();
    Ok(source.migrations().await?)
}

/// Get `PostgreSQL` config migrations for testing
///
/// This function extracts the migrations from the internal migration source
/// and returns them as a Vec for use with test utilities like `MigrationTestBuilder`.
///
/// # Errors
///
/// * `MigrateError::Schema` - If the migration source fails to provide migrations
///
/// # Examples
///
/// ```rust,ignore
/// use moosicbox_schema::get_postgres_config_migrations;
/// use switchy_schema_test_utils::MigrationTestBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let migrations = get_postgres_config_migrations().await?;
///
/// let db = switchy_database_connection::init_postgres_sqlx(
///     "postgres://user:pass@localhost/test"
/// ).await?;
///
/// // Use with MigrationTestBuilder
/// MigrationTestBuilder::new(migrations)
///     .with_table_name("__moosicbox_schema_migrations")
///     .run(&*db)
///     .await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "postgres")]
pub async fn get_postgres_config_migrations() -> Result<
    Vec<std::sync::Arc<dyn switchy_schema::migration::Migration<'static> + 'static>>,
    MigrateError,
> {
    use switchy_schema::migration::MigrationSource;
    let source = postgres_config_migrations();
    Ok(source.migrations().await?)
}

#[cfg(not(feature = "sqlite"))]
#[allow(unused, clippy::missing_const_for_fn)]
fn sqlite_config_migrations() -> CodeMigrationSource<'static> {
    CodeMigrationSource::new()
}

#[cfg(not(feature = "sqlite"))]
#[allow(unused, clippy::missing_const_for_fn)]
fn sqlite_library_migrations() -> CodeMigrationSource<'static> {
    CodeMigrationSource::new()
}

#[cfg(not(feature = "postgres"))]
#[allow(unused, clippy::missing_const_for_fn)]
fn postgres_config_migrations() -> CodeMigrationSource<'static> {
    CodeMigrationSource::new()
}

#[cfg(not(feature = "postgres"))]
#[allow(unused, clippy::missing_const_for_fn)]
fn postgres_library_migrations() -> CodeMigrationSource<'static> {
    CodeMigrationSource::new()
}

#[cfg(feature = "sqlite")]
#[cfg(test)]
mod sqlite_tests {
    use moosicbox_json_utils::ToValueType;
    use moosicbox_music_models::{
        ApiSource, ApiSources,
        id::{ApiId, Id},
    };
    use pretty_assertions::assert_eq;
    use switchy_database::DatabaseValue;

    use super::*;

    #[test_log::test(switchy_async::test)]
    async fn sqlx_config_migrations() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();

        migrate_config_sqlite(&*db).await.unwrap();
    }

    #[test_log::test(switchy_async::test)]
    async fn sqlx_library_migrations() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();

        migrate_library_sqlite(&*db).await.unwrap();
    }

    #[test_log::test(switchy_async::test)]
    async fn rusqlite_config_migrations() {
        let db = switchy_database_connection::init_sqlite_rusqlite(None).unwrap();

        migrate_config_sqlite(&*db).await.unwrap();
    }

    #[test_log::test(switchy_async::test)]
    async fn rusqlite_library_migrations() {
        let db = switchy_database_connection::init_sqlite_rusqlite(None).unwrap();

        migrate_library_sqlite(&*db).await.unwrap();
    }

    #[test_log::test(switchy_async::test)]
    async fn test_api_sources_table_migration() {
        const API_SOURCES_COLUMN: &str = "
            (
                SELECT json_group_array(
                    json_object(
                       'id', api_sources.source_id,
                       'source', api_sources.source
                    )
                )
                FROM api_sources
                WHERE api_sources.entity_type='{table}' AND api_sources.entity_id = {table}.id
            ) AS api_sources
            ";

        let tidal = ApiSource::register("Tidal", "Tidal");
        let qobuz = ApiSource::register("Qobuz", "Qobuz");

        let db = switchy_database_connection::init_sqlite_rusqlite(None).unwrap();

        migrate_library_sqlite_until(&*db, Some("2024-09-21-130720_set_journal_mode_to_wal"))
            .await
            .unwrap();

        // Insert test data
        db.exec_raw(
            "
            INSERT INTO artists (id, title, cover, tidal_id, qobuz_id) VALUES
                (1, 'title1', '', 'art123', 'art456'),
                (2, 'title2', '', 'art789', NULL),
                (3, 'title3', '', NULL, 'art101112'),
                (4, 'title4', '', NULL, NULL);
            INSERT INTO albums (id, artist_id, title, date_released, date_added, artwork, directory, blur, tidal_id, qobuz_id) VALUES
                (1, 1, 'title1', '2022-01-01', '2022-01-01', '', '', 0, 'alb123', 'alb456'),
                (2, 2, 'title2', '2022-01-01', '2022-01-01', '', '', 0, 'alb789', NULL),
                (3, 3, 'title3', '2022-01-01', '2022-01-01', '', '', 0, NULL, 'alb101112'),
                (4, 4, 'title4', '2022-01-01', '2022-01-01', '', '', 0, NULL, NULL);
            INSERT INTO tracks (id, album_id, number, title, duration, file, format, source, tidal_id, qobuz_id) VALUES
                (1, 1, 1, 'title1', 10, 'file1', 'FLAC', 'LOCAL', '123', '456'),
                (2, 2, 2, 'title2', 13, 'file2', 'FLAC', 'LOCAL', '789', NULL),
                (3, 3, 3, 'title3', 19, 'file3', 'FLAC', 'LOCAL', NULL, '101112'),
                (4, 4, 4, 'title4', 15, 'file4', 'FLAC', 'LOCAL', NULL, NULL),
                (5, 4, 4, 'title4', 15, NULL, 'SOURCE', 'LOCAL', '123', NULL),
                (6, 4, 4, 'title4', 15, NULL, 'SOURCE', 'LOCAL', NULL, '123');
        ",
        )
        .await
        .unwrap();

        // Run the migration
        migrate_library_sqlite(&*db).await.unwrap();

        // Verify artists migration
        let artists = db
            .select("artists")
            .columns(&[&API_SOURCES_COLUMN.replace("{table}", "artists")])
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(artists.len(), 4);
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[0].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("art123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("art456".into())
                })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[1].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: tidal.clone(),
                id: Id::String("art789".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[2].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: qobuz.clone(),
                id: Id::String("art101112".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[3].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
        );

        // Verify albums migration
        let albums = db
            .select("albums")
            .columns(&[&API_SOURCES_COLUMN.replace("{table}", "albums")])
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(albums.len(), 4);
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[0].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("alb123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("alb456".into())
                })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[1].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: tidal.clone(),
                id: Id::String("alb789".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[2].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: qobuz.clone(),
                id: Id::String("alb101112".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[3].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
        );

        // Verify tracks migration
        let tracks = db
            .select("tracks")
            .columns(&["id", &API_SOURCES_COLUMN.replace("{table}", "tracks")])
            .sort("id", switchy_database::query::SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(tracks.len(), 5);
        assert_eq!(
            tracks
                .iter()
                .filter_map(|x| x.get("id").and_then(|x| x.as_u64()))
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 6]
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[0].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("456".into())
                })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[1].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: tidal.clone(),
                id: Id::String("789".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[2].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: qobuz.clone(),
                id: Id::String("101112".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[3].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[4].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("123".into())
                })
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_api_sources_column_migration() {
        let tidal = ApiSource::register("Tidal", "Tidal");
        let qobuz = ApiSource::register("Qobuz", "Qobuz");

        let db = switchy_database_connection::init_sqlite_rusqlite(None).unwrap();

        migrate_library_sqlite_until(
            &*db,
            Some("2025-05-31-110603_update_api_source_id_structure"),
        )
        .await
        .unwrap();

        // Insert test data
        db.exec_raw(
            "
            INSERT INTO artists (id, title, cover) VALUES
                (1, 'title1', ''),
                (2, 'title2', ''),
                (3, 'title3', ''),
                (4, 'title4', '');
            INSERT INTO albums (id, artist_id, title, date_released, date_added, artwork, directory, blur) VALUES
                (1, 1, 'title1', '2022-01-01', '2022-01-01', '', '', 0),
                (2, 2, 'title2', '2022-01-01', '2022-01-01', '', '', 0),
                (3, 3, 'title3', '2022-01-01', '2022-01-01', '', '', 0),
                (4, 4, 'title4', '2022-01-01', '2022-01-01', '', '', 0);
            INSERT INTO tracks (id, album_id, number, title, duration, file, format, source) VALUES
                (1, 1, 1, 'title1', 10, 'file1', 'FLAC', 'LOCAL'),
                (2, 2, 2, 'title2', 13, 'file2', 'FLAC', 'LOCAL'),
                (3, 3, 3, 'title3', 19, 'file3', 'FLAC', 'LOCAL'),
                (4, 4, 4, 'title4', 15, 'file4', 'FLAC', 'LOCAL'),
                (6, 4, 4, 'title4', 15, NULL, 'SOURCE', 'LOCAL');

            INSERT INTO api_sources (entity_type, entity_id, source, source_id) VALUES
                ('artists', 1, 'Tidal', 'art123'),
                ('artists', 1, 'Qobuz', 'art456'),
                ('artists', 2, 'Tidal', 'art789'),
                ('artists', 3, 'Qobuz', 'art101112');
            INSERT INTO api_sources (entity_type, entity_id, source, source_id) VALUES
                ('albums', 1, 'Tidal', 'alb123'),
                ('albums', 1, 'Qobuz', 'alb456'),
                ('albums', 2, 'Tidal', 'alb789'),
                ('albums', 3, 'Qobuz', 'alb101112');
            INSERT INTO api_sources (entity_type, entity_id, source, source_id) VALUES
                ('tracks', 1, 'Tidal', '123'),
                ('tracks', 1, 'Qobuz', '456'),
                ('tracks', 2, 'Tidal', '789'),
                ('tracks', 3, 'Qobuz', '101112'),
                ('tracks', 6, 'Tidal', '123'),
                ('tracks', 6, 'Qobuz', '123');
        ",
        )
        .await
        .unwrap();

        // Run the migration
        migrate_library_sqlite(&*db).await.unwrap();

        // Verify artists migration
        let artists = db
            .select("artists")
            .columns(&["api_sources"])
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(artists.len(), 4);
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[0].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("art123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("art456".into())
                })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[1].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: tidal.clone(),
                id: Id::String("art789".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[2].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: qobuz.clone(),
                id: Id::String("art101112".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                artists[3].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
        );

        // Verify albums migration
        let albums = db
            .select("albums")
            .columns(&["api_sources"])
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(albums.len(), 4);
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[0].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("alb123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("alb456".into())
                })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[1].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: tidal.clone(),
                id: Id::String("alb789".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[2].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: qobuz.clone(),
                id: Id::String("alb101112".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                albums[3].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
        );

        // Verify tracks migration
        let tracks = db
            .select("tracks")
            .columns(&["id", "api_sources"])
            .sort("id", switchy_database::query::SortDirection::Asc)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(tracks.len(), 5);
        assert_eq!(
            tracks
                .iter()
                .filter_map(|x| x.get("id").and_then(|x| x.as_u64()))
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 6]
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[0].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("456".into())
                })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[1].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: tidal.clone(),
                id: Id::String("789".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[2].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default().with_api_id(ApiId {
                source: qobuz.clone(),
                id: Id::String("101112".into())
            })
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[3].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
        );
        assert_eq!(
            <DatabaseValue as ToValueType<ApiSources>>::to_value_type(
                tracks[4].get("api_sources").unwrap()
            )
            .unwrap(),
            ApiSources::default()
                .with_api_id(ApiId {
                    source: tidal.clone(),
                    id: Id::String("123".into())
                })
                .with_api_id(ApiId {
                    source: qobuz.clone(),
                    id: Id::String("123".into())
                })
        );
    }
}
