use std::str::FromStr;

use moosicbox_core::sqlite::models::{ApiSource, Id, TrackApiSource};
use moosicbox_database::{AsId, DatabaseValue};
use moosicbox_json_utils::{
    database::ToValue as _, serde_json::ToValue, MissingValue, ParseError, ToValueType,
};
use moosicbox_music_api::models::TrackAudioQuality;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DownloadLocation {
    pub id: u64,
    pub path: String,
    pub created: String,
    pub updated: String,
}

impl ToValueType<DownloadLocation> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<DownloadLocation, ParseError> {
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
    fn as_id(&self) -> DatabaseValue {
        #[allow(clippy::cast_possible_wrap)]
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(
    Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, Copy, PartialEq, Eq, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadTaskState {
    #[default]
    Pending,
    Paused,
    Cancelled,
    Started,
    Finished,
    Error,
}

impl MissingValue<DownloadTaskState> for &moosicbox_database::Row {}
impl ToValueType<DownloadTaskState> for DatabaseValue {
    fn to_value_type(self) -> Result<DownloadTaskState, ParseError> {
        DownloadTaskState::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadTaskState".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadTaskState".into()))
    }
}

impl ToValueType<DownloadTaskState> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadTaskState, ParseError> {
        DownloadTaskState::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadTaskState".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadTaskState".into()))
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum DownloadApiSource {
    #[cfg(feature = "tidal")]
    Tidal,
    #[cfg(feature = "qobuz")]
    Qobuz,
    #[cfg(feature = "yt")]
    Yt,
}

impl From<ApiSource> for DownloadApiSource {
    fn from(value: ApiSource) -> Self {
        match value {
            #[cfg(feature = "tidal")]
            ApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            ApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            ApiSource::Yt => Self::Yt,
            _ => unreachable!(),
        }
    }
}

impl From<DownloadApiSource> for ApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            #[cfg(feature = "tidal")]
            DownloadApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            DownloadApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            DownloadApiSource::Yt => Self::Yt,
        }
    }
}

impl From<DownloadApiSource> for TrackApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            #[cfg(feature = "tidal")]
            DownloadApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            DownloadApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            DownloadApiSource::Yt => Self::Yt,
        }
    }
}

#[derive(Debug, Error)]
pub enum TryFromTrackApiSourceError {
    #[error("Invalid source")]
    InvalidSource,
}

impl TryFrom<TrackApiSource> for DownloadApiSource {
    type Error = TryFromTrackApiSourceError;

    fn try_from(value: TrackApiSource) -> Result<Self, Self::Error> {
        #[allow(unreachable_code)]
        Ok(match value {
            #[cfg(feature = "tidal")]
            TrackApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            TrackApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            TrackApiSource::Yt => Self::Yt,
            _ => return Err(Self::Error::InvalidSource),
        })
    }
}

impl MissingValue<DownloadApiSource> for &moosicbox_database::Row {}
impl ToValueType<DownloadApiSource> for DatabaseValue {
    fn to_value_type(self) -> Result<DownloadApiSource, ParseError> {
        DownloadApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadApiSource".into()))
    }
}

impl ToValueType<DownloadApiSource> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadApiSource, ParseError> {
        DownloadApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadApiSource".into()))
    }
}

#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum DownloadItem {
    Track {
        source: DownloadApiSource,
        track_id: Id,
        quality: TrackAudioQuality,
        artist_id: Id,
        artist: String,
        album_id: Id,
        album: String,
        title: String,
        contains_cover: bool,
    },
    AlbumCover {
        source: DownloadApiSource,
        artist_id: Id,
        artist: String,
        album_id: Id,
        title: String,
        contains_cover: bool,
    },
    ArtistCover {
        source: DownloadApiSource,
        artist_id: Id,
        album_id: Id,
        title: String,
        contains_cover: bool,
    },
}

#[allow(clippy::uninhabited_references)]
impl DownloadItem {
    pub const fn source(&self) -> &DownloadApiSource {
        match self {
            Self::Track { source, .. }
            | Self::AlbumCover { source, .. }
            | Self::ArtistCover { source, .. } => source,
        }
    }

    pub const fn quality(&self) -> Option<&TrackAudioQuality> {
        match self {
            Self::Track { quality, .. } => Some(quality),
            Self::AlbumCover { .. } | Self::ArtistCover { .. } => None,
        }
    }

    pub const fn track(&self) -> Option<&String> {
        match self {
            Self::Track { title, .. } => Some(title),
            Self::AlbumCover { .. } | Self::ArtistCover { .. } => None,
        }
    }

    pub const fn track_id(&self) -> Option<&Id> {
        match self {
            Self::Track { track_id, .. } => Some(track_id),
            Self::AlbumCover { .. } | Self::ArtistCover { .. } => None,
        }
    }

    pub const fn album(&self) -> Option<&String> {
        match self {
            Self::Track { album, .. } => Some(album),
            Self::AlbumCover { title, .. } => Some(title),
            Self::ArtistCover { .. } => None,
        }
    }

    pub const fn album_id(&self) -> &Id {
        match self {
            Self::Track { album_id, .. }
            | Self::AlbumCover { album_id, .. }
            | Self::ArtistCover { album_id, .. } => album_id,
        }
    }

    pub const fn artist(&self) -> &String {
        match self {
            Self::Track { artist, .. } | Self::AlbumCover { artist, .. } => artist,
            Self::ArtistCover { title, .. } => title,
        }
    }

    pub const fn artist_id(&self) -> &Id {
        match self {
            Self::Track { artist_id, .. }
            | Self::AlbumCover { artist_id, .. }
            | Self::ArtistCover { artist_id, .. } => artist_id,
        }
    }

    pub const fn contains_cover(&self) -> bool {
        match self {
            Self::Track { contains_cover, .. }
            | Self::AlbumCover { contains_cover, .. }
            | Self::ArtistCover { contains_cover, .. } => *contains_cover,
        }
    }
}

impl ToValueType<DownloadItem> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => DownloadItem::Track {
                source: self.to_value("source")?,
                track_id: self.to_value("trackId")?,
                quality: self.to_value("quality")?,
                artist_id: self.to_value("artistId")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("albumId")?,
                album: self.to_value("album")?,
                title: self.to_value("track")?,
                contains_cover: self.to_value("containsCover")?,
            },
            "ALBUM_COVER" => DownloadItem::AlbumCover {
                source: self.to_value("source")?,
                artist_id: self.to_value("artistId")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("albumId")?,
                title: self.to_value("album")?,
                contains_cover: self.to_value("containsCover")?,
            },
            "ARTIST_COVER" => DownloadItem::ArtistCover {
                source: self.to_value("source")?,
                artist_id: self.to_value("artistId")?,
                album_id: self.to_value("albumId")?,
                title: self.to_value("artist")?,
                contains_cover: self.to_value("containsCover")?,
            },
            _ => {
                return Err(ParseError::ConvertType(format!(
                    "Invalid DownloadItem type '{item_type}'"
                )));
            }
        })
    }
}

impl MissingValue<DownloadItem> for &moosicbox_database::Row {}
impl ToValueType<DownloadItem> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<DownloadItem, ParseError> {
        let item_type: String = self.to_value("type")?;

        Ok(match item_type.as_str() {
            "TRACK" => DownloadItem::Track {
                source: self.to_value("source")?,
                track_id: self.to_value("track_id")?,
                quality: self.to_value("quality")?,
                artist_id: self.to_value("artist_id")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("album_id")?,
                album: self.to_value("album")?,
                title: self.to_value("track")?,
                contains_cover: self.to_value("contains_cover")?,
            },
            "ALBUM_COVER" => DownloadItem::AlbumCover {
                source: self.to_value("source")?,
                artist_id: self.to_value("artist_id")?,
                artist: self.to_value("artist")?,
                album_id: self.to_value("album_id")?,
                title: self.to_value("album")?,
                contains_cover: self.to_value("contains_cover")?,
            },
            "ARTIST_COVER" => DownloadItem::ArtistCover {
                source: self.to_value("source")?,
                artist_id: self.to_value("artist_id")?,
                album_id: self.to_value("album_id")?,
                title: self.to_value("artist")?,
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
    pub total_bytes: Option<u64>,
    pub created: String,
    pub updated: String,
}

impl ToValueType<DownloadTask> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<DownloadTask, ParseError> {
        Ok(DownloadTask {
            id: self.to_value("id")?,
            state: self.to_value("state")?,
            item: self.to_value_type()?,
            file_path: self.to_value("file_path")?,
            total_bytes: self.to_value("total_bytes")?,
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
            total_bytes: self.to_value("total_bytes")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for DownloadTask {
    fn as_id(&self) -> DatabaseValue {
        #[allow(clippy::cast_possible_wrap)]
        DatabaseValue::Number(self.id as i64)
    }
}
