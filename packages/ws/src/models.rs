use moosicbox_audio_zone::models::{ApiAudioZoneWithSession, CreateAudioZone};
use moosicbox_session::models::{
    ApiConnection, ApiPlaybackTarget, ApiSession, ApiUpdateSession, CreateSession, DeleteSession,
    RegisterConnection, RegisterPlayer, UpdateSession,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::AsRefStr;

#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum InboundPayload {
    Ping(EmptyPayload),
    GetConnectionId(EmptyPayload),
    GetSessions(EmptyPayload),
    CreateSession(CreateSessionPayload),
    UpdateSession(UpdateSessionPayload),
    DeleteSession(DeleteSessionPayload),
    RegisterConnection(RegisterConnectionPayload),
    RegisterPlayers(RegisterPlayersPayload),
    CreateAudioZone(CreateAudioZonePayload),
    SetSeek(SetSeekPayload),
}

impl std::fmt::Display for InboundPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum OutboundPayload {
    ConnectionId(ConnectionIdPayload),
    Sessions(SessionsPayload),
    SessionUpdated(SessionUpdatedPayload),
    AudioZoneWithSessions(AudioZoneWithSessionsPayload),
    DownloadEvent(DownloadEventPayload),
    ScanEvent(ScanEventPayload),
    Connections(ConnectionsPayload),
    SetSeek(SetSeekPayload),
}

impl std::fmt::Display for OutboundPayload {
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
pub struct CreateAudioZonePayload {
    pub payload: CreateAudioZone,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackActionPayload {
    pub payload: Value,
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
pub struct AudioZoneWithSessionsPayload {
    pub payload: Vec<ApiAudioZoneWithSession>,
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
pub struct ScanEventPayload {
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetSeek {
    pub session_id: u64,
    pub profile: String,
    pub playback_target: ApiPlaybackTarget,
    pub seek: u64,
}
