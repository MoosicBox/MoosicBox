use std::str::FromStr;

use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel, AsModelResult, TrackApiSource},
};
use moosicbox_files::files::track::TrackAudioQuality;
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

impl ToValueType<DownloadTaskState> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadTaskState, ParseError> {
        Ok(DownloadTaskState::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadTaskState".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadTaskState".into()))?)
    }
}

impl MissingValue<DownloadTaskState> for Value {}
impl MissingValue<DownloadTaskState> for &Row<'_> {}
impl ToValueType<DownloadTaskState> for Value {
    fn to_value_type(self) -> Result<DownloadTaskState, ParseError> {
        match self {
            Value::Text(str) => Ok(DownloadTaskState::from_str(&str)
                .map_err(|_| ParseError::ConvertType("DownloadTaskState".into()))?),
            _ => Err(ParseError::ConvertType("DownloadTaskState".into())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadApiSource {
    Tidal,
    Qobuz,
}

impl From<DownloadApiSource> for TrackApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            DownloadApiSource::Tidal => TrackApiSource::Tidal,
            DownloadApiSource::Qobuz => TrackApiSource::Qobuz,
        }
    }
}

impl From<TrackApiSource> for DownloadApiSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Tidal => DownloadApiSource::Tidal,
            TrackApiSource::Qobuz => DownloadApiSource::Qobuz,
            _ => panic!("Invalid TrackApiSource"),
        }
    }
}

impl ToValueType<DownloadApiSource> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadApiSource, ParseError> {
        Ok(DownloadApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadApiSource".into()))?)
    }
}

impl MissingValue<DownloadApiSource> for &Row<'_> {}
impl MissingValue<DownloadApiSource> for Value {}
impl ToValueType<DownloadApiSource> for Value {
    fn to_value_type(self) -> Result<DownloadApiSource, ParseError> {
        match self {
            Value::Text(str) => Ok(DownloadApiSource::from_str(&str)
                .map_err(|_| ParseError::ConvertType("DownloadApiSource".into()))?),
            _ => Err(ParseError::ConvertType("DownloadApiSource".into())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum DownloadItem {
    Track {
        track_id: u64,
        source: DownloadApiSource,
        quality: TrackAudioQuality,
    },
    AlbumCover(u64),
    ArtistCover(u64),
}

impl ToValueType<DownloadItem> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => DownloadItem::Track {
                track_id: self.to_value("track_id")?,
                source: self.to_value("source")?,
                quality: self.to_value("quality")?,
            },
            "ALBUM_COVER" => DownloadItem::AlbumCover(self.to_value("album_id")?),
            "ARTIST_COVER" => DownloadItem::ArtistCover(self.to_value("album_id")?),
            _ => {
                return Err(ParseError::ConvertType(format!(
                    "Invalid DownloadItem type '{item_type}'"
                )));
            }
        })
    }
}

impl MissingValue<DownloadItem> for &Row<'_> {}
impl ToValueType<DownloadItem> for &Row<'_> {
    fn to_value_type(self) -> Result<DownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => DownloadItem::Track {
                track_id: self.to_value("track_id")?,
                source: self.to_value("source")?,
                quality: self.to_value("quality")?,
            },
            "ALBUM_COVER" => DownloadItem::AlbumCover(self.to_value("album_id")?),
            "ARTIST_COVER" => DownloadItem::ArtistCover(self.to_value("album_id")?),
            _ => {
                return Err(ParseError::ConvertType(format!(
                    "Invalid DownloadItem type '{item_type}'"
                )));
            }
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateDownloadTask {
    pub item: DownloadItem,
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: u64,
    pub state: DownloadTaskState,
    pub item: DownloadItem,
    pub file_path: String,
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
            state: self.to_value("state")?,
            item: self.to_value_type()?,
            file_path: self.to_value("file_path")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl ToValueType<DownloadTask> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadTask, ParseError> {
        Ok(DownloadTask {
            id: self.to_value("id")?,
            state: self.to_value("state")?,
            item: self.to_value_type()?,
            file_path: self.to_value("file_path")?,
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
