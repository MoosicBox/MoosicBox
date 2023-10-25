use rusqlite::{params, Connection, Row, Statement};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    sync::PoisonError,
};
use thiserror::Error;

use super::models::{
    ActivePlayer, Album, Artist, AsId, AsModel, AsModelQuery, CreateSession, NumberId, Player,
    Session, SessionPlaylist, Track, UpdateSession,
};

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
    SqliteError(#[from] rusqlite::Error),
    #[error("Unknown DbError")]
    Unknown,
}

pub fn get_session_playlist_tracks(
    db: &Connection,
    session_playlist_id: i32,
) -> Result<Vec<Track>, DbError> {
    Ok(db
        .prepare_cached(
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
            WHERE session_playlist_tracks.session_playlist_id=?1
            ORDER BY number ASC
            ",
        )?
        .query_map(params![session_playlist_id], |row| Ok(row.as_model()))?
        .filter_map(|t| t.ok())
        .collect())
}

pub fn get_session_playlist(
    db: &Connection,
    session_id: i32,
) -> Result<Option<SessionPlaylist>, DbError> {
    db.prepare_cached(
        "
            SELECT session_playlists.*
            FROM session_playlists
            WHERE id=?1
            ",
    )?
    .query_map(params![session_id], |row| Ok(row.as_model_query(db)))?
    .find_map(|row| row.ok())
    .transpose()
}

pub fn get_session_active_players(
    db: &Connection,
    session_id: i32,
) -> Result<Vec<Player>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT players.*
            FROM active_players
            JOIN players on players.id=active_players.player_id
            WHERE active_players.session_id=?1
            ",
        )?
        .query_map(params![session_id], |row| Ok(row.as_model()))?
        .filter_map(|row| row.ok())
        .collect())
}

pub fn get_session(db: &Connection, id: i32) -> Result<Option<Session>, DbError> {
    db.prepare_cached(
        "
            SELECT sessions.*
            FROM sessions
            WHERE id=?1
            ",
    )?
    .query_map(params![id], |row| Ok(row.as_model_query(db)))?
    .find_map(|row| row.ok())
    .transpose()
}

pub fn get_sessions(db: &Connection) -> Result<Vec<Session>, DbError> {
    db.prepare_cached(
        "
            SELECT sessions.*
            FROM sessions
            ",
    )?
    .query_map([], |row| Ok(row.as_model_query(db)))?
    .filter_map(|row| row.ok())
    .collect()
}

pub fn create_session(db: &Connection, session: &CreateSession) -> Result<Session, DbError> {
    let tracks = get_tracks(db, &session.playlist.tracks)?;
    let playlist: SessionPlaylist = insert_and_get_row(
        db,
        "session_playlists",
        vec![("id", SqliteValue::StringOpt(None))],
    )?;
    tracks
        .iter()
        .map(|track| {
            insert_and_get_row::<NumberId>(
                db,
                "session_playlist_tracks",
                vec![
                    (
                        "session_playlist_id",
                        SqliteValue::Number(playlist.id as i64),
                    ),
                    ("track_id", SqliteValue::Number(track.id as i64)),
                ],
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let new_session: Session = insert_and_get_row(
        db,
        "sessions",
        vec![
            (
                "session_playlist_id",
                SqliteValue::Number(playlist.id as i64),
            ),
            ("name", SqliteValue::String(session.name.clone())),
        ],
    )?;

    for player_id in &session.active_players {
        insert_and_get_row::<ActivePlayer>(
            db,
            "active_players",
            vec![
                ("session_id", SqliteValue::Number(new_session.id as i64)),
                ("player_id", SqliteValue::Number(*player_id as i64)),
            ],
        )?;
    }

    Ok(Session {
        id: new_session.id,
        active: new_session.active,
        playing: new_session.playing,
        position: new_session.position,
        seek: new_session.seek,
        name: new_session.name,
        active_players: get_session_active_players(db, new_session.id)?,
        playlist,
    })
}

pub fn update_session(db: &Connection, session: &UpdateSession) -> Result<Session, DbError> {
    if session.playlist.is_some() {
        db
        .prepare_cached(
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
        .query(params![session.session_id])?
        .next()?;
    }

    let playlist_id = session
        .playlist
        .as_ref()
        .map(|p| p.session_playlist_id as i64);

    let tracks: Option<Vec<Track>> = session
        .playlist
        .as_ref()
        .map(|p| {
            let tracks = get_tracks(db, &p.tracks)?;
            tracks
                .iter()
                .map(|track| {
                    insert_and_get_row::<NumberId>(
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

    let new_session: Session = update_and_get_row(
        db,
        "sessions",
        SqliteValue::Number(session.session_id as i64),
        &values,
    )?
    .expect("Session failed to update");

    let playlist = if session.playlist.is_some() {
        SessionPlaylist {
            id: playlist_id.unwrap() as i32,
            tracks: tracks.unwrap(),
        }
    } else if let Some(playlist) = get_session_playlist(db, session.session_id)? {
        playlist
    } else {
        return Err(DbError::InvalidRequest);
    };

    Ok(Session {
        id: new_session.id,
        active: new_session.active,
        playing: new_session.playing,
        position: new_session.position,
        seek: new_session.seek,
        name: new_session.name,
        active_players: get_session_active_players(db, new_session.id)?,
        playlist,
    })
}

pub fn delete_session(db: &Connection, session_id: i32) -> Result<(), DbError> {
    db
        .prepare_cached(
            "
            DELETE FROM session_playlist_tracks
            WHERE session_playlist_tracks.id IN (
                SELECT session_playlist_tracks.id FROM session_playlist_tracks
                JOIN session_playlists ON session_playlists.id=sessions.session_playlist_id
                JOIN sessions ON sessions.session_playlist_id=session_playlists.id
                WHERE sessions.id=?1 AND session_playlist_tracks.session_playlist_id=session_playlists.id
            )
            ",
        )?
        .query(params![session_id as i64])?
        .next()?;

    let mut statement = db.prepare_cached(
        "
            DELETE FROM sessions
            WHERE id=?1
            RETURNING *
            ",
    )?;

    let mut query = statement.query(params![session_id as i64])?;

    let session_row = query.next().transpose().ok_or(DbError::InvalidRequest)??;

    db.prepare_cached(
        "
            DELETE FROM session_playlists
            WHERE id=?
            ",
    )?
    .query(params![session_row.get::<&str, i32>("id").unwrap()])?
    .next()?;

    Ok(())
}

pub fn get_connections(db: &Connection) -> Result<Vec<super::models::Connection>, DbError> {
    db.prepare_cached(
        "
            SELECT connections.*
            FROM connections
            ",
    )?
    .query_map([], |row| Ok(row.as_model_query(db)))?
    .filter_map(|c| c.ok())
    .collect()
}

pub fn register_connection(
    db: &Connection,
    connection: &super::models::RegisterConnection,
) -> Result<super::models::Connection, DbError> {
    let row: super::models::Connection = upsert_and_get_row(
        db,
        "connections",
        vec![("id", SqliteValue::String(connection.connection_id.clone()))],
        vec![
            ("id", SqliteValue::String(connection.connection_id.clone())),
            ("name", SqliteValue::String(connection.name.clone())),
        ],
    )?;

    for player in &connection.players {
        create_player(db, &connection.connection_id, player)?;
    }

    Ok(super::models::Connection {
        id: row.id.clone(),
        name: row.name,
        created: row.created,
        updated: row.updated,
        players: get_players(db, &row.id)?,
    })
}

pub fn delete_connection(db: &Connection, connection_id: &str) -> Result<(), DbError> {
    db.prepare_cached(
        "
            DELETE FROM players
            WHERE players.id IN (
                SELECT players.id FROM players
                JOIN connections ON connections.id=players.connection_id
                WHERE connections.id=?1
            )
            ",
    )?
    .query(params![connection_id])?
    .next()?;

    db.prepare_cached(
        "
            DELETE FROM connections
            WHERE id=?1
            ",
    )?
    .query(params![connection_id])?
    .next()?;

    Ok(())
}

pub fn get_players(
    db: &Connection,
    connection_id: &str,
) -> Result<Vec<super::models::Player>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT players.*
            FROM players
            WHERE connection_id=?1
            ",
        )?
        .query_map(params![connection_id], |row| Ok(row.as_model()))?
        .filter_map(|c| c.ok())
        .collect())
}

pub fn create_player(
    db: &Connection,
    connection_id: &str,
    player: &super::models::RegisterPlayer,
) -> Result<super::models::Player, DbError> {
    let player: Player = upsert_and_get_row(
        db,
        "players",
        vec![
            ("connection_id", SqliteValue::String(connection_id.into())),
            ("name", SqliteValue::String(player.name.clone())),
            ("type", SqliteValue::String(player.r#type.clone())),
        ],
        vec![
            ("connection_id", SqliteValue::String(connection_id.into())),
            ("name", SqliteValue::String(player.name.clone())),
            ("type", SqliteValue::String(player.r#type.clone())),
        ],
    )?;

    Ok(player)
}

pub fn set_session_active_players(
    db: &Connection,
    set_session_active_players: &super::models::SetSessionActivePlayers,
) -> Result<(), DbError> {
    db.prepare_cached(
        "
            DELETE FROM active_players
            WHERE session_id=?1
            ",
    )?
    .query(params![set_session_active_players.session_id])?
    .next()?;

    for player_id in &set_session_active_players.players {
        insert_and_get_row::<ActivePlayer>(
            db,
            "active_players",
            vec![
                (
                    "session_id",
                    SqliteValue::Number(set_session_active_players.session_id as i64),
                ),
                ("player_id", SqliteValue::Number(*player_id as i64)),
            ],
        )?;
    }

    Ok(())
}

pub fn delete_player(db: &Connection, player_id: i32) -> Result<(), DbError> {
    db.prepare_cached(
        "
            DELETE FROM players
            WHERE id=?1
            ",
    )?
    .query(params![player_id])?
    .next()?;

    Ok(())
}

pub fn get_artists(db: &Connection) -> Result<Vec<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT artists.*
            FROM artists",
        )?
        .query_map([], |row| Ok(row.as_model()))?
        .filter_map(|c| c.ok())
        .collect())
}

pub fn get_albums(db: &Connection) -> Result<Vec<Album>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id",
        )?
        .query_map([], |row| Ok(row.as_model()))?
        .filter_map(|c| c.ok())
        .collect())
}

pub fn get_artist(db: &Connection, id: i32) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT *
            FROM artists
            WHERE artists.id=?1",
        )?
        .query_map(params![id], |row| Ok(row.as_model()))?
        .find_map(|row| row.ok()))
}

pub fn get_album(db: &Connection, id: i32) -> Result<Option<Album>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.id=?1",
        )?
        .query_map(params![id], |row| Ok(row.as_model()))?
        .find_map(|row| row.ok()))
}

pub fn get_album_tracks(db: &Connection, album_id: i32) -> Result<Vec<Track>, DbError> {
    Ok(db
        .prepare_cached(
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
            WHERE tracks.album_id=?1
            ORDER BY number ASC",
        )?
        .query_map(params![album_id], |row| Ok(row.as_model()))?
        .filter_map(|row| row.ok())
        .collect())
}

pub fn get_artist_albums(db: &Connection, artist_id: i32) -> Result<Vec<Album>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.artist_id=?1",
        )?
        .query_map(params![artist_id], |row| Ok(row.as_model()))?
        .filter_map(|row| row.ok())
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
    let mut query = db.prepare_cached(&format!(
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
    ))?;

    let mut index = 1;
    for id in ids {
        query.raw_bind_parameter(index, *id)?;
        index += 1;
    }

    Ok(query
        .raw_query()
        .mapped(|row| Ok(row.as_model()))
        .filter_map(|row| row.ok())
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

fn select<T>(
    connection: &Connection,
    table_name: &str,
    filters: &Vec<(&str, SqliteValue)>,
    values: &[&str],
) -> Result<Vec<T>, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
{
    let mut statement = connection.prepare_cached(&format!(
        "SELECT {} FROM {table_name} {}",
        values.join(" AND "),
        build_where_clause(filters),
    ))?;

    bind_values(&mut statement, filters)?;

    Ok(statement
        .raw_query()
        .mapped(|row| Ok(row.as_model()))
        .filter_map(|row| row.ok())
        .collect::<Vec<_>>())
}

fn find_row<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    filters: &Vec<(&'a str, SqliteValue)>,
) -> Result<Option<T>, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
{
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
    let mut i = 0;
    let mut props = Vec::new();
    for (name, value) in values {
        props.push(match value {
            SqliteValue::StringOpt(None) => format!("{name}=NULL"),
            SqliteValue::NumberOpt(None) => format!("{name}=NULL"),
            _ => {
                i += 1;
                format!("`{name}`=?{i}").to_string()
            }
        });
    }
    props
}

fn build_values_clause<'a>(values: &'a Vec<(&'a str, SqliteValue)>) -> String {
    if values.is_empty() {
        "".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

fn build_values_props<'a>(values: &'a [(&'a str, SqliteValue)]) -> Vec<String> {
    let mut i = 0;
    let mut props = Vec::new();
    for (_name, value) in values {
        props.push(match value {
            SqliteValue::StringOpt(None) => "NULL".to_string(),
            SqliteValue::NumberOpt(None) => "NULL".to_string(),
            _ => {
                i += 1;
                format!("?{i}").to_string()
            }
        });
    }
    props
}

fn bind_values<'a>(
    statement: &mut Statement<'_>,
    values: &'a Vec<(&'a str, SqliteValue)>,
) -> Result<(), DbError> {
    let mut i = 1;
    for (_key, value) in values {
        match value {
            SqliteValue::String(value) => {
                statement.raw_bind_parameter(i, value)?;
                i += 1;
            }
            SqliteValue::StringOpt(Some(value)) => {
                statement.raw_bind_parameter(i, value)?;
                i += 1;
            }
            SqliteValue::StringOpt(None) => (),
            SqliteValue::Bool(value) => {
                statement.raw_bind_parameter(i, if *value { 1 } else { 0 })?;
                i += 1;
            }
            SqliteValue::Number(value) => {
                statement.raw_bind_parameter(i, *value)?;
                i += 1;
            }
            SqliteValue::NumberOpt(Some(value)) => {
                statement.raw_bind_parameter(i, *value)?;
                i += 1;
            }
            SqliteValue::NumberOpt(None) => (),
            SqliteValue::Real(value) => {
                statement.raw_bind_parameter(i, *value)?;
                i += 1;
            }
        }
    }

    Ok(())
}

fn insert_and_get_row<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<T, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
{
    let column_names = values
        .clone()
        .into_iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let statement = format!(
        "INSERT INTO {table_name} ({column_names}) {} RETURNING *",
        build_values_clause(&values),
    );

    let mut statement = connection.prepare_cached(&statement)?;
    bind_values(&mut statement, &values)?;
    let mut query = statement.raw_query();

    let value = query.next().transpose().ok_or(DbError::Unknown)??;

    Ok(value.as_model())
}

fn update_and_get_row<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    id: SqliteValue,
    values: &Vec<(&'a str, SqliteValue)>,
) -> Result<Option<T>, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
{
    let variable_count: i32 = values
        .iter()
        .map(|v| match v.1 {
            SqliteValue::StringOpt(None) => 0,
            SqliteValue::NumberOpt(None) => 0,
            _ => 1,
        })
        .sum();

    let statement = format!(
        "UPDATE {table_name} {} WHERE id=?{} RETURNING *",
        build_set_clause(values),
        variable_count + 1
    );
    println!("running statement {statement}");
    let mut statement = connection.prepare_cached(&statement)?;
    bind_values(&mut statement, &[values.clone(), vec![("id", id)]].concat())?;
    let mut query = statement.raw_query();

    Ok(query.next()?.map(|row| row.as_model()))
}

fn upsert<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<T, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
    T: AsId,
{
    match find_row(connection, table_name, &filters)? {
        Some(row) => Ok(update_and_get_row(connection, table_name, row.as_id(), &values)?.unwrap()),
        None => insert_and_get_row(connection, table_name, values),
    }
}

fn upsert_and_get_row<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<T, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
    T: AsId,
    T: Debug,
{
    match find_row(connection, table_name, &filters)? {
        Some(row) => {
            println!("ONE {table_name}");
            let updated =
                update_and_get_row(connection, table_name, row.as_id(), &values)?.unwrap();
            println!("aft ONE {table_name}");

            let str1 = format!("{row:?}");
            let str2 = format!("{updated:?}");

            if str1 == str2 {
                println!("No updates to {table_name}");
            } else {
                println!("Changed {table_name} from {str1} to {str2}");
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
            let row: Artist = upsert_and_get_row(
                db,
                "artists",
                vec![("title", artist.get("title").unwrap().clone())],
                artist.into_iter().collect::<Vec<_>>(),
            )?;

            Ok::<_, DbError>(row)
        })
        .filter_map(|artist| artist.ok())
        .collect())
}

pub fn add_albums(db: &Connection, albums: Vec<Album>) -> Result<Vec<Album>, DbError> {
    let mut data: Vec<Album> = Vec::new();

    for album in albums {
        data.push(upsert::<Album>(
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

    Ok(data)
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

            Ok::<_, DbError>(row)
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

pub fn add_tracks(db: &Connection, tracks: Vec<InsertTrack>) -> Result<Vec<Track>, DbError> {
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
