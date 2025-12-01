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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_api::models::TrackAudioQuality;
    use moosicbox_music_models::ApiSource;
    use pretty_assertions::assert_eq;
    use std::sync::LazyLock;

    static TEST_API_SOURCE: LazyLock<ApiSource> =
        LazyLock::new(|| ApiSource::register("TestApi", "TestApi"));

    fn create_test_track_item() -> DownloadItem {
        DownloadItem::Track {
            source: crate::DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            track_id: 123.into(),
            quality: TrackAudioQuality::FlacHighestRes,
            artist_id: 456.into(),
            artist: "Test Artist".to_string(),
            album_id: 789.into(),
            album: "Test Album".to_string(),
            title: "Test Track".to_string(),
            contains_cover: true,
        }
    }

    fn create_test_album_cover_item() -> DownloadItem {
        DownloadItem::AlbumCover {
            source: crate::DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            artist_id: 456.into(),
            artist: "Test Artist".to_string(),
            album_id: 789.into(),
            title: "Test Album".to_string(),
            contains_cover: true,
        }
    }

    fn create_test_artist_cover_item() -> DownloadItem {
        DownloadItem::ArtistCover {
            source: crate::DownloadApiSource::Api(TEST_API_SOURCE.clone()),
            artist_id: 456.into(),
            album_id: 789.into(),
            title: "Test Artist".to_string(),
            contains_cover: false,
        }
    }

    #[test_log::test]
    fn test_download_item_source_track() {
        let item = create_test_track_item();
        let source = item.source();

        assert_eq!(
            source,
            &crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())
        );
    }

    #[test_log::test]
    fn test_download_item_source_album_cover() {
        let item = create_test_album_cover_item();
        let source = item.source();

        assert_eq!(
            source,
            &crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())
        );
    }

    #[test_log::test]
    fn test_download_item_source_artist_cover() {
        let item = create_test_artist_cover_item();
        let source = item.source();

        assert_eq!(
            source,
            &crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())
        );
    }

    #[test_log::test]
    fn test_download_item_quality_track() {
        let item = create_test_track_item();
        let quality = item.quality();

        assert_eq!(quality, Some(&TrackAudioQuality::FlacHighestRes));
    }

    #[test_log::test]
    fn test_download_item_quality_album_cover() {
        let item = create_test_album_cover_item();
        let quality = item.quality();

        assert_eq!(quality, None);
    }

    #[test_log::test]
    fn test_download_item_quality_artist_cover() {
        let item = create_test_artist_cover_item();
        let quality = item.quality();

        assert_eq!(quality, None);
    }

    #[test_log::test]
    fn test_download_item_track_title() {
        let item = create_test_track_item();
        let track = item.track();

        assert_eq!(track, Some(&"Test Track".to_string()));
    }

    #[test_log::test]
    fn test_download_item_track_non_track_returns_none() {
        let item = create_test_album_cover_item();
        let track = item.track();

        assert_eq!(track, None);
    }

    #[test_log::test]
    fn test_download_item_track_id() {
        let item = create_test_track_item();
        let track_id = item.track_id();

        assert_eq!(track_id, Some(&Id::from(123)));
    }

    #[test_log::test]
    fn test_download_item_track_id_non_track_returns_none() {
        let item = create_test_album_cover_item();
        let track_id = item.track_id();

        assert_eq!(track_id, None);
    }

    #[test_log::test]
    fn test_download_item_album_track() {
        let item = create_test_track_item();
        let album = item.album();

        assert_eq!(album, Some(&"Test Album".to_string()));
    }

    #[test_log::test]
    fn test_download_item_album_album_cover() {
        let item = create_test_album_cover_item();
        let album = item.album();

        assert_eq!(album, Some(&"Test Album".to_string()));
    }

    #[test_log::test]
    fn test_download_item_album_artist_cover_returns_none() {
        let item = create_test_artist_cover_item();
        let album = item.album();

        assert_eq!(album, None);
    }

    #[test_log::test]
    fn test_download_item_album_id() {
        let item = create_test_track_item();
        let album_id = item.album_id();

        assert_eq!(album_id, &Id::from(789));
    }

    #[test_log::test]
    fn test_download_item_artist() {
        let item = create_test_track_item();
        let artist = item.artist();

        assert_eq!(artist, &"Test Artist".to_string());
    }

    #[test_log::test]
    fn test_download_item_artist_for_artist_cover() {
        let item = create_test_artist_cover_item();
        let artist = item.artist();

        // For artist covers, the title field contains the artist name
        assert_eq!(artist, &"Test Artist".to_string());
    }

    #[test_log::test]
    fn test_download_item_artist_id() {
        let item = create_test_track_item();
        let artist_id = item.artist_id();

        assert_eq!(artist_id, &Id::from(456));
    }

    #[test_log::test]
    fn test_download_item_contains_cover_true() {
        let item = create_test_track_item();
        let contains_cover = item.contains_cover();

        assert!(contains_cover);
    }

    #[test_log::test]
    fn test_download_item_contains_cover_false() {
        let item = create_test_artist_cover_item();
        let contains_cover = item.contains_cover();

        assert!(!contains_cover);
    }

    #[test_log::test]
    fn test_download_task_state_as_ref() {
        assert_eq!(DownloadTaskState::Pending.as_ref(), "PENDING");
        assert_eq!(DownloadTaskState::Paused.as_ref(), "PAUSED");
        assert_eq!(DownloadTaskState::Cancelled.as_ref(), "CANCELLED");
        assert_eq!(DownloadTaskState::Started.as_ref(), "STARTED");
        assert_eq!(DownloadTaskState::Finished.as_ref(), "FINISHED");
        assert_eq!(DownloadTaskState::Error.as_ref(), "ERROR");
    }

    #[test_log::test]
    fn test_download_task_state_from_str() {
        assert_eq!(
            DownloadTaskState::from_str("PENDING").unwrap(),
            DownloadTaskState::Pending
        );
        assert_eq!(
            DownloadTaskState::from_str("PAUSED").unwrap(),
            DownloadTaskState::Paused
        );
        assert_eq!(
            DownloadTaskState::from_str("CANCELLED").unwrap(),
            DownloadTaskState::Cancelled
        );
        assert_eq!(
            DownloadTaskState::from_str("STARTED").unwrap(),
            DownloadTaskState::Started
        );
        assert_eq!(
            DownloadTaskState::from_str("FINISHED").unwrap(),
            DownloadTaskState::Finished
        );
        assert_eq!(
            DownloadTaskState::from_str("ERROR").unwrap(),
            DownloadTaskState::Error
        );
    }

    #[test_log::test]
    fn test_download_task_state_from_str_invalid() {
        assert!(DownloadTaskState::from_str("INVALID").is_err());
    }

    #[test_log::test]
    fn test_download_task_state_default() {
        let state = DownloadTaskState::default();
        assert_eq!(state, DownloadTaskState::Pending);
    }

    #[test_log::test]
    fn test_download_item_as_ref_track() {
        let item = create_test_track_item();
        assert_eq!(item.as_ref(), "TRACK");
    }

    #[test_log::test]
    fn test_download_item_as_ref_album_cover() {
        let item = create_test_album_cover_item();
        assert_eq!(item.as_ref(), "ALBUM_COVER");
    }

    #[test_log::test]
    fn test_download_item_as_ref_artist_cover() {
        let item = create_test_artist_cover_item();
        assert_eq!(item.as_ref(), "ARTIST_COVER");
    }

    #[test_log::test]
    fn test_download_item_to_value_type_from_json_invalid_type_returns_error() {
        use moosicbox_json_utils::ParseError;

        let json = serde_json::json!({
            "type": "INVALID_DOWNLOAD_TYPE",
            "source": {"source": "API", "url": "test"},
            "trackId": 1,
            "quality": "FLAC_HIGHEST_RES",
            "artistId": 2,
            "artist": "Artist",
            "albumId": 3,
            "album": "Album",
            "track": "Title",
            "containsCover": false
        });

        let result: Result<DownloadItem, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("Invalid DownloadItem type"));
                assert!(msg.contains("INVALID_DOWNLOAD_TYPE"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_task_state_to_value_type_from_json_non_string_returns_error() {
        use moosicbox_json_utils::ParseError;

        let json = serde_json::json!(42);

        let result: Result<DownloadTaskState, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("DownloadTaskState"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_task_state_to_value_type_from_json_invalid_string_returns_error() {
        use moosicbox_json_utils::ParseError;

        let json = serde_json::json!("NOT_A_STATE");

        let result: Result<DownloadTaskState, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("DownloadTaskState"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_api_source_to_value_type_from_json_invalid_json_returns_error() {
        use moosicbox_json_utils::ParseError;

        // Use a JSON value that is not valid for DownloadApiSource
        let json = serde_json::json!({"invalid": "json_structure"});

        let result: Result<crate::DownloadApiSource, ParseError> = (&json).to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("DownloadApiSource"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_location_as_id_returns_correct_database_value() {
        let location = DownloadLocation {
            id: 123,
            path: "/test/path".to_string(),
            created: "2024-01-01".to_string(),
            updated: "2024-01-02".to_string(),
        };

        let db_value = location.as_id();

        match db_value {
            DatabaseValue::Int64(val) => assert_eq!(val, 123),
            _ => panic!("Expected Int64 database value"),
        }
    }

    #[test_log::test]
    fn test_download_task_as_id_returns_correct_database_value() {
        let task = DownloadTask {
            id: 456,
            state: DownloadTaskState::Pending,
            item: create_test_track_item(),
            file_path: "/test/path".to_string(),
            total_bytes: None,
            created: String::new(),
            updated: String::new(),
        };

        let db_value = task.as_id();

        match db_value {
            DatabaseValue::Int64(val) => assert_eq!(val, 456),
            _ => panic!("Expected Int64 database value"),
        }
    }

    #[test_log::test]
    fn test_download_task_state_to_value_type_from_database_value() {
        let db_value = DatabaseValue::String("PENDING".to_string());
        let result: Result<DownloadTaskState, _> = db_value.to_value_type();
        assert_eq!(result.unwrap(), DownloadTaskState::Pending);

        let db_value = DatabaseValue::String("STARTED".to_string());
        let result: Result<DownloadTaskState, _> = db_value.to_value_type();
        assert_eq!(result.unwrap(), DownloadTaskState::Started);

        let db_value = DatabaseValue::String("FINISHED".to_string());
        let result: Result<DownloadTaskState, _> = db_value.to_value_type();
        assert_eq!(result.unwrap(), DownloadTaskState::Finished);

        let db_value = DatabaseValue::String("ERROR".to_string());
        let result: Result<DownloadTaskState, _> = db_value.to_value_type();
        assert_eq!(result.unwrap(), DownloadTaskState::Error);
    }

    #[test_log::test]
    fn test_download_task_state_to_value_type_from_database_value_non_string_returns_error() {
        use moosicbox_json_utils::ParseError;

        let db_value = DatabaseValue::Int64(42);
        let result: Result<DownloadTaskState, ParseError> = db_value.to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("DownloadTaskState"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_task_state_to_value_type_from_database_value_invalid_string_returns_error() {
        use moosicbox_json_utils::ParseError;

        let db_value = DatabaseValue::String("INVALID_STATE".to_string());
        let result: Result<DownloadTaskState, ParseError> = db_value.to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("DownloadTaskState"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_api_source_to_value_type_from_database_value_moosicbox() {
        let json_str = r#"{"source":"MOOSIC_BOX","url":"http://localhost:8080"}"#;
        let db_value = DatabaseValue::String(json_str.to_string());
        let result: Result<crate::DownloadApiSource, _> = db_value.to_value_type();

        let source = result.unwrap();
        assert!(
            matches!(source, crate::DownloadApiSource::MoosicBox(url) if url == "http://localhost:8080")
        );
    }

    #[test_log::test]
    fn test_download_api_source_to_value_type_from_database_value_non_string_returns_error() {
        use moosicbox_json_utils::ParseError;

        let db_value = DatabaseValue::Int64(42);
        let result: Result<crate::DownloadApiSource, ParseError> = db_value.to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("DownloadApiSource"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_api_source_to_value_type_from_database_value_invalid_json_returns_error() {
        use moosicbox_json_utils::ParseError;

        let db_value = DatabaseValue::String("not valid json".to_string());
        let result: Result<crate::DownloadApiSource, ParseError> = db_value.to_value_type();

        assert!(result.is_err());
        match result.unwrap_err() {
            ParseError::ConvertType(msg) => {
                assert!(msg.contains("DownloadApiSource"));
            }
            _ => panic!("Expected ConvertType error"),
        }
    }

    #[test_log::test]
    fn test_download_location_to_value_type_from_row() {
        use switchy_database::Row;

        let row = Row {
            columns: vec![
                ("id".to_string(), DatabaseValue::UInt64(42)),
                (
                    "path".to_string(),
                    DatabaseValue::String("/music/downloads".to_string()),
                ),
                (
                    "created".to_string(),
                    DatabaseValue::String("2024-01-15T10:30:00Z".to_string()),
                ),
                (
                    "updated".to_string(),
                    DatabaseValue::String("2024-01-16T14:45:00Z".to_string()),
                ),
            ],
        };

        let result: Result<DownloadLocation, _> = (&row).to_value_type();
        let location = result.unwrap();

        assert_eq!(location.id, 42);
        assert_eq!(location.path, "/music/downloads");
        assert_eq!(location.created, "2024-01-15T10:30:00Z");
        assert_eq!(location.updated, "2024-01-16T14:45:00Z");
    }

    #[test_log::test]
    fn test_download_location_to_value_type_from_json() {
        let json = serde_json::json!({
            "id": 99,
            "path": "/another/path",
            "created": "2024-02-01",
            "updated": "2024-02-02"
        });

        let result: Result<DownloadLocation, _> = (&json).to_value_type();
        let location = result.unwrap();

        assert_eq!(location.id, 99);
        assert_eq!(location.path, "/another/path");
        assert_eq!(location.created, "2024-02-01");
        assert_eq!(location.updated, "2024-02-02");
    }

    #[test_log::test]
    fn test_download_item_to_value_type_from_row_track() {
        use switchy_database::Row;

        let source_json =
            serde_json::to_string(&crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let row = Row {
            columns: vec![
                (
                    "type".to_string(),
                    DatabaseValue::String("TRACK".to_string()),
                ),
                ("source".to_string(), DatabaseValue::String(source_json)),
                ("track_id".to_string(), DatabaseValue::UInt64(123)),
                (
                    "quality".to_string(),
                    DatabaseValue::String("FLAC_HIGHEST_RES".to_string()),
                ),
                ("artist_id".to_string(), DatabaseValue::UInt64(456)),
                (
                    "artist".to_string(),
                    DatabaseValue::String("Test Artist".to_string()),
                ),
                ("album_id".to_string(), DatabaseValue::UInt64(789)),
                (
                    "album".to_string(),
                    DatabaseValue::String("Test Album".to_string()),
                ),
                (
                    "track".to_string(),
                    DatabaseValue::String("Test Track".to_string()),
                ),
                ("contains_cover".to_string(), DatabaseValue::Bool(true)),
            ],
        };

        let result: Result<DownloadItem, _> = (&row).to_value_type();
        let item = result.unwrap();

        match item {
            DownloadItem::Track {
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
    fn test_download_item_to_value_type_from_row_album_cover() {
        use switchy_database::Row;

        let source_json =
            serde_json::to_string(&crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let row = Row {
            columns: vec![
                (
                    "type".to_string(),
                    DatabaseValue::String("ALBUM_COVER".to_string()),
                ),
                ("source".to_string(), DatabaseValue::String(source_json)),
                ("artist_id".to_string(), DatabaseValue::UInt64(456)),
                (
                    "artist".to_string(),
                    DatabaseValue::String("Test Artist".to_string()),
                ),
                ("album_id".to_string(), DatabaseValue::UInt64(789)),
                (
                    "album".to_string(),
                    DatabaseValue::String("Test Album".to_string()),
                ),
                ("contains_cover".to_string(), DatabaseValue::Bool(true)),
            ],
        };

        let result: Result<DownloadItem, _> = (&row).to_value_type();
        let item = result.unwrap();

        match item {
            DownloadItem::AlbumCover {
                album_id,
                artist,
                title,
                contains_cover,
                ..
            } => {
                assert_eq!(album_id, 789.into());
                assert_eq!(artist, "Test Artist");
                assert_eq!(title, "Test Album");
                assert!(contains_cover);
            }
            _ => panic!("Expected AlbumCover variant"),
        }
    }

    #[test_log::test]
    fn test_download_item_to_value_type_from_row_artist_cover() {
        use switchy_database::Row;

        let source_json =
            serde_json::to_string(&crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let row = Row {
            columns: vec![
                (
                    "type".to_string(),
                    DatabaseValue::String("ARTIST_COVER".to_string()),
                ),
                ("source".to_string(), DatabaseValue::String(source_json)),
                ("artist_id".to_string(), DatabaseValue::UInt64(456)),
                (
                    "artist".to_string(),
                    DatabaseValue::String("Test Artist".to_string()),
                ),
                ("album_id".to_string(), DatabaseValue::UInt64(789)),
                ("contains_cover".to_string(), DatabaseValue::Bool(false)),
            ],
        };

        let result: Result<DownloadItem, _> = (&row).to_value_type();
        let item = result.unwrap();

        match item {
            DownloadItem::ArtistCover {
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
    fn test_download_item_to_value_type_from_row_invalid_type_returns_error() {
        use moosicbox_json_utils::ParseError;
        use switchy_database::Row;

        let row = Row {
            columns: vec![(
                "type".to_string(),
                DatabaseValue::String("INVALID_TYPE".to_string()),
            )],
        };

        let result: Result<DownloadItem, ParseError> = (&row).to_value_type();

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
    fn test_download_task_to_value_type_from_row() {
        use switchy_database::Row;

        let source_json =
            serde_json::to_string(&crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let row = Row {
            columns: vec![
                ("id".to_string(), DatabaseValue::UInt64(100)),
                (
                    "state".to_string(),
                    DatabaseValue::String("STARTED".to_string()),
                ),
                (
                    "type".to_string(),
                    DatabaseValue::String("TRACK".to_string()),
                ),
                ("source".to_string(), DatabaseValue::String(source_json)),
                ("track_id".to_string(), DatabaseValue::UInt64(123)),
                (
                    "quality".to_string(),
                    DatabaseValue::String("FLAC_HIGHEST_RES".to_string()),
                ),
                ("artist_id".to_string(), DatabaseValue::UInt64(456)),
                (
                    "artist".to_string(),
                    DatabaseValue::String("Test Artist".to_string()),
                ),
                ("album_id".to_string(), DatabaseValue::UInt64(789)),
                (
                    "album".to_string(),
                    DatabaseValue::String("Test Album".to_string()),
                ),
                (
                    "track".to_string(),
                    DatabaseValue::String("Test Track".to_string()),
                ),
                ("contains_cover".to_string(), DatabaseValue::Bool(true)),
                (
                    "file_path".to_string(),
                    DatabaseValue::String("/music/test.flac".to_string()),
                ),
                ("total_bytes".to_string(), DatabaseValue::UInt64(1_024_000)),
                (
                    "created".to_string(),
                    DatabaseValue::String("2024-01-15".to_string()),
                ),
                (
                    "updated".to_string(),
                    DatabaseValue::String("2024-01-16".to_string()),
                ),
            ],
        };

        let result: Result<DownloadTask, _> = (&row).to_value_type();
        let task = result.unwrap();

        assert_eq!(task.id, 100);
        assert_eq!(task.state, DownloadTaskState::Started);
        assert_eq!(task.file_path, "/music/test.flac");
        assert_eq!(task.total_bytes, Some(1_024_000));
        assert_eq!(task.created, "2024-01-15");
        assert_eq!(task.updated, "2024-01-16");

        match task.item {
            DownloadItem::Track {
                track_id, title, ..
            } => {
                assert_eq!(track_id, 123.into());
                assert_eq!(title, "Test Track");
            }
            _ => panic!("Expected Track item"),
        }
    }

    #[test_log::test]
    fn test_download_task_to_value_type_from_row_with_null_total_bytes() {
        use switchy_database::Row;

        let source_json =
            serde_json::to_string(&crate::DownloadApiSource::Api(TEST_API_SOURCE.clone())).unwrap();
        let row = Row {
            columns: vec![
                ("id".to_string(), DatabaseValue::UInt64(101)),
                (
                    "state".to_string(),
                    DatabaseValue::String("PENDING".to_string()),
                ),
                (
                    "type".to_string(),
                    DatabaseValue::String("ALBUM_COVER".to_string()),
                ),
                ("source".to_string(), DatabaseValue::String(source_json)),
                ("artist_id".to_string(), DatabaseValue::UInt64(456)),
                (
                    "artist".to_string(),
                    DatabaseValue::String("Test Artist".to_string()),
                ),
                ("album_id".to_string(), DatabaseValue::UInt64(789)),
                (
                    "album".to_string(),
                    DatabaseValue::String("Test Album".to_string()),
                ),
                ("contains_cover".to_string(), DatabaseValue::Bool(true)),
                (
                    "file_path".to_string(),
                    DatabaseValue::String("/music/cover.jpg".to_string()),
                ),
                ("total_bytes".to_string(), DatabaseValue::Null),
                (
                    "created".to_string(),
                    DatabaseValue::String("2024-01-15".to_string()),
                ),
                (
                    "updated".to_string(),
                    DatabaseValue::String("2024-01-16".to_string()),
                ),
            ],
        };

        let result: Result<DownloadTask, _> = (&row).to_value_type();
        let task = result.unwrap();

        assert_eq!(task.id, 101);
        assert_eq!(task.state, DownloadTaskState::Pending);
        assert_eq!(task.total_bytes, None);

        match task.item {
            DownloadItem::AlbumCover { album_id, .. } => {
                assert_eq!(album_id, 789.into());
            }
            _ => panic!("Expected AlbumCover item"),
        }
    }
}
