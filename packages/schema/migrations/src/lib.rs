//! Embedded historical `MoosicBox` SQL migrations.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(any(feature = "sqlite", feature = "postgres"))]
use switchy_schema::discovery::code::CodeMigrationSource;

// Include migration directories at compile time
#[cfg(feature = "sqlite")]
static SQLITE_CONFIG_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/server/config/sqlite");

#[cfg(feature = "sqlite")]
static SQLITE_LIBRARY_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/server/library/sqlite");

#[cfg(feature = "postgres")]
static POSTGRES_CONFIG_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/server/config/postgres");

#[cfg(feature = "postgres")]
static POSTGRES_LIBRARY_MIGRATIONS_DIR: include_dir::Dir =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/server/library/postgres");

/// Load migrations from an embedded directory
///
/// Scans the provided directory for migration subdirectories containing `up.sql` and
/// optionally `down.sql` files, and returns a `CodeMigrationSource` with all discovered
/// migrations sorted by directory name (which includes timestamps).
#[cfg(any(feature = "sqlite", feature = "postgres"))]
#[must_use]
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

/// Load embedded `SQLite` configuration migrations
#[cfg(feature = "sqlite")]
#[must_use]
pub fn sqlite_config_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&SQLITE_CONFIG_MIGRATIONS_DIR)
}

/// Load embedded `SQLite` library migrations
#[cfg(feature = "sqlite")]
#[must_use]
pub fn sqlite_library_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&SQLITE_LIBRARY_MIGRATIONS_DIR)
}

/// Load embedded `PostgreSQL` configuration migrations
#[cfg(feature = "postgres")]
#[must_use]
pub fn postgres_config_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&POSTGRES_CONFIG_MIGRATIONS_DIR)
}

/// Load embedded `PostgreSQL` library migrations
#[cfg(feature = "postgres")]
#[must_use]
pub fn postgres_library_migrations() -> CodeMigrationSource<'static> {
    load_migrations_from_dir(&POSTGRES_LIBRARY_MIGRATIONS_DIR)
}
