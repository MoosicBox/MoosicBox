#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;

use include_dir::{Dir, DirEntry, File};
use moosicbox_database::{
    Database, DatabaseError, DatabaseValue,
    query::FilterableQuery,
    schema::{Column, DataType},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// # Panics
///
/// * If the db connection fails to establish
///
/// # Errors
///
/// * If the migrations fail to run
#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub async fn migrate_config(db: &dyn Database) -> Result<(), MigrateError> {
    #[cfg(feature = "postgres")]
    {
        log::debug!("migrate_config: running postgres migrations");
        POSTGRES_CONFIG_MIGRATIONS.run(db).await?;
        log::debug!("migrate_config: finished running postgres migrations");
    }

    #[cfg(feature = "sqlite")]
    {
        log::debug!("migrate_config: running sqlite migrations");
        SQLITE_CONFIG_MIGRATIONS.run(db).await?;
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
pub async fn migrate_library(db: &dyn Database) -> Result<(), MigrateError> {
    #[cfg(feature = "postgres")]
    {
        log::debug!("migrate_library: running postgres migrations");
        POSTGRES_LIBRARY_MIGRATIONS.run(db).await?;
        log::debug!("migrate_library: finished running postgres migrations");
    }

    #[cfg(feature = "sqlite")]
    {
        log::debug!("migrate_library: running sqlite migrations");
        SQLITE_LIBRARY_MIGRATIONS.run(db).await?;
        log::debug!("migrate_library: finished running sqlite migrations");
    }

    Ok(())
}

const MIGRATIONS_TABLE_NAME: &str = "__moosicbox_schema_migrations";

pub struct Migrations {
    directory: Dir<'static>,
}

impl Migrations {
    fn walk_dir(&'static self, up: bool, mut on_file: impl FnMut(&str, &'static File<'static>)) {
        fn walk(
            dir: &'static Dir<'static>,
            target: &'static str,
            on_file: &mut impl FnMut(&str, &'static File<'static>),
        ) {
            struct Migration<'a> {
                migration_name: &'a str,
                file: &'static File<'static>,
            }

            let mut entries = vec![];
            let mut skip_migrations = vec![];

            for entry in dir.entries() {
                match entry {
                    DirEntry::Dir(dir) => walk(dir, target, on_file),
                    DirEntry::File(file) => {
                        let path = file.path();
                        let Some(parent) = path.parent() else {
                            continue;
                        };
                        let Some(migration_name) = parent.file_name().and_then(|x| x.to_str())
                        else {
                            continue;
                        };
                        let Some(name) = path.file_name().and_then(|x| x.to_str()) else {
                            continue;
                        };

                        if name == "metadata.toml" {
                            if let Some(contents) = file.contents_utf8() {
                                // FIXME: Actually parse the toml and don't actually skip this
                                if contents.trim() == "run_in_transaction = false" {
                                    skip_migrations.push(migration_name);
                                    continue;
                                }
                            }
                        }

                        let Some(extension) = path.extension().and_then(|x| x.to_str()) else {
                            continue;
                        };
                        if extension.to_lowercase() == "sql" {
                            let name = &name[0..(name.len() - extension.len() - 1)];

                            if name == target {
                                entries.push(Migration {
                                    migration_name,
                                    file,
                                });
                            }
                        }
                    }
                }
            }

            for entry in entries {
                if skip_migrations.iter().any(|x| *x == entry.migration_name) {
                    continue;
                }

                on_file(entry.migration_name, entry.file);
            }
        }

        walk(
            &self.directory,
            if up { "up" } else { "down" },
            &mut on_file,
        );
    }

    fn as_btree(&'static self, up: bool) -> BTreeMap<String, &'static [u8]> {
        let mut map = BTreeMap::new();

        self.walk_dir(up, |name, file| {
            map.insert(name.to_string(), file.contents());
        });

        map
    }

    /// # Errors
    ///
    /// * If the migrations table fails to be created
    /// * If fails to select existing ran migrations
    /// * If fails to insert new migration runs
    ///
    /// # Panics
    ///
    /// * If any asserts fail
    pub async fn run(&'static self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.create_table(MIGRATIONS_TABLE_NAME)
            .if_not_exists(true)
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "run_on".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::DateTime,
                default: Some(DatabaseValue::Now),
            })
            .execute(db)
            .await?;

        let migrations = self.as_btree(true);

        for (name, migration) in migrations {
            let results = db
                .select(MIGRATIONS_TABLE_NAME)
                .columns(&["name"])
                .where_eq("name", &name)
                .execute(db)
                .await?;

            moosicbox_assert::assert!(
                results.len() <= 1,
                "Migration {name} expected to have run at most 1 time, but has ran {} times",
                results.len()
            );

            if results.is_empty() {
                if std::env::var("MOOSICBOX_SKIP_MIGRATION_EXECUTION").as_deref() != Ok("1") {
                    log::info!("run: running name={name}");

                    let migration = String::from_utf8_lossy(migration).to_string();
                    db.exec_raw(&migration).await?;

                    log::info!("run: successfully ran name={name}");
                }

                db.insert(MIGRATIONS_TABLE_NAME)
                    .value("name", &name)
                    .execute(db)
                    .await?;
            } else {
                log::debug!("run: already ran name={name}");
            }
        }

        Ok(())
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite::*;

#[cfg(feature = "sqlite")]
mod sqlite {
    use include_dir::include_dir;

    use crate::Migrations;

    pub const SQLITE_CONFIG_MIGRATIONS: Migrations = Migrations {
        directory: include_dir!("$CARGO_MANIFEST_DIR/migrations/server/config/sqlite"),
    };

    pub const SQLITE_LIBRARY_MIGRATIONS: Migrations = Migrations {
        directory: include_dir!("$CARGO_MANIFEST_DIR/migrations/server/library/sqlite"),
    };
}

#[cfg(feature = "postgres")]
pub use postgres::*;

#[cfg(feature = "postgres")]
mod postgres {
    use include_dir::include_dir;

    use crate::Migrations;

    pub const POSTGRES_CONFIG_MIGRATIONS: Migrations = Migrations {
        directory: include_dir!("$CARGO_MANIFEST_DIR/migrations/server/config/postgres"),
    };

    pub const POSTGRES_LIBRARY_MIGRATIONS: Migrations = Migrations {
        directory: include_dir!("$CARGO_MANIFEST_DIR/migrations/server/library/postgres"),
    };
}
