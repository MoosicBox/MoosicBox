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
    /// Session creation details.
    pub payload: CreateSession,
}

/// Payload for updating an existing session.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPayload {
    /// Session update details.
    pub payload: UpdateSession,
}

/// Payload for deleting a session.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionPayload {
    /// Session deletion details.
    pub payload: DeleteSession,
}

/// Payload for registering a new connection.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterConnectionPayload {
    /// Connection registration details.
    pub payload: RegisterConnection,
}

/// Payload for registering multiple players.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPlayersPayload {
    /// List of players to register.
    pub payload: Vec<RegisterPlayer>,
}

/// Payload for creating a new audio zone.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZonePayload {
    /// Audio zone creation details.
    pub payload: CreateAudioZone,
}

/// Payload for playback actions.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackActionPayload {
    /// Playback action data as a JSON value.
    pub payload: Value,
}

/// Payload containing a connection ID.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionIdPayload {
    /// Unique identifier for the connection.
    pub connection_id: String,
}

/// Payload containing a list of sessions.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionsPayload {
    /// List of active sessions.
    pub payload: Vec<ApiSession>,
}

/// Payload containing audio zones with their sessions.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AudioZoneWithSessionsPayload {
    /// List of audio zones and their associated sessions.
    pub payload: Vec<ApiAudioZoneWithSession>,
}

/// Payload for session update notifications.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdatedPayload {
    /// Session update details.
    pub payload: ApiUpdateSession,
}

/// Payload for download event notifications.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEventPayload {
    /// Download event data as a JSON value.
    pub payload: Value,
}

/// Payload for scan event notifications.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanEventPayload {
    /// Scan event data as a JSON value.
    pub payload: Value,
}

/// Payload containing a list of connections.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsPayload {
    /// List of registered connections.
    pub payload: Vec<ApiConnection>,
}

/// Payload for setting seek position.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetSeekPayload {
    /// Seek position details.
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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_session::models::ApiPlaybackTarget;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_inbound_payload_display_ping() {
        let payload = InboundPayload::Ping(EmptyPayload {});
        assert_eq!(payload.to_string(), "Ping");
    }

    #[test]
    fn test_inbound_payload_display_get_connection_id() {
        let payload = InboundPayload::GetConnectionId(EmptyPayload {});
        assert_eq!(payload.to_string(), "GetConnectionId");
    }

    #[test]
    fn test_inbound_payload_display_get_sessions() {
        let payload = InboundPayload::GetSessions(EmptyPayload {});
        assert_eq!(payload.to_string(), "GetSessions");
    }

    #[test]
    fn test_inbound_payload_display_set_seek() {
        let payload = InboundPayload::SetSeek(SetSeekPayload {
            payload: SetSeek::default(),
        });
        assert_eq!(payload.to_string(), "SetSeek");
    }

    #[test]
    fn test_outbound_payload_display_connection_id() {
        let payload = OutboundPayload::ConnectionId(ConnectionIdPayload {
            connection_id: "test-id".to_string(),
        });
        assert_eq!(payload.to_string(), "ConnectionId");
    }

    #[test]
    fn test_outbound_payload_display_sessions() {
        let payload = OutboundPayload::Sessions(SessionsPayload { payload: vec![] });
        assert_eq!(payload.to_string(), "Sessions");
    }

    #[test]
    fn test_outbound_payload_display_audio_zone_with_sessions() {
        let payload = OutboundPayload::AudioZoneWithSessions(AudioZoneWithSessionsPayload {
            payload: vec![],
        });
        assert_eq!(payload.to_string(), "AudioZoneWithSessions");
    }

    #[test]
    fn test_outbound_payload_display_download_event() {
        let payload = OutboundPayload::DownloadEvent(DownloadEventPayload {
            payload: serde_json::json!({}),
        });
        assert_eq!(payload.to_string(), "DownloadEvent");
    }

    #[test]
    fn test_outbound_payload_display_scan_event() {
        let payload = OutboundPayload::ScanEvent(ScanEventPayload {
            payload: serde_json::json!({}),
        });
        assert_eq!(payload.to_string(), "ScanEvent");
    }

    #[test]
    fn test_outbound_payload_display_connections() {
        let payload = OutboundPayload::Connections(ConnectionsPayload { payload: vec![] });
        assert_eq!(payload.to_string(), "Connections");
    }

    #[test]
    fn test_set_seek_default() {
        let set_seek = SetSeek::default();
        assert_eq!(set_seek.session_id, 0);
        assert_eq!(set_seek.profile, "");
        assert_eq!(set_seek.playback_target, ApiPlaybackTarget::default());
        assert_eq!(set_seek.seek, 0);
    }

    #[test]
    fn test_inbound_payload_ping_serialization() {
        let payload = InboundPayload::Ping(EmptyPayload {});
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains(r#""type":"PING"#));
    }

    #[test]
    fn test_inbound_payload_ping_deserialization() {
        let json = r#"{"type":"PING"}"#;
        let payload: InboundPayload = serde_json::from_str(json).unwrap();
        assert!(matches!(payload, InboundPayload::Ping(_)));
    }

    #[test]
    fn test_inbound_payload_get_connection_id_serialization() {
        let payload = InboundPayload::GetConnectionId(EmptyPayload {});
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains(r#""type":"GET_CONNECTION_ID"#));
    }

    #[test]
    fn test_inbound_payload_get_sessions_serialization() {
        let payload = InboundPayload::GetSessions(EmptyPayload {});
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains(r#""type":"GET_SESSIONS"#));
    }

    #[test]
    fn test_outbound_payload_connection_id_serialization() {
        let payload = OutboundPayload::ConnectionId(ConnectionIdPayload {
            connection_id: "test-123".to_string(),
        });
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains(r#""type":"CONNECTION_ID"#));
        assert!(json.contains(r#""connectionId":"test-123"#));
    }

    #[test]
    fn test_outbound_payload_connection_id_deserialization() {
        let json = r#"{"type":"CONNECTION_ID","connectionId":"test-456"}"#;
        let payload: OutboundPayload = serde_json::from_str(json).unwrap();
        if let OutboundPayload::ConnectionId(connection_id_payload) = payload {
            assert_eq!(connection_id_payload.connection_id, "test-456");
        } else {
            panic!("Expected ConnectionId payload");
        }
    }

    #[test]
    fn test_set_seek_serialization() {
        let set_seek = SetSeek {
            session_id: 42,
            profile: "test-profile".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 120,
        };
        let json = serde_json::to_string(&set_seek).unwrap();
        assert!(json.contains(r#""sessionId":42"#));
        assert!(json.contains(r#""profile":"test-profile"#));
        assert!(json.contains(r#""seek":120"#));
    }

    #[test]
    fn test_set_seek_deserialization() {
        let json = r#"{"sessionId":99,"profile":"my-profile","playbackTarget":{"type":"AUDIO_ZONE","audioZoneId":5},"seek":300}"#;
        let set_seek: SetSeek = serde_json::from_str(json).unwrap();
        assert_eq!(set_seek.session_id, 99);
        assert_eq!(set_seek.profile, "my-profile");
        assert_eq!(set_seek.seek, 300);
        assert_eq!(
            set_seek.playback_target,
            ApiPlaybackTarget::AudioZone { audio_zone_id: 5 }
        );
    }

    #[test]
    fn test_empty_payload_serialization() {
        let payload = EmptyPayload {};
        let json = serde_json::to_string(&payload).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_empty_payload_deserialization() {
        let json = "{}";
        let payload: EmptyPayload = serde_json::from_str(json).unwrap();
        // Just verify it deserializes without error
        let _ = payload;
    }

    #[test]
    fn test_set_seek_equality() {
        let seek1 = SetSeek {
            session_id: 1,
            profile: "profile1".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 100,
        };
        let seek2 = SetSeek {
            session_id: 1,
            profile: "profile1".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 100,
        };
        let seek3 = SetSeek {
            session_id: 2,
            profile: "profile1".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 100,
        };

        assert_eq!(seek1, seek2);
        assert_ne!(seek1, seek3);
    }

    #[test]
    fn test_set_seek_clone() {
        let original = SetSeek {
            session_id: 10,
            profile: "clone-test".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 50,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
