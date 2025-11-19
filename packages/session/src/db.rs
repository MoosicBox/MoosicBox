//! Database operations for session management.
//!
//! This module contains internal database query implementations for session, playlist,
//! connection, and player operations. All functions in this module are private to the
//! crate and are exposed through the public API in the parent module.
//!
//! The database layer handles:
//! * Session CRUD operations
//! * Playlist and track management
//! * Connection and player registration
//! * Audio zone associations

use std::sync::Arc;

use moosicbox_audio_zone::{db::models::AudioZoneModel, models::Player};
use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::{DatabaseFetchError, ToValue as _},
};
use moosicbox_library::db::get_tracks;
use moosicbox_music_models::{api::ApiTrack, id::Id};
use moosicbox_session_models::Connection;
use switchy_database::{
    Database, DatabaseValue,
    config::ConfigDatabase,
    profiles::LibraryDatabase,
    query::{FilterableQuery as _, SortDirection, select, where_in},
};

use crate::models::{
    self, CreateSession, PlaybackTarget, Session, SessionPlaylist, SetSessionAudioZone,
    UpdateSession,
};

pub async fn get_session_playlist_tracks(
    db: &LibraryDatabase,
    session_playlist_id: u64,
) -> Result<Vec<ApiTrack>, DatabaseFetchError> {
    db.select("session_playlist_tracks")
        .where_eq("session_playlist_id", session_playlist_id)
        .sort("id", SortDirection::Asc)
        .execute(&**db)
        .await?
        .into_iter()
        .filter_map(|x| x.get("data"))
        .filter_map(|x| {
            x.as_str().map(serde_json::from_str).map(|x| {
                x.map_err(|e| DatabaseFetchError::Parse(ParseError::Parse(format!("data: {e:?}"))))
            })
        })
        .collect::<Result<Vec<_>, _>>()
}

pub async fn get_session_playlist(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<Option<SessionPlaylist>, DatabaseFetchError> {
    if let Some(playlist) = &db
        .select("session_playlists")
        .where_eq("id", session_id)
        .execute_first(&**db)
        .await?
    {
        Ok(Some(
            session_playlist_as_model_query(playlist, db.into()).await?,
        ))
    } else {
        Ok(None)
    }
}

pub async fn get_session_audio_zone(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<Option<AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .select("audio_zones")
        .columns(&["audio_zones.*"])
        .join(
            "audio_zone_sessions",
            "audio_zones.id=audio_zone_sessions.audio_zone_id",
        )
        .where_eq("audio_zone_sessions.session_id", session_id)
        .execute_first(&**db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_session_playing(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<bool>, DatabaseFetchError> {
    Ok(db
        .select("sessions")
        .columns(&["playing"])
        .where_eq("id", id)
        .execute_first(&**db)
        .await?
        .and_then(|row| row.get("playing"))
        .map(|x| x.to_value_type() as Result<Option<bool>, _>)
        .transpose()?
        .flatten())
}

pub async fn get_session(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<Session>, DatabaseFetchError> {
    Ok(
        if let Some(session) = &db
            .select("sessions")
            .where_eq("id", id)
            .execute_first(&**db)
            .await?
        {
            Some(session_as_model_query(session, db.into()).await?)
        } else {
            None
        },
    )
}

pub async fn get_sessions(db: &LibraryDatabase) -> Result<Vec<Session>, DatabaseFetchError> {
    let mut sessions = vec![];

    for session in &db.select("sessions").execute(&**db).await? {
        sessions.push(session_as_model_query(session, db.into()).await?);
    }

    Ok(sessions)
}

pub async fn create_session(
    db: &LibraryDatabase,
    session: &CreateSession,
) -> Result<Session, DatabaseFetchError> {
    let tracks = get_tracks(
        db,
        Some(
            &session
                .playlist
                .tracks
                .iter()
                .map(Into::into)
                .collect::<Vec<Id>>(),
        ),
    )
    .await?;
    let playlist: SessionPlaylist = db
        .insert("session_playlists")
        .execute(&**db)
        .await?
        .to_value_type()?;

    for track in tracks {
        db.insert("session_playlist_tracks")
            .value("session_playlist_id", playlist.id)
            .value("track_id", track.id)
            .execute(&**db)
            .await?;
    }

    let new_session: Session = db
        .insert("sessions")
        .value("session_playlist_id", playlist.id)
        .value("name", session.name.clone())
        .value("audio_zone_id", session.audio_zone_id)
        .execute(&**db)
        .await?
        .to_value_type()?;

    if let Some(id) = session.audio_zone_id {
        db.insert("audio_zone_sessions")
            .value("session_id", new_session.id)
            .value("audio_zone_id", id)
            .execute(&**db)
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
        playback_target: session
            .audio_zone_id
            .map(|audio_zone_id| PlaybackTarget::AudioZone { audio_zone_id }),
        playlist,
    })
}

pub async fn update_session(
    db: &LibraryDatabase,
    session: &UpdateSession,
) -> Result<(), DatabaseFetchError> {
    if session.playlist.is_some() {
        log::trace!("update_session: Deleting existing session_playlist_tracks");
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
            .execute(&**db)
            .await?;
    } else {
        log::trace!("update_session: No playlist");
    }

    let playlist_id = session
        .playlist
        .as_ref()
        .map(|p| i64::try_from(p.session_playlist_id).unwrap());

    if let Some(tracks) = session.playlist.as_ref().map(|p| &p.tracks) {
        log::trace!("update_session: Inserting new tracks");
        for track in tracks {
            log::trace!("update_session: Inserting track {track:?}");
            db.insert("session_playlist_tracks")
                .value("session_playlist_id", playlist_id)
                .value("track_id", &track.track_id)
                .value("type", track.api_source.to_string())
                .value(
                    "data",
                    serde_json::to_string(track).map_err(|e| {
                        DatabaseFetchError::Parse(ParseError::Parse(format!("data: {e:?}")))
                    })?,
                )
                .execute(&**db)
                .await?;
        }
    } else {
        log::trace!("update_session: No tracks to insert");
    }

    let mut values = vec![(
        "playback_target",
        DatabaseValue::String(session.playback_target.as_ref().to_string()),
    )];

    match &session.playback_target {
        PlaybackTarget::AudioZone { audio_zone_id } => {
            values.push(("audio_zone_id", DatabaseValue::UInt64(*audio_zone_id)));
        }
        PlaybackTarget::ConnectionOutput {
            connection_id,
            output_id,
        } => {
            values.push((
                "connection_id",
                DatabaseValue::String(connection_id.to_owned()),
            ));
            values.push(("output_id", DatabaseValue::String(output_id.to_owned())));
        }
    }

    if let Some(name) = &session.name {
        values.push(("name", DatabaseValue::String(name.clone())));
    }
    if let Some(active) = session.active {
        values.push(("active", DatabaseValue::Bool(active)));
    }
    if let Some(playing) = session.playing {
        values.push(("playing", DatabaseValue::Bool(playing)));
    }
    if let Some(position) = session.position {
        values.push(("position", DatabaseValue::Int64(i64::from(position))));
    }
    if let Some(seek) = session.seek {
        #[allow(clippy::cast_possible_truncation)]
        values.push(("seek", DatabaseValue::Int64(seek as i64)));
    }
    if let Some(volume) = session.volume {
        values.push(("volume", DatabaseValue::Real64(volume)));
    }

    if values.is_empty() {
        log::trace!("update_session: No values to update on the session");
    } else {
        log::trace!("update_session: Updating session values values={values:?}");
        db.update("sessions")
            .where_eq("id", session.session_id)
            .values(values)
            .execute_first(&**db)
            .await?;
    }

    log::trace!("update_session: Finished updating session");
    Ok(())
}

pub async fn delete_session(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<(), DatabaseFetchError> {
    log::debug!("Deleting session_playlist_tracks for session_id={session_id}");
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
        .execute(&**db)
        .await?;

    log::debug!("Deleting active_players for session_id={session_id}");
    db.delete("active_players")
        .where_eq("session_id", session_id)
        .execute(&**db)
        .await?;

    log::debug!("Deleting audio_zone_sessions for session_id={session_id}");
    db.delete("audio_zone_sessions")
        .where_eq("session_id", session_id)
        .execute(&**db)
        .await?;

    log::debug!("Deleting session for session_id={session_id}");
    db.delete("sessions")
        .where_eq("id", session_id)
        .execute(&**db)
        .await?
        .into_iter()
        .next()
        .ok_or(DatabaseFetchError::InvalidRequest)?;

    log::debug!("Deleting session_playlists for session_id={session_id}");
    db.delete("session_playlists")
        .where_eq("id", session_id)
        .execute(&**db)
        .await?;

    Ok(())
}

pub async fn get_connections(
    db: &ConfigDatabase,
) -> Result<Vec<models::Connection>, DatabaseFetchError> {
    let mut connections = vec![];

    for connection in &db.select("connections").execute(&**db).await? {
        connections.push(connection_as_model_query(connection, db.into()).await?);
    }

    Ok(connections)
}

pub async fn register_connection(
    db: &ConfigDatabase,
    connection: &models::RegisterConnection,
) -> Result<models::Connection, DatabaseFetchError> {
    let row: models::Connection = db
        .upsert("connections")
        .where_eq("id", connection.connection_id.clone())
        .value("id", connection.connection_id.clone())
        .value("name", connection.name.clone())
        .execute_first(&**db)
        .await?
        .to_value_type()?;

    Ok(models::Connection {
        id: row.id.clone(),
        name: row.name,
        created: row.created,
        updated: row.updated,
        players: get_players(db, &row.id).await?,
    })
}

pub async fn delete_connection(
    db: &ConfigDatabase,
    connection_id: &str,
) -> Result<(), DatabaseFetchError> {
    db.delete("players")
        .where_in(
            "players.id",
            select("players")
                .columns(&["players.id"])
                .join("connections", "connections.id=players.connection_id")
                .where_eq("connections.id", connection_id),
        )
        .execute(&**db)
        .await?;

    db.delete("connections")
        .where_eq("id", connection_id)
        .execute(&**db)
        .await?;

    Ok(())
}

pub async fn get_players(
    db: &ConfigDatabase,
    connection_id: &str,
) -> Result<Vec<Player>, DatabaseFetchError> {
    Ok(db
        .select("players")
        .where_eq("connection_id", connection_id)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

pub async fn create_player(
    db: &ConfigDatabase,
    connection_id: &str,
    player: &models::RegisterPlayer,
) -> Result<Player, DatabaseFetchError> {
    Ok(db
        .upsert("players")
        .where_eq("connection_id", connection_id)
        .where_eq("audio_output_id", &player.audio_output_id)
        .where_eq("name", &player.name)
        .value("connection_id", connection_id)
        .value("name", &player.name)
        .value("audio_output_id", &player.audio_output_id)
        .execute_first(&**db)
        .await?
        .to_value_type()?)
}

pub async fn set_session_audio_zone(
    db: &LibraryDatabase,
    set_session_audio_zone: &SetSessionAudioZone,
) -> Result<(), DatabaseFetchError> {
    db.delete("audio_zone_sessions")
        .where_eq("session_id", set_session_audio_zone.session_id)
        .execute(&**db)
        .await?;

    db.insert("audio_zone_sessions")
        .value("session_id", set_session_audio_zone.session_id)
        .value("audio_zone_id", set_session_audio_zone.audio_zone_id)
        .execute(&**db)
        .await?;

    Ok(())
}

pub async fn delete_player(db: &ConfigDatabase, player_id: u64) -> Result<(), DatabaseFetchError> {
    db.delete("players")
        .where_eq("id", player_id)
        .execute(&**db)
        .await?;

    Ok(())
}

pub async fn delete_session_playlist_track_by_track_id(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<ApiTrack>, DatabaseFetchError> {
    Ok(
        delete_session_playlist_tracks_by_track_id(db, Some(&vec![id]))
            .await?
            .into_iter()
            .next(),
    )
}

pub async fn delete_session_playlist_tracks_by_track_id(
    db: &LibraryDatabase,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<ApiTrack>, DatabaseFetchError> {
    if ids.is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    db.delete("session_playlist_tracks")
        .where_eq("type", "'Library'")
        .filter_if_some(ids.map(|ids| where_in("track_id", ids.clone())))
        .execute(&**db)
        .await?
        .into_iter()
        .filter_map(|x| x.get("data"))
        .filter_map(|x| {
            x.as_str().map(serde_json::from_str).map(|x| {
                x.map_err(|e| DatabaseFetchError::Parse(ParseError::Parse(format!("data: {e:?}"))))
            })
        })
        .collect::<Result<Vec<_>, _>>()
}

async fn connection_as_model_query(
    row: &switchy_database::Row,
    db: Arc<Box<dyn Database>>,
) -> Result<Connection, DatabaseFetchError> {
    let id = row.to_value::<String>("id")?;
    let players = get_players(&db.clone().into(), &id).await?;
    Ok(Connection {
        id,
        name: row.to_value("name")?,
        created: row.to_value("created")?,
        updated: row.to_value("updated")?,
        players,
    })
}

async fn session_as_model_query(
    row: &switchy_database::Row,
    db: Arc<Box<dyn Database>>,
) -> Result<Session, DatabaseFetchError> {
    let id = row.to_value("id")?;
    let playback_target_type: Option<String> = row.to_value("playback_target")?;
    let playback_target_type =
        playback_target_type.and_then(|x| PlaybackTarget::default_from_str(&x));

    match get_session_playlist(&db.into(), id).await? {
        Some(playlist) => Ok(Session {
            id,
            name: row.to_value("name")?,
            active: row.to_value("active")?,
            playing: row.to_value("playing")?,
            position: row.to_value("position")?,
            #[allow(clippy::cast_precision_loss)]
            seek: row.to_value::<Option<i64>>("seek")?.map(|x| x as f64),
            volume: row.to_value("volume")?,
            playback_target: match playback_target_type {
                Some(PlaybackTarget::AudioZone { .. }) => Some(PlaybackTarget::AudioZone {
                    audio_zone_id: row.to_value("audio_zone_id")?,
                }),
                Some(PlaybackTarget::ConnectionOutput { .. }) => {
                    Some(PlaybackTarget::ConnectionOutput {
                        connection_id: row.to_value("connection_id")?,
                        output_id: row.to_value("output_id")?,
                    })
                }
                None => None,
            },
            playlist,
        }),
        None => Err(DatabaseFetchError::InvalidRequest),
    }
}

async fn session_playlist_as_model_query(
    row: &switchy_database::Row,
    db: Arc<Box<dyn Database>>,
) -> Result<SessionPlaylist, DatabaseFetchError> {
    let id = row.to_value("id")?;
    let tracks = get_session_playlist_tracks(&db.clone().into(), id).await?;
    log::trace!("Got SessionPlaylistTracks for session_playlist {id}: {tracks:?}");

    Ok(SessionPlaylist { id, tracks })
}
