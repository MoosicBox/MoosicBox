use actix_web::error::ErrorInternalServerError;
use moosicbox_database::{boxed, query::*, Database, DatabaseError, DatabaseValue};
use moosicbox_json_utils::{ParseError, ToValueType as _};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug, sync::PoisonError};
use thiserror::Error;

use crate::types::{AudioFormat, PlaybackQuality};

use super::models::{
    AlbumVersionQuality, AsModelQuery as _, AsModelResultMapped as _, CreateSession, LibraryAlbum,
    LibraryArtist, LibraryTrack, Player, Session, SessionPlaylist, SessionPlaylistTrack,
    TrackApiSource, TrackSize, UpdateSession,
};

impl<T> From<PoisonError<T>> for DbError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("No row")]
    NoRow,
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
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl From<DbError> for actix_web::Error {
    fn from(err: DbError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError("Database error".to_string())
    }
}

pub async fn get_session_playlist_tracks(
    db: &dyn Database,
    session_playlist_id: i32,
) -> Result<Vec<SessionPlaylistTrack>, DbError> {
    Ok(db
        .select("session_playlist_tracks")
        .where_eq("session_playlist_id", session_playlist_id)
        .sort("id", SortDirection::Asc)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_client_id(db: &dyn Database) -> Result<Option<String>, DbError> {
    Ok(db
        .select("client_access_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .sort("updated", SortDirection::Desc)
        .execute_first(db)
        .await?
        .and_then(|row| row.get("client_id"))
        .map(|value| value.to_value_type())
        .transpose()?)
}

pub async fn get_client_access_token(
    db: &dyn Database,
) -> Result<Option<(String, String)>, DbError> {
    Ok(db
        .select("client_access_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .sort("updated", SortDirection::Desc)
        .execute_first(db)
        .await?
        .and_then(|row| {
            if let (Some(a), Some(b)) = (row.get("client_id"), row.get("token")) {
                Some((a, b))
            } else {
                None
            }
        })
        .map(|(client_id, token)| {
            Ok::<_, ParseError>((client_id.to_value_type()?, token.to_value_type()?))
        })
        .transpose()?)
}

pub async fn create_client_access_token(
    db: &dyn Database,
    client_id: &str,
    token: &str,
) -> Result<(), DbError> {
    db.upsert("client_access_tokens")
        .where_eq("token", token)
        .where_eq("client_id", client_id)
        .value("token", token)
        .value("client_id", client_id)
        .execute_first(db)
        .await?;

    Ok(())
}

pub async fn delete_magic_token(db: &dyn Database, magic_token: &str) -> Result<(), DbError> {
    db.delete("magic_tokens")
        .where_eq("magic_token", magic_token)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_credentials_from_magic_token(
    db: &dyn Database,
    magic_token: &str,
) -> Result<Option<(String, String)>, DbError> {
    if let Some((client_id, access_token)) = db
        .select("magic_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .where_eq("magic_token", magic_token)
        .execute_first(db)
        .await?
        .and_then(|row| {
            if let (Some(a), Some(b)) = (row.get("client_id"), row.get("access_token")) {
                Some((a, b))
            } else {
                None
            }
        })
        .map(|(client_id, token)| {
            Ok::<_, ParseError>((client_id.to_value_type()?, token.to_value_type()?))
        })
        .transpose()?
    {
        delete_magic_token(db, magic_token).await?;

        Ok(Some((client_id, access_token)))
    } else {
        Ok(None)
    }
}

pub async fn save_magic_token(
    db: &dyn Database,
    magic_token: &str,
    client_id: &str,
    access_token: &str,
) -> Result<(), DbError> {
    db.upsert("magic_tokens")
        .where_eq("magic_token", magic_token)
        .where_eq("access_token", access_token)
        .where_eq("client_id", client_id)
        .value("magic_token", magic_token)
        .value("access_token", access_token)
        .value("client_id", client_id)
        .value("expires", DatabaseValue::NowAdd("'+1 Day'".into()))
        .execute_first(db)
        .await?;

    Ok(())
}

pub async fn get_session_playlist(
    db: &dyn Database,
    session_id: i32,
) -> Result<Option<SessionPlaylist>, DbError> {
    if let Some(ref playlist) = db
        .select("session_playlists")
        .where_eq("id", session_id)
        .execute_first(db)
        .await?
    {
        Ok(Some(playlist.as_model_query(db).await?))
    } else {
        Ok(None)
    }
}

pub async fn get_session_active_players(
    db: &dyn Database,
    session_id: i32,
) -> Result<Vec<Player>, DbError> {
    Ok(db
        .select("active_players")
        .columns(&["players.*"])
        .join("players", "players.id=active_players.player_id")
        .where_eq("active_players.session_id", session_id)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_session_playing(db: &dyn Database, id: i32) -> Result<Option<bool>, DbError> {
    Ok(db
        .select("sessions")
        .columns(&["playing"])
        .where_eq("id", id)
        .execute_first(db)
        .await?
        .and_then(|row| row.get("playing"))
        .map(|x| x.to_value_type() as Result<Option<bool>, _>)
        .transpose()?
        .flatten())
}

pub async fn get_session(db: &dyn Database, id: i32) -> Result<Option<Session>, DbError> {
    Ok(
        if let Some(ref session) = db
            .select("sessions")
            .where_eq("id", id)
            .execute_first(db)
            .await?
        {
            Some(session.as_model_query(db).await?)
        } else {
            None
        },
    )
}

pub async fn get_sessions(db: &dyn Database) -> Result<Vec<Session>, DbError> {
    let mut sessions = vec![];

    for ref session in db.select("sessions").execute(db).await? {
        sessions.push(session.as_model_query(db).await?);
    }

    Ok(sessions)
}

pub async fn create_session(
    db: &dyn Database,
    session: &CreateSession,
) -> Result<Session, DbError> {
    let tracks = get_tracks(db, Some(&session.playlist.tracks)).await?;
    let playlist: SessionPlaylist = db
        .insert("session_playlists")
        .execute(db)
        .await?
        .to_value_type()?;

    for track in tracks {
        db.insert("session_playlist_tracks")
            .value("session_playlist_id", playlist.id)
            .value("track_id", track.id)
            .execute(db)
            .await?;
    }

    let new_session: Session = db
        .insert("sessions")
        .value("session_playlist_id", playlist.id)
        .value("name", session.name.clone())
        .execute(db)
        .await?
        .to_value_type()?;

    for player_id in &session.active_players {
        db.insert("active_players")
            .value("session_id", new_session.id)
            .value("player_id", *player_id)
            .execute(db)
            .await?;
    }

    Ok(Session {
        id: new_session.id,
        active: new_session.active,
        playing: new_session.playing,
        position: new_session.position,
        seek: new_session.seek,
        volume: new_session.volume,
        name: new_session.name,
        active_players: get_session_active_players(db, new_session.id).await?,
        playlist,
    })
}

pub async fn update_session(db: &dyn Database, session: &UpdateSession) -> Result<(), DbError> {
    if session.playlist.is_some() {
        db.delete("session_playlist_tracks")
            .where_in(
                "session_playlist_tracks.id",
                select("session_playlist_tracks")
                    .columns(&["session_playlist_tracks.id"])
                    .join(
                        "session_playlists",
                        "session_playlist_tracks.session_playlist_id=session_playlists.id",
                    )
                    .join(
                        "sessions",
                        "sessions.session_playlist_id=session_playlists.id",
                    )
                    .where_eq("sessions.id", session.session_id),
            )
            .execute(db)
            .await?;
    }

    let playlist_id = session
        .playlist
        .as_ref()
        .map(|p| p.session_playlist_id as i64);

    if let Some(tracks) = session.playlist.as_ref().map(|p| &p.tracks) {
        for track in tracks {
            db.insert("session_playlist_tracks")
                .value("session_playlist_id", playlist_id)
                .value("track_id", track.id)
                .value("type", track.r#type.as_ref())
                .value("data", track.data.clone())
                .execute(db)
                .await?;
        }
    }

    let mut values = Vec::new();

    if let Some(name) = &session.name {
        values.push(("name", DatabaseValue::String(name.clone())))
    }
    if let Some(active) = session.active {
        values.push(("active", DatabaseValue::Bool(active)))
    }
    if let Some(playing) = session.playing {
        values.push(("playing", DatabaseValue::Bool(playing)))
    }
    if let Some(position) = session.position {
        values.push(("position", DatabaseValue::Number(position as i64)))
    }
    if let Some(seek) = session.seek {
        values.push(("seek", DatabaseValue::Number(seek as i64)))
    }
    if let Some(volume) = session.volume {
        values.push(("volume", DatabaseValue::Real(volume)))
    }

    if !values.is_empty() {
        db.update("sessions")
            .where_eq("id", session.session_id)
            .values(values)
            .execute_first(db)
            .await?;
    }

    Ok(())
}

pub async fn delete_session(db: &dyn Database, session_id: i32) -> Result<(), DbError> {
    db.delete("session_playlist_tracks")
        .where_in(
            "session_playlist_tracks.id",
            select("session_playlist_tracks")
                .columns(&["session_playlist_tracks.id"])
                .join(
                    "session_playlists",
                    "session_playlist_tracks.session_playlist_id=session_playlists.id",
                )
                .join(
                    "sessions",
                    "sessions.session_playlist_id=session_playlists.id",
                )
                .where_eq("sessions.id", session_id),
        )
        .execute(db)
        .await?;

    db.delete("active_players")
        .where_eq("session_id", session_id)
        .execute(db)
        .await?
        .into_iter()
        .next()
        .ok_or(DbError::NoRow)?;

    db.delete("sessions")
        .where_eq("id", session_id)
        .execute(db)
        .await?
        .into_iter()
        .next()
        .ok_or(DbError::NoRow)?;

    db.delete("session_playlists")
        .where_eq("id", session_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_connections(db: &dyn Database) -> Result<Vec<super::models::Connection>, DbError> {
    let mut connections = vec![];

    for ref connection in db.select("connections").execute(db).await? {
        connections.push(connection.as_model_query(db).await?);
    }

    Ok(connections)
}

pub async fn register_connection(
    db: &dyn Database,
    connection: &super::models::RegisterConnection,
) -> Result<super::models::Connection, DbError> {
    let row: super::models::Connection = db
        .upsert("connections")
        .where_eq("id", connection.connection_id.clone())
        .value("id", connection.connection_id.clone())
        .value("name", connection.name.clone())
        .execute_first(db)
        .await?
        .to_value_type()?;

    for player in &connection.players {
        create_player(db, &connection.connection_id, player).await?;
    }

    Ok(super::models::Connection {
        id: row.id.clone(),
        name: row.name,
        created: row.created,
        updated: row.updated,
        players: get_players(db, &row.id).await?,
    })
}

pub async fn delete_connection(db: &dyn Database, connection_id: &str) -> Result<(), DbError> {
    db.delete("players")
        .where_in(
            "players.id",
            select("players")
                .columns(&["players.id"])
                .join("connections", "connections.id=players.connection_id")
                .where_eq("connections.id", connection_id),
        )
        .execute(db)
        .await?;

    db.delete("connections")
        .where_eq("id", connection_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_players(
    db: &dyn Database,
    connection_id: &str,
) -> Result<Vec<super::models::Player>, DbError> {
    Ok(db
        .select("players")
        .where_eq("connection_id", connection_id)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn create_player(
    db: &dyn Database,
    connection_id: &str,
    player: &super::models::RegisterPlayer,
) -> Result<super::models::Player, DbError> {
    Ok(db
        .upsert("players")
        .where_eq("connection_id", connection_id)
        .where_eq("name", player.name.clone())
        .where_eq("type", player.r#type.clone())
        .value("connection_id", connection_id)
        .value("name", player.name.clone())
        .value("type", player.r#type.clone())
        .execute_first(db)
        .await?
        .to_value_type()?)
}

pub async fn set_session_active_players(
    db: &dyn Database,
    set_session_active_players: &super::models::SetSessionActivePlayers,
) -> Result<(), DbError> {
    db.delete("active_players")
        .where_eq("session_id", set_session_active_players.session_id)
        .execute(db)
        .await?;

    for player_id in &set_session_active_players.players {
        db.insert("active_players")
            .value("session_id", set_session_active_players.session_id)
            .value("player_id", *player_id)
            .execute(db)
            .await?;
    }

    Ok(())
}

pub async fn delete_player(db: &dyn Database, player_id: i32) -> Result<(), DbError> {
    db.delete("players")
        .where_eq("id", player_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_artists(db: &dyn Database) -> Result<Vec<LibraryArtist>, DbError> {
    Ok(db.select("artists").execute(db).await?.to_value_type()?)
}

pub async fn get_albums(db: &dyn Database) -> Result<Vec<LibraryAlbum>, DbError> {
    db.select("albums")
        .distinct()
        .columns(&[
            "albums.*",
            "albums.id as album_id",
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "track_sizes.format",
            "artists.title as artist",
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "tracks.source",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .join("artists", "artists.id=albums.artist_id")
        .sort("albums.id", SortDirection::Desc)
        .where_or(boxed![
            where_not_eq("track_sizes.format", AudioFormat::Source.as_ref()),
            where_not_eq("tracks.source", TrackApiSource::Local.as_ref())
        ])
        .execute(db)
        .await?
        .as_model_mapped()
}

pub async fn get_all_album_version_qualities(
    db: &dyn Database,
    album_ids: Vec<i32>,
) -> Result<Vec<AlbumVersionQuality>, DbError> {
    let mut versions: Vec<AlbumVersionQuality> = db
        .select("albums")
        .distinct()
        .columns(&[
            "albums.id as album_id",
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "track_sizes.format",
            "tracks.source",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .where_in("albums.id", album_ids)
        .sort("albums.id", SortDirection::Desc)
        .where_or(boxed![
            where_not_eq("track_sizes.format", AudioFormat::Source.as_ref()),
            where_not_eq("tracks.source", TrackApiSource::Local.as_ref())
        ])
        .execute(db)
        .await?
        .to_value_type()?;

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

pub async fn get_album_version_qualities(
    db: &dyn Database,
    album_id: i32,
) -> Result<Vec<AlbumVersionQuality>, DbError> {
    let mut versions: Vec<AlbumVersionQuality> = db
        .select("albums")
        .distinct()
        .columns(&[
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "tracks.format",
            "tracks.source",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .where_eq("albums.id", album_id)
        .execute(db)
        .await?
        .to_value_type()?;

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

pub async fn get_artist<Id: Into<Box<dyn Expression>>>(
    db: &dyn Database,
    column: &str,
    id: Id,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .where_eq(column.to_string(), id.into())
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_artist_by_album_id(
    db: &dyn Database,
    id: u64,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .where_eq("albums.id", id)
        .join("albums", "albums.artist_id = artists.id")
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_artists_by_album_ids(
    db: &dyn Database,
    album_ids: &[i32],
) -> Result<Vec<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .distinct()
        .join("albums", "albums.artist_id = artists.id")
        .where_in("album.id", album_ids.to_vec())
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_album_artist(
    db: &dyn Database,
    album_id: i32,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.id", album_id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_tidal_album_artist(
    db: &dyn Database,
    tidal_album_id: i32,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.tidal_id", tidal_album_id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_qobuz_album_artist(
    db: &dyn Database,
    qobuz_album_id: i32,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.qobuz_id", qobuz_album_id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_album<Id: Into<Box<dyn Expression>>>(
    db: &dyn Database,
    column: &str,
    id: Id,
) -> Result<Option<LibraryAlbum>, DbError> {
    Ok(db
        .select("albums")
        .columns(&[
            "albums.*",
            "artists.title as artist",
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
        ])
        .where_eq(format!("albums.{column}"), id.into())
        .join("artists", "artists.id = albums.artist_id")
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_album_tracks(
    db: &dyn Database,
    album_id: u64,
) -> Result<Vec<LibraryTrack>, DbError> {
    Ok(db
        .select("tracks")
        .columns(&[
            "tracks.*",
            "albums.title as album",
            "albums.blur as blur",
            "albums.date_released as date_released",
            "albums.date_added as date_added",
            "artists.title as artist",
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "artists.id as artist_id",
            "albums.artwork",
            "track_sizes.format",
            "track_sizes.bytes",
            "track_sizes.bit_depth",
            "track_sizes.audio_bitrate",
            "track_sizes.overall_bitrate",
            "track_sizes.sample_rate",
            "track_sizes.channels",
        ])
        .where_eq("tracks.album_id", DatabaseValue::UNumber(album_id))
        .join("albums", "albums.id=tracks.album_id")
        .join("artists", "artists.id=albums.artist_id")
        .left_join(
            "track_sizes",
            "tracks.id=track_sizes.track_id AND track_sizes.format=tracks.format",
        )
        .sort("number", SortDirection::Asc)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_artist_albums(
    db: &dyn Database,
    artist_id: i32,
) -> Result<Vec<LibraryAlbum>, DbError> {
    db.select("albums")
        .distinct()
        .columns(&[
            "albums.*",
            "albums.id as album_id",
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "artists.title as artist",
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "tracks.format",
            "tracks.source",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .join("artists", "artists.id=albums.artist_id")
        .where_eq("albums.artist_id", artist_id)
        .sort("albums.id", SortDirection::Desc)
        .execute(db)
        .await?
        .as_model_mapped()
}

#[derive(Debug, Clone)]
pub struct SetTrackSize {
    pub track_id: i32,
    pub quality: PlaybackQuality,
    pub bytes: Option<Option<u64>>,
    pub bit_depth: Option<Option<u8>>,
    pub audio_bitrate: Option<Option<u32>>,
    pub overall_bitrate: Option<Option<u32>>,
    pub sample_rate: Option<Option<u32>>,
    pub channels: Option<Option<u8>>,
}

pub async fn set_track_size(db: &dyn Database, value: SetTrackSize) -> Result<TrackSize, DbError> {
    Ok(set_track_sizes(db, &[value])
        .await?
        .first()
        .ok_or(DbError::NoRow)?
        .clone())
}

pub async fn set_track_sizes(
    db: &dyn Database,
    values: &[SetTrackSize],
) -> Result<Vec<TrackSize>, DbError> {
    let values = values
        .iter()
        .map(|v| {
            let mut values = vec![
                ("track_id", DatabaseValue::Number(v.track_id as i64)),
                (
                    "format",
                    DatabaseValue::String(v.quality.format.as_ref().to_string()),
                ),
            ];

            if let Some(bytes) = v.bytes {
                values.push(("bytes", DatabaseValue::NumberOpt(bytes.map(|x| x as i64))));
            }
            if let Some(bit_depth) = v.bit_depth {
                values.push((
                    "bit_depth",
                    DatabaseValue::NumberOpt(bit_depth.map(|x| x as i64)),
                ));
            }
            if let Some(audio_bitrate) = v.audio_bitrate {
                values.push((
                    "audio_bitrate",
                    DatabaseValue::NumberOpt(audio_bitrate.map(|x| x as i64)),
                ));
            }
            if let Some(overall_bitrate) = v.overall_bitrate {
                values.push((
                    "overall_bitrate",
                    DatabaseValue::NumberOpt(overall_bitrate.map(|x| x as i64)),
                ));
            }
            if let Some(sample_rate) = v.sample_rate {
                values.push((
                    "sample_rate",
                    DatabaseValue::NumberOpt(sample_rate.map(|x| x as i64)),
                ));
            }
            if let Some(channels) = v.channels {
                values.push((
                    "channels",
                    DatabaseValue::NumberOpt(channels.map(|x| x as i64)),
                ));
            }

            values
        })
        .collect::<Vec<_>>();

    Ok(db
        .upsert_multi("track_sizes")
        .unique(boxed![
            identifier("track_id"),
            coalesce(boxed![identifier("format"), literal("''")]),
            coalesce(boxed![identifier("audio_bitrate"), literal("0")]),
            coalesce(boxed![identifier("overall_bitrate"), literal("0")]),
            coalesce(boxed![identifier("bit_depth"), literal("0")]),
            coalesce(boxed![identifier("sample_rate"), literal("0")]),
            coalesce(boxed![identifier("channels"), literal("0")]),
        ])
        .values(values.clone())
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_track_size(
    db: &dyn Database,
    id: u64,
    quality: &PlaybackQuality,
) -> Result<Option<u64>, DbError> {
    Ok(db
        .select("track_sizes")
        .columns(&["bytes"])
        .where_eq("track_id", id)
        .where_eq("format", quality.format.as_ref())
        .execute_first(db)
        .await?
        .and_then(|x| x.columns.first().cloned())
        .map(|(_, value)| value)
        .map(|col| col.to_value_type() as Result<Option<u64>, _>)
        .transpose()?
        .flatten())
}

pub async fn get_track(db: &dyn Database, id: u64) -> Result<Option<LibraryTrack>, DbError> {
    Ok(get_tracks(db, Some(&vec![id])).await?.into_iter().next())
}

pub async fn get_tracks(
    db: &dyn Database,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<LibraryTrack>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }

    Ok(db
        .select("tracks")
        .columns(&[
            "tracks.*",
            "albums.title as album",
            "albums.blur as blur",
            "albums.date_released as date_released",
            "albums.date_added as date_added",
            "artists.title as artist",
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "artists.id as artist_id",
            "albums.artwork",
            "track_sizes.format",
            "track_sizes.bytes",
            "track_sizes.bit_depth",
            "track_sizes.audio_bitrate",
            "track_sizes.overall_bitrate",
            "track_sizes.sample_rate",
            "track_sizes.channels",
        ])
        .filter_if_some(ids.map(|ids| where_in("tracks.id", ids.to_vec())))
        .join("albums", "albums.id=tracks.album_id")
        .join("artists", "artists.id=albums.artist_id")
        .left_join(
            "track_sizes",
            "tracks.id=track_sizes.track_id AND track_sizes.format=tracks.format",
        )
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn delete_track(db: &dyn Database, id: i32) -> Result<Option<LibraryTrack>, DbError> {
    Ok(delete_tracks(db, Some(&vec![id])).await?.into_iter().next())
}

pub async fn delete_tracks(
    db: &dyn Database,
    ids: Option<&Vec<i32>>,
) -> Result<Vec<LibraryTrack>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("tracks")
        .filter_if_some(ids.map(|ids| where_in("id", ids.to_vec())))
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn delete_track_size_by_track_id(
    db: &dyn Database,
    id: i32,
) -> Result<Option<TrackSize>, DbError> {
    Ok(delete_track_sizes_by_track_id(db, Some(&vec![id]))
        .await?
        .into_iter()
        .next())
}

pub async fn delete_track_sizes_by_track_id(
    db: &dyn Database,
    ids: Option<&Vec<i32>>,
) -> Result<Vec<TrackSize>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("track_sizes")
        .filter_if_some(ids.map(|ids| where_in("track_id", ids.to_vec())))
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn delete_session_playlist_track_by_track_id(
    db: &dyn Database,
    id: i32,
) -> Result<Option<SessionPlaylistTrack>, DbError> {
    Ok(
        delete_session_playlist_tracks_by_track_id(db, Some(&vec![id]))
            .await?
            .into_iter()
            .next(),
    )
}

pub async fn delete_session_playlist_tracks_by_track_id(
    db: &dyn Database,
    ids: Option<&Vec<i32>>,
) -> Result<Vec<SessionPlaylistTrack>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("session_playlist_tracks")
        .where_eq("type", "'LIBRARY'")
        .filter_if_some(ids.map(|ids| where_in("track_id", ids.to_vec())))
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn add_artist_and_get_artist(
    db: &dyn Database,
    artist: LibraryArtist,
) -> Result<LibraryArtist, DbError> {
    Ok(add_artists_and_get_artists(db, vec![artist]).await?[0].clone())
}

pub async fn add_artist_map_and_get_artist(
    db: &dyn Database,
    artist: HashMap<&str, DatabaseValue>,
) -> Result<LibraryArtist, DbError> {
    Ok(add_artist_maps_and_get_artists(db, vec![artist]).await?[0].clone())
}

pub async fn add_artists_and_get_artists(
    db: &dyn Database,
    artists: Vec<LibraryArtist>,
) -> Result<Vec<LibraryArtist>, DbError> {
    add_artist_maps_and_get_artists(
        db,
        artists
            .into_iter()
            .map(|artist| {
                HashMap::from([
                    ("title", DatabaseValue::String(artist.title)),
                    ("cover", DatabaseValue::StringOpt(artist.cover)),
                ])
            })
            .collect(),
    )
    .await
}

pub async fn add_artist_maps_and_get_artists(
    db: &dyn Database,
    artists: Vec<HashMap<&str, DatabaseValue>>,
) -> Result<Vec<LibraryArtist>, DbError> {
    let mut results = vec![];

    for artist in artists {
        if !artist.contains_key("title") {
            return Err(DbError::InvalidRequest);
        }

        let row: LibraryArtist = db
            .upsert("artists")
            .where_eq("title", artist.get("title").unwrap().clone())
            .values(artist.into_iter().collect::<Vec<_>>())
            .execute_first(db)
            .await?
            .to_value_type()?;

        results.push(row);
    }

    Ok(results)
}

pub async fn add_albums(
    db: &dyn Database,
    albums: Vec<LibraryAlbum>,
) -> Result<Vec<LibraryAlbum>, DbError> {
    let mut data: Vec<LibraryAlbum> = Vec::new();

    for album in albums {
        data.push(
            db.upsert("albums")
                .where_eq("artist_id", album.artist_id)
                .where_eq("title", album.title.clone())
                .where_eq("directory", album.directory.clone())
                .value("artist_id", album.artist_id)
                .value("title", album.title)
                .value("directory", album.directory)
                .value("date_released", album.date_released)
                .value("artwork", album.artwork)
                .execute_first(db)
                .await?
                .to_value_type()?,
        );
    }

    Ok(data)
}

pub async fn add_album_and_get_album(
    db: &dyn Database,
    album: LibraryAlbum,
) -> Result<LibraryAlbum, DbError> {
    Ok(add_albums_and_get_albums(db, vec![album]).await?[0].clone())
}

pub async fn add_album_map_and_get_album(
    db: &dyn Database,
    album: HashMap<&str, DatabaseValue>,
) -> Result<LibraryAlbum, DbError> {
    Ok(add_album_maps_and_get_albums(db, vec![album]).await?[0].clone())
}

pub async fn add_albums_and_get_albums(
    db: &dyn Database,
    albums: Vec<LibraryAlbum>,
) -> Result<Vec<LibraryAlbum>, DbError> {
    add_album_maps_and_get_albums(
        db,
        albums
            .into_iter()
            .map(|album| {
                HashMap::from([
                    ("artist_id", DatabaseValue::Number(album.artist_id as i64)),
                    ("title", DatabaseValue::String(album.title)),
                    (
                        "date_released",
                        DatabaseValue::StringOpt(album.date_released),
                    ),
                    ("artwork", DatabaseValue::StringOpt(album.artwork)),
                    ("directory", DatabaseValue::StringOpt(album.directory)),
                ])
            })
            .collect(),
    )
    .await
}

pub async fn add_album_maps_and_get_albums(
    db: &dyn Database,
    albums: Vec<HashMap<&str, DatabaseValue>>,
) -> Result<Vec<LibraryAlbum>, DbError> {
    let mut values = vec![];

    for album in albums {
        if !album.contains_key("artist_id") || !album.contains_key("title") {
            return Err(DbError::InvalidRequest);
        }

        let mut album_values = album.into_iter().collect::<Vec<_>>();
        album_values.sort_by(|a, b| a.0.cmp(b.0));
        values.push(album_values);
    }

    Ok(db
        .upsert_multi("albums")
        .unique(boxed![identifier("artist_id"), identifier("title"),])
        .values(values)
        .execute(db)
        .await?
        .to_value_type()?)
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct InsertTrack {
    pub track: LibraryTrack,
    pub album_id: i32,
    pub file: Option<String>,
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
}

pub async fn add_tracks(
    db: &dyn Database,
    tracks: Vec<InsertTrack>,
) -> Result<Vec<LibraryTrack>, DbError> {
    let values = tracks
        .iter()
        .map(|insert| {
            let mut values = vec![
                ("number", DatabaseValue::Number(insert.track.number as i64)),
                ("duration", DatabaseValue::Real(insert.track.duration)),
                ("album_id", DatabaseValue::Number(insert.album_id as i64)),
                ("title", DatabaseValue::String(insert.track.title.clone())),
                (
                    "format",
                    DatabaseValue::String(
                        insert.track.format.unwrap_or_default().as_ref().to_string(),
                    ),
                ),
                (
                    "source",
                    DatabaseValue::String(insert.track.source.as_ref().to_string()),
                ),
            ];

            if let Some(file) = &insert.file {
                values.push(("file", DatabaseValue::String(file.clone())));
            }

            if let Some(qobuz_id) = &insert.qobuz_id {
                values.push(("qobuz_id", DatabaseValue::Number(*qobuz_id as i64)));
            }

            if let Some(tidal_id) = &insert.tidal_id {
                values.push(("tidal_id", DatabaseValue::Number(*tidal_id as i64)));
            }

            values
        })
        .collect::<Vec<_>>();

    Ok(db
        .upsert_multi("tracks")
        .unique(boxed![
            coalesce(boxed![identifier("file"), literal("''")]),
            identifier("album_id"),
            identifier("title"),
            identifier("duration"),
            identifier("number"),
            coalesce(boxed![identifier("format"), literal("''")]),
            identifier("source"),
            coalesce(boxed![identifier("tidal_id"), literal("0")]),
            coalesce(boxed![identifier("qobuz_id"), literal("0")]),
        ])
        .values(values)
        .execute(db)
        .await?
        .to_value_type()?)
}
