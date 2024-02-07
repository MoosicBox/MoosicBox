use std::str::FromStr;

use moosicbox_core::sqlite::models::{LibraryAlbum, LibraryTrack};
use moosicbox_files::files::track::TrackAudioQuality;
use moosicbox_json_utils::{serde_json::ToValue, ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

use crate::db::models::{DownloadApiSource, DownloadItem, DownloadTask, DownloadTaskState};

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
}

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

impl ToValueType<ApiDownloadApiSource> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadApiSource, ParseError> {
        Ok(ApiDownloadApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiDownloadApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ApiDownloadApiSource".into()))?)
    }
}

#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiDownloadItem {
    #[serde(rename_all = "camelCase")]
    Track {
        track_id: u64,
        source: DownloadApiSource,
        quality: TrackAudioQuality,
        artist_id: u64,
        artist: String,
        album_id: u64,
        album: String,
        title: String,
    },
    #[serde(rename_all = "camelCase")]
    AlbumCover {
        artist_id: u64,
        artist: String,
        album_id: u64,
        title: String,
    },
    #[serde(rename_all = "camelCase")]
    ArtistCover {
        artist_id: u64,
        album_id: u64,
        title: String,
    },
}

pub(crate) fn to_api_download_item(
    item: DownloadItem,
    tracks: &[LibraryTrack],
    albums: &[LibraryAlbum],
) -> ApiDownloadItem {
    match item {
        DownloadItem::Track {
            track_id,
            source,
            quality,
        } => {
            let track = tracks
                .iter()
                .find(|track| track.id == track_id as i32)
                .unwrap_or_else(|| panic!("No Track for id {track_id}"));

            ApiDownloadItem::Track {
                track_id,
                source,
                quality,
                artist_id: track.artist_id as u64,
                artist: track.artist.clone(),
                album_id: track.album_id as u64,
                album: track.album.clone(),
                title: track.title.clone(),
            }
        }
        DownloadItem::AlbumCover(album_id) => {
            let album = albums
                .iter()
                .find(|album| album.id == album_id as i32)
                .unwrap_or_else(|| panic!("No Album for id {album_id}"));

            ApiDownloadItem::AlbumCover {
                artist_id: album.artist_id as u64,
                artist: album.artist.clone(),
                album_id,
                title: album.title.clone(),
            }
        }
        DownloadItem::ArtistCover(album_id) => {
            let album = albums
                .iter()
                .find(|album| album.id == album_id as i32)
                .unwrap_or_else(|| panic!("No Album for id {album_id}"));

            ApiDownloadItem::ArtistCover {
                artist_id: album.artist_id as u64,
                album_id,
                title: album.artist.clone(),
            }
        }
    }
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
            },
            "ALBUM_COVER" => ApiDownloadItem::AlbumCover {
                artist_id: self.to_value("artist_id")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("album_id")?,
                title: self.to_value("title")?,
            },
            "ARTIST_COVER" => ApiDownloadItem::ArtistCover {
                artist_id: self.to_value("artist_id")?,
                album_id: self.to_value("album_id")?,
                title: self.to_value("title")?,
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
    pub file_path: String,
    pub progress: f64,
    pub bytes: u64,
    pub speed: Option<u64>,
}

impl ToValueType<ApiDownloadTask> for &serde_json::Value {
    fn to_value_type(self) -> Result<ApiDownloadTask, ParseError> {
        Ok(ApiDownloadTask {
            id: self.to_value("id")?,
            state: self.to_value("state")?,
            item: self.to_value_type()?,
            file_path: self.to_value("file_path")?,
            progress: 0.0,
            bytes: 0,
            speed: None,
        })
    }
}

pub(crate) fn to_api_download_task(
    task: DownloadTask,
    tracks: &[LibraryTrack],
    albums: &[LibraryAlbum],
) -> ApiDownloadTask {
    ApiDownloadTask {
        id: task.id,
        state: task.state.into(),
        item: to_api_download_item(task.item, tracks, albums),
        file_path: task.file_path,
        progress: 0.0,
        bytes: 0,
        speed: None,
    }
}
