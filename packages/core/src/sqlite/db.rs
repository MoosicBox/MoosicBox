use crate::slim::{
    menu::{Album, AlbumSource, Artist},
    player::Track,
};
use serde::{Deserialize, Serialize};
use sqlite::{Connection, CursorWithOwnership, Row};
use std::{collections::HashMap, fmt::Display, sync::PoisonError};
use thiserror::Error;

use super::models::{CreateSession, Session, SessionPlaylist, UpdateSession};

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
    #[error("Unknown DbError")]
    Unknown,
}

pub fn get_session_playlist_tracks(
    db: &Connection,
    session_playlist_id: i32,
) -> Result<Vec<Track>, DbError> {
    Ok(db
        .prepare(
            "
            SELECT tracks.*,
                albums.title as album,
                albums.blur as blur,
                albums.date_released as date_released,
                artists.title as artist,
                artists.id as artist_id,
                albums.artwork
            FROM session_playlist_tracks
            JOIN tracks ON tracks.id=session_playlist_tracks.track_id
            JOIN albums ON albums.id=tracks.album_id
            JOIN artists ON artists.id=albums.artist_id
            WHERE session_playlist_tracks.session_playlist_id=?
            ORDER BY number ASC
            ",
        )?
        .into_iter()
        .bind((1, session_playlist_id as i64))?
        .filter_map(|row| row.ok())
        .map(|row| Track {
            id: row.read::<i64, _>("id") as i32,
            number: row.read::<i64, _>("number") as i32,
            title: row.read::<&str, _>("title").to_string(),
            duration: row.read::<f64, _>("duration"),
            album: row.read::<&str, _>("album").to_string(),
            album_id: row.read::<i64, _>("album_id") as i32,
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            artist: row.read::<&str, _>("artist").to_string(),
            artist_id: row.read::<i64, _>("artist_id") as i32,
            file: row.read::<Option<&str>, _>("file").map(|f| f.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            blur: row.read::<i64, _>("blur") == 1,
        })
        .collect())
}

pub fn get_session_playlist(db: &Connection, session_id: i32) -> Result<SessionPlaylist, DbError> {
    db.prepare(
        "
            SELECT session_playlists.*
            FROM session_playlists
            WHERE id=?
            ",
    )?
    .into_iter()
    .bind((1, session_id as i64))?
    .filter_map(|row| row.ok())
    .map(|row| {
        let id = row.read::<i64, _>("id") as i32;
        Ok::<SessionPlaylist, DbError>(SessionPlaylist {
            id,
            tracks: get_session_playlist_tracks(db, id)?,
        })
    })
    .next()
    .unwrap_or(Err(DbError::InvalidRequest))
}

pub fn get_session(db: &Connection, id: i32) -> Result<Option<Session>, DbError> {
    db.prepare(
        "
            SELECT sessions.*
            FROM sessions
            WHERE id=?
            ",
    )?
    .into_iter()
    .bind((1, id as i64))?
    .filter_map(|row| row.ok())
    .map(|row| {
        let id = row.read::<i64, _>("id") as i32;
        Ok::<Session, DbError>(Session {
            id,
            active: row.read::<i64, _>("active") == 1,
            playing: row.read::<i64, _>("playing") == 1,
            position: row.read::<Option<i64>, _>("position").map(|x| x as i32),
            seek: row.read::<Option<i64>, _>("seek").map(|x| x as i32),
            name: row.read::<&str, _>("name").to_string(),
            playlist: get_session_playlist(db, id)?,
        })
    })
    .next()
    .transpose()
}

pub fn get_sessions(db: &Connection) -> Result<Vec<Session>, DbError> {
    db.prepare(
        "
            SELECT sessions.*
            FROM sessions
            ",
    )?
    .into_iter()
    .filter_map(|row| row.ok())
    .map(|row| {
        let id = row.read::<i64, _>("id") as i32;
        Ok::<Session, DbError>(Session {
            id,
            active: row.read::<i64, _>("active") == 1,
            playing: row.read::<i64, _>("playing") == 1,
            position: row.read::<Option<i64>, _>("position").map(|x| x as i32),
            seek: row.read::<Option<i64>, _>("seek").map(|x| x as i32),
            name: row.read::<&str, _>("name").to_string(),
            playlist: get_session_playlist(db, id)?,
        })
    })
    .collect()
}

pub fn create_session(db: &Connection, session: &CreateSession) -> Result<Session, DbError> {
    let tracks = get_tracks(db, &session.playlist.tracks)?;
    let playlist = insert_and_get_row(
        db,
        "session_playlists",
        vec![("id", SqliteValue::StringOpt(None))],
    )?;
    let playlist_id = playlist.read::<i64, _>("id");
    tracks
        .iter()
        .map(|track| {
            insert_and_get_row(
                db,
                "session_playlist_tracks",
                vec![
                    ("session_playlist_id", SqliteValue::Number(playlist_id)),
                    ("track_id", SqliteValue::Number(track.id as i64)),
                ],
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let session = insert_and_get_row(
        db,
        "sessions",
        vec![
            ("session_playlist_id", SqliteValue::Number(playlist_id)),
            ("name", SqliteValue::String(session.name.clone())),
        ],
    )?;

    Ok(Session {
        id: session.read::<i64, _>("id") as i32,
        active: session.read::<i64, _>("active") == 1,
        playing: session.read::<i64, _>("playing") == 1,
        position: session.read::<Option<i64>, _>("position").map(|x| x as i32),
        seek: session.read::<Option<i64>, _>("seek").map(|x| x as i32),
        name: session.read::<&str, _>("name").to_string(),
        playlist: SessionPlaylist {
            id: playlist_id as i32,
            tracks,
        },
    })
}

pub fn update_session(db: &Connection, session: &UpdateSession) -> Result<Session, DbError> {
    if session.playlist.is_some() {
        db
        .prepare(
            "
            DELETE FROM session_playlist_tracks
            WHERE session_playlist_tracks.id IN (
                SELECT session_playlist_tracks.id FROM session_playlist_tracks
                JOIN session_playlists ON session_playlists.id=sessions.session_playlist_id
                JOIN sessions ON sessions.session_playlist_id=session_playlists.id
                WHERE sessions.id=? AND session_playlist_tracks.session_playlist_id=session_playlists.id
            )
            ",
        )?
        .into_iter()
        .bind((1, session.id as i64))?
        .next();
    }

    let playlist_id = session.playlist.as_ref().map(|p| p.id as i64);

    let tracks: Option<Vec<Track>> = session
        .playlist
        .as_ref()
        .map(|p| {
            let tracks = get_tracks(db, &p.tracks)?;
            tracks
                .iter()
                .map(|track| {
                    insert_and_get_row(
                        db,
                        "session_playlist_tracks",
                        vec![
                            (
                                "session_playlist_id",
                                SqliteValue::Number(playlist_id.unwrap()),
                            ),
                            ("track_id", SqliteValue::Number(track.id as i64)),
                        ],
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<Vec<_>, DbError>(tracks)
        })
        .transpose()?;

    let mut values = Vec::new();

    if session.name.is_some() {
        values.push(("name", SqliteValue::String(session.name.clone().unwrap())))
    }
    if session.active.is_some() {
        values.push(("active", SqliteValue::Bool(session.active.unwrap())))
    }
    if session.playing.is_some() {
        values.push(("playing", SqliteValue::Bool(session.playing.unwrap())))
    }
    if session.position.is_some() {
        values.push((
            "position",
            SqliteValue::Number(session.position.unwrap() as i64),
        ))
    }
    if session.seek.is_some() {
        values.push(("seek", SqliteValue::Number(session.seek.unwrap() as i64)))
    }

    let row = update_and_get_row(db, "sessions", session.id as i64, &values)?
        .expect("Session failed to update");

    let playlist = if session.playlist.is_some() {
        SessionPlaylist {
            id: playlist_id.unwrap() as i32,
            tracks: tracks.unwrap(),
        }
    } else {
        get_session_playlist(db, session.id)?
    };

    Ok(Session {
        id: row.read::<i64, _>("id") as i32,
        active: row.read::<i64, _>("active") == 1,
        playing: row.read::<i64, _>("playing") == 1,
        position: row.read::<Option<i64>, _>("position").map(|x| x as i32),
        seek: row.read::<Option<i64>, _>("seek").map(|x| x as i32),
        name: row.read::<&str, _>("name").to_string(),
        playlist,
    })
}

pub fn delete_session(db: &Connection, session_id: i32) -> Result<(), DbError> {
    db
        .prepare(
            "
            DELETE FROM session_playlist_tracks
            WHERE session_playlist_tracks.id IN (
                SELECT session_playlist_tracks.id FROM session_playlist_tracks
                JOIN session_playlists ON session_playlists.id=sessions.session_playlist_id
                JOIN sessions ON sessions.session_playlist_id=session_playlists.id
                WHERE sessions.id=? AND session_playlist_tracks.session_playlist_id=session_playlists.id
            )
            ",
        )?
        .into_iter()
        .bind((1, session_id as i64))?
        .next();

    let session_row = db
        .prepare(
            "
            DELETE FROM sessions
            WHERE id=?
            RETURNING *
            ",
        )?
        .into_iter()
        .bind((1, session_id as i64))?
        .next()
        .ok_or(DbError::InvalidRequest)??;

    db.prepare(
        "
            DELETE FROM session_playlists
            WHERE id=?
            ",
    )?
    .into_iter()
    .bind((1, session_row.read::<i64, _>("id")))?
    .next();

    Ok(())
}

pub fn get_artists(db: &Connection) -> Result<Vec<Artist>, DbError> {
    Ok(db
        .prepare(
            "
            SELECT artists.*
            FROM artists",
        )?
        .into_iter()
        .filter_map(|row| row.ok())
        .map(|row| Artist {
            id: row.read::<i64, _>("id") as i32,
            title: row.read::<&str, _>("title").to_string(),
            cover: row.read::<Option<&str>, _>("cover").map(|c| c.to_string()),
        })
        .collect())
}

pub fn get_albums(db: &Connection) -> Result<Vec<Album>, DbError> {
    Ok(db
        .prepare(
            "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id",
        )?
        .into_iter()
        .filter_map(|row| row.ok())
        .map(|row| Album {
            id: row.read::<i64, _>("id") as i32,
            artist: row.read::<&str, _>("artist").to_string(),
            artist_id: row.read::<i64, _>("artist_id") as i32,
            title: row.read::<&str, _>("title").to_string(),
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|d| d.to_string()),
            date_added: row
                .read::<Option<&str>, _>("date_added")
                .map(|d| d.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|d| d.to_string()),
            directory: row
                .read::<Option<&str>, _>("directory")
                .map(|date| date.to_string()),
            source: AlbumSource::Local,
            blur: row.read::<i64, _>("blur") == 1,
        })
        .collect())
}

pub fn get_artist(db: &Connection, id: i32) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare(
            "
            SELECT *
            FROM artists
            WHERE artists.id=?",
        )?
        .into_iter()
        .bind((1, id as i64))?
        .filter_map(|row| row.ok())
        .map(|row| Artist {
            id: row.read::<i64, _>("id") as i32,
            title: row.read::<&str, _>("title").to_string(),
            cover: row.read::<Option<&str>, _>("cover").map(|c| c.to_string()),
        })
        .next())
}

pub fn get_album(db: &Connection, id: i32) -> Result<Option<Album>, DbError> {
    Ok(db
        .prepare(
            "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.id=?",
        )?
        .into_iter()
        .bind((1, id as i64))?
        .filter_map(|row| row.ok())
        .map(|row| Album {
            id: row.read::<i64, _>("id") as i32,
            artist: row.read::<&str, _>("artist").to_string(),
            artist_id: row.read::<i64, _>("artist_id") as i32,
            title: row.read::<&str, _>("title").to_string(),
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            date_added: row
                .read::<Option<&str>, _>("date_added")
                .map(|date| date.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            directory: row
                .read::<Option<&str>, _>("directory")
                .map(|date| date.to_string()),
            source: AlbumSource::Local,
            blur: row.read::<i64, _>("blur") == 1,
        })
        .next())
}

pub fn get_album_tracks(db: &Connection, album_id: i32) -> Result<Vec<Track>, DbError> {
    Ok(db
        .prepare(
            "
            SELECT tracks.*,
                albums.title as album,
                albums.blur as blur,
                albums.date_released as date_released,
                artists.title as artist,
                artists.id as artist_id,
                albums.artwork
            FROM tracks
            JOIN albums ON albums.id=tracks.album_id
            JOIN artists ON artists.id=albums.artist_id
            WHERE tracks.album_id=?
            ORDER BY number ASC",
        )?
        .into_iter()
        .bind((1, album_id as i64))?
        .filter_map(|row| row.ok())
        .map(|row| Track {
            id: row.read::<i64, _>("id") as i32,
            number: row.read::<i64, _>("number") as i32,
            title: row.read::<&str, _>("title").to_string(),
            duration: row.read::<f64, _>("duration"),
            album: row.read::<&str, _>("album").to_string(),
            album_id: row.read::<i64, _>("album_id") as i32,
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            artist: row.read::<&str, _>("artist").to_string(),
            artist_id: row.read::<i64, _>("artist_id") as i32,
            file: row.read::<Option<&str>, _>("file").map(|f| f.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            blur: row.read::<i64, _>("blur") == 1,
        })
        .collect())
}

pub fn get_artist_albums(db: &Connection, artist_id: i32) -> Result<Vec<Album>, DbError> {
    Ok(db
        .prepare(
            "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.artist_id=?",
        )?
        .into_iter()
        .bind((1, artist_id as i64))?
        .filter_map(|row| row.ok())
        .map(|row| Album {
            id: row.read::<i64, _>("id") as i32,
            artist: row.read::<&str, _>("artist").to_string(),
            artist_id: row.read::<i64, _>("artist_id") as i32,
            title: row.read::<&str, _>("title").to_string(),
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            date_added: row
                .read::<Option<&str>, _>("date_added")
                .map(|date| date.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            directory: row
                .read::<Option<&str>, _>("directory")
                .map(|date| date.to_string()),
            source: AlbumSource::Local,
            blur: row.read::<i64, _>("blur") == 1,
        })
        .collect())
}

pub fn get_track(db: &Connection, id: i32) -> Result<Option<Track>, DbError> {
    Ok(get_tracks(db, &vec![id])?.into_iter().next())
}
pub fn get_tracks(db: &Connection, ids: &Vec<i32>) -> Result<Vec<Track>, DbError> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let ids_param = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut query = db
        .prepare(format!(
            "
            SELECT tracks.*,
                albums.title as album,
                albums.blur as blur,
                albums.date_released as date_released,
                artists.title as artist,
                artists.id as artist_id,
                albums.artwork
            FROM tracks
            JOIN albums ON albums.id=tracks.album_id
            JOIN artists ON artists.id=albums.artist_id
            WHERE tracks.id IN ({ids_param})"
        ))?
        .into_iter();

    let mut index = 1;
    for id in ids {
        query = query.bind((index, (*id) as i64))?;
        index += 1;
    }

    Ok(query
        .filter_map(|row| row.ok())
        .map(|row| Track {
            id: row.read::<i64, _>("id") as i32,
            number: row.read::<i64, _>("number") as i32,
            title: row.read::<&str, _>("title").to_string(),
            duration: row.read::<f64, _>("duration"),
            album: row.read::<&str, _>("album").to_string(),
            album_id: row.read::<i64, _>("album_id") as i32,
            date_released: row
                .read::<Option<&str>, _>("date_released")
                .map(|date| date.to_string()),
            artist: row.read::<&str, _>("artist").to_string(),
            artist_id: row.read::<i64, _>("artist_id") as i32,
            file: row.read::<Option<&str>, _>("file").map(|f| f.to_string()),
            artwork: row
                .read::<Option<&str>, _>("artwork")
                .map(|date| date.to_string()),
            blur: row.read::<i64, _>("blur") == 1,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
pub enum SqliteValue {
    String(String),
    StringOpt(Option<String>),
    Bool(bool),
    Number(i64),
    NumberOpt(Option<i64>),
    Real(f64),
}

impl Display for SqliteValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                SqliteValue::String(str) => str.to_string(),
                SqliteValue::StringOpt(str_opt) => str_opt.clone().unwrap_or("NULL".to_string()),
                SqliteValue::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                SqliteValue::Number(num) => num.to_string(),
                SqliteValue::NumberOpt(num_opt) => {
                    num_opt.map(|n| n.to_string()).unwrap_or("NULL".to_string())
                }
                SqliteValue::Real(num) => num.to_string(),
            }
            .as_str(),
        )
    }
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

fn find_row<'a>(
    connection: &'a Connection,
    table_name: &str,
    filters: &Vec<(&'a str, SqliteValue)>,
) -> Result<Option<Row>, DbError> {
    Ok(select(connection, table_name, filters, &["*"])?
        .into_iter()
        .next())
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
            SqliteValue::NumberOpt(None) => format!("{name} IS NULL"),
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
            SqliteValue::NumberOpt(None) => format!("{name}=NULL"),
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
            SqliteValue::NumberOpt(None) => "NULL".to_string(),
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
            SqliteValue::Bool(value) => {
                cursor = cursor.bind((i, if *value { 1 } else { 0 }))?;
                i += 1;
            }
            SqliteValue::Number(value) => {
                cursor = cursor.bind((i, *value))?;
                i += 1;
            }
            SqliteValue::NumberOpt(Some(value)) => {
                cursor = cursor.bind((i, *value))?;
                i += 1;
            }
            SqliteValue::NumberOpt(None) => (),
            SqliteValue::Real(value) => {
                cursor = cursor.bind((i, *value))?;
                i += 1;
            }
        }
    }

    Ok(cursor)
}

fn get_value(row: &Row, key: &str, value: &SqliteValue) -> SqliteValue {
    match value {
        SqliteValue::String(_value) => SqliteValue::String(row.read::<&str, _>(key).to_string()),
        SqliteValue::StringOpt(_value) => {
            SqliteValue::StringOpt(row.read::<Option<&str>, _>(key).map(|s| s.to_string()))
        }
        SqliteValue::Bool(_value) => SqliteValue::Bool(row.read::<i64, _>(key) == 1),
        SqliteValue::Number(_value) => SqliteValue::Number(row.read::<i64, _>(key)),
        SqliteValue::NumberOpt(_value) => SqliteValue::NumberOpt(row.read::<Option<i64>, _>(key)),
        SqliteValue::Real(_value) => SqliteValue::Real(row.read::<f64, _>(key)),
    }
}

fn insert_and_get_row<'a>(
    connection: &'a Connection,
    table_name: &str,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<Row, DbError> {
    let column_names = values
        .clone()
        .into_iter()
        .map(|(key, _v)| key)
        .collect::<Vec<_>>()
        .join(", ");

    let value = bind_values(
        connection
            .prepare(format!(
                "INSERT INTO {table_name} ({column_names}) {} RETURNING *",
                build_values_clause(&values),
            ))?
            .into_iter(),
        &values,
    )?
    .next()
    .ok_or(DbError::Unknown)??;

    Ok(value)
}

fn update_and_get_row<'a>(
    connection: &'a Connection,
    table_name: &str,
    id: i64,
    values: &Vec<(&'a str, SqliteValue)>,
) -> Result<Option<Row>, DbError> {
    let statement = format!(
        "UPDATE {table_name} {} WHERE id=? RETURNING *",
        build_set_clause(values),
    );

    Ok(bind_values(
        connection.prepare(statement)?.into_iter(),
        &[values.clone(), vec![("id", SqliteValue::Number(id))]].concat(),
    )?
    .next()
    .transpose()?)
}

fn upsert<'a>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<Row, DbError> {
    match find_row(connection, table_name, &filters)? {
        Some(row) => {
            Ok(
                update_and_get_row(connection, table_name, row.read::<i64, _>("id"), &values)?
                    .unwrap(),
            )
        }
        None => insert_and_get_row(connection, table_name, values),
    }
}

fn upsert_and_get_row<'a>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<Row, DbError> {
    match find_row(connection, table_name, &filters)? {
        Some(row) => {
            let updated =
                update_and_get_row(connection, table_name, row.read::<i64, _>("id"), &values)?
                    .unwrap();

            if values
                .iter()
                .filter(|(key, new_value)| {
                    let old_value = &get_value(&row, key, new_value);

                    if old_value != new_value {
                        println!("Changed {key} from {old_value} to {new_value}");
                        true
                    } else {
                        false
                    }
                })
                .count()
                == 0
            {
                println!("No updates to {table_name}");
            }

            Ok(updated)
        }
        None => Ok(insert_and_get_row(connection, table_name, values)?),
    }
}

pub fn add_artist_and_get_artist(db: &Connection, artist: Artist) -> Result<Artist, DbError> {
    Ok(add_artists_and_get_artists(db, vec![artist])?[0].clone())
}

pub fn add_artist_map_and_get_artist(
    db: &Connection,
    artist: HashMap<&str, SqliteValue>,
) -> Result<Artist, DbError> {
    Ok(add_artist_maps_and_get_artists(db, vec![artist])?[0].clone())
}

pub fn add_artists_and_get_artists(
    db: &Connection,
    artists: Vec<Artist>,
) -> Result<Vec<Artist>, DbError> {
    add_artist_maps_and_get_artists(
        db,
        artists
            .into_iter()
            .map(|artist| {
                HashMap::from([
                    ("title", SqliteValue::String(artist.title)),
                    ("cover", SqliteValue::StringOpt(artist.cover)),
                ])
            })
            .collect(),
    )
}

pub fn add_artist_maps_and_get_artists(
    db: &Connection,
    artists: Vec<HashMap<&str, SqliteValue>>,
) -> Result<Vec<Artist>, DbError> {
    Ok(artists
        .into_iter()
        .map(|artist| {
            if !artist.contains_key("title") {
                return Err(DbError::InvalidRequest);
            }
            let row = upsert_and_get_row(
                db,
                "artists",
                vec![("title", artist.get("title").unwrap().clone())],
                artist.into_iter().collect::<Vec<_>>(),
            )?;

            Ok::<_, DbError>(Artist {
                id: row.read::<i64, _>("id") as i32,
                title: row.read::<&str, _>("title").to_string(),
                cover: row.read::<Option<&str>, _>("cover").map(|c| c.to_string()),
            })
        })
        .filter_map(|artist| artist.ok())
        .collect())
}

pub fn add_albums(db: &Connection, albums: Vec<Album>) -> Result<Vec<Row>, DbError> {
    let mut ids = Vec::new();

    for album in albums {
        ids.push(upsert(
            db,
            "albums",
            vec![
                ("artist_id", SqliteValue::Number(album.artist_id as i64)),
                ("title", SqliteValue::String(album.title.clone())),
                ("directory", SqliteValue::StringOpt(album.directory.clone())),
            ],
            vec![
                ("artist_id", SqliteValue::Number(album.artist_id as i64)),
                ("title", SqliteValue::String(album.title)),
                ("date_released", SqliteValue::StringOpt(album.date_released)),
                ("artwork", SqliteValue::StringOpt(album.artwork)),
                ("directory", SqliteValue::StringOpt(album.directory)),
            ],
        )?);
    }

    Ok(ids)
}

pub fn add_album_and_get_album(db: &Connection, album: Album) -> Result<Album, DbError> {
    Ok(add_albums_and_get_albums(db, vec![album])?[0].clone())
}

pub fn add_album_map_and_get_album(
    db: &Connection,
    album: HashMap<&str, SqliteValue>,
) -> Result<Album, DbError> {
    Ok(add_album_maps_and_get_albums(db, vec![album])?[0].clone())
}

pub fn add_albums_and_get_albums(
    db: &Connection,
    albums: Vec<Album>,
) -> Result<Vec<Album>, DbError> {
    add_album_maps_and_get_albums(
        db,
        albums
            .into_iter()
            .map(|album| {
                HashMap::from([
                    ("artist_id", SqliteValue::Number(album.artist_id as i64)),
                    ("title", SqliteValue::String(album.title)),
                    ("date_released", SqliteValue::StringOpt(album.date_released)),
                    ("artwork", SqliteValue::StringOpt(album.artwork)),
                    ("directory", SqliteValue::StringOpt(album.directory)),
                ])
            })
            .collect(),
    )
}

pub fn add_album_maps_and_get_albums(
    db: &Connection,
    albums: Vec<HashMap<&str, SqliteValue>>,
) -> Result<Vec<Album>, DbError> {
    Ok(albums
        .into_iter()
        .map(|album| {
            if !album.contains_key("artist_id") || !album.contains_key("title") {
                return Err(DbError::InvalidRequest);
            }
            let filters = if album.contains_key("directory") && album.get("directory").is_some() {
                vec![("directory", album.get("directory").unwrap().clone())]
            } else {
                let mut values = vec![
                    ("artist_id", album.get("artist_id").unwrap().clone()),
                    ("title", album.get("title").unwrap().clone()),
                ];
                if album.contains_key("directory") {
                    values.push(("directory", album.get("directory").unwrap().clone()));
                }
                values
            };
            let row =
                upsert_and_get_row(db, "albums", filters, album.into_iter().collect::<Vec<_>>())?;

            Ok::<_, DbError>(Album {
                id: row.read::<i64, _>("id") as i32,
                artist_id: row.read::<i64, _>("artist_id") as i32,
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

pub fn add_tracks(db: &Connection, tracks: Vec<InsertTrack>) -> Result<Vec<Row>, DbError> {
    Ok(tracks
        .iter()
        .map(|insert| {
            upsert(
                db,
                "tracks",
                vec![
                    ("number", SqliteValue::Number(insert.track.number as i64)),
                    ("album_id", SqliteValue::Number(insert.album_id as i64)),
                    ("title", SqliteValue::String(insert.track.title.clone())),
                ],
                vec![
                    ("number", SqliteValue::Number(insert.track.number as i64)),
                    ("duration", SqliteValue::Real(insert.track.duration)),
                    ("album_id", SqliteValue::Number(insert.album_id as i64)),
                    ("title", SqliteValue::String(insert.track.title.clone())),
                    ("file", SqliteValue::String(insert.file.clone())),
                ],
            )
        })
        .filter_map(|track| track.ok())
        .collect())
}
