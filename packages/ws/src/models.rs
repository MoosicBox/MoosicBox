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
    use moosicbox_audio_zone::models::CreateAudioZone;
    use moosicbox_session::models::{
        ApiPlaybackTarget, DeleteSession, RegisterConnection, RegisterPlayer,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test_log::test]
    fn test_inbound_payload_ping_serialization() {
        let payload = InboundPayload::Ping(EmptyPayload {});
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "PING");
    }

    #[test_log::test]
    fn test_inbound_payload_ping_deserialization() {
        let json = json!({"type": "PING"});
        let payload: InboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            InboundPayload::Ping(_) => {}
            _ => panic!("Expected Ping variant"),
        }
    }

    #[test_log::test]
    fn test_inbound_payload_get_connection_id_serialization() {
        let payload = InboundPayload::GetConnectionId(EmptyPayload {});
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "GET_CONNECTION_ID");
    }

    #[test_log::test]
    fn test_inbound_payload_get_sessions_serialization() {
        let payload = InboundPayload::GetSessions(EmptyPayload {});
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "GET_SESSIONS");
    }

    #[test_log::test]
    fn test_inbound_payload_display() {
        let payload = InboundPayload::Ping(EmptyPayload {});
        assert_eq!(payload.to_string(), "Ping");

        let payload = InboundPayload::GetConnectionId(EmptyPayload {});
        assert_eq!(payload.to_string(), "GetConnectionId");

        let payload = InboundPayload::GetSessions(EmptyPayload {});
        assert_eq!(payload.to_string(), "GetSessions");
    }

    #[test_log::test]
    fn test_outbound_payload_connection_id_serialization() {
        let payload = OutboundPayload::ConnectionId(ConnectionIdPayload {
            connection_id: "test-123".to_string(),
        });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "CONNECTION_ID");
        assert_eq!(json["connectionId"], "test-123");
    }

    #[test_log::test]
    fn test_outbound_payload_connection_id_deserialization() {
        let json = json!({
            "type": "CONNECTION_ID",
            "connectionId": "test-456"
        });
        let payload: OutboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            OutboundPayload::ConnectionId(p) => {
                assert_eq!(p.connection_id, "test-456");
            }
            _ => panic!("Expected ConnectionId variant"),
        }
    }

    #[test_log::test]
    fn test_outbound_payload_display() {
        let payload = OutboundPayload::ConnectionId(ConnectionIdPayload {
            connection_id: "test".to_string(),
        });
        assert_eq!(payload.to_string(), "ConnectionId");

        let payload = OutboundPayload::Sessions(SessionsPayload { payload: vec![] });
        assert_eq!(payload.to_string(), "Sessions");
    }

    #[test_log::test]
    fn test_set_seek_serialization() {
        let seek = SetSeek {
            session_id: 42,
            profile: "test-profile".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 120,
        };

        let json = serde_json::to_value(&seek).unwrap();

        assert_eq!(json["sessionId"], 42);
        assert_eq!(json["profile"], "test-profile");
        assert_eq!(json["seek"], 120);
    }

    #[test_log::test]
    fn test_set_seek_deserialization() {
        let json = json!({
            "sessionId": 99,
            "profile": "my-profile",
            "playbackTarget": {
                "type": "AUDIO_ZONE",
                "audioZoneId": 1
            },
            "seek": 300
        });

        let seek: SetSeek = serde_json::from_value(json).unwrap();

        assert_eq!(seek.session_id, 99);
        assert_eq!(seek.profile, "my-profile");
        assert_eq!(seek.seek, 300);
    }

    #[test_log::test]
    fn test_set_seek_default() {
        let seek = SetSeek::default();

        assert_eq!(seek.session_id, 0);
        assert_eq!(seek.profile, "");
        assert_eq!(seek.seek, 0);
    }

    #[test_log::test]
    fn test_set_seek_clone_and_equality() {
        let seek1 = SetSeek {
            session_id: 10,
            profile: "profile1".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 50,
        };

        let seek2 = seek1.clone();
        assert_eq!(seek1, seek2);

        let seek3 = SetSeek {
            session_id: 11,
            profile: "profile1".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 50,
        };
        assert_ne!(seek1, seek3);
    }

    #[test_log::test]
    fn test_empty_payload_serialization() {
        let payload = EmptyPayload {};
        let json = serde_json::to_value(&payload).unwrap();

        assert!(json.is_object());
        assert_eq!(json.as_object().unwrap().len(), 0);
    }

    #[test_log::test]
    fn test_sessions_payload_serialization() {
        let payload = SessionsPayload { payload: vec![] };
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["payload"], json!([]));
    }

    #[test_log::test]
    fn test_connections_payload_serialization() {
        let payload = ConnectionsPayload { payload: vec![] };
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["payload"], json!([]));
    }

    #[test_log::test]
    fn test_audio_zone_with_sessions_payload_serialization() {
        let payload = AudioZoneWithSessionsPayload { payload: vec![] };
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["payload"], json!([]));
    }

    #[test_log::test]
    fn test_inbound_payload_set_seek_serialization() {
        let seek = SetSeek {
            session_id: 42,
            profile: "test-profile".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 120,
        };
        let payload = InboundPayload::SetSeek(SetSeekPayload { payload: seek });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "SET_SEEK");
        assert_eq!(json["payload"]["sessionId"], 42);
        assert_eq!(json["payload"]["profile"], "test-profile");
        assert_eq!(json["payload"]["seek"], 120);
    }

    #[test_log::test]
    fn test_inbound_payload_set_seek_deserialization() {
        let json = json!({
            "type": "SET_SEEK",
            "payload": {
                "sessionId": 99,
                "profile": "my-profile",
                "playbackTarget": {
                    "type": "AUDIO_ZONE",
                    "audioZoneId": 1
                },
                "seek": 300
            }
        });
        let payload: InboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            InboundPayload::SetSeek(p) => {
                assert_eq!(p.payload.session_id, 99);
                assert_eq!(p.payload.profile, "my-profile");
                assert_eq!(p.payload.seek, 300);
            }
            _ => panic!("Expected SetSeek variant"),
        }
    }

    #[test_log::test]
    fn test_inbound_payload_delete_session_serialization() {
        let delete = DeleteSession { session_id: 123 };
        let payload = InboundPayload::DeleteSession(DeleteSessionPayload { payload: delete });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "DELETE_SESSION");
        assert_eq!(json["payload"]["sessionId"], 123);
    }

    #[test_log::test]
    fn test_inbound_payload_delete_session_deserialization() {
        let json = json!({
            "type": "DELETE_SESSION",
            "payload": {
                "sessionId": 456
            }
        });
        let payload: InboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            InboundPayload::DeleteSession(p) => {
                assert_eq!(p.payload.session_id, 456);
            }
            _ => panic!("Expected DeleteSession variant"),
        }
    }

    #[test_log::test]
    fn test_inbound_payload_register_connection_serialization() {
        let register = RegisterConnection {
            connection_id: "conn-123".to_string(),
            name: "Test Connection".to_string(),
            players: vec![],
        };
        let payload =
            InboundPayload::RegisterConnection(RegisterConnectionPayload { payload: register });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "REGISTER_CONNECTION");
        assert_eq!(json["payload"]["connectionId"], "conn-123");
        assert_eq!(json["payload"]["name"], "Test Connection");
    }

    #[test_log::test]
    fn test_inbound_payload_register_connection_deserialization() {
        let json = json!({
            "type": "REGISTER_CONNECTION",
            "payload": {
                "connectionId": "conn-456",
                "name": "My Connection",
                "players": []
            }
        });
        let payload: InboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            InboundPayload::RegisterConnection(p) => {
                assert_eq!(p.payload.connection_id, "conn-456");
                assert_eq!(p.payload.name, "My Connection");
                assert!(p.payload.players.is_empty());
            }
            _ => panic!("Expected RegisterConnection variant"),
        }
    }

    #[test_log::test]
    fn test_inbound_payload_register_players_serialization() {
        let players = vec![RegisterPlayer {
            audio_output_id: "output-1".to_string(),
            name: "Speaker 1".to_string(),
        }];
        let payload = InboundPayload::RegisterPlayers(RegisterPlayersPayload { payload: players });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "REGISTER_PLAYERS");
        assert_eq!(json["payload"][0]["audioOutputId"], "output-1");
        assert_eq!(json["payload"][0]["name"], "Speaker 1");
    }

    #[test_log::test]
    fn test_inbound_payload_register_players_deserialization() {
        let json = json!({
            "type": "REGISTER_PLAYERS",
            "payload": [
                {
                    "audioOutputId": "output-2",
                    "name": "Speaker 2"
                },
                {
                    "audioOutputId": "output-3",
                    "name": "Speaker 3"
                }
            ]
        });
        let payload: InboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            InboundPayload::RegisterPlayers(p) => {
                assert_eq!(p.payload.len(), 2);
                assert_eq!(p.payload[0].audio_output_id, "output-2");
                assert_eq!(p.payload[0].name, "Speaker 2");
                assert_eq!(p.payload[1].audio_output_id, "output-3");
                assert_eq!(p.payload[1].name, "Speaker 3");
            }
            _ => panic!("Expected RegisterPlayers variant"),
        }
    }

    #[test_log::test]
    fn test_inbound_payload_create_audio_zone_serialization() {
        let create = CreateAudioZone {
            name: "Living Room".to_string(),
        };
        let payload = InboundPayload::CreateAudioZone(CreateAudioZonePayload { payload: create });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "CREATE_AUDIO_ZONE");
        assert_eq!(json["payload"]["name"], "Living Room");
    }

    #[test_log::test]
    fn test_inbound_payload_create_audio_zone_deserialization() {
        let json = json!({
            "type": "CREATE_AUDIO_ZONE",
            "payload": {
                "name": "Kitchen"
            }
        });
        let payload: InboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            InboundPayload::CreateAudioZone(p) => {
                assert_eq!(p.payload.name, "Kitchen");
            }
            _ => panic!("Expected CreateAudioZone variant"),
        }
    }

    #[test_log::test]
    fn test_outbound_payload_set_seek_serialization() {
        let seek = SetSeek {
            session_id: 55,
            profile: "outbound-profile".to_string(),
            playback_target: ApiPlaybackTarget::default(),
            seek: 250,
        };
        let payload = OutboundPayload::SetSeek(SetSeekPayload { payload: seek });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "SET_SEEK");
        assert_eq!(json["payload"]["sessionId"], 55);
        assert_eq!(json["payload"]["profile"], "outbound-profile");
        assert_eq!(json["payload"]["seek"], 250);
    }

    #[test_log::test]
    fn test_outbound_payload_set_seek_deserialization() {
        let json = json!({
            "type": "SET_SEEK",
            "payload": {
                "sessionId": 77,
                "profile": "test-profile",
                "playbackTarget": {
                    "type": "AUDIO_ZONE",
                    "audioZoneId": 5
                },
                "seek": 180
            }
        });
        let payload: OutboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            OutboundPayload::SetSeek(p) => {
                assert_eq!(p.payload.session_id, 77);
                assert_eq!(p.payload.profile, "test-profile");
                assert_eq!(p.payload.seek, 180);
            }
            _ => panic!("Expected SetSeek variant"),
        }
    }

    #[test_log::test]
    fn test_outbound_payload_download_event_serialization() {
        let event_data = json!({"progress": 75, "file": "song.mp3"});
        let payload = OutboundPayload::DownloadEvent(DownloadEventPayload {
            payload: event_data,
        });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "DOWNLOAD_EVENT");
        assert_eq!(json["payload"]["progress"], 75);
        assert_eq!(json["payload"]["file"], "song.mp3");
    }

    #[test_log::test]
    fn test_outbound_payload_download_event_deserialization() {
        let json = json!({
            "type": "DOWNLOAD_EVENT",
            "payload": {
                "status": "complete",
                "bytes": 1024
            }
        });
        let payload: OutboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            OutboundPayload::DownloadEvent(p) => {
                assert_eq!(p.payload["status"], "complete");
                assert_eq!(p.payload["bytes"], 1024);
            }
            _ => panic!("Expected DownloadEvent variant"),
        }
    }

    #[test_log::test]
    fn test_outbound_payload_scan_event_serialization() {
        let event_data = json!({"scanned": 150, "total": 500});
        let payload = OutboundPayload::ScanEvent(ScanEventPayload {
            payload: event_data,
        });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "SCAN_EVENT");
        assert_eq!(json["payload"]["scanned"], 150);
        assert_eq!(json["payload"]["total"], 500);
    }

    #[test_log::test]
    fn test_outbound_payload_scan_event_deserialization() {
        let json = json!({
            "type": "SCAN_EVENT",
            "payload": {
                "phase": "analyzing",
                "count": 42
            }
        });
        let payload: OutboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            OutboundPayload::ScanEvent(p) => {
                assert_eq!(p.payload["phase"], "analyzing");
                assert_eq!(p.payload["count"], 42);
            }
            _ => panic!("Expected ScanEvent variant"),
        }
    }

    #[test_log::test]
    fn test_outbound_payload_connections_serialization() {
        let payload = OutboundPayload::Connections(ConnectionsPayload { payload: vec![] });
        let json = serde_json::to_value(&payload).unwrap();

        assert_eq!(json["type"], "CONNECTIONS");
        assert_eq!(json["payload"], json!([]));
    }

    #[test_log::test]
    fn test_outbound_payload_connections_deserialization() {
        let json = json!({
            "type": "CONNECTIONS",
            "payload": []
        });
        let payload: OutboundPayload = serde_json::from_value(json).unwrap();

        match payload {
            OutboundPayload::Connections(p) => {
                assert!(p.payload.is_empty());
            }
            _ => panic!("Expected Connections variant"),
        }
    }

    #[test_log::test]
    fn test_inbound_payload_invalid_type_fails_deserialization() {
        let json = json!({
            "type": "INVALID_TYPE",
            "payload": {}
        });
        let result: Result<InboundPayload, _> = serde_json::from_value(json);

        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_outbound_payload_invalid_type_fails_deserialization() {
        let json = json!({
            "type": "INVALID_TYPE",
            "payload": {}
        });
        let result: Result<OutboundPayload, _> = serde_json::from_value(json);

        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_inbound_payload_missing_type_fails_deserialization() {
        let json = json!({
            "payload": {}
        });
        let result: Result<InboundPayload, _> = serde_json::from_value(json);

        assert!(result.is_err());
    }
}
