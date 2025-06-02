#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;

use include_dir::{Dir, DirEntry, File};
use switchy_database::{
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
        self.run_until(db, None).await
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
    pub async fn run_until(
        &'static self,
        db: &dyn Database,
        migration_name: Option<&str>,
    ) -> Result<(), DatabaseError> {
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
            if let Some(migration_name) = migration_name {
                if migration_name == name {
                    log::info!("run_until: stopping on migration_name={name}");
                    break;
                }
            }

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
                    db.exec_raw(&migration).await.inspect_err(|e| {
                        log::error!("run: failed to run name={name} migration: {e:?}");
                    })?;

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

#[cfg(feature = "sqlite")]
#[cfg(test)]
mod sqlite_tests {
    use moosicbox_json_utils::ToValueType;
    use moosicbox_music_models::{
        ApiSource, ApiSources,
        id::{ApiId, Id},
    };
    use pretty_assertions::assert_eq;

    use super::*;

    #[test_log::test(tokio::test)]
    async fn sqlx_config_migrations() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();

        sqlite::SQLITE_CONFIG_MIGRATIONS.run(&*db).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn sqlx_library_migrations() {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .unwrap();

        sqlite::SQLITE_LIBRARY_MIGRATIONS.run(&*db).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn rusqlite_config_migrations() {
        let db = switchy_database_connection::init_sqlite_rusqlite(None).unwrap();

        sqlite::SQLITE_CONFIG_MIGRATIONS.run(&*db).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn rusqlite_library_migrations() {
        let db = switchy_database_connection::init_sqlite_rusqlite(None).unwrap();

        sqlite::SQLITE_LIBRARY_MIGRATIONS.run(&*db).await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_api_sources_migration() {
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

        sqlite::SQLITE_LIBRARY_MIGRATIONS
            .run_until(
                &*db,
                Some("2025-05-31-110603_update_api_source_id_structure"),
            )
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
        sqlite::SQLITE_LIBRARY_MIGRATIONS.run(&*db).await.unwrap();

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
}
