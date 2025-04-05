#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;

use include_dir::{Dir, DirEntry, File};
use moosicbox_database::{Database, DatabaseError, query::FilterableQuery};
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
    fn walk_dir(&'static self, mut on_file: impl FnMut(&'static File<'static>)) {
        fn walk(dir: &'static Dir<'static>, on_file: &mut impl FnMut(&'static File<'static>)) {
            for entry in dir.entries() {
                match entry {
                    DirEntry::Dir(dir) => walk(dir, on_file),
                    DirEntry::File(file) => {
                        on_file(file);
                    }
                }
            }
        }

        walk(&self.directory, &mut on_file);
    }

    fn as_btree(&'static self) -> BTreeMap<String, &'static [u8]> {
        let mut map = BTreeMap::new();

        self.walk_dir(|file| {
            let Some(name) = file.path().file_name().and_then(|x| x.to_str()) else {
                panic!("Invalid file name: {file:?}");
            };
            map.insert(name.to_string(), file.contents());
        });

        map
    }

    async fn run(&'static self, db: &dyn Database) -> Result<(), DatabaseError> {
        let migrations = self.as_btree();

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
                let migration = String::from_utf8_lossy(migration).to_string();
                db.exec_raw(&migration).await?;

                db.insert(MIGRATIONS_TABLE_NAME)
                    .value("name", &name)
                    .execute(db)
                    .await?;
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
