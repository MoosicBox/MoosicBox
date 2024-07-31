use moosicbox_core::sqlite::models::SetSeek;
use moosicbox_session::models::{
    ApiConnection, ApiSession, ApiUpdateSession, CreateSession, DeleteSession, RegisterConnection,
    RegisterPlayer, SetSessionActivePlayers, UpdateSession,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};

#[derive(Debug, Serialize, Deserialize, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Connect,
    Disconnect,
    Message,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr)]
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

#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum InboundMessagePayload {
    Ping(EmptyPayload),
    GetConnectionId(EmptyPayload),
    GetSessions(EmptyPayload),
    CreateSession(CreateSessionPayload),
    UpdateSession(UpdateSessionPayload),
    DeleteSession(DeleteSessionPayload),
    RegisterConnection(RegisterConnectionPayload),
    RegisterPlayers(RegisterPlayersPayload),
    SetActivePlayers(SetActivePlayersPayload),
    PlaybackAction(PlaybackActionPayload),
    SetSeek(SetSeekPayload),
}

impl std::fmt::Display for InboundMessagePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EmptyPayload {}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionPayload {
    pub payload: CreateSession,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPayload {
    pub payload: UpdateSession,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionPayload {
    pub payload: DeleteSession,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterConnectionPayload {
    pub payload: RegisterConnection,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPlayersPayload {
    pub payload: Vec<RegisterPlayer>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetActivePlayersPayload {
    pub payload: SetSessionActivePlayers,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackActionPayload {
    pub payload: Value,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
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
