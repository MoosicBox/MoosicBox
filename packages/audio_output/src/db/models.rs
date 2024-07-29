use moosicbox_database::{AsId, DatabaseValue};
use moosicbox_json_utils::{database::ToValue as _, serde_json::ToValue, ParseError, ToValueType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutputModel {
    pub id: String,
    pub name: String,
    pub spec_rate: u32,
    pub spec_channels: u32,
    pub created: String,
    pub updated: String,
}

impl ToValueType<AudioOutputModel> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<AudioOutputModel, ParseError> {
        Ok(AudioOutputModel {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            spec_rate: self.to_value("spec_rate")?,
            spec_channels: self.to_value("spec_channels")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl ToValueType<AudioOutputModel> for &serde_json::Value {
    fn to_value_type(self) -> Result<AudioOutputModel, ParseError> {
        Ok(AudioOutputModel {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            spec_rate: self.to_value("spec_rate")?,
            spec_channels: self.to_value("spec_channels")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for AudioOutputModel {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.id.clone())
    }
}
