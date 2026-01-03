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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{DownloadItem, DownloadLocation, DownloadTaskState};
    use crate::queue::ProgressEvent;
    use crate::{DownloadApiSource, db::models::DownloadTask};
    use moosicbox_music_api::models::TrackAudioQuality;
    use moosicbox_music_models::ApiSource;
    use pretty_assertions::assert_eq;
    use std::sync::LazyLock;

    static TEST_API_SOURCE: LazyLock<ApiSource> =
        LazyLock::new(|| ApiSource::register("TestApi", "TestApi"));

    #[test_log::test]
    fn test_api_download_task_state_display() {
        assert_eq!(ApiDownloadTaskState::Pending.to_string(), "pending");
        assert_eq!(ApiDownloadTaskState::Paused.to_string(), "paused");
        assert_eq!(ApiDownloadTaskState::Cancelled.to_string(), "cancelled");
        assert_eq!(ApiDownloadTaskState::Started.to_string(), "started");
        assert_eq!(ApiDownloadTaskState::Finished.to_string(), "finished");
        assert_eq!(ApiDownloadTaskState::Error.to_string(), "error");
    }

    #[test_log::test]
    fn test_api_download_task_state_from_download_task_state() {
        assert_eq!(
            ApiDownloadTaskState::from(DownloadTaskState::Pending),
            ApiDownloadTaskState::Pending
        );
        assert_eq!(
            ApiDownloadTaskState::from(DownloadTaskState::Paused),
            ApiDownloadTaskState::Paused
        );
        assert_eq!(
            ApiDownloadTaskState::from(DownloadTaskState::Cancelled),
            ApiDownloadTaskState::Cancelled
        );
        assert_eq!(
            ApiDownloadTaskState::from(DownloadTaskState::Started),
            ApiDownloadTaskState::Started
        );
        assert_eq!(
            ApiDownloadTaskState::from(DownloadTaskState::Finished),
            ApiDownloadTaskState::Finished
        );
        assert_eq!(
            ApiDownloadTaskState::from(DownloadTaskState::Error),
            ApiDownloadTaskState::Error
        );
    }

    #[test_log::test]
    fn test_download_task_state_from_api_download_task_state() {
        assert_eq!(
            DownloadTaskState::from(ApiDownloadTaskState::Pending),
            DownloadTaskState::Pending
        );
        assert_eq!(
            DownloadTaskState::from(ApiDownloadTaskState::Paused),
            DownloadTaskState::Paused
        );
        assert_eq!(
            DownloadTaskState::from(ApiDownloadTaskState::Cancelled),
            DownloadTaskState::Cancelled
        );
        assert_eq!(
            DownloadTaskState::from(ApiDownloadTaskState::Started),
            DownloadTaskState::Started
        );
        assert_eq!(
            DownloadTaskState::from(ApiDownloadTaskState::Finished),
            DownloadTaskState::Finished
        );
        assert_eq!(
            DownloadTaskState::from(ApiDownloadTaskState::Error),
            DownloadTaskState::Error
        );
    }

    #[test_log::test]
    fn test_api_download_location_from_download_location() {
        let location = DownloadLocation {
            id: 123,
            path: "/test/path".to_string(),
            created: "2024-01-01".to_string(),
            updated: "2024-01-02".to_string(),
        };

        let api_location: ApiDownloadLocation = location.into();

        assert_eq!(api_location.id, 123);
        assert_eq!(api_location.path, "/test/path");
    }

    #[test_log::test]
    fn test_api_download_api_source_from_download_api_source() {
        let source = DownloadApiSource::Api(TEST_API_SOURCE.clone());
        let api_source: ApiDownloadApiSource = source.into();

        assert_eq!(
            api_source,
            ApiDownloadApiSource::Api(TEST_API_SOURCE.clone())
        );
    }

    #[test_log::test]
    fn test_api_download_api_source_from_download_api_source_moosicbox() {
        let source = DownloadApiSource::MoosicBox("http://localhost".to_string());
        let api_source: ApiDownloadApiSource = source.into();

        assert_eq!(
            api_source,
            ApiDownloadApiSource::MoosicBox("http://localhost".to_string())
        );
    }

    #[test_log::test]
    fn test_api_download_item_from_download_item_track() {
        let item = DownloadItem::Track {
            source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            track_id: 123.into(),
            quality: TrackAudioQuality::FlacHighestRes,
            artist_id: 456.into(),
            artist: "Test Artist".to_string(),
            album_id: 789.into(),
            album: "Test Album".to_string(),
            title: "Test Track".to_string(),
            contains_cover: true,
        };

        let api_item: ApiDownloadItem = item.into();

        match api_item {
            ApiDownloadItem::Track {
                track_id,
                quality,
                title,
                ..
            } => {
                assert_eq!(track_id, 123.into());
                assert_eq!(quality, TrackAudioQuality::FlacHighestRes);
                assert_eq!(title, "Test Track");
            }
            _ => panic!("Expected Track variant"),
        }
    }

    #[test_log::test]
    fn test_stripped_api_download_item_from_download_item_track() {
        let item = DownloadItem::Track {
            source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            track_id: 123.into(),
            quality: TrackAudioQuality::FlacHighestRes,
            artist_id: 456.into(),
            artist: "Test Artist".to_string(),
            album_id: 789.into(),
            album: "Test Album".to_string(),
            title: "Test Track".to_string(),
            contains_cover: true,
        };

        let stripped_item: StrippedApiDownloadItem = item.into();

        match stripped_item {
            StrippedApiDownloadItem::Track {
                track_id,
                quality,
                source,
            } => {
                assert_eq!(track_id, 123.into());
                assert_eq!(quality, TrackAudioQuality::FlacHighestRes);
                assert_eq!(source, DownloadApiSource::Api(TEST_API_SOURCE.clone()));
            }
            _ => panic!("Expected Track variant"),
        }
    }

    #[test_log::test]
    fn test_stripped_api_download_item_from_download_item_album_cover() {
        let item = DownloadItem::AlbumCover {
            source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            artist_id: 456.into(),
            artist: "Test Artist".to_string(),
            album_id: 789.into(),
            title: "Test Album".to_string(),
            contains_cover: true,
        };

        let stripped_item: StrippedApiDownloadItem = item.into();

        match stripped_item {
            StrippedApiDownloadItem::AlbumCover { album_id } => {
                assert_eq!(album_id, 789.into());
            }
            _ => panic!("Expected AlbumCover variant"),
        }
    }

    #[test_log::test]
    fn test_api_progress_event_from_progress_event_size() {
        let task = DownloadTask {
            id: 42,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/test".to_string(),
            total_bytes: None,
            created: String::new(),
            updated: String::new(),
        };

        let event = ProgressEvent::Size {
            task,
            bytes: Some(1024),
        };
        let api_event: ApiProgressEvent = event.into();

        match api_event {
            ApiProgressEvent::Size { task_id, bytes } => {
                assert_eq!(task_id, 42);
                assert_eq!(bytes, Some(1024));
            }
            _ => panic!("Expected Size variant"),
        }
    }

    #[test_log::test]
    fn test_api_progress_event_from_progress_event_speed() {
        let task = DownloadTask {
            id: 42,
            state: DownloadTaskState::Started,
            item: DownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/test".to_string(),
            total_bytes: None,
            created: String::new(),
            updated: String::new(),
        };

        let event = ProgressEvent::Speed {
            task,
            bytes_per_second: 1024.5,
        };
        let api_event: ApiProgressEvent = event.into();

        match api_event {
            ApiProgressEvent::Speed {
                task_id,
                bytes_per_second,
            } => {
                assert_eq!(task_id, 42);
                assert!((bytes_per_second - 1024.5).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Speed variant"),
        }
    }

    #[test_log::test]
    fn test_api_download_task_from_download_task() {
        let task = DownloadTask {
            id: 99,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/test/path".to_string(),
            total_bytes: Some(2048),
            created: String::new(),
            updated: String::new(),
        };

        let api_task: ApiDownloadTask = task.into();

        assert_eq!(api_task.id, 99);
        assert_eq!(api_task.state, ApiDownloadTaskState::Pending);
        assert_eq!(api_task.file_path, "/test/path");
        assert_eq!(api_task.total_bytes, Some(2048));
        assert!(api_task.progress.abs() < f64::EPSILON);
        assert_eq!(api_task.bytes, 0);
        assert_eq!(api_task.speed, None);
    }

    #[test_log::test]
    fn test_stripped_api_download_task_from_download_task() {
        let task = DownloadTask {
            id: 99,
            state: DownloadTaskState::Pending,
            item: DownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/test/path".to_string(),
            total_bytes: Some(2048),
            created: String::new(),
            updated: String::new(),
        };

        let stripped_task: StrippedApiDownloadTask = task.into();

        assert_eq!(stripped_task.id, 99);
        assert_eq!(stripped_task.state, ApiDownloadTaskState::Pending);
        assert_eq!(stripped_task.file_path, "/test/path");
        assert_eq!(stripped_task.total_bytes, Some(2048));
    }

    #[test_log::test]
    fn test_api_progress_event_from_progress_event_bytes_read() {
        let task = DownloadTask {
            id: 42,
            state: DownloadTaskState::Started,
            item: DownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/test".to_string(),
            total_bytes: None,
            created: String::new(),
            updated: String::new(),
        };

        let event = ProgressEvent::BytesRead {
            task,
            read: 512,
            total: 1024,
        };
        let api_event: ApiProgressEvent = event.into();

        match api_event {
            ApiProgressEvent::BytesRead {
                task_id,
                read,
                total,
            } => {
                assert_eq!(task_id, 42);
                assert_eq!(read, 512);
                assert_eq!(total, 1024);
            }
            _ => panic!("Expected BytesRead variant"),
        }
    }

    #[test_log::test]
    fn test_api_progress_event_from_progress_event_state() {
        let task = DownloadTask {
            id: 42,
            state: DownloadTaskState::Finished,
            item: DownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/test".to_string(),
            total_bytes: None,
            created: String::new(),
            updated: String::new(),
        };

        let event = ProgressEvent::State {
            task,
            state: DownloadTaskState::Finished,
        };
        let api_event: ApiProgressEvent = event.into();

        match api_event {
            ApiProgressEvent::State { task_id, state } => {
                assert_eq!(task_id, 42);
                assert_eq!(state, ApiDownloadTaskState::Finished);
            }
            _ => panic!("Expected State variant"),
        }
    }

    #[test_log::test]
    fn test_api_progress_event_from_progress_event_ref() {
        let task = DownloadTask {
            id: 42,
            state: DownloadTaskState::Started,
            item: DownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/test".to_string(),
            total_bytes: None,
            created: String::new(),
            updated: String::new(),
        };

        let event = ProgressEvent::Speed {
            task,
            bytes_per_second: 2048.0,
        };
        // Test the reference conversion (From<&ProgressEvent>)
        let api_event: ApiProgressEvent = (&event).into();

        match api_event {
            ApiProgressEvent::Speed {
                task_id,
                bytes_per_second,
            } => {
                assert_eq!(task_id, 42);
                assert!((bytes_per_second - 2048.0).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Speed variant"),
        }
    }

    #[test_log::test]
    fn test_calc_progress_for_task_nonexistent_file_sets_zero_bytes() {
        let task = ApiDownloadTask {
            id: 1,
            state: ApiDownloadTaskState::Pending,
            item: ApiDownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/nonexistent/path/file.flac".to_string(),
            progress: 0.0,
            bytes: 0,
            total_bytes: Some(1024),
            speed: None,
        };

        let result = calc_progress_for_task(task);

        assert_eq!(result.bytes, 0);
        assert!(result.progress.abs() < f64::EPSILON);
    }

    #[test_log::test]
    fn test_calc_progress_for_task_finished_without_total_bytes_sets_100_percent() {
        let task = ApiDownloadTask {
            id: 1,
            state: ApiDownloadTaskState::Finished,
            item: ApiDownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            file_path: "/nonexistent/path/file.flac".to_string(),
            progress: 0.0,
            bytes: 0,
            total_bytes: None,
            speed: None,
        };

        let result = calc_progress_for_task(task);

        // When finished and no total_bytes, progress should be 100%
        assert!((result.progress - 100.0).abs() < f64::EPSILON);
    }

    #[test_log::test]
    fn test_api_download_item_from_download_item_album_cover() {
        let item = DownloadItem::AlbumCover {
            source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            artist_id: 456.into(),
            artist: "Test Artist".to_string(),
            album_id: 789.into(),
            title: "Test Album".to_string(),
            contains_cover: true,
        };

        let api_item: ApiDownloadItem = item.into();

        match api_item {
            ApiDownloadItem::AlbumCover {
                album_id,
                artist_id,
                title,
                contains_cover,
                ..
            } => {
                assert_eq!(album_id, 789.into());
                assert_eq!(artist_id, 456.into());
                assert_eq!(title, "Test Album");
                assert!(contains_cover);
            }
            _ => panic!("Expected AlbumCover variant"),
        }
    }

    #[test_log::test]
    fn test_api_download_item_from_download_item_artist_cover() {
        let item = DownloadItem::ArtistCover {
            source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            artist_id: 456.into(),
            album_id: 789.into(),
            title: "Test Artist".to_string(),
            contains_cover: false,
        };

        let api_item: ApiDownloadItem = item.into();

        match api_item {
            ApiDownloadItem::ArtistCover {
                artist_id,
                album_id,
                title,
                contains_cover,
                ..
            } => {
                assert_eq!(artist_id, 456.into());
                assert_eq!(album_id, 789.into());
                assert_eq!(title, "Test Artist");
                assert!(!contains_cover);
            }
            _ => panic!("Expected ArtistCover variant"),
        }
    }

    #[test_log::test]
    fn test_stripped_api_download_item_from_download_item_artist_cover() {
        let item = DownloadItem::ArtistCover {
            source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            artist_id: 456.into(),
            album_id: 789.into(),
            title: "Test Artist".to_string(),
            contains_cover: false,
        };

        let stripped_item: StrippedApiDownloadItem = item.into();

        match stripped_item {
            StrippedApiDownloadItem::ArtistCover { album_id } => {
                assert_eq!(album_id, 789.into());
            }
            _ => panic!("Expected ArtistCover variant"),
        }
    }

    #[test_log::test]
    fn test_api_download_task_state_from_str() {
        assert_eq!(
            ApiDownloadTaskState::from_str("PENDING").unwrap(),
            ApiDownloadTaskState::Pending
        );
        assert_eq!(
            ApiDownloadTaskState::from_str("PAUSED").unwrap(),
            ApiDownloadTaskState::Paused
        );
        assert_eq!(
            ApiDownloadTaskState::from_str("CANCELLED").unwrap(),
            ApiDownloadTaskState::Cancelled
        );
        assert_eq!(
            ApiDownloadTaskState::from_str("STARTED").unwrap(),
            ApiDownloadTaskState::Started
        );
        assert_eq!(
            ApiDownloadTaskState::from_str("FINISHED").unwrap(),
            ApiDownloadTaskState::Finished
        );
        assert_eq!(
            ApiDownloadTaskState::from_str("ERROR").unwrap(),
            ApiDownloadTaskState::Error
        );
    }

    #[test_log::test]
    fn test_api_download_task_state_from_str_invalid() {
        assert!(ApiDownloadTaskState::from_str("INVALID").is_err());
    }

    #[test_log::test]
    fn test_api_download_task_state_default() {
        let state = ApiDownloadTaskState::default();
        assert_eq!(state, ApiDownloadTaskState::Pending);
    }

    #[test_log::test]
    fn test_stripped_api_download_item_to_value_type_invalid_type_returns_error() {
        let json = serde_json::json!({
            "type": "INVALID_TYPE",
            "track_id": 123,
            "source": {"source": "API", "url": "test"},
            "quality": "FLAC_HIGHEST_RES"
        });

        let result: Result<StrippedApiDownloadItem, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("Invalid DownloadItem type"));
                assert!(msg.contains("INVALID_TYPE"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_api_download_item_to_value_type_invalid_type_returns_error() {
        let json = serde_json::json!({
            "type": "UNKNOWN_TYPE",
            "source": {"source": "API", "url": "test"},
            "track_id": 1,
            "quality": "FLAC_HIGHEST_RES",
            "artist_id": 2,
            "artist": "Artist",
            "album_id": 3,
            "album": "Album",
            "title": "Title",
            "contains_cover": false
        });

        let result: Result<ApiDownloadItem, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("Invalid DownloadItem type"));
                assert!(msg.contains("UNKNOWN_TYPE"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_api_download_task_state_to_value_type_non_string_returns_error() {
        let json = serde_json::json!(123);

        let result: Result<ApiDownloadTaskState, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("ApiDownloadTaskState"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_api_download_task_state_to_value_type_invalid_string_returns_error() {
        let json = serde_json::json!("NOT_A_VALID_STATE");

        let result: Result<ApiDownloadTaskState, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("ApiDownloadTaskState"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_api_download_api_source_to_value_type_non_string_returns_error() {
        let json = serde_json::json!(42);

        let result: Result<ApiDownloadApiSource, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("ApiDownloadApiSource"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_api_download_api_source_to_value_type_invalid_string_returns_error() {
        let json = serde_json::json!("NOT_A_VALID_SOURCE");

        let result: Result<ApiDownloadApiSource, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("ApiDownloadApiSource"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_calc_progress_caps_at_100_percent_when_bytes_exceed_total() {
        // Create a task file that exists with a known size
        let task = ApiDownloadTask {
            id: 1,
            state: ApiDownloadTaskState::Started,
            item: ApiDownloadItem::Track {
                source: DownloadApiSource::Api(TEST_API_SOURCE.clone()),
                track_id: 1.into(),
                quality: TrackAudioQuality::FlacHighestRes,
                artist_id: 2.into(),
                artist: "Artist".to_string(),
                album_id: 3.into(),
                album: "Album".to_string(),
                title: "Title".to_string(),
                contains_cover: false,
            },
            // Use a real file path that exists (the test file)
            file_path: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("Cargo.toml")
                .to_str()
                .unwrap()
                .to_string(),
            progress: 0.0,
            bytes: 0,
            // Set total_bytes to a very small number so actual bytes will exceed it
            total_bytes: Some(1),
            speed: None,
        };

        let result = calc_progress_for_task(task);

        // Progress should be capped at 100%, not exceed it even if bytes > total_bytes
        assert!((result.progress - 100.0).abs() < f64::EPSILON);
        // Bytes should reflect actual file size (greater than 1)
        assert!(result.bytes > 1);
    }

    #[test_log::test]
    fn test_api_download_location_to_value_type_from_json() {
        let json = serde_json::json!({
            "id": 42,
            "path": "/music/downloads"
        });

        let result: Result<ApiDownloadLocation, _> = (&json).to_value_type();
        let location = result.unwrap();

        assert_eq!(location.id, 42);
        assert_eq!(location.path, "/music/downloads");
    }

    #[test_log::test]
    fn test_stripped_api_download_item_to_value_type_track_from_json() {
        let source =
            serde_json::to_value(DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let json = serde_json::json!({
            "type": "TRACK",
            "track_id": 123,
            "source": source,
            "quality": "FLAC_HIGHEST_RES"
        });

        let result: Result<StrippedApiDownloadItem, _> = (&json).to_value_type();
        let item = result.unwrap();

        match item {
            StrippedApiDownloadItem::Track {
                track_id,
                quality,
                source,
            } => {
                assert_eq!(track_id, 123.into());
                assert_eq!(quality, TrackAudioQuality::FlacHighestRes);
                assert_eq!(source, DownloadApiSource::Api(TEST_API_SOURCE.clone()));
            }
            _ => panic!("Expected Track variant"),
        }
    }

    #[test_log::test]
    fn test_stripped_api_download_item_to_value_type_album_cover_from_json() {
        let json = serde_json::json!({
            "type": "ALBUM_COVER",
            "album_id": 456
        });

        let result: Result<StrippedApiDownloadItem, _> = (&json).to_value_type();
        let item = result.unwrap();

        match item {
            StrippedApiDownloadItem::AlbumCover { album_id } => {
                assert_eq!(album_id, 456.into());
            }
            _ => panic!("Expected AlbumCover variant"),
        }
    }

    #[test_log::test]
    fn test_stripped_api_download_item_to_value_type_artist_cover_from_json() {
        let json = serde_json::json!({
            "type": "ARTIST_COVER",
            "album_id": 789
        });

        let result: Result<StrippedApiDownloadItem, _> = (&json).to_value_type();
        let item = result.unwrap();

        match item {
            StrippedApiDownloadItem::ArtistCover { album_id } => {
                assert_eq!(album_id, 789.into());
            }
            _ => panic!("Expected ArtistCover variant"),
        }
    }

    #[test_log::test]
    fn test_api_download_item_to_value_type_track_from_json() {
        let source =
            serde_json::to_value(DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let json = serde_json::json!({
            "type": "TRACK",
            "source": source,
            "track_id": 123,
            "quality": "FLAC_HIGHEST_RES",
            "artist_id": 456,
            "artist": "Test Artist",
            "album_id": 789,
            "album": "Test Album",
            "title": "Test Track",
            "contains_cover": true
        });

        let result: Result<ApiDownloadItem, _> = (&json).to_value_type();
        let item = result.unwrap();

        match item {
            ApiDownloadItem::Track {
                track_id,
                quality,
                artist,
                album,
                title,
                contains_cover,
                ..
            } => {
                assert_eq!(track_id, 123.into());
                assert_eq!(quality, TrackAudioQuality::FlacHighestRes);
                assert_eq!(artist, "Test Artist");
                assert_eq!(album, "Test Album");
                assert_eq!(title, "Test Track");
                assert!(contains_cover);
            }
            _ => panic!("Expected Track variant"),
        }
    }

    #[test_log::test]
    fn test_api_download_item_to_value_type_album_cover_from_json() {
        let source =
            serde_json::to_value(DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let json = serde_json::json!({
            "type": "ALBUM_COVER",
            "source": source,
            "artist_id": 456,
            "artist": "Test Artist",
            "album_id": 789,
            "title": "Test Album",
            "contains_cover": true
        });

        let result: Result<ApiDownloadItem, _> = (&json).to_value_type();
        let item = result.unwrap();

        match item {
            ApiDownloadItem::AlbumCover {
                album_id,
                artist_id,
                artist,
                title,
                contains_cover,
                ..
            } => {
                assert_eq!(album_id, 789.into());
                assert_eq!(artist_id, 456.into());
                assert_eq!(artist, "Test Artist");
                assert_eq!(title, "Test Album");
                assert!(contains_cover);
            }
            _ => panic!("Expected AlbumCover variant"),
        }
    }

    #[test_log::test]
    fn test_api_download_item_to_value_type_artist_cover_from_json() {
        let source =
            serde_json::to_value(DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let json = serde_json::json!({
            "type": "ARTIST_COVER",
            "source": source,
            "artist_id": 456,
            "album_id": 789,
            "title": "Test Artist",
            "contains_cover": false
        });

        let result: Result<ApiDownloadItem, _> = (&json).to_value_type();
        let item = result.unwrap();

        match item {
            ApiDownloadItem::ArtistCover {
                artist_id,
                album_id,
                title,
                contains_cover,
                ..
            } => {
                assert_eq!(artist_id, 456.into());
                assert_eq!(album_id, 789.into());
                assert_eq!(title, "Test Artist");
                assert!(!contains_cover);
            }
            _ => panic!("Expected ArtistCover variant"),
        }
    }

    #[test_log::test]
    fn test_api_download_task_to_value_type_from_json() {
        let source =
            serde_json::to_value(DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let json = serde_json::json!({
            "id": 99,
            "state": "PENDING",
            "type": "TRACK",
            "source": source,
            "track_id": 123,
            "quality": "FLAC_HIGHEST_RES",
            "artist_id": 456,
            "artist": "Test Artist",
            "album_id": 789,
            "album": "Test Album",
            "title": "Test Track",
            "contains_cover": false,
            "file_path": "/nonexistent/test/path.flac",
            "total_bytes": 1024
        });

        let result: Result<ApiDownloadTask, _> = (&json).to_value_type();
        let task = result.unwrap();

        assert_eq!(task.id, 99);
        assert_eq!(task.state, ApiDownloadTaskState::Pending);
        assert_eq!(task.file_path, "/nonexistent/test/path.flac");
        assert_eq!(task.total_bytes, Some(1024));
        // bytes should be 0 since file doesn't exist
        assert_eq!(task.bytes, 0);
        // progress should be 0 since file doesn't exist
        assert!(task.progress.abs() < f64::EPSILON);
    }

    #[test_log::test]
    fn test_api_download_task_to_value_type_from_json_with_null_total_bytes() {
        let source =
            serde_json::to_value(DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let json = serde_json::json!({
            "id": 100,
            "state": "STARTED",
            "type": "ALBUM_COVER",
            "source": source,
            "artist_id": 456,
            "artist": "Test Artist",
            "album_id": 789,
            "title": "Test Album",
            "contains_cover": true,
            "file_path": "/test/cover.jpg",
            "total_bytes": null
        });

        let result: Result<ApiDownloadTask, _> = (&json).to_value_type();
        let task = result.unwrap();

        assert_eq!(task.id, 100);
        assert_eq!(task.state, ApiDownloadTaskState::Started);
        assert_eq!(task.total_bytes, None);
    }
}
