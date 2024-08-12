use moosicbox_json_utils::{database::ToValue as _, ParseError, ToValueType};

#[derive(Debug, Clone)]
pub struct AudioZoneModel {
    pub id: u64,
    pub name: String,
}

impl ToValueType<AudioZoneModel> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<AudioZoneModel, ParseError> {
        Ok(AudioZoneModel {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AudioZoneWithSessionModel {
    pub id: u64,
    pub session_id: u64,
    pub name: String,
}

impl ToValueType<AudioZoneWithSessionModel> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<AudioZoneWithSessionModel, ParseError> {
        Ok(AudioZoneWithSessionModel {
            id: self.to_value("id")?,
            session_id: self.to_value("session_id")?,
            name: self.to_value("name")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AudioZonePlayer {
    pub audio_zone_id: u64,
    pub player_id: u64,
}

impl ToValueType<AudioZonePlayer> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<AudioZonePlayer, ParseError> {
        Ok(AudioZonePlayer {
            audio_zone_id: self.to_value("audio_zone_id")?,
            player_id: self.to_value("player_id")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AudioZoneAndPlayer {
    pub audio_zone_id: u64,
    pub player_id: u64,
}

impl ToValueType<AudioZoneAndPlayer> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<AudioZoneAndPlayer, ParseError> {
        Ok(AudioZoneAndPlayer {
            audio_zone_id: self.to_value("audio_zone_id")?,
            player_id: self.to_value("player_id")?,
        })
    }
}
