use std::str::FromStr;

use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel, AsModelResult},
};
use moosicbox_json_utils::{
    rusqlite::ToValue as RusqliteToValue, serde_json::ToValue, MissingValue, ParseError,
    ToValueType,
};
use rusqlite::{types::Value, Row};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DownloadLocation {
    pub id: u64,
    pub path: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<DownloadLocation> for Row<'_> {
    fn as_model(&self) -> DownloadLocation {
        AsModelResult::as_model(self)
            .unwrap_or_else(|e| panic!("Failed to get DownloadLocation: {e:?}"))
    }
}

impl AsModelResult<DownloadLocation, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<DownloadLocation, ParseError> {
        Ok(DownloadLocation {
            id: self.to_value("id")?,
            path: self.to_value("path")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl MissingValue<DownloadLocation> for &serde_json::Value {}
impl ToValueType<DownloadLocation> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadLocation, ParseError> {
        Ok(DownloadLocation {
            id: self.to_value("id")?,
            path: self.to_value("path")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for DownloadLocation {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, PartialEq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadTaskState {
    #[default]
    Pending,
    Paused,
    Cancelled,
    Started,
    Finished,
}

impl MissingValue<DownloadTaskState> for &serde_json::Value {}
impl MissingValue<DownloadTaskState> for serde_json::Value {}
impl ToValueType<DownloadTaskState> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadTaskState, ParseError> {
        Ok(DownloadTaskState::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadTaskState".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadTaskState".into()))?)
    }
}

impl MissingValue<DownloadTaskState> for &Row<'_> {}
impl MissingValue<DownloadTaskState> for Value {}
impl ToValueType<DownloadTaskState> for Value {
    fn to_value_type(self) -> Result<DownloadTaskState, ParseError> {
        match self {
            Value::Text(str) => Ok(DownloadTaskState::from_str(&str)
                .map_err(|_| ParseError::ConvertType("DownloadTaskState".into()))?),
            _ => Err(ParseError::ConvertType("DownloadTaskState".into())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: u64,
    pub download_location_id: u64,
    pub track_id: u64,
    pub progress: f64,
    pub state: DownloadTaskState,
    pub created: String,
    pub updated: String,
}

impl AsModel<DownloadTask> for Row<'_> {
    fn as_model(&self) -> DownloadTask {
        AsModelResult::as_model(self)
            .unwrap_or_else(|e| panic!("Failed to get DownloadTask: {e:?}"))
    }
}

impl AsModelResult<DownloadTask, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<DownloadTask, ParseError> {
        Ok(DownloadTask {
            id: self.to_value("id")?,
            download_location_id: self.to_value("download_location_id")?,
            track_id: self.to_value("track_id")?,
            progress: self.to_value("progress")?,
            state: self.to_value("state")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl MissingValue<DownloadTask> for &serde_json::Value {}
impl ToValueType<DownloadTask> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadTask, ParseError> {
        Ok(DownloadTask {
            id: self.to_value("id")?,
            download_location_id: self.to_value("download_location_id")?,
            track_id: self.to_value("track_id")?,
            progress: self.to_value("progress")?,
            state: self.to_value("state")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for DownloadTask {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}
