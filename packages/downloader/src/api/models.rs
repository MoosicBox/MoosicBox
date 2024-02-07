use std::str::FromStr;

use moosicbox_files::files::track::TrackAudioQuality;
use moosicbox_json_utils::{serde_json::ToValue, MissingValue, ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

use crate::db::models::{DownloadApiSource, DownloadItem, DownloadTask, DownloadTaskState};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiDownloadLocation {
    pub id: u64,
    pub path: String,
}

impl MissingValue<ApiDownloadLocation> for &serde_json::Value {}
impl ToValueType<ApiDownloadLocation> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadLocation, ParseError> {
        Ok(ApiDownloadLocation {
            id: self.to_value("id")?,
            path: self.to_value("path")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, PartialEq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiDownloadTaskState {
    #[default]
    Pending,
    Paused,
    Cancelled,
    Started,
    Finished,
}

impl MissingValue<ApiDownloadTaskState> for &serde_json::Value {}
impl MissingValue<ApiDownloadTaskState> for serde_json::Value {}
impl ToValueType<ApiDownloadTaskState> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadTaskState, ParseError> {
        Ok(ApiDownloadTaskState::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiDownloadTaskState".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ApiDownloadTaskState".into()))?)
    }
}

impl From<DownloadTaskState> for ApiDownloadTaskState {
    fn from(value: DownloadTaskState) -> Self {
        match value {
            DownloadTaskState::Pending => ApiDownloadTaskState::Pending,
            DownloadTaskState::Paused => ApiDownloadTaskState::Paused,
            DownloadTaskState::Cancelled => ApiDownloadTaskState::Cancelled,
            DownloadTaskState::Started => ApiDownloadTaskState::Started,
            DownloadTaskState::Finished => ApiDownloadTaskState::Finished,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiDownloadApiSource {
    Tidal,
    Qobuz,
}

impl From<DownloadApiSource> for ApiDownloadApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            DownloadApiSource::Tidal => ApiDownloadApiSource::Tidal,
            DownloadApiSource::Qobuz => ApiDownloadApiSource::Qobuz,
        }
    }
}

impl MissingValue<ApiDownloadApiSource> for &serde_json::Value {}
impl MissingValue<ApiDownloadApiSource> for serde_json::Value {}
impl ToValueType<ApiDownloadApiSource> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadApiSource, ParseError> {
        Ok(ApiDownloadApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiDownloadApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ApiDownloadApiSource".into()))?)
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiDownloadItem {
    #[serde(rename_all = "camelCase")]
    Track { track_id: u64 },
    #[serde(rename_all = "camelCase")]
    AlbumCover { album_id: u64 },
    #[serde(rename_all = "camelCase")]
    ArtistCover { album_id: u64 },
}

impl From<DownloadItem> for ApiDownloadItem {
    fn from(value: DownloadItem) -> Self {
        match value {
            DownloadItem::Track(track_id) => ApiDownloadItem::Track { track_id },
            DownloadItem::AlbumCover(album_id) => ApiDownloadItem::AlbumCover { album_id },
            DownloadItem::ArtistCover(album_id) => ApiDownloadItem::ArtistCover { album_id },
        }
    }
}

impl MissingValue<ApiDownloadItem> for &serde_json::Value {}
impl ToValueType<ApiDownloadItem> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => ApiDownloadItem::Track {
                track_id: self.to_value("track_id")?,
            },
            "ALBUM_COVER" => ApiDownloadItem::AlbumCover {
                album_id: self.to_value("album_id")?,
            },
            "ARTIST_COVER" => ApiDownloadItem::ArtistCover {
                album_id: self.to_value("album_id")?,
            },
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
pub struct ApiDownloadTask {
    pub id: u64,
    pub state: ApiDownloadTaskState,
    pub item: ApiDownloadItem,
    pub source: Option<ApiDownloadApiSource>,
    pub quality: Option<TrackAudioQuality>,
    pub file_path: String,
    pub progress: f64,
    pub bytes: u64,
    pub speed: Option<u64>,
}

impl MissingValue<ApiDownloadTask> for &serde_json::Value {}
impl ToValueType<ApiDownloadTask> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadTask, ParseError> {
        Ok(ApiDownloadTask {
            id: self.to_value("id")?,
            state: self.to_value("state")?,
            item: self.to_value_type()?,
            source: self.to_value("source")?,
            quality: self.to_value("quality")?,
            file_path: self.to_value("file_path")?,
            progress: 0.0,
            bytes: 0,
            speed: None,
        })
    }
}

impl From<DownloadTask> for ApiDownloadTask {
    fn from(value: DownloadTask) -> Self {
        Self {
            id: value.id,
            state: value.state.into(),
            item: value.item.into(),
            source: value.source.map(|source| source.into()),
            quality: value.quality.map(|quality| quality.into()),
            file_path: value.file_path,
            progress: 0.0,
            bytes: 0,
            speed: None,
        }
    }
}
