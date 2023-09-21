use crate::{
    app::Db,
    slim::{
        menu::{Album, AlbumSource},
        player::Track,
    },
};
use serde::{Deserialize, Serialize};
use sqlite::{Connection, CursorWithOwnership, Row};
use std::{collections::HashMap, sync::PoisonError};
use thiserror::Error;

impl<T> From<PoisonError<T>> for DbError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Invalid Request")]
    InvalidRequest,
    #[error("Poison Error")]
    PoisonError,
    #[error(transparent)]
    SqliteError(#[from] sqlite::Error),
}

pub async fn init_db(db: &Db) -> Result<(), DbError> {
    if !does_table_exist(&db.library, "albums").await? {
        db
            .library
            .prepare("CREATE TABLE albums (id INTEGER PRIMARY KEY, artist TEXT, title TEXT, date_released TEXT, artwork TEXT, directory TEXT)")?
            .into_iter()
            .next();
    }
    if !does_table_exist(&db.library, "tracks").await? {
        db.library
            .prepare("CREATE TABLE tracks (id INTEGER PRIMARY KEY, album_id INTEGER, title TEXT, file TEXT)")?
            .into_iter()
            .next();
    }
    Ok(())
}

pub async fn get_albums(db: &Db) -> Result<Vec<Album>, DbError> {
    Ok(db
        .library
        .prepare("SELECT * FROM albums")?
        .into_iter()
        .filter_map(|row| row.ok())
        .map(|row| Album {
            id: row.read::<i64, _>("id").to_string(),
            artist: row.read::<&str, _>("artist").to_string(),
            title: row.read::<&str, _>("title").to_string(),
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|d| d.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|d| d.to_string()),
            directory: row
                .read::<Option<&str>, _>("directory")
                .map(|date| date.to_string()),
            source: AlbumSource::Local,
            ..Default::default()
        })
        .collect())
}

pub async fn get_album(db: &Db, id: i32) -> Result<Option<Album>, DbError> {
    Ok(db
        .library
        .prepare("SELECT * FROM albums WHERE id=?")?
        .into_iter()
        .bind((1, id as i64))?
        .filter_map(|row| row.ok())
        .map(|row| Album {
            id: row.read::<i64, _>("id").to_string(),
            artist: row.read::<&str, _>("artist").to_string(),
            title: row.read::<&str, _>("title").to_string(),
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            directory: row
                .read::<Option<&str>, _>("directory")
                .map(|date| date.to_string()),
            ..Default::default()
        })
        .next())
}

pub async fn get_track(db: &Db, id: i32) -> Result<Option<Track>, DbError> {
    Ok(db
        .library
        .prepare("SELECT * FROM tracks WHERE id=?")?
        .into_iter()
        .bind((1, id as i64))?
        .filter_map(|row| row.ok())
        .map(|row| Track {
            title: row.read::<&str, _>("title").to_string(),
            file: row.read::<Option<&str>, _>("file").map(|f| f.to_string()),
            ..Default::default()
        })
        .next())
}

#[derive(Clone)]
pub enum SqliteValue {
    String(String),
    StringOpt(Option<String>),
    Number(i64),
}

fn select<'a>(
    connection: &'a Connection,
    table_name: &str,
    filters: &Vec<(&'a str, SqliteValue)>,
    values: &[&'a str],
) -> Result<Vec<Row>, DbError> {
    Ok(bind_values(
        connection
            .prepare(format!(
                "SELECT {} FROM {table_name} {}",
                values.join(" AND "),
                build_where_clause(filters),
            ))?
            .into_iter(),
        filters,
    )?
    .filter_map(|row| row.ok())
    .collect())
}

fn find_id<'a>(
    connection: &'a Connection,
    table_name: &str,
    filters: &Vec<(&'a str, SqliteValue)>,
) -> Result<Option<i64>, DbError> {
    Ok(select(connection, table_name, filters, &["id"])?
        .first()
        .map(|row| row.read::<i64, _>("id")))
}

fn build_where_clause<'a>(values: &'a Vec<(&'a str, SqliteValue)>) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", build_where_props(values).join(" AND "))
    }
}

fn build_where_props<'a>(values: &'a [(&'a str, SqliteValue)]) -> Vec<String> {
    values
        .iter()
        .map(|(name, value)| match value {
            SqliteValue::StringOpt(None) => format!("{name} IS NULL"),
            _ => format!("{name}=?"),
        })
        .collect()
}

fn build_set_clause<'a>(values: &'a Vec<(&'a str, SqliteValue)>) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("SET {}", build_set_props(values).join(", "))
    }
}

fn build_set_props<'a>(values: &'a [(&'a str, SqliteValue)]) -> Vec<String> {
    values
        .iter()
        .map(|(name, value)| match value {
            SqliteValue::StringOpt(None) => format!("{name}=NULL"),
            _ => format!("{name}=?"),
        })
        .collect()
}

fn build_values_clause<'a>(values: &'a Vec<(&'a str, SqliteValue)>) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

fn build_values_props<'a>(values: &'a [(&'a str, SqliteValue)]) -> Vec<String> {
    values
        .iter()
        .map(|(_name, value)| match value {
            SqliteValue::StringOpt(None) => "NULL".to_string(),
            _ => "?".to_string(),
        })
        .collect()
}

fn bind_values<'a>(
    mut cursor: CursorWithOwnership<'a>,
    values: &'a Vec<(&'a str, SqliteValue)>,
) -> Result<CursorWithOwnership<'a>, DbError> {
    let mut i = 1;
    for (_key, value) in values {
        match value {
            SqliteValue::String(value) => {
                cursor = cursor.bind((i, value.as_str()))?;
                i += 1;
            }
            SqliteValue::StringOpt(Some(value)) => {
                cursor = cursor.bind((i, value.to_string().as_str()))?;
                i += 1;
            }
            SqliteValue::StringOpt(None) => (),
            SqliteValue::Number(value) => {
                cursor = cursor.bind((i, *value))?;
                i += 1;
            }
        }
    }

    Ok(cursor)
}

fn insert_and_get_id<'a>(
    connection: &'a Connection,
    table_name: &str,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<i64, DbError> {
    let column_names = values
        .clone()
        .into_iter()
        .map(|(key, _v)| key)
        .collect::<Vec<_>>()
        .join(", ");

    bind_values(
        connection
            .prepare(format!(
                "INSERT INTO {table_name} ({}) {}",
                column_names,
                build_values_clause(&values),
            ))?
            .into_iter(),
        &values,
    )?
    .next();

    Ok(find_id(connection, table_name, &values)?.unwrap())
}

fn insert_and_get_values<'a>(
    connection: &'a Connection,
    table_name: &str,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<Option<Row>, DbError> {
    let column_names = values
        .clone()
        .into_iter()
        .map(|(key, _v)| key)
        .collect::<Vec<_>>()
        .join(", ");

    bind_values(
        connection
            .prepare(format!(
                "INSERT INTO {table_name} ({}) {}",
                column_names,
                build_values_clause(&values),
            ))?
            .into_iter(),
        &values,
    )?
    .next();

    Ok(select(connection, table_name, &values, &["*"])?
        .into_iter()
        .last())
}

fn update_and_get_values<'a>(
    connection: &'a Connection,
    table_name: &str,
    id: i64,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<Option<Row>, DbError> {
    bind_values(
        connection
            .prepare(format!(
                "UPDATE {table_name} {} WHERE id=?",
                build_set_clause(&values),
            ))?
            .into_iter(),
        &[values.clone(), vec![("id", SqliteValue::Number(id))]].concat(),
    )?
    .next();

    Ok(select(
        connection,
        table_name,
        &vec![("id", SqliteValue::Number(id))],
        &["*"],
    )?
    .into_iter()
    .next())
}

fn update_and_get_id<'a>(
    connection: &'a Connection,
    table_name: &str,
    id: i64,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<i64, DbError> {
    bind_values(
        connection
            .prepare(format!(
                "UPDATE {table_name} {} WHERE id=?",
                build_set_clause(&values)
            ))?
            .into_iter(),
        &[values.clone(), vec![("id", SqliteValue::Number(id))]].concat(),
    )?
    .next();

    Ok(id)
}

fn upsert<'a>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<i64, DbError> {
    match find_id(connection, table_name, &filters)? {
        Some(id) => update_and_get_id(connection, table_name, id, values),
        None => insert_and_get_id(connection, table_name, values),
    }
}

fn upsert_and_get_values<'a>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<Row, DbError> {
    match find_id(connection, table_name, &filters)? {
        Some(id) => Ok(update_and_get_values(connection, table_name, id, values)?.unwrap()),
        None => Ok(insert_and_get_values(connection, table_name, values)?.unwrap()),
    }
}

pub fn add_albums(db: &Db, albums: Vec<Album>) -> Result<Vec<i64>, DbError> {
    let mut ids = Vec::new();

    for album in albums {
        ids.push(upsert(
            &db.library,
            "albums",
            vec![
                ("artist", SqliteValue::String(album.artist.clone())),
                ("title", SqliteValue::String(album.title.clone())),
                ("directory", SqliteValue::StringOpt(album.directory.clone())),
            ],
            vec![
                ("artist", SqliteValue::String(album.artist)),
                ("title", SqliteValue::String(album.title)),
                ("date_released", SqliteValue::StringOpt(album.date_released)),
                ("artwork", SqliteValue::StringOpt(album.artwork)),
                ("directory", SqliteValue::StringOpt(album.directory)),
            ],
        )?);
    }

    Ok(ids)
}

pub fn add_album_and_get_value(db: &Db, album: Album) -> Result<Album, DbError> {
    Ok(add_albums_and_get_values(db, vec![album])?[0].clone())
}

pub fn add_album_map_and_get_value(
    db: &Db,
    album: HashMap<&str, SqliteValue>,
) -> Result<Album, DbError> {
    Ok(add_album_maps_and_get_values(db, vec![album])?[0].clone())
}

pub fn add_albums_and_get_values(db: &Db, albums: Vec<Album>) -> Result<Vec<Album>, DbError> {
    add_album_maps_and_get_values(
        db,
        albums
            .into_iter()
            .map(|album| {
                HashMap::from([
                    ("artist", SqliteValue::String(album.artist)),
                    ("title", SqliteValue::String(album.title)),
                    ("date_released", SqliteValue::StringOpt(album.date_released)),
                    ("artwork", SqliteValue::StringOpt(album.artwork)),
                    ("directory", SqliteValue::StringOpt(album.directory)),
                ])
            })
            .collect(),
    )
}

pub fn add_album_maps_and_get_values(
    db: &Db,
    albums: Vec<HashMap<&str, SqliteValue>>,
) -> Result<Vec<Album>, DbError> {
    Ok(albums
        .into_iter()
        .map(|album| {
            if !album.contains_key("artist") || !album.contains_key("title") {
                return Err(DbError::InvalidRequest);
            }
            let filters = if album.contains_key("directory") && album.get("directory").is_some() {
                vec![("directory", album.get("directory").unwrap().clone())]
            } else {
                let mut values = vec![
                    ("artist", album.get("artist").unwrap().clone()),
                    ("title", album.get("title").unwrap().clone()),
                ];
                if album.contains_key("directory") {
                    values.push(("directory", album.get("directory").unwrap().clone()));
                }
                values
            };
            let row = upsert_and_get_values(
                &db.library,
                "albums",
                filters,
                album.into_iter().collect::<Vec<_>>(),
            )?;

            Ok::<_, DbError>(Album {
                id: row.read::<i64, _>("id").to_string(),
                artist: row.read::<&str, _>("artist").to_string(),
                title: row.read::<&str, _>("title").to_string(),
                date_released: row
                    .read::<Option<&str>, _>("date_released")
                    .map(|date| date.to_string()),
                artwork: row
                    .read::<Option<&str>, _>("artwork")
                    .map(|date| date.to_string()),
                directory: row
                    .read::<Option<&str>, _>("directory")
                    .map(|date| date.to_string()),
                ..Default::default()
            })
        })
        .filter_map(|album| album.ok())
        .collect())
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct InsertTrack {
    pub track: Track,
    pub album_id: i32,
    pub file: String,
}

pub fn add_tracks(db: &Db, tracks: Vec<InsertTrack>) -> Result<Vec<i64>, DbError> {
    Ok(tracks
        .iter()
        .map(|insert| {
            upsert(
                &db.library,
                "tracks",
                vec![
                    ("album_id", SqliteValue::Number(insert.album_id as i64)),
                    ("title", SqliteValue::String(insert.track.title.clone())),
                ],
                vec![
                    ("album_id", SqliteValue::Number(insert.album_id as i64)),
                    ("title", SqliteValue::String(insert.track.title.clone())),
                    ("file", SqliteValue::String(insert.file.clone())),
                ],
            )
        })
        .filter_map(|album| album.ok())
        .collect())
}

async fn does_table_exist(connection: &Connection, name: &str) -> Result<bool, sqlite::Error> {
    Ok(connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name=?")?
        .into_iter()
        .bind((1, name))
        .unwrap()
        .filter_map(|row| row.ok())
        .count()
        > 0)
}
