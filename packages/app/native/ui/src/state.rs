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
