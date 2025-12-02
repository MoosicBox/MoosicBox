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
