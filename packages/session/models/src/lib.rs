#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![doc = include_str!("../README.md")]

use std::sync::LazyLock;

use moosicbox_audio_zone_models::{ApiPlayer, Player};
use moosicbox_json_utils::{ParseError, ToValueType, database::ToValue as _};
use moosicbox_music_models::{PlaybackQuality, api::ApiTrack};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use switchy_database::{AsId, DatabaseValue};

/// Request to associate a session with an audio zone.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionAudioZone {
    /// The session ID to update.
    pub session_id: u64,
    /// The audio zone ID to associate with the session.
    pub audio_zone_id: u64,
}

/// Request to create a new playback session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateSession {
    /// The name of the session.
    pub name: String,
    /// Optional audio zone ID for the session.
    pub audio_zone_id: Option<u64>,
    /// The playlist configuration.
    pub playlist: CreateSessionPlaylist,
}

/// Playlist configuration for creating a session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionPlaylist {
    /// Track IDs to include in the playlist.
    pub tracks: Vec<u64>,
}

/// Target destination for playback output.
#[derive(Debug, Serialize, Deserialize, Clone, EnumString, AsRefStr, PartialEq, Eq)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum PlaybackTarget {
    /// Audio zone playback target.
    #[serde(rename_all = "camelCase")]
    AudioZone {
        /// The audio zone ID.
        audio_zone_id: u64,
    },
    /// Connection-specific output target.
    #[serde(rename_all = "camelCase")]
    ConnectionOutput {
        /// The connection ID.
        connection_id: String,
        /// The output ID within the connection.
        output_id: String,
    },
}

const DEFAULT_AUDIO_ZONE: PlaybackTarget = PlaybackTarget::AudioZone { audio_zone_id: 0 };
static DEFAULT_CONNECTION_OUTPUT: LazyLock<PlaybackTarget> =
    LazyLock::new(|| PlaybackTarget::ConnectionOutput {
        connection_id: String::new(),
        output_id: String::new(),
    });

impl PlaybackTarget {
    /// Returns a default playback target based on the type string.
    ///
    /// Returns `Some(PlaybackTarget)` for recognized type strings, or `None` if the type is unknown.
    #[must_use]
    pub fn default_from_str(r#type: &str) -> Option<Self> {
        if DEFAULT_AUDIO_ZONE.as_ref() == r#type {
            Some(DEFAULT_AUDIO_ZONE)
        } else if DEFAULT_CONNECTION_OUTPUT.as_ref() == r#type {
            Some(DEFAULT_CONNECTION_OUTPUT.clone())
        } else {
            None
        }
    }
}

/// Default implementation for `PlaybackTarget`.
impl Default for PlaybackTarget {
    /// Returns the default playback target (`AudioZone` with ID 0).
    fn default() -> Self {
        Self::AudioZone { audio_zone_id: 0 }
    }
}

/// Converts an `ApiPlaybackTarget` to a `PlaybackTarget`.
impl From<ApiPlaybackTarget> for PlaybackTarget {
    /// Converts an `ApiPlaybackTarget` into its internal representation.
    fn from(value: ApiPlaybackTarget) -> Self {
        match value {
            ApiPlaybackTarget::AudioZone { audio_zone_id } => Self::AudioZone { audio_zone_id },
            ApiPlaybackTarget::ConnectionOutput {
                connection_id,
                output_id,
            } => Self::ConnectionOutput {
                connection_id,
                output_id,
            },
        }
    }
}

/// Request to update an existing playback session.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSession {
    /// The session ID to update.
    pub session_id: u64,
    /// The playback profile.
    pub profile: String,
    /// The playback target destination.
    pub playback_target: PlaybackTarget,
    /// Whether to start playback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    /// Whether to stop playback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<bool>,
    /// New name for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the session is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// Whether playback is currently playing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    /// Playlist position index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u16>,
    /// Seek position in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<f64>,
    /// Volume level (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    /// Updated playlist data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<UpdateSessionPlaylist>,
    /// Playback quality setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<PlaybackQuality>,
}

impl UpdateSession {
    /// Checks if any playback-related fields have been updated.
    ///
    /// Returns `true` if any playback field (play, stop, active, playing, position, volume, seek, or playlist) is set.
    #[must_use]
    pub const fn playback_updated(&self) -> bool {
        self.play.is_some()
            || self.stop.is_some()
            || self.active.is_some()
            || self.playing.is_some()
            || self.position.is_some()
            || self.volume.is_some()
            || self.seek.is_some()
            || self.playlist.is_some()
    }
}

/// Converts an `ApiUpdateSession` to an `UpdateSession`.
impl From<ApiUpdateSession> for UpdateSession {
    /// Converts an `ApiUpdateSession` into its internal representation.
    fn from(value: ApiUpdateSession) -> Self {
        Self {
            session_id: value.session_id,
            profile: value.profile,
            playback_target: value.playback_target.into(),
            play: value.play,
            stop: value.stop,
            name: value.name,
            active: value.active,
            playing: value.playing,
            position: value.position,
            seek: value.seek,
            volume: value.volume,
            playlist: value.playlist.map(Into::into),
            quality: value.quality,
        }
    }
}

/// Converts an `UpdateSession` to an `ApiUpdateSession`.
impl From<UpdateSession> for ApiUpdateSession {
    /// Converts an `UpdateSession` into its API representation.
    fn from(value: UpdateSession) -> Self {
        Self {
            session_id: value.session_id,
            profile: value.profile,
            playback_target: value.playback_target.into(),
            play: value.play,
            stop: value.stop,
            name: value.name,
            active: value.active,
            playing: value.playing,
            position: value.position,
            seek: value.seek,
            volume: value.volume,
            playlist: value.playlist.map(Into::into),
            quality: value.quality,
        }
    }
}

/// Updated playlist data for a session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPlaylist {
    /// The session playlist ID.
    pub session_playlist_id: u64,
    /// The updated track list.
    pub tracks: Vec<ApiTrack>,
}

/// Converts an `UpdateSessionPlaylist` to an `ApiUpdateSessionPlaylist`.
impl From<UpdateSessionPlaylist> for ApiUpdateSessionPlaylist {
    /// Converts an `UpdateSessionPlaylist` into its API representation.
    fn from(value: UpdateSessionPlaylist) -> Self {
        Self {
            session_playlist_id: value.session_playlist_id,
            tracks: value.tracks,
        }
    }
}

/// Converts an `ApiUpdateSessionPlaylist` to an `UpdateSessionPlaylist`.
impl From<ApiUpdateSessionPlaylist> for UpdateSessionPlaylist {
    /// Converts an `ApiUpdateSessionPlaylist` into its internal representation.
    fn from(value: ApiUpdateSessionPlaylist) -> Self {
        Self {
            session_playlist_id: value.session_playlist_id,
            tracks: value.tracks,
        }
    }
}

/// API representation of playback target destination.
#[derive(Debug, Serialize, Deserialize, Clone, EnumString, AsRefStr, PartialEq, Eq)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiPlaybackTarget {
    /// Audio zone playback target.
    #[serde(rename_all = "camelCase")]
    AudioZone {
        /// The audio zone ID.
        audio_zone_id: u64,
    },
    /// Connection-specific output target.
    #[serde(rename_all = "camelCase")]
    ConnectionOutput {
        /// The connection ID.
        connection_id: String,
        /// The output ID within the connection.
        output_id: String,
    },
}

/// Default implementation for `ApiPlaybackTarget`.
impl Default for ApiPlaybackTarget {
    /// Returns the default API playback target (`AudioZone` with ID 0).
    fn default() -> Self {
        Self::AudioZone { audio_zone_id: 0 }
    }
}

/// Converts a `PlaybackTarget` to an `ApiPlaybackTarget`.
impl From<PlaybackTarget> for ApiPlaybackTarget {
    /// Converts a `PlaybackTarget` into its API representation.
    fn from(value: PlaybackTarget) -> Self {
        match value {
            PlaybackTarget::AudioZone { audio_zone_id } => Self::AudioZone { audio_zone_id },
            PlaybackTarget::ConnectionOutput {
                connection_id,
                output_id,
            } => Self::ConnectionOutput {
                connection_id,
                output_id,
            },
        }
    }
}

/// API request to update an existing playback session.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSession {
    /// The session ID to update.
    pub session_id: u64,
    /// The playback profile.
    pub profile: String,
    /// The playback target destination.
    pub playback_target: ApiPlaybackTarget,
    /// Whether to start playback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    /// Whether to stop playback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<bool>,
    /// New name for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the session is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    /// Whether playback is currently playing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    /// Playlist position index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u16>,
    /// Seek position in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<f64>,
    /// Volume level (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    /// Updated playlist data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<ApiUpdateSessionPlaylist>,
    /// Playback quality setting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<PlaybackQuality>,
}

/// API representation of updated playlist data for a session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSessionPlaylist {
    /// The session playlist ID.
    pub session_playlist_id: u64,
    /// The updated track list.
    pub tracks: Vec<ApiTrack>,
}

/// Request to delete a playback session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSession {
    /// The session ID to delete.
    pub session_id: u64,
}

/// A playback session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// The session ID.
    pub id: u64,
    /// The session name.
    pub name: String,
    /// Whether the session is active.
    pub active: bool,
    /// Whether playback is currently playing.
    pub playing: bool,
    /// Current playlist position index.
    pub position: Option<u16>,
    /// Current seek position in seconds.
    pub seek: Option<f64>,
    /// Current volume level (0.0 to 1.0).
    pub volume: Option<f64>,
    /// The playback target destination.
    pub playback_target: Option<PlaybackTarget>,
    /// The session's playlist.
    pub playlist: SessionPlaylist,
}

/// Converts a database row into a `Session`.
impl ToValueType<Session> for &switchy_database::Row {
    /// Converts this database row into a `Session`.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * Required database columns are missing
    /// * Database column values have invalid or incompatible types
    fn to_value_type(self) -> Result<Session, ParseError> {
        let playback_target_type: Option<String> = self.to_value("playback_target")?;
        let playback_target_type =
            playback_target_type.and_then(|x| PlaybackTarget::default_from_str(&x));

        Ok(Session {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            active: self.to_value("active")?,
            playing: self.to_value("playing")?,
            position: self.to_value("position")?,
            #[allow(clippy::cast_precision_loss)]
            seek: self.to_value::<Option<i64>>("seek")?.map(|x| x as f64),
            volume: self.to_value("volume")?,
            playback_target: match playback_target_type {
                Some(PlaybackTarget::AudioZone { .. }) => Some(PlaybackTarget::AudioZone {
                    audio_zone_id: self.to_value("audio_zone_id")?,
                }),
                Some(PlaybackTarget::ConnectionOutput { .. }) => {
                    Some(PlaybackTarget::ConnectionOutput {
                        connection_id: self.to_value("connection_id")?,
                        output_id: self.to_value("output_id")?,
                    })
                }
                None => None,
            },
            ..Default::default()
        })
    }
}

/// Converts a `Session` ID to a database value.
impl AsId for Session {
    /// Returns the session ID as a database value.
    fn as_id(&self) -> DatabaseValue {
        #[allow(clippy::cast_possible_wrap)]
        DatabaseValue::Int64(self.id as i64)
    }
}

/// API representation of a playback session.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSession {
    /// The session ID.
    pub session_id: u64,
    /// The session name.
    pub name: String,
    /// Whether the session is active.
    pub active: bool,
    /// Whether playback is currently playing.
    pub playing: bool,
    /// Current playlist position index.
    pub position: Option<u16>,
    /// Current seek position in seconds.
    pub seek: Option<f64>,
    /// Current volume level (0.0 to 1.0).
    pub volume: Option<f64>,
    /// The playback target destination.
    pub playback_target: Option<PlaybackTarget>,
    /// The session's playlist.
    pub playlist: ApiSessionPlaylist,
}

/// Converts a `Session` to an `ApiSession`.
impl From<Session> for ApiSession {
    /// Converts a `Session` into its API representation.
    fn from(value: Session) -> Self {
        Self {
            session_id: value.id,
            name: value.name,
            active: value.active,
            playing: value.playing,
            position: value.position,
            seek: value.seek,
            volume: value.volume,
            playback_target: value.playback_target,
            playlist: value.playlist.into(),
        }
    }
}

/// A session's playlist.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlaylist {
    /// The playlist ID.
    pub id: u64,
    /// The tracks in the playlist.
    pub tracks: Vec<ApiTrack>,
}

/// Converts a database row into a `SessionPlaylist`.
impl ToValueType<SessionPlaylist> for &switchy_database::Row {
    /// Converts this database row into a `SessionPlaylist`.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * Required database columns are missing
    /// * Database column values have invalid or incompatible types
    fn to_value_type(self) -> Result<SessionPlaylist, ParseError> {
        Ok(SessionPlaylist {
            id: self.to_value("id")?,
            ..Default::default()
        })
    }
}

/// Wrapper for session playlist tracks.
#[derive(Debug)]
pub struct SessionPlaylistTracks(
    /// The list of tracks in the session playlist.
    pub Vec<ApiTrack>,
);

/// Converts a `SessionPlaylist` ID to a database value.
impl AsId for SessionPlaylist {
    /// Returns the session playlist ID as a database value.
    fn as_id(&self) -> DatabaseValue {
        #[allow(clippy::cast_possible_wrap)]
        DatabaseValue::Int64(self.id as i64)
    }
}

/// API representation of a session's playlist.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSessionPlaylist {
    /// The session playlist ID.
    pub session_playlist_id: u64,
    /// The tracks in the playlist.
    pub tracks: Vec<ApiTrack>,
}

/// Converts a `SessionPlaylist` to an `ApiSessionPlaylist`.
impl From<SessionPlaylist> for ApiSessionPlaylist {
    /// Converts a `SessionPlaylist` into its API representation.
    fn from(value: SessionPlaylist) -> Self {
        Self {
            session_playlist_id: value.id,
            tracks: value.tracks,
        }
    }
}

/// Request to register a new connection.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegisterConnection {
    /// The connection ID.
    pub connection_id: String,
    /// The connection name.
    pub name: String,
    /// Players available in this connection.
    pub players: Vec<RegisterPlayer>,
}

/// A client connection.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// The connection ID.
    pub id: String,
    /// The connection name.
    pub name: String,
    /// Creation timestamp.
    pub created: String,
    /// Last update timestamp.
    pub updated: String,
    /// Players available in this connection.
    pub players: Vec<Player>,
}

/// Converts a database row into a `Connection`.
impl ToValueType<Connection> for &switchy_database::Row {
    /// Converts this database row into a `Connection`.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * Required database columns are missing
    /// * Database column values have invalid or incompatible types
    fn to_value_type(self) -> Result<Connection, ParseError> {
        Ok(Connection {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
            ..Default::default()
        })
    }
}

/// Converts a `Connection` ID to a database value.
impl AsId for Connection {
    /// Returns the connection ID as a database value.
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.id.clone())
    }
}

/// API representation of a client connection.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiConnection {
    /// The connection ID.
    pub connection_id: String,
    /// The connection name.
    pub name: String,
    /// Whether the connection is currently alive.
    pub alive: bool,
    /// Players available in this connection.
    pub players: Vec<ApiPlayer>,
}

/// Converts a `Connection` to an `ApiConnection`.
impl From<Connection> for ApiConnection {
    /// Converts a `Connection` into its API representation.
    fn from(value: Connection) -> Self {
        Self {
            connection_id: value.id,
            name: value.name,
            alive: false,
            players: value.players.into_iter().map(Into::into).collect(),
        }
    }
}

/// Player registration data.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegisterPlayer {
    /// The audio output ID for the player.
    pub audio_output_id: String,
    /// The player name.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_playback_target_default() {
        let default = PlaybackTarget::default();
        assert_eq!(default, PlaybackTarget::AudioZone { audio_zone_id: 0 });
    }

    #[test]
    fn test_playback_target_default_from_str_audio_zone() {
        let result = PlaybackTarget::default_from_str("AUDIO_ZONE");
        assert_eq!(result, Some(PlaybackTarget::AudioZone { audio_zone_id: 0 }));
    }

    #[test]
    fn test_playback_target_default_from_str_connection_output() {
        let result = PlaybackTarget::default_from_str("CONNECTION_OUTPUT");
        assert_eq!(
            result,
            Some(PlaybackTarget::ConnectionOutput {
                connection_id: String::new(),
                output_id: String::new(),
            })
        );
    }

    #[test]
    fn test_playback_target_default_from_str_invalid() {
        let result = PlaybackTarget::default_from_str("INVALID_TYPE");
        assert_eq!(result, None);
    }

    #[test]
    fn test_playback_target_default_from_str_empty() {
        let result = PlaybackTarget::default_from_str("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_playback_target_default_from_str_case_sensitive() {
        // Test that the function is case-sensitive
        let result = PlaybackTarget::default_from_str("audio_zone");
        assert_eq!(result, None);
    }

    #[test]
    fn test_update_session_playback_updated_none() {
        let session = UpdateSession::default();
        assert!(!session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_play() {
        let session = UpdateSession {
            play: Some(true),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_stop() {
        let session = UpdateSession {
            stop: Some(true),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_active() {
        let session = UpdateSession {
            active: Some(true),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_playing() {
        let session = UpdateSession {
            playing: Some(false),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_position() {
        let session = UpdateSession {
            position: Some(5),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_volume() {
        let session = UpdateSession {
            volume: Some(0.8),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_seek() {
        let session = UpdateSession {
            seek: Some(30.5),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_with_playlist() {
        let session = UpdateSession {
            playlist: Some(UpdateSessionPlaylist {
                session_playlist_id: 1,
                tracks: vec![],
            }),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_multiple_fields() {
        let session = UpdateSession {
            play: Some(true),
            volume: Some(0.5),
            position: Some(2),
            ..Default::default()
        };
        assert!(session.playback_updated());
    }

    #[test]
    fn test_update_session_playback_updated_only_non_playback_fields() {
        let session = UpdateSession {
            session_id: 1,
            profile: "test".to_string(),
            name: Some("Test Session".to_string()),
            quality: Some(PlaybackQuality::default()),
            ..Default::default()
        };
        assert!(!session.playback_updated());
    }

    #[test]
    fn test_playback_target_conversion_audio_zone() {
        let api_target = ApiPlaybackTarget::AudioZone { audio_zone_id: 42 };
        let target: PlaybackTarget = api_target.clone().into();
        assert_eq!(target, PlaybackTarget::AudioZone { audio_zone_id: 42 });

        // Test reverse conversion
        let api_target_back: ApiPlaybackTarget = target.into();
        assert_eq!(api_target_back, api_target);
    }

    #[test]
    fn test_playback_target_conversion_connection_output() {
        let api_target = ApiPlaybackTarget::ConnectionOutput {
            connection_id: "conn123".to_string(),
            output_id: "out456".to_string(),
        };
        let target: PlaybackTarget = api_target.clone().into();
        assert_eq!(
            target,
            PlaybackTarget::ConnectionOutput {
                connection_id: "conn123".to_string(),
                output_id: "out456".to_string(),
            }
        );

        // Test reverse conversion
        let api_target_back: ApiPlaybackTarget = target.into();
        assert_eq!(api_target_back, api_target);
    }

    #[test]
    fn test_update_session_conversion() {
        let quality = PlaybackQuality::default();
        let api_session = ApiUpdateSession {
            session_id: 1,
            profile: "high".to_string(),
            playback_target: ApiPlaybackTarget::AudioZone { audio_zone_id: 5 },
            play: Some(true),
            stop: None,
            name: Some("My Session".to_string()),
            active: Some(true),
            playing: Some(false),
            position: Some(3),
            seek: Some(45.5),
            volume: Some(0.75),
            playlist: Some(ApiUpdateSessionPlaylist {
                session_playlist_id: 10,
                tracks: vec![],
            }),
            quality: Some(quality),
        };

        let session: UpdateSession = api_session.clone().into();
        assert_eq!(session.session_id, 1);
        assert_eq!(session.profile, "high");
        assert_eq!(
            session.playback_target,
            PlaybackTarget::AudioZone { audio_zone_id: 5 }
        );
        assert_eq!(session.play, Some(true));
        assert_eq!(session.name, Some("My Session".to_string()));
        assert_eq!(session.quality, Some(quality));

        // Test reverse conversion
        let api_session_back: ApiUpdateSession = session.into();
        assert_eq!(api_session_back.session_id, api_session.session_id);
        assert_eq!(api_session_back.profile, api_session.profile);
    }

    #[test]
    fn test_update_session_playlist_conversion() {
        let api_playlist = ApiUpdateSessionPlaylist {
            session_playlist_id: 100,
            tracks: vec![],
        };

        let playlist: UpdateSessionPlaylist = api_playlist.clone().into();
        assert_eq!(playlist.session_playlist_id, 100);
        assert_eq!(playlist.tracks.len(), 0);

        // Test reverse conversion
        let api_playlist_back: ApiUpdateSessionPlaylist = playlist.into();
        assert_eq!(api_playlist_back, api_playlist);
    }

    #[test]
    fn test_session_conversion_to_api_session() {
        let session = Session {
            id: 42,
            name: "Test Session".to_string(),
            active: true,
            playing: false,
            position: Some(5),
            seek: Some(120.5),
            volume: Some(0.8),
            playback_target: Some(PlaybackTarget::AudioZone { audio_zone_id: 3 }),
            playlist: SessionPlaylist {
                id: 99,
                tracks: vec![],
            },
        };

        let api_session: ApiSession = session.into();
        assert_eq!(api_session.session_id, 42);
        assert_eq!(api_session.name, "Test Session");
        assert_eq!(api_session.active, true);
        assert_eq!(api_session.playing, false);
        assert_eq!(api_session.position, Some(5));
        assert_eq!(api_session.seek, Some(120.5));
        assert_eq!(api_session.volume, Some(0.8));
        assert_eq!(
            api_session.playback_target,
            Some(PlaybackTarget::AudioZone { audio_zone_id: 3 })
        );
        assert_eq!(api_session.playlist.session_playlist_id, 99);
    }

    #[test]
    fn test_session_playlist_conversion_to_api() {
        let playlist = SessionPlaylist {
            id: 123,
            tracks: vec![],
        };

        let api_playlist: ApiSessionPlaylist = playlist.into();
        assert_eq!(api_playlist.session_playlist_id, 123);
        assert_eq!(api_playlist.tracks.len(), 0);
    }

    #[test]
    fn test_connection_conversion_to_api() {
        let connection = Connection {
            id: "conn-abc123".to_string(),
            name: "My Connection".to_string(),
            created: "2024-01-01T00:00:00Z".to_string(),
            updated: "2024-01-02T00:00:00Z".to_string(),
            players: vec![],
        };

        let api_connection: ApiConnection = connection.into();
        assert_eq!(api_connection.connection_id, "conn-abc123");
        assert_eq!(api_connection.name, "My Connection");
        assert_eq!(api_connection.alive, false); // Always false in conversion
        assert_eq!(api_connection.players.len(), 0);
    }

    #[test]
    fn test_session_as_id() {
        let session = Session {
            id: 999,
            ..Default::default()
        };
        let db_value = session.as_id();
        assert_eq!(db_value, DatabaseValue::Int64(999));
    }

    #[test]
    fn test_session_playlist_as_id() {
        let playlist = SessionPlaylist {
            id: 777,
            tracks: vec![],
        };
        let db_value = playlist.as_id();
        assert_eq!(db_value, DatabaseValue::Int64(777));
    }

    #[test]
    fn test_connection_as_id() {
        let connection = Connection {
            id: "test-connection-id".to_string(),
            ..Default::default()
        };
        let db_value = connection.as_id();
        assert_eq!(
            db_value,
            DatabaseValue::String("test-connection-id".to_string())
        );
    }

    #[test_log::test]
    fn test_playback_target_serialization_audio_zone() {
        let target = PlaybackTarget::AudioZone { audio_zone_id: 42 };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains(r#""type":"AUDIO_ZONE"#));
        assert!(json.contains(r#""audioZoneId":42"#));

        let deserialized: PlaybackTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, target);
    }

    #[test_log::test]
    fn test_playback_target_serialization_connection_output() {
        let target = PlaybackTarget::ConnectionOutput {
            connection_id: "conn123".to_string(),
            output_id: "out456".to_string(),
        };
        let json = serde_json::to_string(&target).unwrap();
        assert!(json.contains(r#""type":"CONNECTION_OUTPUT"#));
        assert!(json.contains(r#""connectionId":"conn123"#));
        assert!(json.contains(r#""outputId":"out456"#));

        let deserialized: PlaybackTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, target);
    }

    #[test_log::test]
    fn test_update_session_serialization_skips_none_fields() {
        let session = UpdateSession {
            session_id: 1,
            profile: "test".to_string(),
            playback_target: PlaybackTarget::default(),
            play: Some(true),
            stop: None, // Should be skipped
            name: None, // Should be skipped
            active: Some(false),
            ..Default::default()
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains(r#""sessionId":1"#));
        assert!(json.contains(r#""play":true"#));
        assert!(json.contains(r#""active":false"#));
        assert!(!json.contains(r#""stop""#)); // Verify None fields are skipped
        assert!(!json.contains(r#""name""#));
    }

    #[test_log::test]
    fn test_create_session_serialization() {
        let create = CreateSession {
            name: "New Session".to_string(),
            audio_zone_id: Some(5),
            playlist: CreateSessionPlaylist {
                tracks: vec![1, 2, 3],
            },
        };

        let json = serde_json::to_string(&create).unwrap();
        assert!(json.contains(r#""name":"New Session"#));
        assert!(json.contains(r#""audioZoneId":5"#));
        assert!(json.contains(r#""tracks":[1,2,3]"#));

        let deserialized: CreateSession = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "New Session");
        assert_eq!(deserialized.audio_zone_id, Some(5));
        assert_eq!(deserialized.playlist.tracks, vec![1, 2, 3]);
    }

    #[test_log::test]
    fn test_register_connection_serialization() {
        let register = RegisterConnection {
            connection_id: "conn-xyz".to_string(),
            name: "Test Connection".to_string(),
            players: vec![RegisterPlayer {
                audio_output_id: "output1".to_string(),
                name: "Player 1".to_string(),
            }],
        };

        let json = serde_json::to_string(&register).unwrap();
        assert!(json.contains(r#""connectionId":"conn-xyz"#));
        assert!(json.contains(r#""name":"Test Connection"#));
        assert!(json.contains(r#""audioOutputId":"output1"#));

        let deserialized: RegisterConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.connection_id, "conn-xyz");
        assert_eq!(deserialized.players.len(), 1);
        assert_eq!(deserialized.players[0].name, "Player 1");
    }

    #[test]
    fn test_api_playback_target_default() {
        let default = ApiPlaybackTarget::default();
        assert_eq!(default, ApiPlaybackTarget::AudioZone { audio_zone_id: 0 });
    }

    #[test]
    fn test_set_session_audio_zone_default() {
        let set_zone = SetSessionAudioZone::default();
        assert_eq!(set_zone.session_id, 0);
        assert_eq!(set_zone.audio_zone_id, 0);
    }

    #[test]
    fn test_delete_session_default() {
        let delete = DeleteSession::default();
        assert_eq!(delete.session_id, 0);
    }

    #[test]
    fn test_session_default() {
        let session = Session::default();
        assert_eq!(session.id, 0);
        assert_eq!(session.name, "");
        assert!(!session.active);
        assert!(!session.playing);
        assert_eq!(session.position, None);
        assert_eq!(session.seek, None);
        assert_eq!(session.volume, None);
        assert_eq!(session.playback_target, None);
        assert_eq!(session.playlist.id, 0);
        assert_eq!(session.playlist.tracks.len(), 0);
    }

    #[test]
    fn test_connection_default() {
        let connection = Connection::default();
        assert_eq!(connection.id, "");
        assert_eq!(connection.name, "");
        assert_eq!(connection.created, "");
        assert_eq!(connection.updated, "");
        assert_eq!(connection.players.len(), 0);
    }

    #[test]
    fn test_register_player_default() {
        let player = RegisterPlayer::default();
        assert_eq!(player.audio_output_id, "");
        assert_eq!(player.name, "");
    }

    #[test]
    fn test_update_session_with_quality() {
        let quality = PlaybackQuality::default();
        let session = UpdateSession {
            session_id: 1,
            profile: "test".to_string(),
            playback_target: PlaybackTarget::default(),
            quality: Some(quality),
            ..Default::default()
        };

        assert_eq!(session.quality, Some(quality));
        assert!(!session.playback_updated()); // quality alone doesn't trigger playback update
    }

    #[test]
    fn test_session_playlist_tracks_wrapper() {
        let tracks = SessionPlaylistTracks(vec![]);
        assert_eq!(tracks.0.len(), 0);
    }
}
