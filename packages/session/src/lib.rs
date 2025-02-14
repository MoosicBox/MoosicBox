#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use moosicbox_audio_zone::{
    db::audio_zone_try_from_db,
    models::{AudioZone, Player},
};
use moosicbox_database::{config::ConfigDatabase, profiles::LibraryDatabase};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_models::api::ApiTrack;
use moosicbox_session_models::{
    CreateSession, PlaybackTarget, Session, SessionPlaylist, SetSessionAudioZone, UpdateSession,
};

mod db;
pub use moosicbox_session_models as models;
use thiserror::Error;

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "events")]
pub mod events;

/// # Errors
///
/// * If a database error occurs
pub async fn get_session_playlist_tracks(
    db: &LibraryDatabase,
    session_playlist_id: u64,
) -> Result<Vec<ApiTrack>, DatabaseFetchError> {
    crate::db::get_session_playlist_tracks(db, session_playlist_id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_session_playlist(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<Option<SessionPlaylist>, DatabaseFetchError> {
    crate::db::get_session_playlist(db, session_id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_session_audio_zone(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<Option<AudioZone>, DatabaseFetchError> {
    Ok(
        if let Some(zone) = crate::db::get_session_audio_zone(db, session_id).await? {
            Some(audio_zone_try_from_db(zone, db.into()).await?)
        } else {
            None
        },
    )
}

/// # Errors
///
/// * If a database error occurs
pub async fn set_session_audio_zone(
    db: &LibraryDatabase,
    set_session_audio_zone: &SetSessionAudioZone,
) -> Result<(), DatabaseFetchError> {
    crate::db::set_session_audio_zone(db, set_session_audio_zone).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_session_playing(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<bool>, DatabaseFetchError> {
    crate::db::get_session_playing(db, id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_session(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<Session>, DatabaseFetchError> {
    crate::db::get_session(db, id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_sessions(db: &LibraryDatabase) -> Result<Vec<Session>, DatabaseFetchError> {
    crate::db::get_sessions(db).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn create_session(
    db: &LibraryDatabase,
    session: &CreateSession,
) -> Result<Session, DatabaseFetchError> {
    crate::db::create_session(db, session).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn update_session(
    db: &LibraryDatabase,
    session: &UpdateSession,
) -> Result<(), DatabaseFetchError> {
    crate::db::update_session(db, session).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn delete_session(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<(), DatabaseFetchError> {
    crate::db::delete_session(db, session_id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_connections(
    db: &ConfigDatabase,
) -> Result<Vec<models::Connection>, DatabaseFetchError> {
    crate::db::get_connections(db).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn register_connection(
    db: &ConfigDatabase,
    connection: &models::RegisterConnection,
) -> Result<models::Connection, DatabaseFetchError> {
    let result = crate::db::register_connection(db, connection).await?;

    for player in &connection.players {
        create_player(db, &connection.connection_id, player).await?;
    }

    let players = get_players(db, &result.id).await?;

    Ok(models::Connection {
        id: result.id,
        name: result.name,
        created: result.created,
        updated: result.updated,
        players,
    })
}

/// # Errors
///
/// * If a database error occurs
pub async fn delete_connection(
    db: &ConfigDatabase,
    connection_id: &str,
) -> Result<(), DatabaseFetchError> {
    crate::db::delete_connection(db, connection_id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn get_players(
    db: &ConfigDatabase,
    connection_id: &str,
) -> Result<Vec<Player>, DatabaseFetchError> {
    crate::db::get_players(db, connection_id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn create_player(
    db: &ConfigDatabase,
    connection_id: &str,
    player: &models::RegisterPlayer,
) -> Result<Player, DatabaseFetchError> {
    let result = crate::db::create_player(db, connection_id, player).await?;

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("create_player updated_events", async move {
            if let Err(e) = crate::events::trigger_players_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(result)
}

#[derive(Debug, Error)]
pub enum CreatePlayersError {
    #[error(transparent)]
    Db(#[from] DatabaseFetchError),
    #[error("Invalid connection")]
    InvalidConnection,
}

/// # Errors
///
/// * If a database error occurs
pub async fn create_players(
    db: &ConfigDatabase,
    connection_id: &str,
    players: &[models::RegisterPlayer],
) -> Result<Vec<Player>, CreatePlayersError> {
    let connections = crate::db::get_connections(db).await?;
    if !connections.iter().any(|x| x.id == connection_id) {
        return Err(CreatePlayersError::InvalidConnection);
    }

    let mut results = vec![];

    for player in players {
        results.push(crate::db::create_player(db, connection_id, player).await?);
    }

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("create_players updated_events", async move {
            if let Err(e) = crate::events::trigger_players_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(results)
}

/// # Errors
///
/// * If a database error occurs
pub async fn delete_player(db: &ConfigDatabase, player_id: u64) -> Result<(), DatabaseFetchError> {
    crate::db::delete_player(db, player_id).await?;

    #[cfg(feature = "events")]
    {
        moosicbox_task::spawn("delete_player updated_events", async move {
            if let Err(e) = crate::events::trigger_players_updated_event().await {
                moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
            }
        });
    }

    Ok(())
}

/// # Errors
///
/// * If a database error occurs
pub async fn delete_session_playlist_track_by_track_id(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<ApiTrack>, DatabaseFetchError> {
    crate::db::delete_session_playlist_track_by_track_id(db, id).await
}

/// # Errors
///
/// * If a database error occurs
pub async fn delete_session_playlist_tracks_by_track_id(
    db: &LibraryDatabase,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<ApiTrack>, DatabaseFetchError> {
    crate::db::delete_session_playlist_tracks_by_track_id(db, ids).await
}

/// # Errors
///
/// * If the audio zone fails to be fetched
pub async fn update_session_audio_output_ids(
    session: &UpdateSession,
    db: &ConfigDatabase,
) -> Result<Vec<String>, DatabaseFetchError> {
    Ok(match &session.playback_target {
        PlaybackTarget::AudioZone { audio_zone_id } => {
            let Some(output) = moosicbox_audio_zone::get_zone(db, *audio_zone_id).await? else {
                return Ok(vec![]);
            };

            output
                .players
                .into_iter()
                .map(|x| x.audio_output_id)
                .collect::<Vec<_>>()
        }
        PlaybackTarget::ConnectionOutput { output_id, .. } => vec![output_id.to_owned()],
    })
}
