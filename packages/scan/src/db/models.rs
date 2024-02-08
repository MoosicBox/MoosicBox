use std::str::FromStr;

use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel, AsModelResult},
};
use moosicbox_json_utils::{rusqlite::ToValue, MissingValue, ParseError, ToValueType};
use rusqlite::{types::Value, Row};
use serde::{Deserialize, Serialize};

use crate::ScanOrigin;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScanLocation {
    pub id: u32,
    pub origin: ScanOrigin,
    pub path: Option<String>,
    pub created: String,
    pub updated: String,
}

impl MissingValue<ScanOrigin> for &rusqlite::Row<'_> {}
impl ToValueType<ScanOrigin> for Value {
    fn to_value_type(self) -> Result<ScanOrigin, ParseError> {
        match self {
            Value::Text(str) => Ok(ScanOrigin::from_str(&str).expect("Invalid ScanOrigin")),
            _ => Err(ParseError::ConvertType("ScanOrigin".into())),
        }
    }
}

impl MissingValue<ScanLocation> for &rusqlite::Row<'_> {}
impl ToValueType<ScanLocation> for &Row<'_> {
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

impl AsModelResult<ScanLocation, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<ScanLocation, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<ScanLocation> for Row<'_> {
    fn as_model(&self) -> ScanLocation {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsId for ScanLocation {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

impl AsModelResult<ScanOrigin, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<ScanOrigin, ParseError> {
        self.to_value("origin")
    }
}

impl AsModel<ScanOrigin> for Row<'_> {
    fn as_model(&self) -> ScanOrigin {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsId for ScanOrigin {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.as_ref().to_string())
    }
}
