//! Data models for audio zone management in `MoosicBox`.
//!
//! This crate provides the core data structures for managing audio zones, which are groups
//! of audio players that can be controlled together for synchronized playback. Audio zones
//! enable multi-room audio functionality by coordinating playback across multiple devices.
//!
//! # Main Types
//!
//! * [`AudioZone`] - Represents a group of audio players
//! * [`Player`] - Represents an individual audio output device within a zone
//! * [`AudioZoneWithSession`] - An audio zone associated with a playback session
//! * [`CreateAudioZone`] / [`UpdateAudioZone`] - Request types for zone management
//!
//! # API Representations
//!
//! The crate provides separate API-friendly types (e.g., [`ApiAudioZone`], [`ApiPlayer`])
//! with camelCase field names for JSON serialization, which can be converted to/from
//! internal types using `From` trait implementations.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_json_utils::{MissingValue, ParseError, ToValueType, database::ToValue as _};
use serde::{Deserialize, Serialize};
use switchy_database::{AsId, DatabaseValue};

/// Represents an audio zone containing multiple audio players.
///
/// An audio zone groups multiple players together for synchronized audio playback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioZone {
    /// Unique identifier for the audio zone
    pub id: u64,
    /// Human-readable name of the audio zone
    pub name: String,
    /// List of players in this audio zone
    pub players: Vec<Player>,
}

/// Converts an API audio zone into an internal audio zone.
impl From<ApiAudioZone> for AudioZone {
    fn from(value: ApiAudioZone) -> Self {
        Self {
            id: value.id,
            name: value.name,
            players: value
                .players
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        }
    }
}

/// API representation of an audio zone.
///
/// This is the serialization format used for API responses and requests,
/// with camelCase field names.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiAudioZone {
    /// Unique identifier for the audio zone
    pub id: u64,
    /// Human-readable name of the audio zone
    pub name: String,
    /// List of players in this audio zone
    pub players: Vec<ApiPlayer>,
}

/// Converts an internal audio zone into an API audio zone.
impl From<AudioZone> for ApiAudioZone {
    fn from(value: AudioZone) -> Self {
        Self {
            id: value.id,
            name: value.name,
            players: value
                .players
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        }
    }
}

/// Represents an audio zone with an associated playback session.
///
/// This extends [`AudioZone`] with session tracking information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioZoneWithSession {
    /// Unique identifier for the audio zone
    pub id: u64,
    /// Identifier of the associated playback session
    pub session_id: u64,
    /// Human-readable name of the audio zone
    pub name: String,
    /// List of players in this audio zone
    pub players: Vec<Player>,
}

/// Converts an API audio zone with session into an internal audio zone with session.
impl From<ApiAudioZoneWithSession> for AudioZoneWithSession {
    fn from(value: ApiAudioZoneWithSession) -> Self {
        Self {
            id: value.id,
            session_id: value.session_id,
            name: value.name,
            players: value
                .players
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        }
    }
}

/// API representation of an audio zone with session information.
///
/// This is the serialization format used for API responses and requests,
/// with camelCase field names.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiAudioZoneWithSession {
    /// Unique identifier for the audio zone
    pub id: u64,
    /// Identifier of the associated playback session
    pub session_id: u64,
    /// Human-readable name of the audio zone
    pub name: String,
    /// List of players in this audio zone
    pub players: Vec<ApiPlayer>,
}

/// Converts an internal audio zone with session into an API audio zone with session.
impl From<AudioZoneWithSession> for ApiAudioZoneWithSession {
    fn from(value: AudioZoneWithSession) -> Self {
        Self {
            id: value.id,
            session_id: value.session_id,
            name: value.name,
            players: value
                .players
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        }
    }
}

/// Represents an audio player within an audio zone.
///
/// A player corresponds to an audio output device and maintains playback state.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    /// Unique identifier for the player
    pub id: u64,
    /// Identifier of the associated audio output device
    pub audio_output_id: String,
    /// Human-readable name of the player
    pub name: String,
    /// Whether the player is currently playing
    pub playing: bool,
    /// Timestamp when the player was created
    pub created: String,
    /// Timestamp when the player was last updated
    pub updated: String,
}

/// Converts an API player into an internal player.
///
/// The `created` and `updated` timestamps are initialized as empty strings since
/// the API representation does not include timestamp information.
impl From<ApiPlayer> for Player {
    fn from(value: ApiPlayer) -> Self {
        Self {
            id: value.player_id,
            audio_output_id: value.audio_output_id,
            name: value.name,
            playing: value.playing,
            created: String::new(),
            updated: String::new(),
        }
    }
}

/// Enables handling of missing values when converting database rows to `Player`.
impl MissingValue<Player> for &switchy_database::Row {}

/// Converts a database row into a `Player`.
///
/// # Errors
///
/// * Returns an error if any required column is missing from the row
/// * Returns an error if any column value cannot be converted to the expected type
impl ToValueType<Player> for &switchy_database::Row {
    fn to_value_type(self) -> Result<Player, ParseError> {
        Ok(Player {
            id: self.to_value("id")?,
            audio_output_id: self.to_value("audio_output_id")?,
            name: self.to_value("name")?,
            playing: self.to_value("playing")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

/// Converts a `Player` into a database ID value.
///
/// Returns the player's ID as a signed 64-bit integer suitable for database operations.
impl AsId for Player {
    fn as_id(&self) -> DatabaseValue {
        #[allow(clippy::cast_possible_wrap)]
        DatabaseValue::Int64(self.id as i64)
    }
}

/// API representation of an audio player.
///
/// This is the serialization format used for API responses and requests,
/// with camelCase field names.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiPlayer {
    /// Unique identifier for the player
    pub player_id: u64,
    /// Identifier of the associated audio output device
    pub audio_output_id: String,
    /// Human-readable name of the player
    pub name: String,
    /// Whether the player is currently playing
    pub playing: bool,
}

/// Converts an internal player into an API player.
impl From<Player> for ApiPlayer {
    fn from(value: Player) -> Self {
        Self {
            player_id: value.id,
            audio_output_id: value.audio_output_id,
            name: value.name,
            playing: value.playing,
        }
    }
}

/// Request to create a new audio zone.
///
/// This struct is used to specify the initial properties when creating
/// an audio zone.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZone {
    /// Name for the new audio zone
    pub name: String,
}

/// Request to update an existing audio zone.
///
/// All fields except `id` are optional. Only provided fields will be updated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateAudioZone {
    /// Identifier of the audio zone to update
    pub id: u64,
    /// New name for the audio zone (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New list of player IDs for the audio zone (if provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<Vec<u64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_zone_from_api_audio_zone() {
        let api_zone = ApiAudioZone {
            id: 42,
            name: "Living Room".to_string(),
            players: vec![
                ApiPlayer {
                    player_id: 1,
                    audio_output_id: "output-1".to_string(),
                    name: "Speaker 1".to_string(),
                    playing: true,
                },
                ApiPlayer {
                    player_id: 2,
                    audio_output_id: "output-2".to_string(),
                    name: "Speaker 2".to_string(),
                    playing: false,
                },
            ],
        };

        let zone: AudioZone = api_zone.into();

        assert_eq!(zone.id, 42);
        assert_eq!(zone.name, "Living Room");
        assert_eq!(zone.players.len(), 2);
        assert_eq!(zone.players[0].id, 1);
        assert_eq!(zone.players[0].audio_output_id, "output-1");
        assert_eq!(zone.players[0].name, "Speaker 1");
        assert!(zone.players[0].playing);
        assert_eq!(zone.players[1].id, 2);
    }

    #[test]
    fn test_api_audio_zone_from_audio_zone() {
        let zone = AudioZone {
            id: 99,
            name: "Kitchen".to_string(),
            players: vec![Player {
                id: 5,
                audio_output_id: "output-5".to_string(),
                name: "Kitchen Speaker".to_string(),
                playing: false,
                created: "2024-01-01T00:00:00Z".to_string(),
                updated: "2024-01-02T00:00:00Z".to_string(),
            }],
        };

        let api_zone: ApiAudioZone = zone.into();

        assert_eq!(api_zone.id, 99);
        assert_eq!(api_zone.name, "Kitchen");
        assert_eq!(api_zone.players.len(), 1);
        assert_eq!(api_zone.players[0].player_id, 5);
        assert_eq!(api_zone.players[0].audio_output_id, "output-5");
        assert_eq!(api_zone.players[0].name, "Kitchen Speaker");
        assert!(!api_zone.players[0].playing);
    }

    #[test]
    fn test_audio_zone_with_session_from_api() {
        let api_zone = ApiAudioZoneWithSession {
            id: 10,
            session_id: 20,
            name: "Bedroom".to_string(),
            players: vec![ApiPlayer {
                player_id: 3,
                audio_output_id: "output-3".to_string(),
                name: "Bedroom Speaker".to_string(),
                playing: true,
            }],
        };

        let zone: AudioZoneWithSession = api_zone.into();

        assert_eq!(zone.id, 10);
        assert_eq!(zone.session_id, 20);
        assert_eq!(zone.name, "Bedroom");
        assert_eq!(zone.players.len(), 1);
        assert_eq!(zone.players[0].id, 3);
    }

    #[test]
    fn test_api_audio_zone_with_session_from_internal() {
        let zone = AudioZoneWithSession {
            id: 15,
            session_id: 25,
            name: "Office".to_string(),
            players: vec![Player {
                id: 7,
                audio_output_id: "output-7".to_string(),
                name: "Office Speaker".to_string(),
                playing: true,
                created: "2024-01-01T00:00:00Z".to_string(),
                updated: "2024-01-02T00:00:00Z".to_string(),
            }],
        };

        let api_zone: ApiAudioZoneWithSession = zone.into();

        assert_eq!(api_zone.id, 15);
        assert_eq!(api_zone.session_id, 25);
        assert_eq!(api_zone.name, "Office");
        assert_eq!(api_zone.players.len(), 1);
        assert_eq!(api_zone.players[0].player_id, 7);
    }

    #[test]
    fn test_player_from_api_player_creates_empty_timestamps() {
        let api_player = ApiPlayer {
            player_id: 100,
            audio_output_id: "test-output".to_string(),
            name: "Test Player".to_string(),
            playing: false,
        };

        let player: Player = api_player.into();

        assert_eq!(player.id, 100);
        assert_eq!(player.audio_output_id, "test-output");
        assert_eq!(player.name, "Test Player");
        assert!(!player.playing);
        assert_eq!(player.created, "");
        assert_eq!(player.updated, "");
    }

    #[test]
    fn test_api_player_from_player_omits_timestamps() {
        let player = Player {
            id: 200,
            audio_output_id: "another-output".to_string(),
            name: "Another Player".to_string(),
            playing: true,
            created: "2024-01-01T00:00:00Z".to_string(),
            updated: "2024-01-02T00:00:00Z".to_string(),
        };

        let api_player: ApiPlayer = player.into();

        assert_eq!(api_player.player_id, 200);
        assert_eq!(api_player.audio_output_id, "another-output");
        assert_eq!(api_player.name, "Another Player");
        assert!(api_player.playing);
    }

    #[test]
    fn test_audio_zone_conversion_with_empty_players() {
        let api_zone = ApiAudioZone {
            id: 1,
            name: "Empty Zone".to_string(),
            players: vec![],
        };

        let zone: AudioZone = api_zone.into();

        assert_eq!(zone.id, 1);
        assert_eq!(zone.name, "Empty Zone");
        assert_eq!(zone.players.len(), 0);
    }

    #[test]
    fn test_audio_zone_with_session_conversion_with_multiple_players() {
        let api_zone = ApiAudioZoneWithSession {
            id: 50,
            session_id: 100,
            name: "Multi-Player Zone".to_string(),
            players: vec![
                ApiPlayer {
                    player_id: 1,
                    audio_output_id: "out-1".to_string(),
                    name: "Player 1".to_string(),
                    playing: true,
                },
                ApiPlayer {
                    player_id: 2,
                    audio_output_id: "out-2".to_string(),
                    name: "Player 2".to_string(),
                    playing: false,
                },
                ApiPlayer {
                    player_id: 3,
                    audio_output_id: "out-3".to_string(),
                    name: "Player 3".to_string(),
                    playing: true,
                },
            ],
        };

        let zone: AudioZoneWithSession = api_zone.into();

        assert_eq!(zone.players.len(), 3);
        assert_eq!(zone.players[0].id, 1);
        assert_eq!(zone.players[1].id, 2);
        assert_eq!(zone.players[2].id, 3);
    }
}
