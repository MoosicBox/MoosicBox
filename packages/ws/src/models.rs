//! WebSocket message payload types.
//!
//! This module defines the payload structures for WebSocket communication between
//! `MoosicBox` clients and servers. It includes both inbound (client-to-server) and
//! outbound (server-to-client) message types.
//!
//! # Message Types
//!
//! * [`InboundPayload`] - Messages sent from clients to the server
//! * [`OutboundPayload`] - Messages sent from the server to clients
//!
//! Each payload type is a tagged enum that automatically serializes with a `type` field
//! indicating the message type.

use moosicbox_audio_zone::models::{ApiAudioZoneWithSession, CreateAudioZone};
use moosicbox_session::models::{
    ApiConnection, ApiPlaybackTarget, ApiSession, ApiUpdateSession, CreateSession, DeleteSession,
    RegisterConnection, RegisterPlayer, UpdateSession,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::AsRefStr;

/// Payload types for incoming websocket messages.
#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum InboundPayload {
    /// Ping message to keep connection alive.
    Ping(EmptyPayload),
    /// Request to retrieve the connection ID.
    GetConnectionId(EmptyPayload),
    /// Request to retrieve all sessions.
    GetSessions(EmptyPayload),
    /// Request to create a new session.
    CreateSession(CreateSessionPayload),
    /// Request to update an existing session.
    UpdateSession(UpdateSessionPayload),
    /// Request to delete a session.
    DeleteSession(DeleteSessionPayload),
    /// Request to register a connection.
    RegisterConnection(RegisterConnectionPayload),
    /// Request to register multiple players.
    RegisterPlayers(RegisterPlayersPayload),
    /// Request to create a new audio zone.
    CreateAudioZone(CreateAudioZonePayload),
    /// Request to set seek position.
    SetSeek(SetSeekPayload),
}

impl std::fmt::Display for InboundPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Payload types for outgoing websocket messages.
#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum OutboundPayload {
    /// Connection ID response.
    ConnectionId(ConnectionIdPayload),
    /// List of sessions.
    Sessions(SessionsPayload),
    /// Notification that a session was updated.
    SessionUpdated(SessionUpdatedPayload),
    /// Audio zones with their associated sessions.
    AudioZoneWithSessions(AudioZoneWithSessionsPayload),
    /// Download progress event notification.
    DownloadEvent(DownloadEventPayload),
    /// Scan progress event notification.
    ScanEvent(ScanEventPayload),
    /// List of connections.
    Connections(ConnectionsPayload),
    /// Seek position update.
    SetSeek(SetSeekPayload),
}

impl std::fmt::Display for OutboundPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Empty payload for websocket messages that require no data.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmptyPayload {}

/// Payload for creating a new session.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionPayload {
    pub payload: CreateSession,
}

/// Payload for updating an existing session.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPayload {
    pub payload: UpdateSession,
}

/// Payload for deleting a session.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionPayload {
    pub payload: DeleteSession,
}

/// Payload for registering a new connection.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterConnectionPayload {
    pub payload: RegisterConnection,
}

/// Payload for registering multiple players.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPlayersPayload {
    pub payload: Vec<RegisterPlayer>,
}

/// Payload for creating a new audio zone.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZonePayload {
    pub payload: CreateAudioZone,
}

/// Payload for playback actions.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackActionPayload {
    pub payload: Value,
}

/// Payload containing a connection ID.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionIdPayload {
    pub connection_id: String,
}

/// Payload containing a list of sessions.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionsPayload {
    pub payload: Vec<ApiSession>,
}

/// Payload containing audio zones with their sessions.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AudioZoneWithSessionsPayload {
    pub payload: Vec<ApiAudioZoneWithSession>,
}

/// Payload for session update notifications.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdatedPayload {
    pub payload: ApiUpdateSession,
}

/// Payload for download event notifications.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEventPayload {
    pub payload: Value,
}

/// Payload for scan event notifications.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanEventPayload {
    pub payload: Value,
}

/// Payload containing a list of connections.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsPayload {
    pub payload: Vec<ApiConnection>,
}

/// Payload for setting seek position.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetSeekPayload {
    pub payload: SetSeek,
}

/// Seek position data for a session.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetSeek {
    /// Session ID to seek within.
    pub session_id: u64,
    /// Profile name associated with the session.
    pub profile: String,
    /// Playback target for the session.
    pub playback_target: ApiPlaybackTarget,
    /// Seek position in seconds.
    pub seek: u64,
}
