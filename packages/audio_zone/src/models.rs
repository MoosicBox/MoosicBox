use moosicbox_json_utils::{database::ToValue as _, ParseError, ToValueType};

#[derive(Debug, Clone)]
pub struct AudioZone {
    pub id: String,
    pub name: String,
}

impl ToValueType<AudioZone> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<AudioZone, ParseError> {
        Ok(AudioZone {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
        })
    }
}
