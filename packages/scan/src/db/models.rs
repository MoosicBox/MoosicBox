use std::str::FromStr;

use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel},
};
use rusqlite::Row;
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

impl AsModel<ScanLocation> for Row<'_> {
    fn as_model(&self) -> ScanLocation {
        ScanLocation {
            id: self.get("id").unwrap(),
            origin: ScanOrigin::from_str(&self.get::<_, String>("origin").unwrap())
                .expect("Invalid ScanOrigin"),
            path: self.get("path").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
        }
    }
}

impl AsId for ScanLocation {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

impl AsModel<ScanOrigin> for Row<'_> {
    fn as_model(&self) -> ScanOrigin {
        ScanOrigin::from_str(&self.get::<_, String>("origin").unwrap()).expect("Invalid ScanOrigin")
    }
}

impl AsId for ScanOrigin {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::String(self.as_ref().to_string())
    }
}
