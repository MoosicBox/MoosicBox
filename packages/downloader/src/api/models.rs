//! HTTP API model types for download service.
//!
//! Provides serializable types for HTTP API requests and responses, including
//! download tasks, download locations, progress events, and API sources. These
//! types are optimized for JSON serialization and API communication.

use std::str::FromStr;

use moosicbox_json_utils::{ParseError, ToValueType, serde_json::ToValue};
use moosicbox_music_api::models::TrackAudioQuality;
use moosicbox_music_models::{ApiSource, id::Id};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

use crate::{
    DownloadApiSource,
    db::models::{DownloadItem, DownloadLocation, DownloadTask, DownloadTaskState},
    queue::ProgressEvent,
};

/// Progress event for HTTP API responses.
#[derive(Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiProgressEvent {
    /// Total download size determined
    #[serde(rename_all = "camelCase")]
    Size {
        /// Task identifier
        task_id: u64,
        /// Total bytes to download
        bytes: Option<u64>,
    },
    /// Download speed update
    #[serde(rename_all = "camelCase")]
    Speed {
        /// Task identifier
        task_id: u64,
        /// Current download speed in bytes per second
        bytes_per_second: f64,
    },
    /// Download progress update
    #[serde(rename_all = "camelCase")]
    BytesRead {
        /// Task identifier
        task_id: u64,
        /// Bytes downloaded so far
        read: usize,
        /// Total bytes to download
        total: usize,
    },
    /// Task state changed
    #[serde(rename_all = "camelCase")]
    State {
        /// Task identifier
        task_id: u64,
        /// New task state
        state: ApiDownloadTaskState,
    },
}

impl From<ProgressEvent> for ApiProgressEvent {
    fn from(value: ProgressEvent) -> Self {
        (&value).into()
    }
}

impl From<&ProgressEvent> for ApiProgressEvent {
    fn from(value: &ProgressEvent) -> Self {
        match value {
            ProgressEvent::Size { task, bytes } => Self::Size {
                task_id: task.id,
                bytes: *bytes,
            },
            ProgressEvent::Speed {
                task,
                bytes_per_second,
            } => Self::Speed {
                task_id: task.id,
                bytes_per_second: *bytes_per_second,
            },
            ProgressEvent::BytesRead { task, read, total } => Self::BytesRead {
                task_id: task.id,
                read: *read,
                total: *total,
            },
            ProgressEvent::State { task, state } => Self::State {
                task_id: task.id,
                state: (*state).into(),
            },
        }
    }
}

/// Download location for HTTP API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiDownloadLocation {
    /// Unique identifier
    pub id: u64,
    /// Filesystem path
    pub path: String,
}

impl ToValueType<ApiDownloadLocation> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadLocation, ParseError> {
        Ok(ApiDownloadLocation {
            id: self.to_value("id")?,
            path: self.to_value("path")?,
        })
    }
}

impl From<DownloadLocation> for ApiDownloadLocation {
    fn from(value: DownloadLocation) -> Self {
        Self {
            id: value.id,
            path: value.path,
        }
    }
}

/// Download task state for HTTP API responses.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiDownloadTaskState {
    /// Task is waiting to be processed
    #[default]
    Pending,
    /// Task has been paused
    Paused,
    /// Task has been cancelled
    Cancelled,
    /// Task is currently downloading
    Started,
    /// Task completed successfully
    Finished,
    /// Task encountered an error
    Error,
}

impl std::fmt::Display for ApiDownloadTaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref().to_lowercase().as_str())
    }
}

impl ToValueType<ApiDownloadTaskState> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadTaskState, ParseError> {
        ApiDownloadTaskState::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiDownloadTaskState".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ApiDownloadTaskState".into()))
    }
}

impl From<DownloadTaskState> for ApiDownloadTaskState {
    fn from(value: DownloadTaskState) -> Self {
        match value {
            DownloadTaskState::Pending => Self::Pending,
            DownloadTaskState::Paused => Self::Paused,
            DownloadTaskState::Cancelled => Self::Cancelled,
            DownloadTaskState::Started => Self::Started,
            DownloadTaskState::Finished => Self::Finished,
            DownloadTaskState::Error => Self::Error,
        }
    }
}

impl From<ApiDownloadTaskState> for DownloadTaskState {
    fn from(value: ApiDownloadTaskState) -> Self {
        match value {
            ApiDownloadTaskState::Pending => Self::Pending,
            ApiDownloadTaskState::Paused => Self::Paused,
            ApiDownloadTaskState::Cancelled => Self::Cancelled,
            ApiDownloadTaskState::Started => Self::Started,
            ApiDownloadTaskState::Finished => Self::Finished,
            ApiDownloadTaskState::Error => Self::Error,
        }
    }
}

/// Download API source for HTTP API responses.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiDownloadApiSource {
    /// `MoosicBox` server source
    MoosicBox(String),
    /// Third-party API source
    Api(ApiSource),
}

impl From<DownloadApiSource> for ApiDownloadApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            DownloadApiSource::MoosicBox(host) => Self::MoosicBox(host),
            DownloadApiSource::Api(source) => Self::Api(source),
        }
    }
}

impl ToValueType<ApiDownloadApiSource> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadApiSource, ParseError> {
        ApiDownloadApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiDownloadApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ApiDownloadApiSource".into()))
    }
}

/// Minimal download item representation for HTTP API responses.
///
/// Contains only essential fields needed for identification.
#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum StrippedApiDownloadItem {
    /// Track download with minimal fields
    #[serde(rename_all = "camelCase")]
    Track {
        /// Track identifier
        track_id: Id,
        /// Download source
        source: DownloadApiSource,
        /// Audio quality
        quality: TrackAudioQuality,
    },
    /// Album cover download
    #[serde(rename_all = "camelCase")]
    AlbumCover {
        /// Album identifier
        album_id: Id,
    },
    /// Artist cover download
    #[serde(rename_all = "camelCase")]
    ArtistCover {
        /// Album identifier for locating artist
        album_id: Id,
    },
}

/// Download item for HTTP API responses.
#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiDownloadItem {
    /// Track download
    #[serde(rename_all = "camelCase")]
    Track {
        /// Download source
        source: DownloadApiSource,
        /// Track identifier
        track_id: Id,
        /// Audio quality
        quality: TrackAudioQuality,
        /// Artist identifier
        artist_id: Id,
        /// Artist name
        artist: String,
        /// Album identifier
        album_id: Id,
        /// Album title
        album: String,
        /// Track title
        title: String,
        /// Whether artwork is available
        contains_cover: bool,
    },
    /// Album cover download
    #[serde(rename_all = "camelCase")]
    AlbumCover {
        /// Download source
        source: DownloadApiSource,
        /// Artist identifier
        artist_id: Id,
        /// Artist name
        artist: String,
        /// Album identifier
        album_id: Id,
        /// Album title
        title: String,
        /// Whether artwork is available
        contains_cover: bool,
    },
    /// Artist cover download
    #[serde(rename_all = "camelCase")]
    ArtistCover {
        /// Download source
        source: DownloadApiSource,
        /// Artist identifier
        artist_id: Id,
        /// Album identifier for locating artist
        album_id: Id,
        /// Artist name
        title: String,
        /// Whether artwork is available
        contains_cover: bool,
    },
}

impl From<DownloadItem> for ApiDownloadItem {
    fn from(value: DownloadItem) -> Self {
        match value {
            DownloadItem::Track {
                source,
                track_id,
                quality,
                artist_id,
                artist,
                album_id,
                album,
                title,
                contains_cover,
            } => Self::Track {
                source,
                track_id,
                quality,
                artist_id,
                artist,
                album_id,
                album,
                title,
                contains_cover,
            },
            DownloadItem::AlbumCover {
                source,
                artist_id,
                artist,
                album_id,
                title,
                contains_cover,
                ..
            } => Self::AlbumCover {
                source,
                artist_id,
                artist,
                album_id,
                title,
                contains_cover,
            },
            DownloadItem::ArtistCover {
                source,
                artist_id,
                album_id,
                title,
                contains_cover,
                ..
            } => Self::ArtistCover {
                source,
                artist_id,
                album_id,
                title,
                contains_cover,
            },
        }
    }
}

impl From<DownloadItem> for StrippedApiDownloadItem {
    fn from(value: DownloadItem) -> Self {
        match value {
            DownloadItem::Track {
                track_id,
                source,
                quality,
                ..
            } => Self::Track {
                track_id,
                source,
                quality,
            },
            DownloadItem::AlbumCover { album_id, .. } => Self::AlbumCover { album_id },
            DownloadItem::ArtistCover { album_id, .. } => Self::ArtistCover { album_id },
        }
    }
}

impl ToValueType<StrippedApiDownloadItem> for &serde_json::Value {
    fn to_value_type(self) -> Result<StrippedApiDownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => StrippedApiDownloadItem::Track {
                track_id: self.to_value("track_id")?,
                source: self.to_value("source")?,
                quality: self.to_value("quality")?,
            },
            "ALBUM_COVER" => StrippedApiDownloadItem::AlbumCover {
                album_id: self.to_value("album_id")?,
            },
            "ARTIST_COVER" => StrippedApiDownloadItem::ArtistCover {
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

impl ToValueType<ApiDownloadItem> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => ApiDownloadItem::Track {
                source: self.to_value("source")?,
                track_id: self.to_value("track_id")?,
                quality: self.to_value("quality")?,
                artist_id: self.to_value("artist_id")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("album_id")?,
                album: self.to_value("album")?,
                title: self.to_value("title")?,
                contains_cover: self.to_value("contains_cover")?,
            },
            "ALBUM_COVER" => ApiDownloadItem::AlbumCover {
                source: self.to_value("source")?,
                artist_id: self.to_value("artist_id")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("album_id")?,
                title: self.to_value("title")?,
                contains_cover: self.to_value("contains_cover")?,
            },
            "ARTIST_COVER" => ApiDownloadItem::ArtistCover {
                source: self.to_value("source")?,
                artist_id: self.to_value("artist_id")?,
                album_id: self.to_value("album_id")?,
                title: self.to_value("title")?,
                contains_cover: self.to_value("contains_cover")?,
            },
            _ => {
                return Err(ParseError::ConvertType(format!(
                    "Invalid DownloadItem type '{item_type}'"
                )));
            }
        })
    }
}

/// Minimal download task representation for HTTP API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StrippedApiDownloadTask {
    /// Task identifier
    pub id: u64,
    /// Task state
    pub state: ApiDownloadTaskState,
    /// Download item with minimal fields
    pub item: StrippedApiDownloadItem,
    /// Destination filesystem path
    pub file_path: String,
    /// Total download size in bytes
    pub total_bytes: Option<u64>,
}

/// Download task for HTTP API responses.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiDownloadTask {
    /// Task identifier
    pub id: u64,
    /// Task state
    pub state: ApiDownloadTaskState,
    /// Download item
    pub item: ApiDownloadItem,
    /// Destination filesystem path
    pub file_path: String,
    /// Download progress percentage (0-100)
    pub progress: f64,
    /// Bytes downloaded so far
    pub bytes: u64,
    /// Total download size in bytes
    pub total_bytes: Option<u64>,
    /// Current download speed in bytes per second
    pub speed: Option<u64>,
}

impl From<DownloadTask> for ApiDownloadTask {
    fn from(value: DownloadTask) -> Self {
        #[allow(unreachable_code)]
        Self {
            id: value.id,
            state: value.state.into(),
            item: value.item.into(),
            file_path: value.file_path,
            progress: 0.0,
            bytes: 0,
            total_bytes: value.total_bytes,
            speed: None,
        }
    }
}

impl From<DownloadTask> for StrippedApiDownloadTask {
    fn from(value: DownloadTask) -> Self {
        Self {
            id: value.id,
            state: value.state.into(),
            item: value.item.into(),
            file_path: value.file_path,
            total_bytes: value.total_bytes,
        }
    }
}

impl ToValueType<ApiDownloadTask> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadTask, ParseError> {
        Ok(calc_progress_for_task(ApiDownloadTask {
            id: self.to_value("id")?,
            state: self.to_value("state")?,
            item: self.to_value_type()?,
            file_path: self.to_value("file_path")?,
            progress: 0.0,
            bytes: 0,
            total_bytes: self.to_value("total_bytes")?,
            speed: None,
        }))
    }
}

/// Calculates download progress for a task by reading the current file size.
///
/// Updates the `bytes` and `progress` fields based on the actual file size on disk
/// compared to the total expected size.
fn calc_progress_for_task(mut task: ApiDownloadTask) -> ApiDownloadTask {
    task.bytes = std::fs::File::open(&task.file_path)
        .ok()
        .and_then(|file| file.metadata().ok().map(|metadata| metadata.len()))
        .unwrap_or(0);

    #[allow(clippy::cast_precision_loss)]
    if let Some(total_bytes) = task.total_bytes {
        task.progress = 100.0_f64.min((task.bytes as f64) / (total_bytes as f64) * 100.0);
    } else if task.state == ApiDownloadTaskState::Finished {
        task.progress = 100.0;
    }

    task
}
