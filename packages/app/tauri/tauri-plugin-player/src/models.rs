//! Data models for the player plugin.
//!
//! This module defines the data structures used for player state management,
//! including tracks, playlists, state updates, and media control events.

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

/// Represents a music track with metadata.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Unique identifier for the track.
    pub id: String,
    /// Track number in the album.
    pub number: u32,
    /// Title of the track.
    pub title: String,
    /// Album name.
    pub album: String,
    /// Optional URL to the album cover image.
    pub album_cover: Option<String>,
    /// Artist name.
    pub artist: String,
    /// Optional URL to the artist cover image.
    pub artist_cover: Option<String>,
    /// Duration of the track in seconds.
    pub duration: f64,
}

/// Represents a playlist containing multiple tracks.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    /// List of tracks in the playlist.
    pub tracks: Vec<Track>,
}

/// State update request for the player.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    /// Whether the player should be playing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    /// Position in the playlist (track index).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u16>,
    /// Seek position in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<f64>,
    /// Volume level (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    /// Playlist to set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<Playlist>,
}

/// Response from a state update operation.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateResponse {}

/// Request to initialize a communication channel.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitChannel {
    /// The IPC channel to initialize.
    pub channel: Channel,
}

/// Response from a channel initialization operation.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitChannelResponse {}

/// Media control event from the platform.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaEvent {
    /// Play/pause event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    /// Next track event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_track: Option<bool>,
    /// Previous track event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_track: Option<bool>,
}
