use moosicbox_core::sqlite::models::SetSeek;
use moosicbox_session::models::{ApiConnection, ApiSession, ApiUpdateSession};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::EnumString;

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Connect,
    Disconnect,
    Message,
}

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum InboundMessageType {
    Ping,
    GetConnectionId,
    GetSessions,
    CreateSession,
    UpdateSession,
    DeleteSession,
    RegisterConnection,
    RegisterPlayers,
    SetActivePlayers,
    PlaybackAction,
    SetSeek,
}

impl std::fmt::Display for InboundMessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutboundMessageType {
    Connect,
    NewConnection,
    ConnectionId,
    Sessions,
    SessionUpdated,
    DownloadEvent,
    Connections,
    SetSeek,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum OutboundPayload {
    ConnectionId(ConnectionIdPayload),
    Sessions(SessionsPayload),
    SessionUpdated(SessionUpdatedPayload),
    DownloadEvent(DownloadEventPayload),
    Connections(ConnectionsPayload),
    SetSeek(SetSeekPayload),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionIdPayload {
    pub connection_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionsPayload {
    pub payload: Vec<ApiSession>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdatedPayload {
    pub payload: ApiUpdateSession,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEventPayload {
    pub payload: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsPayload {
    pub payload: Vec<ApiConnection>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetSeekPayload {
    pub payload: SetSeek,
}
