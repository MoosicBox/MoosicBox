use std::str::FromStr;

use moosicbox_database::{AsId, DatabaseValue};
use moosicbox_json_utils::{database::ToValue as _, MissingValue, ParseError, ToValueType};
use serde::{Deserialize, Serialize};

use crate::ScanOrigin;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanLocation {
    pub id: u32,
    pub origin: ScanOrigin,
    pub path: Option<String>,
    pub created: String,
    pub updated: String,
}

impl MissingValue<ScanOrigin> for &moosicbox_database::Row {}
impl ToValueType<ScanOrigin> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<ScanOrigin, ParseError> {
        self.get("origin")
            .ok_or_else(|| ParseError::MissingValue("origin".into()))?
            .to_value_type()
    }
}
impl ToValueType<ScanOrigin> for DatabaseValue {
    fn to_value_type(self) -> Result<ScanOrigin, ParseError> {
        ScanOrigin::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ScanOrigin".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ScanOrigin".into()))
    }
}

impl MissingValue<ScanLocation> for &moosicbox_database::Row {}
impl ToValueType<ScanLocation> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<ScanLocation, ParseError> {
        Ok(ScanLocation {
            id: self.to_value("id")?,
            origin: self.to_value("origin")?,
            path: self.to_value("path")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for ScanLocation {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(i64::from(self.id))
    }
}

impl AsId for ScanOrigin {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.as_ref().to_string())
    }
}
