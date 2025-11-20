//! Application state models.
//!
//! This module defines the core state structures for the `MoosicBox` application,
//! including connection information and player state.

#![allow(clippy::module_name_repetitions)]

use moosicbox_app_models::Connection;
use moosicbox_music_models::api::ApiTrack;
use serde::{Deserialize, Serialize};

/// Represents the current playback state of a music session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct PlaybackState {
    /// The unique identifier for this playback session.
    pub session_id: u64,
    /// Whether the session is currently playing.
    pub playing: bool,
    /// The current track position in the playlist.
    pub position: u16,
    /// The current seek position within the track in seconds.
    pub seek: f64,
    /// The current volume level (0.0 to 1.0).
    pub volume: f64,
    /// The list of tracks in the current playlist.
    pub tracks: Vec<ApiTrack>,
}

/// Represents the state of the music player.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct PlayerState {
    /// The current playback state, if any.
    pub playback: Option<PlaybackState>,
}

/// Root application state containing connection and player information.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct State {
    /// The current server connection, if established.
    pub connection: Option<Connection>,
    /// The current player state.
    pub player: PlayerState,
}

impl std::fmt::Display for State {
    /// # Panics
    ///
    /// * Panics if the state cannot be serialized to JSON
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

impl<'a> TryFrom<&'a str> for State {
    type Error = serde_json::Error;

    /// # Errors
    ///
    /// * Returns an error if the string is not valid JSON or does not match the `State` schema
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_app_models::Connection;
    use moosicbox_music_models::{ApiSource, api::ApiTrack, id::Id};
    use pretty_assertions::assert_eq;

    #[test]
    fn test_state_display_empty() {
        let state = State::default();
        let json = state.to_string();
        assert!(json.contains(r#""type":"State"#));
    }

    #[test]
    fn test_state_display_with_connection() {
        let state = State {
            connection: Some(Connection {
                name: "test-connection".to_string(),
                api_url: "http://localhost:8080".to_string(),
            }),
            player: PlayerState::default(),
        };
        let json = state.to_string();
        assert!(json.contains("test-connection"));
        assert!(json.contains("http://localhost:8080"));
    }

    #[test]
    fn test_state_display_with_playback() {
        let state = State {
            connection: None,
            player: PlayerState {
                playback: Some(PlaybackState {
                    session_id: 42,
                    playing: true,
                    position: 0,
                    seek: 10.5,
                    volume: 0.8,
                    tracks: vec![],
                }),
            },
        };
        let json = state.to_string();
        assert!(json.contains(r#""session_id":42"#));
        assert!(json.contains(r#""playing":true"#));
        assert!(json.contains(r#""seek":10.5"#));
        assert!(json.contains(r#""volume":0.8"#));
    }

    #[test]
    fn test_state_try_from_valid_json() {
        let json =
            r#"{"type":"State","connection":null,"player":{"type":"PlayerState","playback":null}}"#;
        let result = State::try_from(json);
        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(state.connection.is_none());
        assert!(state.player.playback.is_none());
    }

    #[test]
    fn test_state_try_from_invalid_json() {
        let json = "not valid json";
        let result = State::try_from(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_state_try_from_with_playback() {
        let json = r#"{
            "type":"State",
            "connection":null,
            "player":{
                "type":"PlayerState",
                "playback":{
                    "type":"PlaybackState",
                    "session_id":123,
                    "playing":false,
                    "position":2,
                    "seek":45.0,
                    "volume":0.5,
                    "tracks":[]
                }
            }
        }"#;
        let result = State::try_from(json);
        assert!(result.is_ok());
        let state = result.unwrap();
        let playback = state.player.playback.as_ref().unwrap();
        assert_eq!(playback.session_id, 123);
        assert!(!playback.playing);
        assert_eq!(playback.position, 2);
        assert!((playback.seek - 45.0).abs() < f64::EPSILON);
        assert!((playback.volume - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_state_roundtrip() {
        let original_state = State {
            connection: Some(Connection {
                name: "Test Connection".to_string(),
                api_url: "http://example.com".to_string(),
            }),
            player: PlayerState {
                playback: Some(PlaybackState {
                    session_id: 999,
                    playing: true,
                    position: 5,
                    seek: 123.456,
                    volume: 0.75,
                    tracks: vec![ApiTrack {
                        track_id: Id::Number(1),
                        number: 1,
                        title: "Test Track".to_string(),
                        duration: 180.0,
                        album: "Test Album".to_string(),
                        album_id: Id::Number(100),
                        album_type: moosicbox_music_models::AlbumType::Lp,
                        date_released: None,
                        date_added: None,
                        artist: "Test Artist".to_string(),
                        artist_id: Id::Number(200),
                        blur: false,
                        format: None,
                        bit_depth: None,
                        audio_bitrate: None,
                        overall_bitrate: None,
                        sample_rate: None,
                        channels: None,
                        track_source: moosicbox_music_models::TrackApiSource::Local,
                        api_source: ApiSource::library(),
                        sources: moosicbox_music_models::ApiSources::default(),
                        contains_cover: false,
                    }],
                }),
            },
        };

        let json = original_state.to_string();
        let parsed_state = State::try_from(json.as_str()).unwrap();

        assert_eq!(
            original_state.connection.as_ref().unwrap().name,
            parsed_state.connection.as_ref().unwrap().name
        );
        assert_eq!(
            original_state.player.playback.as_ref().unwrap().session_id,
            parsed_state.player.playback.as_ref().unwrap().session_id
        );
    }

    #[test]
    fn test_playback_state_fields() {
        let playback = PlaybackState {
            session_id: 1,
            playing: false,
            position: 0,
            seek: 0.0,
            volume: 1.0,
            tracks: vec![],
        };

        assert_eq!(playback.session_id, 1);
        assert!(!playback.playing);
        assert_eq!(playback.position, 0);
        assert!((playback.seek - 0.0).abs() < f64::EPSILON);
        assert!((playback.volume - 1.0).abs() < f64::EPSILON);
        assert!(playback.tracks.is_empty());
    }

    #[test]
    fn test_player_state_default() {
        let player_state = PlayerState::default();
        assert!(player_state.playback.is_none());
    }

    #[test]
    fn test_state_default() {
        let state = State::default();
        assert!(state.connection.is_none());
        assert!(state.player.playback.is_none());
    }
}
