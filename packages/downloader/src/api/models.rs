use std::str::FromStr;

use moosicbox_core::sqlite::models::{Album, Artist, Id, Track};
use moosicbox_json_utils::{serde_json::ToValue, ParseError, ToValueType};
use moosicbox_music_api::TrackAudioQuality;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

use crate::{
    db::models::{DownloadApiSource, DownloadItem, DownloadTask, DownloadTaskState},
    queue::ProgressEvent,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiProgressEvent {
    #[serde(rename_all = "camelCase")]
    Size { task_id: u64, bytes: Option<u64> },
    #[serde(rename_all = "camelCase")]
    Speed { task_id: u64, bytes_per_second: f64 },
    #[serde(rename_all = "camelCase")]
    BytesRead {
        task_id: u64,
        read: usize,
        total: usize,
    },
    #[serde(rename_all = "camelCase")]
    State {
        task_id: u64,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiDownloadLocation {
    pub id: u64,
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
    Error,
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
            DownloadTaskState::Pending => ApiDownloadTaskState::Pending,
            DownloadTaskState::Paused => ApiDownloadTaskState::Paused,
            DownloadTaskState::Cancelled => ApiDownloadTaskState::Cancelled,
            DownloadTaskState::Started => ApiDownloadTaskState::Started,
            DownloadTaskState::Finished => ApiDownloadTaskState::Finished,
            DownloadTaskState::Error => ApiDownloadTaskState::Error,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiDownloadApiSource {
    Tidal,
    Qobuz,
    Yt,
}

impl From<DownloadApiSource> for ApiDownloadApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            DownloadApiSource::Tidal => ApiDownloadApiSource::Tidal,
            DownloadApiSource::Qobuz => ApiDownloadApiSource::Qobuz,
            DownloadApiSource::Yt => ApiDownloadApiSource::Yt,
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

#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum StrippedApiDownloadItem {
    #[serde(rename_all = "camelCase")]
    Track {
        track_id: Id,
        source: DownloadApiSource,
        quality: TrackAudioQuality,
    },
    #[serde(rename_all = "camelCase")]
    AlbumCover { album_id: Id },
    #[serde(rename_all = "camelCase")]
    ArtistCover { album_id: Id },
}

#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiDownloadItem {
    #[serde(rename_all = "camelCase")]
    Track {
        track_id: Id,
        source: DownloadApiSource,
        quality: TrackAudioQuality,
        artist_id: Id,
        artist: String,
        album_id: Id,
        album: String,
        title: String,
        contains_cover: bool,
    },
    #[serde(rename_all = "camelCase")]
    AlbumCover {
        artist_id: Id,
        artist: String,
        album_id: Id,
        title: String,
        contains_cover: bool,
    },
    #[serde(rename_all = "camelCase")]
    ArtistCover {
        artist_id: Id,
        album_id: Id,
        title: String,
        contains_cover: bool,
    },
}

impl From<DownloadItem> for StrippedApiDownloadItem {
    fn from(value: DownloadItem) -> Self {
        match value {
            DownloadItem::Track {
                track_id,
                source,
                quality,
            } => StrippedApiDownloadItem::Track {
                track_id,
                source,
                quality,
            },
            DownloadItem::AlbumCover { album_id, .. } => {
                StrippedApiDownloadItem::AlbumCover { album_id }
            }
            DownloadItem::ArtistCover { album_id, .. } => {
                StrippedApiDownloadItem::ArtistCover { album_id }
            }
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

pub(crate) fn to_api_download_item(
    item: DownloadItem,
    tracks: &[Track],
    albums: &[Album],
    artists: &[Artist],
) -> Result<Option<ApiDownloadItem>, ParseError> {
    Ok(Some(match item {
        DownloadItem::Track {
            track_id,
            source,
            quality,
        } => {
            if let Some(track) = tracks.iter().find(|track| track.id == track_id) {
                ApiDownloadItem::Track {
                    track_id,
                    source,
                    quality,
                    artist_id: track.artist_id.to_owned(),
                    artist: track.artist.clone(),
                    album_id: track.album_id.to_owned(),
                    album: track.album.clone(),
                    title: track.title.clone(),
                    contains_cover: track.artwork.is_some(),
                }
            } else {
                return Ok(None);
            }
        }
        DownloadItem::AlbumCover { album_id, .. } => {
            if let Some(album) = albums.iter().find(|album| album.id == album_id) {
                ApiDownloadItem::AlbumCover {
                    artist_id: album.artist_id.to_owned(),
                    artist: album.artist.clone(),
                    album_id,
                    title: album.title.clone(),
                    contains_cover: album.artwork.is_some(),
                }
            } else {
                return Ok(None);
            }
        }
        DownloadItem::ArtistCover { album_id, .. } => {
            if let Some(album) = albums.iter().find(|album| album.id == album_id) {
                if let Some(artist) = artists.iter().find(|artist| artist.id == album.artist_id) {
                    ApiDownloadItem::ArtistCover {
                        artist_id: album.artist_id.to_owned(),
                        album_id,
                        title: album.artist.clone(),
                        contains_cover: artist.cover.is_some(),
                    }
                } else {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }
        }
    }))
}

impl ToValueType<ApiDownloadItem> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => ApiDownloadItem::Track {
                track_id: self.to_value("track_id")?,
                source: self.to_value("source")?,
                quality: self.to_value("quality")?,
                artist_id: self.to_value("artist_id")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("album_id")?,
                album: self.to_value("album")?,
                title: self.to_value("title")?,
                contains_cover: self.to_value("contains_cover")?,
            },
            "ALBUM_COVER" => ApiDownloadItem::AlbumCover {
                artist_id: self.to_value("artist_id")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("album_id")?,
                title: self.to_value("title")?,
                contains_cover: self.to_value("contains_cover")?,
            },
            "ARTIST_COVER" => ApiDownloadItem::ArtistCover {
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StrippedApiDownloadTask {
    pub id: u64,
    pub state: ApiDownloadTaskState,
    pub item: StrippedApiDownloadItem,
    pub file_path: String,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiDownloadTask {
    pub id: u64,
    pub state: ApiDownloadTaskState,
    pub item: ApiDownloadItem,
    pub file_path: String,
    pub progress: f64,
    pub bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<u64>,
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

fn calc_progress_for_task(mut task: ApiDownloadTask) -> ApiDownloadTask {
    task.bytes = std::fs::File::open(&task.file_path)
        .ok()
        .and_then(|file| file.metadata().ok().map(|metadata| metadata.len()))
        .unwrap_or(0);

    if let Some(total_bytes) = task.total_bytes {
        task.progress = 100.0_f64.min((task.bytes as f64) / (total_bytes as f64) * 100.0);
    } else if let ApiDownloadTaskState::Finished = task.state {
        task.progress = 100.0;
    }

    task
}

pub(crate) fn to_api_download_task(
    task: DownloadTask,
    tracks: &[Track],
    albums: &[Album],
    artists: &[Artist],
) -> Result<Option<ApiDownloadTask>, ParseError> {
    if let Some(item) = to_api_download_item(task.item, tracks, albums, artists)? {
        Ok(Some(calc_progress_for_task(ApiDownloadTask {
            id: task.id,
            state: task.state.into(),
            item,
            file_path: task.file_path,
            progress: 0.0,
            bytes: 0,
            total_bytes: task.total_bytes,
            speed: None,
        })))
    } else {
        Ok(None)
    }
}
