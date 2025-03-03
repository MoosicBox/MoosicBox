#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use moosicbox_database::{AsId, DatabaseValue};
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType, database::ToValue as _};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioZone {
    pub id: u64,
    pub name: String,
    pub players: Vec<Player>,
}

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

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiAudioZone {
    pub id: u64,
    pub name: String,
    pub players: Vec<ApiPlayer>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioZoneWithSession {
    pub id: u64,
    pub session_id: u64,
    pub name: String,
    pub players: Vec<Player>,
}

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

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiAudioZoneWithSession {
    pub id: u64,
    pub session_id: u64,
    pub name: String,
    pub players: Vec<ApiPlayer>,
}

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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: u64,
    pub audio_output_id: String,
    pub name: String,
    pub playing: bool,
    pub created: String,
    pub updated: String,
}

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

impl MissingValue<Player> for &moosicbox_database::Row {}
impl ToValueType<Player> for &moosicbox_database::Row {
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

impl AsId for Player {
    fn as_id(&self) -> DatabaseValue {
        #[allow(clippy::cast_possible_wrap)]
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiPlayer {
    pub player_id: u64,
    pub audio_output_id: String,
    pub name: String,
    pub playing: bool,
}

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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZone {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateAudioZone {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub players: Option<Vec<u64>>,
}
