use moosicbox_json_utils::{MissingValue, ParseError, ToValueType, database::ToValue as _};
use serde::{Deserialize, Serialize};
use switchy_database::{AsId, DatabaseValue};

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

impl MissingValue<ScanLocation> for &switchy_database::Row {}
impl ToValueType<ScanLocation> for &switchy_database::Row {
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
        DatabaseValue::Int64(i64::from(self.id))
    }
}
