use async_trait::async_trait;
use moosicbox_core::sqlite::models::ToApi;
use moosicbox_database::{AsId, Database, DatabaseValue, TryFromDb};
use moosicbox_json_utils::{
    database::{DatabaseFetchError, ToValue as _},
    MissingValue, ParseError, ToValueType,
};
use serde::{Deserialize, Serialize};

use crate::db::models::AudioZoneModel;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
                .map(|x| x.into())
                .collect::<Vec<_>>(),
        }
    }
}

#[async_trait]
impl TryFromDb<AudioZoneModel> for AudioZone {
    type Error = DatabaseFetchError;

    async fn try_from_db(value: AudioZoneModel, db: &dyn Database) -> Result<Self, Self::Error> {
        Ok(AudioZone {
            id: value.id,
            name: value.name,
            players: crate::db::get_players(db, value.id).await?,
        })
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
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
                .map(|x| x.to_api())
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
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
            created: "".to_string(),
            updated: "".to_string(),
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
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiPlayer {
    pub player_id: u64,
    pub audio_output_id: String,
    pub name: String,
    pub playing: bool,
}

impl ToApi<ApiPlayer> for Player {
    fn to_api(self) -> ApiPlayer {
        ApiPlayer {
            player_id: self.id,
            audio_output_id: self.audio_output_id.clone(),
            name: self.name.clone(),
            playing: self.playing,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateAudioZone {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateAudioZone {
    pub id: u64,
    pub name: Option<String>,
    pub players: Option<Vec<Player>>,
}
