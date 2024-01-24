use log::debug;
use moosicbox_json_utils::ParseError;
use rusqlite::{params, Connection, Row, Statement};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    sync::PoisonError,
};
use thiserror::Error;

use crate::types::PlaybackQuality;

use super::models::{
    ActivePlayer, Album, AlbumVersionQuality, Artist, AsId, AsModel, AsModelQuery, AsModelResult,
    AsModelResultMappedMut, AsModelResultMappedQuery, AsModelResultMut, ClientAccessToken,
    CreateSession, MagicToken, NumberId, Player, Session, SessionPlaylist, SessionPlaylistTrack,
    Track, TrackSize, UpdateSession,
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
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub fn get_session_playlist_tracks(
    db: &Connection,
    session_playlist_id: i32,
) -> Result<Vec<SessionPlaylistTrack>, DbError> {
    AsModelResultMut::as_model_mut(
        &mut db
            .prepare_cached(
                "
                SELECT session_playlist_tracks.*
                FROM session_playlist_tracks
                WHERE session_playlist_tracks.session_playlist_id=?1
                ORDER BY session_playlist_tracks.id ASC
            ",
            )?
            .query(params![session_playlist_id])?,
    )
}

pub fn get_client_id(db: &Connection) -> Result<Option<String>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT client_id
            FROM client_access_tokens
            WHERE expires IS NULL OR expires > date('now')
            ORDER BY updated DESC
            LIMIT 1
            ",
        )?
        .query_map(params![], |row| Ok(row.get("client_id")))?
        .find_map(|row| row.ok())
        .transpose()?)
}

pub fn get_client_access_token(db: &Connection) -> Result<Option<(String, String)>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT client_id, token
            FROM client_access_tokens
            WHERE expires IS NULL OR expires > date('now')
            ORDER BY updated DESC
            LIMIT 1
            ",
        )?
        .query_map(params![], |row| {
            Ok((row.get("client_id").unwrap(), row.get("token").unwrap()))
        })?
        .find_map(|row| row.ok()))
}

pub fn create_client_access_token(
    db: &Connection,
    client_id: &str,
    token: &str,
) -> Result<(), DbError> {
    upsert::<ClientAccessToken>(
        db,
        "client_access_tokens",
        vec![
            ("token", SqliteValue::String(token.to_string())),
            ("client_id", SqliteValue::String(client_id.to_string())),
        ],
        vec![
            ("token", SqliteValue::String(token.to_string())),
            ("client_id", SqliteValue::String(client_id.to_string())),
        ],
    )?;

    Ok(())
}

pub fn delete_magic_token(db: &Connection, magic_token: &str) -> Result<(), DbError> {
    db.prepare_cached(
        "
            DELETE FROM magic_tokens
            WHERE magic_token=?1
            ",
    )?
    .query(params![magic_token])?
    .next()?;

    Ok(())
}

pub fn get_credentials_from_magic_token(
    db: &Connection,
    magic_token: &str,
) -> Result<Option<(String, String)>, DbError> {
    if let Some((client_id, access_token)) = db
        .prepare_cached(
            "
            SELECT client_id, access_token
            FROM magic_tokens
            WHERE (expires IS NULL OR expires > date('now')) AND magic_token = ?1
            ",
        )?
        .query_map(params![magic_token], |row| {
            Ok((
                row.get("client_id").unwrap(),
                row.get("access_token").unwrap(),
            ))
        })?
        .find_map(|row| row.ok())
    {
        delete_magic_token(db, magic_token)?;

        Ok(Some((client_id, access_token)))
    } else {
        Ok(None)
    }
}

pub fn save_magic_token(
    db: &Connection,
    magic_token: &str,
    client_id: &str,
    access_token: &str,
) -> Result<(), DbError> {
    upsert::<MagicToken>(
        db,
        "magic_tokens",
        vec![
            ("magic_token", SqliteValue::String(magic_token.to_string())),
            ("client_id", SqliteValue::String(client_id.to_string())),
            (
                "access_token",
                SqliteValue::String(access_token.to_string()),
            ),
        ],
        vec![
            ("magic_token", SqliteValue::String(magic_token.to_string())),
            ("client_id", SqliteValue::String(client_id.to_string())),
            (
                "access_token",
                SqliteValue::String(access_token.to_string()),
            ),
            ("expires", SqliteValue::NowAdd("+1 Day".into())),
        ],
    )?;

    Ok(())
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
        .query_map(params![session_id], |row| Ok(AsModel::as_model(row)))?
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
    let tracks = get_tracks(db, Some(&session.playlist.tracks))?;
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
        volume: new_session.volume,
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

    if let Some(tracks) = session.playlist.as_ref().map(|p| &p.tracks) {
        for track in tracks {
            insert_and_get_row::<NumberId>(
                db,
                "session_playlist_tracks",
                vec![
                    (
                        "session_playlist_id",
                        SqliteValue::Number(playlist_id.unwrap()),
                    ),
                    ("track_id", SqliteValue::Number(track.id as i64)),
                    (
                        "type",
                        SqliteValue::String(track.r#type.as_ref().to_string()),
                    ),
                    ("data", SqliteValue::StringOpt(track.data.clone())),
                ],
            )?;
        }
    }

    let mut values = Vec::new();

    if let Some(name) = &session.name {
        values.push(("name", SqliteValue::String(name.clone())))
    }
    if let Some(active) = session.active {
        values.push(("active", SqliteValue::Bool(active)))
    }
    if let Some(playing) = session.playing {
        values.push(("playing", SqliteValue::Bool(playing)))
    }
    if let Some(position) = session.position {
        values.push(("position", SqliteValue::Number(position as i64)))
    }
    if let Some(seek) = session.seek {
        values.push(("seek", SqliteValue::Number(seek as i64)))
    }
    if let Some(volume) = session.volume {
        values.push(("volume", SqliteValue::Real(volume)))
    }

    let new_session: Session = if values.is_empty() {
        select::<Session>(
            db,
            "sessions",
            &vec![("id", SqliteValue::Number(session.session_id as i64))],
            &["*"],
        )?
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("No session exists for id {}", session.session_id))
    } else {
        update_and_get_row(
            db,
            "sessions",
            SqliteValue::Number(session.session_id as i64),
            &values,
        )?
        .expect("Session failed to update")
    };

    let playlist = if let Some(playlist) = &session.playlist {
        SessionPlaylist {
            id: playlist_id.unwrap() as i32,
            tracks: playlist
                .tracks
                .iter()
                .map(|track| track.clone().into())
                .collect::<Vec<_>>()
                .as_model_mapped_query(db)?,
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
        volume: new_session.volume,
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
        .query_map(params![connection_id], |row| Ok(AsModel::as_model(row)))?
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
        .query_map([], |row| Ok(AsModel::as_model(row)))?
        .filter_map(|c| c.ok())
        .collect())
}

pub fn get_albums(db: &Connection) -> Result<Vec<Album>, DbError> {
    db.prepare_cached(
        "
            SELECT DISTINCT
                albums.*,
                albums.id as album_id,
                track_sizes.bit_depth,
                track_sizes.sample_rate,
                track_sizes.channels,
                artists.title as artist,
                tracks.format,
                tracks.source
            FROM albums
            JOIN tracks ON tracks.album_id=albums.id
            JOIN track_sizes ON track_sizes.track_id=tracks.id
            JOIN artists ON artists.id=albums.artist_id
            ORDER BY albums.id desc
        ",
    )?
    .query([])?
    .as_model_mapped_mut()
}

pub fn get_all_album_version_qualities(
    db: &Connection,
    album_ids: Vec<i32>,
) -> Result<Vec<AlbumVersionQuality>, DbError> {
    let ids_str = album_ids
        .iter()
        .enumerate()
        .map(|(i, _id)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let mut query = db.prepare_cached(&format!(
        "
            SELECT DISTINCT
                albums.id as album_id,
                track_sizes.bit_depth,
                track_sizes.sample_rate,
                track_sizes.channels,
                tracks.format,
                tracks.source
            FROM albums
            JOIN tracks ON tracks.album_id=albums.id
            JOIN track_sizes ON track_sizes.track_id=tracks.id
            WHERE albums.id=({ids_str})
            ORDER BY albums.id desc
            ",
    ))?;

    for (i, id) in album_ids.iter().enumerate() {
        query.raw_bind_parameter(i + 1, id)?;
    }

    let mut versions = query
        .query_map([], |row| Ok(AsModel::as_model(row)))?
        .filter_map(|c| c.ok())
        .collect::<Vec<_>>();

    versions.sort_by(|a: &AlbumVersionQuality, b: &AlbumVersionQuality| {
        b.sample_rate
            .unwrap_or_default()
            .cmp(&a.sample_rate.unwrap_or_default())
    });
    versions.sort_by(|a: &AlbumVersionQuality, b: &AlbumVersionQuality| {
        b.bit_depth
            .unwrap_or_default()
            .cmp(&a.bit_depth.unwrap_or_default())
    });

    Ok(versions)
}

pub fn get_album_version_qualities(
    db: &Connection,
    album_id: i32,
) -> Result<Vec<AlbumVersionQuality>, DbError> {
    let mut versions = AsModelResultMut::<AlbumVersionQuality, DbError>::as_model_mut(
        &mut db
            .prepare_cached(
                "
            SELECT DISTINCT 
                track_sizes.bit_depth,
                track_sizes.sample_rate,
                track_sizes.channels,
                tracks.format,
                tracks.source
            FROM albums
            JOIN tracks ON tracks.album_id=albums.id
            JOIN track_sizes ON track_sizes.track_id=tracks.id
            WHERE albums.id=?1",
            )?
            .query(params![album_id])?,
    )?;

    versions.sort_by(|a: &AlbumVersionQuality, b: &AlbumVersionQuality| {
        b.sample_rate
            .unwrap_or_default()
            .cmp(&a.sample_rate.unwrap_or_default())
    });
    versions.sort_by(|a: &AlbumVersionQuality, b: &AlbumVersionQuality| {
        b.bit_depth
            .unwrap_or_default()
            .cmp(&a.bit_depth.unwrap_or_default())
    });

    Ok(versions)
}

pub fn get_artist(db: &Connection, id: i32) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT *
            FROM artists
            WHERE artists.id=?1",
        )?
        .query_map(params![id], |row| Ok(AsModel::as_model(row)))?
        .find_map(|row| row.ok()))
}

pub fn get_tidal_artist(db: &Connection, tidal_id: i32) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT *
            FROM artists
            WHERE artists.tidal_id=?1",
        )?
        .query_map(params![tidal_id], |row| Ok(AsModel::as_model(row)))?
        .find_map(|row| row.ok()))
}

pub fn get_qobuz_artist(db: &Connection, qobuz_id: i32) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT *
            FROM artists
            WHERE artists.qobuz_id=?1",
        )?
        .query_map(params![qobuz_id], |row| Ok(AsModel::as_model(row)))?
        .find_map(|row| row.ok()))
}

pub fn get_album_artist(db: &Connection, album_id: i32) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT artists.*
            FROM artists
            JOIN albums on albums.artist_id=artists.id
            WHERE albums.id=?1",
        )?
        .query_map(params![album_id], |row| Ok(AsModel::as_model(row)))?
        .find_map(|row| row.ok()))
}

pub fn get_tidal_album_artist(
    db: &Connection,
    tidal_album_id: i32,
) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT artists.*
            FROM artists
            JOIN albums on albums.artist_id=artists.id
            WHERE albums.tidal_id=?1",
        )?
        .query_map(params![tidal_album_id], |row| Ok(AsModel::as_model(row)))?
        .find_map(|row| row.ok()))
}

pub fn get_qobuz_album_artist(
    db: &Connection,
    qobuz_album_id: i32,
) -> Result<Option<Artist>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT artists.*
            FROM artists
            JOIN albums on albums.artist_id=artists.id
            WHERE albums.qobuz_id=?1",
        )?
        .query_map(params![qobuz_album_id], |row| Ok(AsModel::as_model(row)))?
        .find_map(|row| row.ok()))
}

pub fn get_album(db: &Connection, id: i32) -> Result<Option<Album>, DbError> {
    db.prepare_cached(
        "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.id=?1",
    )?
    .query_map(params![id], |row| Ok(row.as_model_query(db)))?
    .find_map(|row| row.ok())
    .transpose()
}

pub fn get_tidal_album(db: &Connection, tidal_id: i32) -> Result<Option<Album>, DbError> {
    db.prepare_cached(
        "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.tidal_id=?1",
    )?
    .query_map(params![tidal_id], |row| Ok(row.as_model_query(db)))?
    .find_map(|row| row.ok())
    .transpose()
}

pub fn get_qobuz_album(db: &Connection, qobuz_id: i32) -> Result<Option<Album>, DbError> {
    db.prepare_cached(
        "
            SELECT albums.*, artists.title as artist
            FROM albums
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.qobuz_id=?1",
    )?
    .query_map(params![qobuz_id], |row| Ok(row.as_model_query(db)))?
    .find_map(|row| row.ok())
    .transpose()
}

pub fn get_album_tracks(db: &Connection, album_id: i32) -> Result<Vec<Track>, DbError> {
    db.prepare_cached(
        "
            SELECT tracks.*,
                albums.title as album,
                albums.blur as blur,
                albums.date_released as date_released,
                albums.date_added as date_added,
                artists.title as artist,
                artists.id as artist_id,
                albums.artwork,
                track_sizes.format,
                track_sizes.bytes,
                track_sizes.bit_depth,
                track_sizes.audio_bitrate,
                track_sizes.overall_bitrate,
                track_sizes.sample_rate,
                track_sizes.channels
            FROM tracks
            JOIN albums ON albums.id=tracks.album_id
            JOIN artists ON artists.id=albums.artist_id
            JOIN track_sizes ON tracks.id=track_sizes.track_id AND track_sizes.format=tracks.format
            WHERE tracks.album_id=?1
            ORDER BY number ASC",
    )?
    .query(params![album_id])?
    .as_model_mut()
}

pub fn get_artist_albums(db: &Connection, artist_id: i32) -> Result<Vec<Album>, DbError> {
    db.prepare_cached(
        "
            SELECT DISTINCT
                albums.*,
                albums.id as album_id,
                track_sizes.bit_depth,
                track_sizes.sample_rate,
                track_sizes.channels,
                artists.title as artist,
                tracks.format,
                tracks.source
            FROM albums
            JOIN tracks ON tracks.album_id=albums.id
            JOIN track_sizes ON track_sizes.track_id=tracks.id
            JOIN artists ON artists.id=albums.artist_id
            WHERE albums.artist_id=?1
            ORDER BY albums.id desc
        ",
    )?
    .query(params![artist_id])?
    .as_model_mapped_mut()
}

#[derive(Debug, Clone)]
pub struct SetTrackSize {
    pub track_id: i32,
    pub quality: PlaybackQuality,
    pub bytes: u64,
    pub bit_depth: Option<Option<u8>>,
    pub audio_bitrate: Option<Option<u32>>,
    pub overall_bitrate: Option<Option<u32>>,
    pub sample_rate: Option<Option<u32>>,
    pub channels: Option<Option<u8>>,
}

pub fn set_track_size(db: &Connection, value: SetTrackSize) -> Result<TrackSize, DbError> {
    Ok(set_track_sizes(db, &[value])?.first().unwrap().clone())
}

pub fn set_track_sizes(
    db: &Connection,
    values: &[SetTrackSize],
) -> Result<Vec<TrackSize>, DbError> {
    let values = values
        .iter()
        .map(|v| {
            let mut values = vec![
                ("track_id", SqliteValue::Number(v.track_id as i64)),
                (
                    "format",
                    SqliteValue::String(v.quality.format.as_ref().to_string()),
                ),
                ("bytes", SqliteValue::Number(v.bytes as i64)),
            ];

            if let Some(bit_depth) = v.bit_depth {
                values.push((
                    "bit_depth",
                    SqliteValue::NumberOpt(bit_depth.map(|x| x as i64)),
                ));
            }
            if let Some(audio_bitrate) = v.audio_bitrate {
                values.push((
                    "audio_bitrate",
                    SqliteValue::NumberOpt(audio_bitrate.map(|x| x as i64)),
                ));
            }
            if let Some(overall_bitrate) = v.overall_bitrate {
                values.push((
                    "overall_bitrate",
                    SqliteValue::NumberOpt(overall_bitrate.map(|x| x as i64)),
                ));
            }
            if let Some(sample_rate) = v.sample_rate {
                values.push((
                    "sample_rate",
                    SqliteValue::NumberOpt(sample_rate.map(|x| x as i64)),
                ));
            }
            if let Some(channels) = v.channels {
                values.push((
                    "channels",
                    SqliteValue::NumberOpt(channels.map(|x| x as i64)),
                ));
            }

            values
        })
        .collect::<Vec<_>>();

    upsert_muli(
        db,
        "track_sizes",
        &[
            "track_id",
            "ifnull(`format`, '')",
            "ifnull(`audio_bitrate`, 0)",
            "ifnull(`overall_bitrate`, 0)",
            "ifnull(`bit_depth`, 0)",
            "ifnull(`sample_rate`, 0)",
            "ifnull(`channels`, 0)",
        ],
        &values,
    )
}

pub fn get_track_size(
    db: &Connection,
    id: i32,
    quality: &PlaybackQuality,
) -> Result<Option<u64>, DbError> {
    Ok(db
        .prepare_cached(
            "
            SELECT bytes
            FROM track_sizes
            WHERE track_id=?1 AND format=?2",
        )?
        .query_map(params![id, quality.format.as_ref()], |row| {
            Ok(row.get("bytes").unwrap())
        })?
        .find_map(|row| row.ok()))
}

pub fn get_track(db: &Connection, id: i32) -> Result<Option<Track>, DbError> {
    Ok(get_tracks(db, Some(&vec![id]))?.into_iter().next())
}

pub fn get_tracks(db: &Connection, ids: Option<&Vec<i32>>) -> Result<Vec<Track>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }
    let mut query = db.prepare_cached(&format!(
        "
            SELECT tracks.*,
                albums.title as album,
                albums.blur as blur,
                albums.date_released as date_released,
                albums.date_added as date_added,
                artists.title as artist,
                artists.id as artist_id,
                albums.artwork,
                track_sizes.format,
                track_sizes.bytes,
                track_sizes.bit_depth,
                track_sizes.audio_bitrate,
                track_sizes.overall_bitrate,
                track_sizes.sample_rate,
                track_sizes.channels
            FROM tracks
            JOIN albums ON albums.id=tracks.album_id
            JOIN artists ON artists.id=albums.artist_id
            JOIN track_sizes ON tracks.id=track_sizes.track_id AND track_sizes.format=tracks.format
            {}",
        ids.map(|ids| {
            let ids_param = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            format!("WHERE tracks.id IN ({ids_param})")
        })
        .unwrap_or_default()
    ))?;

    if let Some(ids) = ids {
        let mut index = 1;
        for id in ids {
            query.raw_bind_parameter(index, *id)?;
            index += 1;
        }
    }

    Ok(query
        .raw_query()
        .mapped(|row| Ok(AsModel::as_model(row)))
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
    NowAdd(String),
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
                SqliteValue::NowAdd(add) => {
                    format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))")
                }
            }
            .as_str(),
        )
    }
}

pub fn select_distinct<T>(
    connection: &Connection,
    table_name: &str,
    filters: &Vec<(&str, SqliteValue)>,
    columns: &[&str],
) -> Result<Vec<T>, DbError>
where
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
{
    let mut statement = connection.prepare_cached(&format!(
        "SELECT DISTINCT {} FROM {table_name} {}",
        columns.join(", "),
        build_where_clause(filters),
    ))?;

    bind_values(&mut statement, filters, false)?;

    let values = AsModelResultMut::<T, DbError>::as_model_mut(&mut statement.raw_query())?;

    Ok(values)
}

pub fn select<T>(
    connection: &Connection,
    table_name: &str,
    filters: &Vec<(&str, SqliteValue)>,
    columns: &[&str],
) -> Result<Vec<T>, DbError>
where
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
{
    let mut statement = connection.prepare_cached(&format!(
        "SELECT {} FROM {table_name} {}",
        columns.join(", "),
        build_where_clause(filters),
    ))?;

    bind_values(&mut statement, filters, false)?;

    let values = AsModelResultMut::<T, DbError>::as_model_mut(&mut statement.raw_query())?;

    Ok(values)
}

pub fn delete<T>(
    connection: &Connection,
    table_name: &str,
    filters: &Vec<(&str, SqliteValue)>,
) -> Result<Vec<T>, DbError>
where
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
{
    let mut statement = connection.prepare_cached(&format!(
        "DELETE FROM {table_name} {} RETURNING *",
        build_where_clause(filters),
    ))?;

    bind_values(&mut statement, filters, false)?;

    let values = AsModelResultMut::<T, DbError>::as_model_mut(&mut statement.raw_query())?;

    Ok(values)
}

fn find_row<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    filters: &Vec<(&'a str, SqliteValue)>,
) -> Result<Option<T>, DbError>
where
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
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
            SqliteValue::NowAdd(add) => {
                format!(
                    "{name} = strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))"
                )
            }
            _ => format!("{name}=?"),
        })
        .collect()
}

fn build_set_clause<'a>(values: &'a [(&'a str, SqliteValue)]) -> String {
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
            SqliteValue::NowAdd(add) => {
                format!(
                    "{name}=strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))"
                )
            }
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
    build_values_props_offset(values, 0, false)
}

fn build_values_props_offset<'a>(
    values: &'a [(&'a str, SqliteValue)],
    offset: u16,
    constant_inc: bool,
) -> Vec<String> {
    let mut i = offset;
    let mut props = Vec::new();
    for (_name, value) in values {
        if constant_inc {
            i += 1;
        }
        props.push(match value {
            SqliteValue::StringOpt(None) => "NULL".to_string(),
            SqliteValue::NumberOpt(None) => "NULL".to_string(),
            SqliteValue::NowAdd(add) => {
                format!("strftime('%Y-%m-%dT%H:%M:%f', DateTime('now', 'LocalTime', '{add}'))")
            }
            _ => {
                if !constant_inc {
                    i += 1;
                }
                format!("?{i}").to_string()
            }
        });
    }
    props
}

fn bind_values<'a>(
    statement: &mut Statement<'_>,
    values: &'a Vec<(&'a str, SqliteValue)>,
    constant_inc: bool,
) -> Result<(), DbError> {
    let mut i = 1;
    for (_key, value) in values {
        match value {
            SqliteValue::String(value) => {
                statement.raw_bind_parameter(i, value)?;
                if !constant_inc {
                    i += 1;
                }
            }
            SqliteValue::StringOpt(Some(value)) => {
                statement.raw_bind_parameter(i, value)?;
                if !constant_inc {
                    i += 1;
                }
            }
            SqliteValue::StringOpt(None) => (),
            SqliteValue::Bool(value) => {
                statement.raw_bind_parameter(i, if *value { 1 } else { 0 })?;
                if !constant_inc {
                    i += 1;
                }
            }
            SqliteValue::Number(value) => {
                statement.raw_bind_parameter(i, *value)?;
                if !constant_inc {
                    i += 1;
                }
            }
            SqliteValue::NumberOpt(Some(value)) => {
                statement.raw_bind_parameter(i, *value)?;
                if !constant_inc {
                    i += 1;
                }
            }
            SqliteValue::NumberOpt(None) => (),
            SqliteValue::Real(value) => {
                statement.raw_bind_parameter(i, *value)?;
                if !constant_inc {
                    i += 1;
                }
            }
            SqliteValue::NowAdd(_add) => (),
        }
        if constant_inc {
            i += 1;
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
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
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
    bind_values(&mut statement, &values, false)?;
    let mut query = statement.raw_query();

    let value = query.next().transpose().ok_or(DbError::Unknown)??;

    Ok(AsModelResult::as_model(value)?)
}

fn update_and_get_row<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    id: SqliteValue,
    values: &[(&'a str, SqliteValue)],
) -> Result<Option<T>, DbError>
where
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
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

    let mut statement = connection.prepare_cached(&statement)?;
    bind_values(&mut statement, &[values, &[("id", id)]].concat(), false)?;
    let mut query = statement.raw_query();

    Ok(query
        .next()?
        .map(|row| AsModelResult::as_model(row))
        .transpose()?)
}

pub fn upsert_muli<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    unique: &[&str],
    values: &'a [Vec<(&'a str, SqliteValue)>],
) -> Result<Vec<T>, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
    T: AsId,
{
    let mut results = vec![];

    if values.is_empty() {
        return Ok(results);
    }

    let mut pos = 0;
    let mut i = 0;
    let mut last_i = i;

    for value in values {
        let count = value.len();
        if pos + count >= (i16::MAX - 1) as usize {
            results.append(&mut upsert_chunk(
                connection,
                table_name,
                unique,
                &values[last_i..i],
            )?);
            last_i = i;
            pos = 0;
        }
        i += 1;
        pos += count;
    }

    if i > last_i {
        results.append(&mut upsert_chunk(
            connection,
            table_name,
            unique,
            &values[last_i..],
        )?);
    }

    Ok(results)
}

fn upsert_chunk<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    unique: &[&str],
    values: &'a [Vec<(&'a str, SqliteValue)>],
) -> Result<Vec<T>, DbError>
where
    for<'b> Row<'b>: AsModel<T>,
    T: AsId,
{
    let first = values[0].clone();
    let expected_value_size = values[0].len();

    if let Some(bad_row) = values.iter().skip(1).find(|v| {
        v.len() != expected_value_size || v.iter().enumerate().any(|(i, c)| c.0 != first[i].0)
    }) {
        log::error!("Bad row: {bad_row:?}. Expected to match schema of first row: {first:?}");
        return Err(DbError::InvalidRequest);
    }

    let set_clause = values[0]
        .iter()
        .map(|(name, _value)| format!("`{name}` = EXCLUDED.`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let column_names = values[0]
        .iter()
        .map(|(key, _v)| format!("`{key}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let values_str_list = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "({})",
                build_values_props_offset(v, (i * expected_value_size) as u16, true).join(", ")
            )
        })
        .collect::<Vec<_>>();

    let values_str = values_str_list.join(", ");

    let unique_conflict = unique.join(", ");

    let statement = format!(
        "
        INSERT INTO {table_name} ({column_names})
        VALUES {values_str}
        ON CONFLICT({unique_conflict}) DO UPDATE
            SET {set_clause}
        RETURNING *"
    );

    let all_values = values
        .iter()
        .flat_map(|f| f.iter().cloned())
        .collect::<Vec<_>>();

    let mut statement = connection.prepare_cached(&statement)?;
    bind_values(&mut statement, &all_values, true)?;

    Ok(statement
        .raw_query()
        .mapped(|row| Ok(AsModel::as_model(row)))
        .filter_map(|row| row.ok())
        .collect())
}

pub fn upsert<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<T, DbError>
where
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
    T: AsId,
{
    match find_row(connection, table_name, &filters)? {
        Some(row) => Ok(update_and_get_row(connection, table_name, row.as_id(), &values)?.unwrap()),
        None => insert_and_get_row(connection, table_name, values),
    }
}

pub fn upsert_and_get_row<'a, T>(
    connection: &'a Connection,
    table_name: &str,
    filters: Vec<(&'a str, SqliteValue)>,
    values: Vec<(&'a str, SqliteValue)>,
) -> Result<T, DbError>
where
    for<'b> Row<'b>: AsModelResult<T, ParseError>,
    T: AsId,
    T: Debug,
{
    match find_row(connection, table_name, &filters)? {
        Some(row) => {
            let updated =
                update_and_get_row(connection, table_name, row.as_id(), &values)?.unwrap();

            let str1 = format!("{row:?}");
            let str2 = format!("{updated:?}");

            if str1 == str2 {
                log::trace!("No updates to {table_name}");
            } else {
                debug!("Changed {table_name} from {str1} to {str2}");
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
            let filters = vec![
                ("artist_id", album.get("artist_id").unwrap().clone()),
                ("title", album.get("title").unwrap().clone()),
            ];
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
    pub file: Option<String>,
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
}

pub fn add_tracks(db: &Connection, tracks: Vec<InsertTrack>) -> Result<Vec<Track>, DbError> {
    let values = tracks
        .iter()
        .map(|insert| {
            let mut values = vec![
                ("number", SqliteValue::Number(insert.track.number as i64)),
                ("duration", SqliteValue::Real(insert.track.duration)),
                ("album_id", SqliteValue::Number(insert.album_id as i64)),
                ("title", SqliteValue::String(insert.track.title.clone())),
                (
                    "format",
                    SqliteValue::String(
                        insert.track.format.unwrap_or_default().as_ref().to_string(),
                    ),
                ),
                (
                    "source",
                    SqliteValue::String(insert.track.source.as_ref().to_string()),
                ),
            ];

            if let Some(file) = &insert.file {
                values.push(("file", SqliteValue::String(file.clone())));
            }

            if let Some(qobuz_id) = &insert.qobuz_id {
                values.push(("qobuz_id", SqliteValue::Number(*qobuz_id as i64)));
            }

            if let Some(tidal_id) = &insert.tidal_id {
                values.push(("tidal_id", SqliteValue::Number(*tidal_id as i64)));
            }

            values
        })
        .collect::<Vec<_>>();

    upsert_muli(
        db,
        "tracks",
        &[
            "ifnull(`file`, '')",
            "`album_id`",
            "`title`",
            "`duration`",
            "`number`",
            "ifnull(`format`, '')",
            "`source`",
            "ifnull(`tidal_id`, 0)",
            "ifnull(`qobuz_id`, 0)",
        ],
        values.as_slice(),
    )
}
