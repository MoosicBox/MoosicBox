//! Session and connection management for `MoosicBox`.
//!
//! This crate provides functionality for managing playback sessions, playlists, connections,
//! and audio zones. It handles session state management, player registration, and coordination
//! between audio outputs.
//!
//! # Features
//!
//! * `api` - Enables REST API endpoints for session management
//! * `events` - Enables event notification system for player updates
//! * `openapi` - Enables `OpenAPI` documentation generation
//! * Audio format features: `aac`, `flac`, `mp3`, `opus`
//!
//! # Main Entry Points
//!
//! Session management:
//! * [`get_session`] - Retrieve a session by ID
//! * [`create_session`] - Create a new playback session
//! * [`update_session`] - Update an existing session
//! * [`delete_session`] - Delete a session
//!
//! Connection and player management:
//! * [`register_connection`] - Register a new connection with players
//! * [`create_player`] - Create a player for a connection
//! * [`get_players`] - Get all players for a connection
//!
//! # Examples
//!
//! ```rust,no_run
//! # use moosicbox_session::{get_session, models::CreateSession};
//! # use switchy_database::profiles::LibraryDatabase;
//! # async fn example(db: LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! // Retrieve a session by ID
//! let session = get_session(&db, 1).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_audio_zone::{
    db::audio_zone_try_from_db,
    models::{AudioZone, Player},
};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_models::api::ApiTrack;
use moosicbox_session_models::{
    CreateSession, PlaybackTarget, Session, SessionPlaylist, SetSessionAudioZone, UpdateSession,
};
use switchy_database::{config::ConfigDatabase, profiles::LibraryDatabase};

mod db;

/// Session data models and types.
///
/// This module re-exports types from `moosicbox_session_models`, including:
/// * [`models::Session`] - A playback session
/// * [`models::SessionPlaylist`] - A session's playlist
/// * [`models::CreateSession`] - Parameters for creating a new session
/// * [`models::UpdateSession`] - Parameters for updating a session
/// * [`models::Connection`] - A registered connection
/// * [`models::RegisterPlayer`] - Parameters for registering a player
pub use moosicbox_session_models as models;
use thiserror::Error;

/// REST API endpoints for session management.
///
/// Available when the `api` feature is enabled. Provides HTTP endpoints for
/// managing sessions, playlists, connections, and players.
#[cfg(feature = "api")]
pub mod api;

/// Event notification system for player updates.
///
/// Available when the `events` feature is enabled. Provides a subscription-based
/// event system for listening to player state changes.
#[cfg(feature = "events")]
pub mod events;

/// Retrieves all tracks in a session playlist.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_session_playlist_tracks(
    db: &LibraryDatabase,
    session_playlist_id: u64,
) -> Result<Vec<ApiTrack>, DatabaseFetchError> {
    crate::db::get_session_playlist_tracks(db, session_playlist_id).await
}

/// Retrieves the playlist associated with a session.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_session_playlist(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<Option<SessionPlaylist>, DatabaseFetchError> {
    crate::db::get_session_playlist(db, session_id).await
}

/// Retrieves the audio zone configuration for a session.
///
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

/// Sets the audio zone configuration for a session.
///
/// # Errors
///
/// * If a database error occurs
pub async fn set_session_audio_zone(
    db: &LibraryDatabase,
    set_session_audio_zone: &SetSessionAudioZone,
) -> Result<(), DatabaseFetchError> {
    crate::db::set_session_audio_zone(db, set_session_audio_zone).await
}

/// Checks whether a session is currently playing.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_session_playing(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<bool>, DatabaseFetchError> {
    crate::db::get_session_playing(db, id).await
}

/// Retrieves a session by its ID.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_session(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<Session>, DatabaseFetchError> {
    crate::db::get_session(db, id).await
}

/// Retrieves all sessions.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_sessions(db: &LibraryDatabase) -> Result<Vec<Session>, DatabaseFetchError> {
    crate::db::get_sessions(db).await
}

/// Creates a new playback session.
///
/// # Errors
///
/// * If a database error occurs
pub async fn create_session(
    db: &LibraryDatabase,
    session: &CreateSession,
) -> Result<Session, DatabaseFetchError> {
    crate::db::create_session(db, session).await
}

/// Updates an existing session with new settings.
///
/// # Errors
///
/// * If a database error occurs
pub async fn update_session(
    db: &LibraryDatabase,
    session: &UpdateSession,
) -> Result<(), DatabaseFetchError> {
    crate::db::update_session(db, session).await
}

/// Deletes a session by its ID.
///
/// # Errors
///
/// * If a database error occurs
pub async fn delete_session(
    db: &LibraryDatabase,
    session_id: u64,
) -> Result<(), DatabaseFetchError> {
    crate::db::delete_session(db, session_id).await
}

/// Retrieves all registered connections.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_connections(
    db: &ConfigDatabase,
) -> Result<Vec<models::Connection>, DatabaseFetchError> {
    crate::db::get_connections(db).await
}

/// Registers a new connection with its associated players.
///
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

/// Deletes a connection by its ID.
///
/// # Errors
///
/// * If a database error occurs
pub async fn delete_connection(
    db: &ConfigDatabase,
    connection_id: &str,
) -> Result<(), DatabaseFetchError> {
    crate::db::delete_connection(db, connection_id).await
}

/// Retrieves all players associated with a connection.
///
/// # Errors
///
/// * If a database error occurs
pub async fn get_players(
    db: &ConfigDatabase,
    connection_id: &str,
) -> Result<Vec<Player>, DatabaseFetchError> {
    crate::db::get_players(db, connection_id).await
}

/// Creates a new player for a connection.
///
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
        switchy_async::runtime::Handle::current().spawn_with_name(
            "create_player updated_events",
            async move {
                if let Err(e) = crate::events::trigger_players_updated_event().await {
                    moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
                }
            },
        );
    }

    Ok(result)
}

/// Error type for player creation operations.
#[derive(Debug, Error)]
pub enum CreatePlayersError {
    /// Database operation failed.
    #[error(transparent)]
    Db(#[from] DatabaseFetchError),
    /// The specified connection ID does not exist.
    #[error("Invalid connection")]
    InvalidConnection,
}

/// Creates multiple players for a connection.
///
/// # Errors
///
/// * If a database error occurs
/// * If the specified connection does not exist
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
        switchy_async::runtime::Handle::current().spawn_with_name(
            "create_players updated_events",
            async move {
                if let Err(e) = crate::events::trigger_players_updated_event().await {
                    moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
                }
            },
        );
    }

    Ok(results)
}

/// Deletes a player by its ID.
///
/// # Errors
///
/// * If a database error occurs
pub async fn delete_player(db: &ConfigDatabase, player_id: u64) -> Result<(), DatabaseFetchError> {
    crate::db::delete_player(db, player_id).await?;

    #[cfg(feature = "events")]
    {
        switchy_async::runtime::Handle::current().spawn_with_name(
            "delete_player updated_events",
            async move {
                if let Err(e) = crate::events::trigger_players_updated_event().await {
                    moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
                }
            },
        );
    }

    Ok(())
}

/// Deletes a track from a session playlist by track ID.
///
/// # Errors
///
/// * If a database error occurs
pub async fn delete_session_playlist_track_by_track_id(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<ApiTrack>, DatabaseFetchError> {
    crate::db::delete_session_playlist_track_by_track_id(db, id).await
}

/// Deletes multiple tracks from session playlists by track IDs.
///
/// # Errors
///
/// * If a database error occurs
pub async fn delete_session_playlist_tracks_by_track_id(
    db: &LibraryDatabase,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<ApiTrack>, DatabaseFetchError> {
    crate::db::delete_session_playlist_tracks_by_track_id(db, ids).await
}

/// Updates the audio output IDs for a session based on its playback target.
///
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_create_players_error_display() {
        let err = CreatePlayersError::InvalidConnection;
        assert_eq!(err.to_string(), "Invalid connection");
    }

    #[test_log::test]
    fn test_create_players_error_db_variant() {
        let db_err = DatabaseFetchError::InvalidRequest;
        let err = CreatePlayersError::Db(db_err);
        // Just verify it's the Db variant and displays something
        assert!(matches!(err, CreatePlayersError::Db(_)));
    }

    #[test_log::test]
    fn test_create_players_error_from_database_error() {
        let db_err = DatabaseFetchError::InvalidRequest;
        let err: CreatePlayersError = db_err.into();
        assert!(matches!(err, CreatePlayersError::Db(_)));
    }
}
