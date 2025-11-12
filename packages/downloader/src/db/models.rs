//! Database model types for download management.
//!
//! Provides types for storing and retrieving download tasks and download locations
//! from the database. These types implement database serialization/deserialization
//! traits for the `switchy_database` interface.

use std::str::FromStr;

use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType, database::ToValue as _, serde_json::ToValue,
};
use moosicbox_music_api::models::TrackAudioQuality;
use moosicbox_music_models::id::Id;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use switchy_database::{AsId, DatabaseValue};

use crate::DownloadApiSource;

/// Represents a configured download location in the database.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DownloadLocation {
    /// Unique identifier for the download location
    pub id: u64,
    /// Filesystem path where downloads will be saved
    pub path: String,
    /// Timestamp when the location was created
    pub created: String,
    /// Timestamp when the location was last updated
    pub updated: String,
}

impl ToValueType<DownloadLocation> for &switchy_database::Row {
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
        DatabaseValue::Int64(self.id as i64)
    }
}

/// State of a download task in the queue.
#[derive(
    Debug, Serialize, Deserialize, EnumString, AsRefStr, Clone, Copy, PartialEq, Eq, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadTaskState {
    /// Task is waiting to be processed
    #[default]
    Pending,
    /// Task has been paused
    Paused,
    /// Task has been cancelled
    Cancelled,
    /// Task is currently being processed
    Started,
    /// Task has completed successfully
    Finished,
    /// Task encountered an error
    Error,
}

impl MissingValue<DownloadTaskState> for &switchy_database::Row {}
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

impl MissingValue<DownloadApiSource> for &switchy_database::Row {}
impl ToValueType<DownloadApiSource> for DatabaseValue {
    fn to_value_type(self) -> Result<DownloadApiSource, ParseError> {
        serde_json::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("DownloadApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("DownloadApiSource".into()))
    }
}

impl ToValueType<DownloadApiSource> for &serde_json::Value {
    fn to_value_type(self) -> Result<DownloadApiSource, ParseError> {
        serde_json::from_value(self.clone())
            .map_err(|_| ParseError::ConvertType("DownloadApiSource".into()))
    }
}

/// Type of item to be downloaded.
#[derive(Debug, Serialize, Deserialize, AsRefStr, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum DownloadItem {
    /// An audio track download
    Track {
        /// API source to download from
        source: DownloadApiSource,
        /// Unique track identifier
        track_id: Id,
        /// Audio quality to download
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
        /// Whether album artwork is available
        contains_cover: bool,
    },
    /// An album cover image download
    AlbumCover {
        /// API source to download from
        source: DownloadApiSource,
        /// Artist identifier
        artist_id: Id,
        /// Artist name
        artist: String,
        /// Album identifier
        album_id: Id,
        /// Album title
        title: String,
        /// Whether cover artwork is available
        contains_cover: bool,
    },
    /// An artist cover image download
    ArtistCover {
        /// API source to download from
        source: DownloadApiSource,
        /// Artist identifier
        artist_id: Id,
        /// Album identifier used to locate artist
        album_id: Id,
        /// Artist name
        title: String,
        /// Whether cover artwork is available
        contains_cover: bool,
    },
}

#[allow(clippy::uninhabited_references)]
impl DownloadItem {
    /// Returns the download API source for this item.
    #[must_use]
    pub const fn source(&self) -> &DownloadApiSource {
        match self {
            Self::Track { source, .. }
            | Self::AlbumCover { source, .. }
            | Self::ArtistCover { source, .. } => source,
        }
    }

    /// Returns the audio quality if this is a track download.
    ///
    /// Returns `None` for album and artist cover downloads.
    #[must_use]
    pub const fn quality(&self) -> Option<&TrackAudioQuality> {
        match self {
            Self::Track { quality, .. } => Some(quality),
            Self::AlbumCover { .. } | Self::ArtistCover { .. } => None,
        }
    }

    /// Returns the track title if this is a track download.
    ///
    /// Returns `None` for album and artist cover downloads.
    #[must_use]
    pub const fn track(&self) -> Option<&String> {
        match self {
            Self::Track { title, .. } => Some(title),
            Self::AlbumCover { .. } | Self::ArtistCover { .. } => None,
        }
    }

    /// Returns the track ID if this is a track download.
    ///
    /// Returns `None` for album and artist cover downloads.
    #[must_use]
    pub const fn track_id(&self) -> Option<&Id> {
        match self {
            Self::Track { track_id, .. } => Some(track_id),
            Self::AlbumCover { .. } | Self::ArtistCover { .. } => None,
        }
    }

    /// Returns the album title if this item is associated with an album.
    ///
    /// Returns `None` for artist cover downloads.
    #[must_use]
    pub const fn album(&self) -> Option<&String> {
        match self {
            Self::Track { album, .. } => Some(album),
            Self::AlbumCover { title, .. } => Some(title),
            Self::ArtistCover { .. } => None,
        }
    }

    /// Returns the album ID for this item.
    ///
    /// All download items are associated with an album.
    #[must_use]
    pub const fn album_id(&self) -> &Id {
        match self {
            Self::Track { album_id, .. }
            | Self::AlbumCover { album_id, .. }
            | Self::ArtistCover { album_id, .. } => album_id,
        }
    }

    /// Returns the artist name for this item.
    ///
    /// For artist covers, returns the artist's title field.
    #[must_use]
    pub const fn artist(&self) -> &String {
        match self {
            Self::Track { artist, .. } | Self::AlbumCover { artist, .. } => artist,
            Self::ArtistCover { title, .. } => title,
        }
    }

    /// Returns the artist ID for this item.
    ///
    /// All download items are associated with an artist.
    #[must_use]
    pub const fn artist_id(&self) -> &Id {
        match self {
            Self::Track { artist_id, .. }
            | Self::AlbumCover { artist_id, .. }
            | Self::ArtistCover { artist_id, .. } => artist_id,
        }
    }

    /// Returns whether cover artwork is available for this item.
    #[must_use]
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

impl MissingValue<DownloadItem> for &switchy_database::Row {}
impl ToValueType<DownloadItem> for &switchy_database::Row {
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

/// Parameters for creating a new download task.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateDownloadTask {
    /// The item to be downloaded
    pub item: DownloadItem,
    /// Destination filesystem path for the download
    pub file_path: String,
}

/// A download task stored in the database.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    /// Unique identifier for the task
    pub id: u64,
    /// Current state of the download
    pub state: DownloadTaskState,
    /// The item being downloaded
    pub item: DownloadItem,
    /// Destination filesystem path
    pub file_path: String,
    /// Total size of the download in bytes, if known
    pub total_bytes: Option<u64>,
    /// Timestamp when the task was created
    pub created: String,
    /// Timestamp when the task was last updated
    pub updated: String,
}

impl ToValueType<DownloadTask> for &switchy_database::Row {
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
        DatabaseValue::Int64(self.id as i64)
    }
}
