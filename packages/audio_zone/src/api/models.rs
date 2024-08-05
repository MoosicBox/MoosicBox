use serde::{Deserialize, Serialize};

use crate::models::AudioZone;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiAudioZone {
    pub id: String,
    pub name: String,
}

impl From<AudioZone> for ApiAudioZone {
    fn from(value: AudioZone) -> Self {
        Self {
            id: value.id,
            name: value.name,
        }
    }
}
